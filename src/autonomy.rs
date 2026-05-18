use std::collections::BTreeMap;

use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use openai_rs::{ChatCompletionRequest, Client, Message};
use pave_router::{
    AgentProfile, Router, RouterConfig, TaskCandidate, ToolAccess, default_agent_profiles,
    normalize_profiles,
};
use scheduler::OpenAiCompatibleConfig;
use serde::Deserialize;
use status::{
    CodebaseStatus, CommandResult, DiagnosticSeverity, SegmentKind, StatusReport, now_timestamp_ms,
};
use ui_bridge::{AutonomyDecision, AutonomySuggestion, AutonomyTrigger, PromptApproval};

const SUGGESTION_COOLDOWN_MS: u64 = 120_000;
const MAX_CANDIDATES: usize = 3;

#[derive(Debug, Clone)]
pub struct SegmenterInput {
    pub trigger: AutonomyTrigger,
    pub report: StatusReport,
    pub status: CodebaseStatus,
}

#[async_trait]
pub trait ProblemSegmenter: Send + Sync {
    async fn segment(&self, input: SegmenterInput) -> Result<Vec<TaskCandidate>>;
}

pub struct OpenAiProblemSegmenter {
    config: OpenAiCompatibleConfig,
    model: String,
    max_tokens: u32,
}

impl OpenAiProblemSegmenter {
    pub fn new(config: OpenAiCompatibleConfig, model: impl Into<String>, max_tokens: u32) -> Self {
        Self {
            config,
            model: model.into(),
            max_tokens,
        }
    }

    fn client(&self) -> Result<Client> {
        Client::builder()
            .api_key(self.config.api_key.clone())
            .base_url(self.config.base_url.clone())
            .build()
            .map_err(|err| anyhow!(err.to_string()))
    }
}

