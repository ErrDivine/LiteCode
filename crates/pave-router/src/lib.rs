use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use status::RiskLevel;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PaveVector {
    #[serde(default, flatten)]
    pub dims: BTreeMap<String, f32>,
}

impl PaveVector {
    pub fn new(dims: impl IntoIterator<Item = (impl Into<String>, f32)>) -> Self {
        Self {
            dims: dims
                .into_iter()
                .map(|(key, value)| (key.into(), value))
                .filter(|(key, value)| !key.trim().is_empty() && value.is_finite())
                .collect(),
        }
    }

    pub fn dot(&self, other: &Self) -> f32 {
        let (small, large) = if self.dims.len() <= other.dims.len() {
            (&self.dims, &other.dims)
        } else {
            (&other.dims, &self.dims)
        };
        small
            .iter()
            .filter_map(|(key, left)| large.get(key).map(|right| left * right))
            .sum()
    }

    pub fn norm(&self) -> f32 {
        self.dims
            .values()
            .map(|value| value * value)
            .sum::<f32>()
            .sqrt()
    }

    pub fn cosine(&self, other: &Self) -> f32 {
        let left = self.norm();
        let right = other.norm();
        if left <= f32::EPSILON || right <= f32::EPSILON {
            return 0.0;
        }
        (self.dot(other) / (left * right)).clamp(-1.0, 1.0)
    }

