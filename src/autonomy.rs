use std::collections::BTreeMap;

use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use openai_rs::{ChatCompletionRequest, Client, Message};
use pave_router::{
    AgentProfile, Router, RouterConfig, TaskCandidate, ToolAccess, normalize_profiles,
};
use scheduler::OpenAiCompatibleConfig;
use serde::Deserialize;
use status::{CodebaseStatus, CommandResult, DiagnosticSeverity, StatusReport, now_timestamp_ms};
use ui_bridge::{AutonomyDecision, AutonomySuggestion, AutonomyTrigger, PromptApproval};

const SUGGESTION_COOLDOWN_MS: u64 = 120_000;
const MAX_CANDIDATES: usize = 3;

#[derive(Debug, Clone)]
pub struct SegmenterInput {
    pub trigger: AutonomyTrigger,
    pub report: StatusReport,
    pub status: CodebaseStatus,
    pub agent_profiles: Vec<AgentProfile>,
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
        let first = request_json(
            &client,
            &self.model,
            self.max_tokens,
            self.config
                .request_options
                .for_chat_request(&self.config.base_url),
            prompt,
        )
        .await?;
        match parse_segmenter_output(&first) {
            Ok(tasks) => Ok(tasks),
            Err(first_err) => {
                let repair_prompt = repair_prompt(&first, &first_err.to_string());
                let repaired = request_json(
                    &client,
                    &self.model,
                    self.max_tokens,
                    self.config
                        .request_options
                        .for_chat_request(&self.config.base_url),
                    repair_prompt,
                )
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

    pub async fn handle_tick<S>(
        &mut self,
        trigger: AutonomyTrigger,
        report: StatusReport,
        status: CodebaseStatus,
        profiles: Vec<AgentProfile>,
        segmenter: &S,
        _default_model: &str,
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

        let profiles = normalize_profiles(profiles);
        let router = match Router::new(profiles, RouterConfig::default()) {
            Ok(router) => router,
            Err(err) => return idle(snapshot_hash, err.to_string()),
        };

        let input = SegmenterInput {
            trigger,
            report: report.clone(),
            status,
            agent_profiles: router.profiles().to_vec(),
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
            return idle(snapshot_hash, "no compatible routed task");
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

    if workspace_is_busy(status) {
        return false;
    }

    let active_editor = status.vscode.active_editor.as_ref();
    let has_cursor_or_selection =
        status.vscode.cursor_context.is_some() || !status.vscode.selections.is_empty();
    let has_active_focus = active_editor.is_some() && has_cursor_or_selection;

    if report.stuckness.is_some()
        && (has_active_focus || recent_command_failure(status) || active_error_count(status) > 0)
    {
        return true;
    }

    if active_editor.is_some_and(|editor| active_error_count_for_path(status, &editor.path) > 0)
        && matches!(
            trigger,
            AutonomyTrigger::DiagnosticsChanged
                | AutonomyTrigger::FileSaved
                | AutonomyTrigger::StatusChange
                | AutonomyTrigger::Idle
        )
    {
        return true;
    }

    if recent_command_failure(status)
        && matches!(
            trigger,
            AutonomyTrigger::CommandResult
                | AutonomyTrigger::TaskEnded
                | AutonomyTrigger::DebugTerminated
                | AutonomyTrigger::Idle
                | AutonomyTrigger::Heartbeat
        )
    {
        return true;
    }

    false
}

fn workspace_is_busy(status: &CodebaseStatus) -> bool {
    status
        .vscode
        .running_tasks
        .iter()
        .any(|task| task.is_running)
        || status.vscode.debug_sessions.iter().any(|session| {
            session
                .state
                .as_deref()
                .is_some_and(|state| matches!(state, "running" | "starting" | "active"))
        })
}

fn recent_command_failure(status: &CodebaseStatus) -> bool {
    status
        .commands
        .recent_results
        .iter()
        .rev()
        .take(2)
        .any(CommandResult::failed)
}

fn active_error_count(status: &CodebaseStatus) -> usize {
    status
        .vscode
        .problems
        .iter()
        .filter(|diagnostic| matches!(diagnostic.severity, DiagnosticSeverity::Error))
        .count()
}

fn active_error_count_for_path(status: &CodebaseStatus, path: &std::path::Path) -> usize {
    status
        .vscode
        .problems
        .iter()
        .filter(|diagnostic| {
            matches!(diagnostic.severity, DiagnosticSeverity::Error)
                && diagnostic.path.as_path() == path
        })
        .count()
}

fn segmentation_prompt(input: &SegmenterInput) -> Result<String> {
    let report_json = serde_json::to_string(&input.report)?;
    let status_json = serde_json::to_string(&compact_status(&input.status))?;
    let agents_json = serde_json::to_string(&agent_profiles_for_prompt(&input.agent_profiles))?;
    Ok(format!(
        "You are Marvis autonomous problem, user-intent, and agent selector. Output JSON only, no markdown.\n\
         Be proactive: create a suggestion whenever VSCode/codebase status gives a plausible, useful next step that an agent can help with after user approval.\n\
         Do not wait for a broken build or perfect certainty. Active editor, cursor context, recent saves, focused diagnostics, recent command failures, selections, changed files, and status segments are all evidence of user intent.\n\
         Infer the user's likely workflow boldly but stay grounded in concrete evidence. Useful suggestions include fixing a focused diagnostic, explaining a failed command, verifying recent edits, adding code at the cursor-indicated location, updating references/imports after a move, adding or adjusting a nearby test, or making a small reversible patch implied by the current context.\n\
         Prefer one narrow, immediately actionable task over silence when the evidence points to a helpful next move. The user will be asked for permission before execution, so low/medium-risk suggestions may be offered when they are specific and reversible.\n\
         Return {{\"tasks\":[]}} only when no concrete helpful next step can be inferred, when only broad git dirtiness/risk is visible, when tools/tasks/debug sessions are still running, or when assistance would be speculative.\n\
         Produce at most {MAX_CANDIDATES} tasks. Each task must have:\n\
         id, title, prompt, agent_id, evidence, files, risk_level, needs_write, desired_tools, pave.\n\
         agent_id must be one id from Available agents. Select the agent before asking the user for permission.\n\
         risk_level must be one of low, medium, high, critical.\n\
         pave is a JSON object with numeric dimensions from: rust, javascript, tests, diagnostics, refactor, explanation, patch, shell, git, docs, frontend, infra, risk_low, risk_medium, risk_high.\n\
         A valid task needs concrete evidence from the status and a specific next action. Task title and prompt must state the inferred user intention and why this agent should handle it now.\n\
         Avoid noisy broad suggestions, but bias toward action when the next step is focused and useful.\n\n\
         Trigger: {:?}\n\
         Available agents JSON: {agents_json}\n\
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
    request_options: scheduler::ChatRequestOptions,
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
            thinking: request_options.thinking,
            reasoning_effort: request_options.reasoning_effort,
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
struct AgentProfileForPrompt<'a> {
    id: &'a str,
    label: &'a str,
    skills: &'a [String],
    mcp_servers: &'a [String],
    tool_allowlist: &'a [String],
    approval: ToolAccess,
    instruction: &'a str,
}

fn agent_profiles_for_prompt(profiles: &[AgentProfile]) -> Vec<AgentProfileForPrompt<'_>> {
    profiles
        .iter()
        .map(|profile| AgentProfileForPrompt {
            id: &profile.id,
            label: &profile.label,
            skills: &profile.skills,
            mcp_servers: &profile.mcp_servers,
            tool_allowlist: &profile.tool_allowlist,
            approval: profile.default_approval,
            instruction: &profile.skill_prompt,
        })
        .collect()
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
    use pave_router::{AgentProfile, PaveVector, ToolAccess};
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

    fn test_agent_profiles() -> Vec<AgentProfile> {
        vec![AgentProfile {
            id: "rust-diagnostic-repair".to_string(),
            label: "Rust Diagnostic Repair".to_string(),
            model: "gpt-test".to_string(),
            skills: vec!["rust-diagnostic-repair".to_string()],
            mcp_servers: Vec::new(),
            skill_prompt: "Repair Rust diagnostics.".to_string(),
            tool_allowlist: vec![
                "read_file".to_string(),
                "apply_patch".to_string(),
                "run_test".to_string(),
            ],
            pave: PaveVector::new([("rust", 1.0), ("diagnostics", 1.0), ("patch", 1.0)]),
            default_approval: ToolAccess::patch_and_checks(),
        }]
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
                test_agent_profiles(),
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
        status.vscode.active_editor = Some(status::EditorRef {
            path: PathBuf::from("src/lib.rs"),
            language_id: Some("rust".to_string()),
            is_dirty: false,
        });
        status.vscode.cursor_context = Some(status::CursorContext {
            path: PathBuf::from("src/lib.rs"),
            line: 0,
            character: 0,
            symbol_hint: None,
            text_before: String::new(),
            text_after: String::new(),
            surrounding_text: String::new(),
        });
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
                test_agent_profiles(),
                &MockSegmenter {
                    fail: false,
                    tasks: vec![TaskCandidate {
                        id: "fix-diagnostic".to_string(),
                        title: "Fix diagnostic".to_string(),
                        prompt: "Fix the Rust diagnostic.".to_string(),
                        agent_id: Some("rust-diagnostic-repair".to_string()),
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
    async fn dirty_tree_without_clear_intent_stays_idle() {
        let mut status = CodebaseStatus::new("/tmp/demo");
        status.git.dirty_files = (0..20)
            .map(|index| PathBuf::from(format!("file-{index}.rs")))
            .collect();
        let report = StatusReport {
            snapshot_hash: "dirty".to_string(),
            summary: "dirty tree".to_string(),
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
                test_agent_profiles(),
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
            agent_id: Some("rust-diagnostic-repair".to_string()),
            evidence: Vec::new(),
            files: Vec::new(),
            risk_level: status::RiskLevel::Medium,
            needs_write: true,
            desired_tools: vec!["apply_patch".to_string()],
            pave: PaveVector::new([("rust", 1.0), ("patch", 1.0)]),
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
                test_agent_profiles(),
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
                test_agent_profiles(),
                &segmenter,
                "gpt-test",
            )
            .await;
        assert!(matches!(second, AutonomyDecision::Suppressed { .. }));
    }

    #[test]
    fn segmentation_prompt_biases_toward_specific_action() {
        let prompt = segmentation_prompt(&SegmenterInput {
            trigger: AutonomyTrigger::Idle,
            report: StatusReport {
                snapshot_hash: "prompt".to_string(),
                summary: "prompt".to_string(),
                active_segments: Vec::new(),
                stuckness: None,
                suggestion: None,
            },
            status: CodebaseStatus::new("/tmp/demo"),
            agent_profiles: test_agent_profiles(),
        })
        .unwrap();

        assert!(prompt.contains("Be proactive"));
        assert!(prompt.contains("Do not wait for a broken build or perfect certainty"));
        assert!(prompt.contains("bias toward action"));
        assert!(!prompt.contains("Return {\"tasks\":[]} when intent is clearly inferred"));
    }

    #[test]
    fn invalid_segmenter_json_is_error() {
        let err = parse_segmenter_output("not json").unwrap_err();
        assert!(err.to_string().contains("no JSON object"));
    }
}