#[async_trait]
impl ProblemSegmenter for OpenAiProblemSegmenter {
    async fn segment(&self, input: SegmenterInput) -> Result<Vec<TaskCandidate>> {
        let client = self.client()?;
        let prompt = segmentation_prompt(&input)?;
        let first = request_json(&client, &self.model, self.max_tokens, prompt).await?;
        match parse_segmenter_output(&first) {
            Ok(tasks) => Ok(tasks),
            Err(first_err) => {
                let repair_prompt = repair_prompt(&first, &first_err.to_string());
                let repaired = request_json(&client, &self.model, self.max_tokens, repair_prompt)
                    .await
                    .context("repair segmenter output")?;
                parse_segmenter_output(&repaired)
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct AutonomyCoordinator {
    last_evaluated_snapshot: Option<String>,
    in_flight: bool,
    suggestions: BTreeMap<String, StoredSuggestion>,
    dismissed_until: BTreeMap<String, u64>,
}

impl AutonomyCoordinator {
    pub fn new() -> Self {
        Self {
            last_evaluated_snapshot: None,
            in_flight: false,
            suggestions: BTreeMap::new(),
            dismissed_until: BTreeMap::new(),
        }
    }

    pub fn set_in_flight(&mut self, in_flight: bool) {
        self.in_flight = in_flight;
    }

    pub fn profiles_or_default(
        &self,
        profiles: Vec<AgentProfile>,
        default_model: &str,
    ) -> Vec<AgentProfile> {
        let profiles = normalize_profiles(profiles);
        if profiles.is_empty() {
            default_agent_profiles(default_model)
        } else {
            profiles
        }
    }

    pub async fn handle_tick<S>(
        &mut self,
        trigger: AutonomyTrigger,
        report: StatusReport,
        status: CodebaseStatus,
        profiles: Vec<AgentProfile>,
        segmenter: &S,
        default_model: &str,
    ) -> AutonomyDecision
    where
        S: ProblemSegmenter,
    {
        let snapshot_hash = report.snapshot_hash.clone();
        let evaluation_key = status_fingerprint(&status, &report);
        self.expire_dismissals(now_timestamp_ms());

        if self.in_flight {
            return idle(snapshot_hash, "agent process is already running");
        }

        if self.last_evaluated_snapshot.as_ref() == Some(&evaluation_key)
            && !matches!(trigger, AutonomyTrigger::Manual)
        {
            return idle(snapshot_hash, "snapshot already evaluated");
        }

        if !has_actionable_status(&report, &status, &trigger) {
            self.last_evaluated_snapshot = Some(evaluation_key);
            return idle(snapshot_hash, "no actionable status signal");
        }

        let profiles = self.profiles_or_default(profiles, default_model);
        let router = match Router::new(profiles, RouterConfig::default()) {
            Ok(router) => router,
            Err(err) => return idle(snapshot_hash, err.to_string()),
        };

        let input = SegmenterInput {
            trigger,
            report: report.clone(),
            status,
        };
        let tasks = match segmenter.segment(input).await {
            Ok(tasks) => tasks.into_iter().take(MAX_CANDIDATES).collect(),
            Err(err) => {
                self.last_evaluated_snapshot = Some(evaluation_key);
                return idle(snapshot_hash, format!("segmenter failed closed: {err}"));
            }
        };

        let Some(route) = router.select(tasks) else {
            self.last_evaluated_snapshot = Some(evaluation_key);
            return idle(snapshot_hash, "no routed task passed thresholds");
        };

        self.last_evaluated_snapshot = Some(evaluation_key);
        if let Some(until) = self.dismissed_until.get(&route.suggestion_id) {
            return AutonomyDecision::Suppressed {
                snapshot_hash,
                suggestion_id: route.suggestion_id,
                reason: format!("suggestion dismissed until {until}"),
            };
        }

        let suggestion = AutonomySuggestion {
            suggestion_id: route.suggestion_id.clone(),
            snapshot_hash,
            created_at_ms: now_timestamp_ms(),
            required_approval: prompt_approval_from_tool_access(route.agent.default_approval),
            route,
        };
        self.suggestions.insert(
            suggestion.suggestion_id.clone(),
            StoredSuggestion {
                suggestion: suggestion.clone(),
            },
        );
        AutonomyDecision::Suggest { suggestion }
    }

    pub fn get_suggestion(&self, suggestion_id: &str) -> Option<AutonomySuggestion> {
        self.suggestions
            .get(suggestion_id)
            .map(|stored| stored.suggestion.clone())
    }

    pub fn dismiss(&mut self, suggestion_id: &str) -> bool {
        let existed = self.suggestions.remove(suggestion_id).is_some();
        self.dismissed_until.insert(
            suggestion_id.to_string(),
            now_timestamp_ms() + SUGGESTION_COOLDOWN_MS,
        );
        existed
    }

    fn expire_dismissals(&mut self, now: u64) {
        self.dismissed_until.retain(|_, until| *until > now);
    }
}

#[derive(Debug, Clone)]
struct StoredSuggestion {
    suggestion: AutonomySuggestion,
}

pub fn prompt_approval_from_tool_access(access: ToolAccess) -> PromptApproval {
    PromptApproval {
        allow_workspace_write: access.allow_workspace_write,
        allow_shell: access.allow_shell,
        allow_risky_shell: access.allow_risky_shell,
        allow_git_write: access.allow_git_write,
        allow_network: access.allow_network,
    }
}

pub fn build_suggestion_prompt(suggestion: &AutonomySuggestion) -> String {
    let route = &suggestion.route;
    let files = route
        .task
        .files
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let evidence = route.task.evidence.join("\n- ");
    format!(
        "Autonomous Marvis suggestion accepted.\n\nTask: {}\nRisk: {:?}\nAgent: {} ({})\nRoute: {}\nFiles: {}\nEvidence:\n- {}\n\nAgent task prompt:\n{}",
        route.task.title,
        route.task.risk_level,
        route.agent.label,
        route.agent.id,
        route.reason,
        if files.is_empty() { "none" } else { &files },
        if evidence.is_empty() {
            "none"
        } else {
            &evidence
        },
        route.task.prompt
    )
}

pub fn filter_tool_names(
    tools: Vec<protocol::DynamicToolSpec>,
    allowlist: &[String],
) -> Vec<protocol::DynamicToolSpec> {
    if allowlist.is_empty() {
        return tools;
    }
    let allowed = allowlist.iter().map(String::as_str).collect::<Vec<_>>();
    tools
        .into_iter()
        .filter(|tool| allowed.contains(&tool.name.as_str()))
        .collect()
}

fn idle(snapshot_hash: String, reason: impl Into<String>) -> AutonomyDecision {
    AutonomyDecision::Idle {
        snapshot_hash,
        reason: reason.into(),
    }
}

fn has_actionable_status(
    report: &StatusReport,
    status: &CodebaseStatus,
    trigger: &AutonomyTrigger,
) -> bool {
    if matches!(trigger, AutonomyTrigger::Manual) {
        return true;
    }
    if report.stuckness.is_some() || report.suggestion.is_some() {
        return true;
    }
    if status
        .vscode
        .problems
        .iter()
        .any(|diagnostic| matches!(diagnostic.severity, DiagnosticSeverity::Error))
    {
        return true;
    }
    if status
        .commands
        .recent_results
        .iter()
        .rev()
        .take(3)
        .any(CommandResult::failed)
    {
        return true;
    }
    report.active_segments.iter().any(|segment| {
        matches!(
            segment.kind,
            SegmentKind::CommandFailure
                | SegmentKind::FailingTest
                | SegmentKind::DiagnosticCluster
                | SegmentKind::Risk
        )
    })
}

fn segmentation_prompt(input: &SegmenterInput) -> Result<String> {
    let report_json = serde_json::to_string(&input.report)?;
    let status_json = serde_json::to_string(&compact_status(&input.status))?;
    Ok(format!(
        "You are Marvis autonomous problem segmenter. Output JSON only, no markdown.\n\
         Return {{\"tasks\":[]}} when there is no useful action.\n\
         Produce at most {MAX_CANDIDATES} tasks. Each task must have:\n\
         id, title, prompt, evidence, files, risk_level, needs_write, desired_tools, pave, confidence.\n\
         risk_level must be one of low, medium, high, critical.\n\
         pave is a JSON object with numeric dimensions from: rust, javascript, tests, diagnostics, refactor, explanation, patch, shell, git, docs, frontend, infra, risk_low, risk_medium, risk_high.\n\
         Use confidence 0.0 to 1.0. Prefer no task over noisy suggestions.\n\n\
         Trigger: {:?}\n\
         Status report JSON: {report_json}\n\
         Codebase status JSON: {status_json}",
        input.trigger
    ))
}

fn repair_prompt(bad_output: &str, error: &str) -> String {
    format!(
        "Repair this Marvis segmenter output into valid JSON only. Error: {error}\n\
         Required top-level shape: {{\"tasks\":[...]}}. If uncertain, output {{\"tasks\":[]}}.\n\n\
         Bad output:\n{bad_output}"
    )
}

async fn request_json(
    client: &Client,
    model: &str,
    max_tokens: u32,
    prompt: String,
) -> Result<String> {
    let response = client
        .chat()
        .completions()
        .create(ChatCompletionRequest {
            model: model.to_string(),
            max_tokens: max_tokens.min(2048),
            messages: vec![
                Message::system("Return strict JSON only."),
                Message::user(prompt),
            ],
            tools: Vec::new(),
            stream: false,
        })
        .await
        .map_err(|err| anyhow!(err.to_string()))?;
    let choice = response
        .choices
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("model returned no choices"))?;
    choice
        .message
        .content
        .ok_or_else(|| anyhow!("model returned no text"))
}

#[derive(Debug, Deserialize)]
struct SegmenterOutput {
    #[serde(default)]
    tasks: Vec<TaskCandidate>,
}

fn parse_segmenter_output(text: &str) -> Result<Vec<TaskCandidate>> {
    let json = extract_json_object(text).ok_or_else(|| anyhow!("no JSON object found"))?;
    let output: SegmenterOutput = serde_json::from_str(json)?;
    Ok(output
        .tasks
        .into_iter()
        .filter_map(TaskCandidate::normalized)
        .take(MAX_CANDIDATES)
        .collect())
}

fn extract_json_object(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    (end >= start).then_some(&text[start..=end])
}

#[derive(Debug, serde::Serialize)]
struct CompactStatus<'a> {
    workspace: &'a status::WorkspaceMeta,
    vscode: CompactVscode<'a>,
    git: &'a status::GitState,
    commands: &'a status::CommandState,
    segments: &'a [status::StatusSegment],
}

#[derive(Debug, serde::Serialize)]
struct CompactVscode<'a> {
    active_editor: &'a Option<status::EditorRef>,
    cursor_context: &'a Option<status::CursorContext>,
    problems: &'a [status::DiagnosticEvent],
    running_tasks: &'a [status::VscodeTaskState],
    debug_sessions: &'a [status::DebugSessionState],
}

fn compact_status(status: &CodebaseStatus) -> CompactStatus<'_> {
    CompactStatus {
        workspace: &status.workspace,
        vscode: CompactVscode {
            active_editor: &status.vscode.active_editor,
            cursor_context: &status.vscode.cursor_context,
            problems: &status.vscode.problems,
            running_tasks: &status.vscode.running_tasks,
            debug_sessions: &status.vscode.debug_sessions,
        },
        git: &status.git,
        commands: &status.commands,
        segments: &status.segments,
    }
}