    pub fn is_zero(&self) -> bool {
        self.norm() <= f32::EPSILON
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolAccess {
    #[serde(default)]
    pub allow_workspace_write: bool,
    #[serde(default)]
    pub allow_shell: bool,
    #[serde(default)]
    pub allow_risky_shell: bool,
    #[serde(default)]
    pub allow_git_write: bool,
    #[serde(default)]
    pub allow_network: bool,
}

impl ToolAccess {
    pub fn read_only() -> Self {
        Self::default()
    }

    pub fn patch_and_checks() -> Self {
        Self {
            allow_workspace_write: true,
            allow_shell: true,
            allow_risky_shell: false,
            allow_git_write: false,
            allow_network: false,
        }
    }

    pub fn can_support_task(&self, task: &TaskCandidate) -> bool {
        !task.needs_write || self.allow_workspace_write
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentProfile {
    pub id: String,
    pub label: String,
    pub model: String,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub mcp_servers: Vec<String>,
    #[serde(default)]
    pub skill_prompt: String,
    #[serde(default)]
    pub tool_allowlist: Vec<String>,
    #[serde(default)]
    pub pave: PaveVector,
    #[serde(default)]
    pub default_approval: ToolAccess,
}

impl AgentProfile {
    pub fn normalized(mut self) -> Option<Self> {
        self.id = self.id.trim().to_string();
        self.label = self.label.trim().to_string();
        self.model = self.model.trim().to_string();
        self.skill_prompt = self.skill_prompt.trim().to_string();
        self.skills = normalize_string_list(self.skills);
        self.mcp_servers = normalize_string_list(self.mcp_servers);
        self.tool_allowlist = normalize_string_list(self.tool_allowlist);
        if self.id.is_empty() || self.label.is_empty() || self.model.is_empty() {
            return None;
        }
        if self.skill_prompt.is_empty() && self.skills.is_empty() {
            return None;
        }
        Some(self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskCandidate {
    pub id: String,
    pub title: String,
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub evidence: Vec<String>,
    #[serde(default)]
    pub files: Vec<PathBuf>,
    pub risk_level: RiskLevel,
    #[serde(default)]
    pub needs_write: bool,
    #[serde(default)]
    pub desired_tools: Vec<String>,
    #[serde(default)]
    pub pave: PaveVector,
}

impl TaskCandidate {
    pub fn normalized(mut self) -> Option<Self> {
        self.id = self.id.trim().to_string();
        self.title = self.title.trim().to_string();
        self.prompt = self.prompt.trim().to_string();
        self.agent_id = self
            .agent_id
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        self.evidence = normalize_string_list(self.evidence);
        self.desired_tools = normalize_string_list(self.desired_tools);
        if self.id.is_empty() || self.title.is_empty() || self.prompt.is_empty() {
            return None;
        }
        Some(self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RouteDecision {
    pub suggestion_id: String,
    pub task: TaskCandidate,
    pub agent: AgentProfile,
    #[serde(skip)]
    pub cosine_score: f32,
    #[serde(skip)]
    pub final_score: f32,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct RouterConfig {
    pub minimum_route_score: f32,
    pub tool_match_bonus: f32,
    pub unavailable_tool_penalty: f32,
    pub risk_mismatch_penalty: f32,
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            minimum_route_score: 0.0,
            tool_match_bonus: 0.08,
            unavailable_tool_penalty: 0.16,
            risk_mismatch_penalty: 0.12,
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum RouterError {
    #[error("no valid agent profiles configured")]
    NoProfiles,
}

#[derive(Debug, Clone)]
pub struct Router {
    profiles: Vec<AgentProfile>,
    config: RouterConfig,
}

impl Router {
    pub fn new(profiles: Vec<AgentProfile>, config: RouterConfig) -> Result<Self, RouterError> {
        let profiles = normalize_profiles(profiles);
        if profiles.is_empty() {
            return Err(RouterError::NoProfiles);
        }
        Ok(Self { profiles, config })
    }

    pub fn profiles(&self) -> &[AgentProfile] {
        &self.profiles
    }

    pub fn select(&self, tasks: Vec<TaskCandidate>) -> Option<RouteDecision> {
        let mut best: Option<RouteDecision> = None;
        for task in tasks.into_iter().filter_map(TaskCandidate::normalized) {
            if let Some(agent_id) = task.agent_id.as_deref() {
                if let Some(profile) = self.profiles.iter().find(|profile| profile.id == agent_id)
                    && let Some(decision) = self.score_with_reason(
                        &task,
                        profile,
                        format!("selected by LLM agent choice `{agent_id}`"),
                    )
                {
                    let is_better = best
                        .as_ref()
                        .is_none_or(|current| decision.final_score > current.final_score);
                    if is_better {
                        best = Some(decision);
                    }
                }
                continue;
            }

            for profile in &self.profiles {
                if let Some(decision) = self.score(&task, profile) {
                    let is_better = best
                        .as_ref()
                        .is_none_or(|current| decision.final_score > current.final_score);
                    if is_better {
                        best = Some(decision);
                    }
                }
            }
        }
        best
    }

    fn score(&self, task: &TaskCandidate, profile: &AgentProfile) -> Option<RouteDecision> {
        self.score_with_reason(
            task,
            profile,
            "matched by PAVE routing with declared tool and risk constraints".to_string(),
        )
    }

    fn score_with_reason(
        &self,
        task: &TaskCandidate,
        profile: &AgentProfile,
        reason: String,
    ) -> Option<RouteDecision> {
        if !profile.default_approval.can_support_task(task) {
            return None;
        }
        let cosine_score = task.pave.cosine(&profile.pave).max(0.0);
        let desired = task
            .desired_tools
            .iter()
            .map(|tool| tool.as_str())
            .collect::<BTreeSet<_>>();
        let allowed = profile
            .tool_allowlist
            .iter()
            .map(|tool| tool.as_str())
            .collect::<BTreeSet<_>>();
        let tool_match_bonus = if !desired.is_empty() && desired.is_subset(&allowed) {
            self.config.tool_match_bonus
        } else {
            0.0
        };
        let unavailable_tool_penalty = if desired.iter().any(|tool| !allowed.contains(tool)) {
            self.config.unavailable_tool_penalty
        } else {
            0.0
        };
        let risk_mismatch_penalty =
            if matches!(task.risk_level, RiskLevel::High | RiskLevel::Critical)
                && !profile.default_approval.allow_risky_shell
            {
                self.config.risk_mismatch_penalty
            } else {
                0.0
            };
        let final_score =
            cosine_score + tool_match_bonus - unavailable_tool_penalty - risk_mismatch_penalty;
        if final_score < self.config.minimum_route_score {
            return None;
        }
        Some(RouteDecision {
            suggestion_id: suggestion_id(task, profile),
            task: task.clone(),
            agent: profile.clone(),
            cosine_score,
            final_score,
            reason,
        })
    }
}

pub fn normalize_profiles(profiles: Vec<AgentProfile>) -> Vec<AgentProfile> {
    let mut seen = BTreeSet::new();
    profiles
        .into_iter()
        .filter_map(AgentProfile::normalized)
        .filter(|profile| seen.insert(profile.id.clone()))
        .collect()
}

#[cfg(test)]
fn test_agent_profiles(default_model: &str) -> Vec<AgentProfile> {
    vec![
        AgentProfile {
            id: "rust-diagnostic-repair".to_string(),
            label: "Rust Diagnostic Repair".to_string(),
            model: default_model.to_string(),
            skills: vec!["rust-diagnostic-repair".to_string()],
            mcp_servers: Vec::new(),
            skill_prompt: "You are a Rust repair agent. Focus on compiler diagnostics, failing tests, and small verified patches.".to_string(),
            tool_allowlist: vec![
                "read_file".to_string(),
                "read_many_files".to_string(),
                "search_files".to_string(),
                "find_files".to_string(),
                "list_directory".to_string(),
                "list_symbols".to_string(),
                "apply_patch".to_string(),
                "run_test".to_string(),
                "run_build".to_string(),
                "run_formatter".to_string(),
                "git_diff".to_string(),
                "git_status".to_string(),
                "list_skills".to_string(),
                "list_skill_resources".to_string(),
                "read_skill_resource".to_string(),
            ],
            pave: PaveVector::new([
                ("rust", 1.0),
                ("diagnostics", 0.95),
                ("tests", 0.85),
                ("patch", 0.9),
                ("risk_medium", 0.5),
            ]),
            default_approval: ToolAccess::patch_and_checks(),
        },
        AgentProfile {
            id: "test-failure-triage".to_string(),
            label: "Test Failure Triage".to_string(),
            model: default_model.to_string(),
            skills: vec!["test-failure-triage".to_string()],
            mcp_servers: Vec::new(),
            skill_prompt: "You are a test failure triage agent. Explain the failing behavior, inspect focused code, and suggest the smallest verification path.".to_string(),
            tool_allowlist: vec![
                "read_file".to_string(),
                "read_many_files".to_string(),
                "search_files".to_string(),
                "find_files".to_string(),
                "list_directory".to_string(),
                "list_symbols".to_string(),
                "run_test".to_string(),
                "git_diff".to_string(),
                "git_status".to_string(),
                "list_skills".to_string(),
                "list_skill_resources".to_string(),
                "read_skill_resource".to_string(),
            ],
            pave: PaveVector::new([
                ("tests", 1.0),
                ("rust", 0.7),
                ("diagnostics", 0.45),
                ("explanation", 0.65),
                ("shell", 0.5),
            ]),
            default_approval: ToolAccess {
                allow_workspace_write: false,
                allow_shell: true,
                allow_risky_shell: false,
                allow_git_write: false,
                allow_network: false,
            },
        },
        AgentProfile {
            id: "repo-explainer".to_string(),
            label: "Repo Explainer".to_string(),
            model: default_model.to_string(),
            skills: vec!["repo-explainer".to_string()],
            mcp_servers: Vec::new(),
            skill_prompt: "You are a repo explanation agent. Read only, summarize relationships, and avoid proposing edits unless the user asks.".to_string(),
            tool_allowlist: vec![
                "read_file".to_string(),
                "read_many_files".to_string(),
                "search_files".to_string(),
                "find_files".to_string(),
                "list_directory".to_string(),
                "list_symbols".to_string(),
                "git_diff".to_string(),
                "git_status".to_string(),
                "list_skills".to_string(),
                "list_skill_resources".to_string(),
                "read_skill_resource".to_string(),
            ],
            pave: PaveVector::new([
                ("explanation", 1.0),
                ("docs", 0.65),
                ("rust", 0.35),
                ("risk_low", 0.6),
            ]),
            default_approval: ToolAccess::read_only(),
        },
    ]
}

fn normalize_string_list(values: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .filter(|value| seen.insert(value.clone()))
        .collect()
}

fn suggestion_id(task: &TaskCandidate, profile: &AgentProfile) -> String {
    format!("{}:{}", task.id, profile.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_uses_shared_dimensions() {
        let left = PaveVector::new([("rust", 1.0), ("tests", 1.0)]);
        let right = PaveVector::new([("rust", 1.0), ("docs", 1.0)]);
        let score = left.cosine(&right);
        assert!((score - 0.5).abs() < 0.0001);
    }

    #[test]
    fn cosine_zero_vector_is_zero() {
        let left = PaveVector::default();
        let right = PaveVector::new([("rust", 1.0)]);
        assert_eq!(left.cosine(&right), 0.0);
    }

    #[test]
    fn router_selects_best_matching_profile() {
        let profiles = test_agent_profiles("gpt-test");
        let router = Router::new(profiles, RouterConfig::default()).unwrap();
        let decision = router
            .select(vec![TaskCandidate {
                id: "task-1".to_string(),
                title: "Fix Rust diagnostic".to_string(),
                prompt: "Fix the compiler error.".to_string(),
                agent_id: None,
                evidence: vec!["diagnostic".to_string()],
                files: vec![PathBuf::from("src/lib.rs")],
                risk_level: RiskLevel::Medium,
                needs_write: true,
                desired_tools: vec!["apply_patch".to_string(), "run_test".to_string()],
                pave: PaveVector::new([("rust", 1.0), ("diagnostics", 1.0), ("patch", 1.0)]),
            }])
            .unwrap();
        assert_eq!(decision.agent.id, "rust-diagnostic-repair");
    }

    #[test]
    fn router_routes_llm_tasks_without_extra_task_gate() {
        let router = Router::new(test_agent_profiles("gpt-test"), RouterConfig::default()).unwrap();
        let decision = router.select(vec![TaskCandidate {
            id: "task-1".to_string(),
            title: "Maybe".to_string(),
            prompt: "Maybe do something.".to_string(),
            agent_id: None,
            evidence: Vec::new(),
            files: Vec::new(),
            risk_level: RiskLevel::Low,
            needs_write: false,
            desired_tools: Vec::new(),
            pave: PaveVector::new([("explanation", 1.0)]),
        }]);
        assert!(decision.is_some());
    }

    #[test]
    fn router_prefers_llm_selected_agent_when_compatible() {
        let router = Router::new(test_agent_profiles("gpt-test"), RouterConfig::default()).unwrap();
        let decision = router
            .select(vec![TaskCandidate {
                id: "task-1".to_string(),
                title: "Explain repo".to_string(),
                prompt: "Explain the focused area.".to_string(),
                agent_id: Some("repo-explainer".to_string()),
                evidence: Vec::new(),
                files: Vec::new(),
                risk_level: RiskLevel::Low,
                needs_write: false,
                desired_tools: Vec::new(),
                pave: PaveVector::new([("rust", 1.0), ("patch", 1.0)]),
            }])
            .unwrap();
        assert_eq!(decision.agent.id, "repo-explainer");
        assert!(decision.reason.contains("selected by LLM agent choice"));
    }
}