fn status_fingerprint(status: &CodebaseStatus, report: &StatusReport) -> String {
    serde_json::to_string(&compact_status(status))
        .unwrap_or_else(|err| format!("{}:status-serialization-error:{err}", report.snapshot_hash))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use pave_router::PaveVector;
    use status::{CodebaseStatus, DiagnosticEvent, Position, TextRange};

    #[derive(Clone)]
    struct MockSegmenter {
        tasks: Vec<TaskCandidate>,
        fail: bool,
    }

    #[async_trait]
    impl ProblemSegmenter for MockSegmenter {
        async fn segment(&self, _input: SegmenterInput) -> Result<Vec<TaskCandidate>> {
            if self.fail {
                Err(anyhow!("mock failure"))
            } else {
                Ok(self.tasks.clone())
            }
        }
    }

    #[tokio::test]
    async fn clean_status_returns_idle_without_segmenting() {
        let status = CodebaseStatus::new("/tmp/demo");
        let report = StatusReport {
            snapshot_hash: "clean".to_string(),
            summary: "clean".to_string(),
            active_segments: Vec::new(),
            stuckness: None,
            suggestion: None,
        };
        let mut coordinator = AutonomyCoordinator::new();
        let decision = coordinator
            .handle_tick(
                AutonomyTrigger::Heartbeat,
                report,
                status,
                Vec::new(),
                &MockSegmenter {
                    tasks: Vec::new(),
                    fail: true,
                },
                "gpt-test",
            )
            .await;
        assert!(matches!(decision, AutonomyDecision::Idle { .. }));
    }

    #[tokio::test]
    async fn routes_diagnostic_task_to_default_agent() {
        let mut status = CodebaseStatus::new("/tmp/demo");
        status.vscode.problems.push(DiagnosticEvent {
            id: "d1".to_string(),
            path: PathBuf::from("src/lib.rs"),
            range: Some(TextRange {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 1,
                },
            }),
            severity: DiagnosticSeverity::Error,
            message: "borrow error".to_string(),
            source: Some("rustc".to_string()),
            code: None,
        });
        let report = StatusReport {
            snapshot_hash: "diagnostic".to_string(),
            summary: "diagnostic".to_string(),
            active_segments: Vec::new(),
            stuckness: None,
            suggestion: None,
        };
        let mut coordinator = AutonomyCoordinator::new();
        let decision = coordinator
            .handle_tick(
                AutonomyTrigger::DiagnosticsChanged,
                report,
                status,
                Vec::new(),
                &MockSegmenter {
                    fail: false,
                    tasks: vec![TaskCandidate {
                        id: "fix-diagnostic".to_string(),
                        title: "Fix diagnostic".to_string(),
                        prompt: "Fix the Rust diagnostic.".to_string(),
                        evidence: vec!["rustc diagnostic".to_string()],
                        files: vec![PathBuf::from("src/lib.rs")],
                        risk_level: status::RiskLevel::Medium,
                        needs_write: true,
                        desired_tools: vec!["apply_patch".to_string(), "run_test".to_string()],
                        pave: PaveVector::new([
                            ("rust", 1.0),
                            ("diagnostics", 1.0),
                            ("patch", 1.0),
                        ]),
                        confidence: 0.9,
                    }],
                },
                "gpt-test",
            )
            .await;
        match decision {
            AutonomyDecision::Suggest { suggestion } => {
                assert_eq!(suggestion.route.agent.id, "rust-diagnostic-repair");
                assert!(suggestion.required_approval.allow_workspace_write);
            }
            other => panic!("unexpected decision: {other:?}"),
        }
    }

    #[tokio::test]
    async fn dismissed_suggestion_is_suppressed() {
        let mut status = CodebaseStatus::new("/tmp/demo");
        status.vscode.problems.push(DiagnosticEvent {
            id: "d1".to_string(),
            path: PathBuf::from("src/lib.rs"),
            range: None,
            severity: DiagnosticSeverity::Error,
            message: "error".to_string(),
            source: None,
            code: None,
        });
        let report = StatusReport {
            snapshot_hash: "s1".to_string(),
            summary: "diagnostic".to_string(),
            active_segments: Vec::new(),
            stuckness: None,
            suggestion: None,
        };
        let task = TaskCandidate {
            id: "fix".to_string(),
            title: "Fix".to_string(),
            prompt: "Fix.".to_string(),
            evidence: Vec::new(),
            files: Vec::new(),
            risk_level: status::RiskLevel::Medium,
            needs_write: true,
            desired_tools: vec!["apply_patch".to_string()],
            pave: PaveVector::new([("rust", 1.0), ("patch", 1.0)]),
            confidence: 0.9,
        };
        let segmenter = MockSegmenter {
            tasks: vec![task],
            fail: false,
        };
        let mut coordinator = AutonomyCoordinator::new();
        let first = coordinator
            .handle_tick(
                AutonomyTrigger::Manual,
                report.clone(),
                status.clone(),
                Vec::new(),
                &segmenter,
                "gpt-test",
            )
            .await;
        let suggestion_id = match first {
            AutonomyDecision::Suggest { suggestion } => suggestion.suggestion_id,
            other => panic!("unexpected decision: {other:?}"),
        };
        assert!(coordinator.dismiss(&suggestion_id));
        let second = coordinator
            .handle_tick(
                AutonomyTrigger::Manual,
                report,
                status,
                Vec::new(),
                &segmenter,
                "gpt-test",
            )
            .await;
        assert!(matches!(second, AutonomyDecision::Suppressed { .. }));
    }

    #[test]
    fn invalid_segmenter_json_is_error() {
        let err = parse_segmenter_output("not json").unwrap_err();
        assert!(err.to_string().contains("no JSON object"));
    }
}
