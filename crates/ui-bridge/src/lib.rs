use pave_router::{AgentProfile, RouteDecision};
use protocol::protocol::{Event, EventMsg};
use protocol::{Op, UserInput};
use serde::{Deserialize, Serialize};
use status::{CommandResult, StatusReport, VscodeStatus};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VscodeRequestEnvelope {
    pub id: u64,
    #[serde(flatten)]
    pub request: VscodeRequest,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum VscodeRequest {
    Initialize {
        workspace_root: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        api_key: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        base_url: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        thinking_mode: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reasoning_effort: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_tokens: Option<u32>,
        #[serde(default)]
        agent_profiles: Vec<AgentProfile>,
    },
    StatusUpdate {
        status: VscodeStatus,
    },
    CommandResult {
        result: CommandResult,
    },
    UserPrompt {
        prompt: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        status: Option<VscodeStatus>,
        #[serde(default)]
        approval: PromptApproval,
    },
    AutonomyTick {
        status: VscodeStatus,
        trigger: AutonomyTrigger,
        #[serde(default)]
        agent_profiles: Vec<AgentProfile>,
    },
    RunSuggestedTask {
        suggestion_id: String,
        #[serde(default)]
        approval: PromptApproval,
    },
    DismissSuggestion {
        suggestion_id: String,
    },
    Shutdown,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct PromptApproval {
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

#[derive(Debug, Clone, Serialize)]
pub struct VscodeResponseEnvelope {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
    #[serde(flatten)]
    pub response: VscodeResponse,
}

impl VscodeResponseEnvelope {
    pub fn for_request(id: u64, response: VscodeResponse) -> Self {
        Self {
            id: Some(id),
            response,
        }
    }

    pub fn notification(response: VscodeResponse) -> Self {
        Self { id: None, response }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum VscodeResponse {
    Ready {
        workspace_root: String,
        model: String,
        base_url: String,
        report: StatusReport,
    },
    StatusReport {
        report: StatusReport,
    },
    AgentEvent {
        event: VscodeRuntimeEvent,
    },
    ProcessUpdate {
        process: ProcessSnapshot,
    },
    AutonomyDecision {
        decision: Box<AutonomyDecision>,
    },
    Complete {
        report: StatusReport,
    },
    Error {
        message: String,
    },
    ShutdownComplete,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomyTrigger {
    Idle,
    Heartbeat,
    StatusChange,
    DiagnosticsChanged,
    FileSaved,
    CommandResult,
    TaskEnded,
    DebugTerminated,
    Manual,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AutonomyDecision {
    Idle {
        snapshot_hash: String,
        reason: String,
    },
    Suggest {
        suggestion: AutonomySuggestion,
    },
    Suppressed {
        snapshot_hash: String,
        suggestion_id: String,
        reason: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AutonomySuggestion {
    pub suggestion_id: String,
    pub snapshot_hash: String,
    pub created_at_ms: u64,
    pub route: RouteDecision,
    pub required_approval: PromptApproval,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum VscodeRuntimeEvent {
    Delta { text: String },
    AgentMessage { text: String },
    ToolStart { name: String, arguments: String },
    ToolEnd { name: String, output: String },
    TurnStarted { turn_id: String },
    TurnComplete { turn_id: String, summary: String },
    Error { message: String },
    Ignore,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProcessSnapshot {
    pub process_id: String,
    pub state: String,
    pub prompt_preview: String,
    pub model: String,
    pub started_at_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at_ms: Option<u64>,
    pub tool_calls_used: u32,
    pub max_tool_calls: u32,
    pub allow_workspace_write: bool,
    pub allow_risky_shell: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct WebEvent {
    pub event: &'static str,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliEvent {
    Print(String),
    ToolStart { name: String, arguments: String },
    ToolEnd { name: String, output: String },
    Error(String),
    Done,
    Ignore,
}

pub fn user_text_op(text: impl Into<String>) -> Op {
    Op::UserInput {
        items: vec![UserInput::text(text)],
        final_output_json_schema: None,
        responsesapi_client_metadata: None,
    }
}

pub fn event_to_web(event: &Event) -> Option<WebEvent> {
    match &event.msg {
        EventMsg::AgentMessageDelta(delta) => Some(WebEvent {
            event: "delta",
            data: serde_json::json!({
                "content": delta.delta,
                "finish_reason": null,
            }),
        }),
        EventMsg::ToolCallBegin(begin) => Some(WebEvent {
            event: "tool_start",
            data: serde_json::json!({
                "name": begin.name,
                "arguments": begin.arguments,
            }),
        }),
        EventMsg::ToolCallEnd(end) => Some(WebEvent {
            event: "tool_end",
            data: serde_json::json!({
                "name": end.name,
                "output": end.output,
            }),
        }),
        EventMsg::Error(error) => Some(WebEvent {
            event: "error",
            data: serde_json::json!({ "error": error.message }),
        }),
        EventMsg::TurnComplete(_) => Some(WebEvent {
            event: "done",
            data: serde_json::json!({ "done": true }),
        }),
        EventMsg::TurnAborted(aborted) => Some(WebEvent {
            event: "error",
            data: serde_json::json!({ "error": aborted.reason }),
        }),
        _ => None,
    }
}

pub fn event_to_cli(event: &Event) -> CliEvent {
    match &event.msg {
        EventMsg::AgentMessageDelta(delta) => CliEvent::Print(delta.delta.clone()),
        EventMsg::ToolCallBegin(begin) => CliEvent::ToolStart {
            name: begin.name.clone(),
            arguments: begin.arguments.clone(),
        },
        EventMsg::ToolCallEnd(end) => CliEvent::ToolEnd {
            name: end.name.clone(),
            output: end.output.clone(),
        },
        EventMsg::Error(error) => CliEvent::Error(error.message.clone()),
        EventMsg::TurnAborted(aborted) => CliEvent::Error(aborted.reason.clone()),
        EventMsg::TurnComplete(_) => CliEvent::Done,
        _ => CliEvent::Ignore,
    }
}

pub fn event_to_vscode(event: &Event) -> Option<VscodeRuntimeEvent> {
    match &event.msg {
        EventMsg::AgentMessageDelta(delta) => Some(VscodeRuntimeEvent::Delta {
            text: delta.delta.clone(),
        }),
        EventMsg::AgentMessage(message) => Some(VscodeRuntimeEvent::AgentMessage {
            text: message.message.clone(),
        }),
        EventMsg::ToolCallBegin(begin) => Some(VscodeRuntimeEvent::ToolStart {
            name: begin.name.clone(),
            arguments: begin.arguments.clone(),
        }),
        EventMsg::ToolCallEnd(end) => Some(VscodeRuntimeEvent::ToolEnd {
            name: end.name.clone(),
            output: end.output.clone(),
        }),
        EventMsg::TurnStarted(started) => Some(VscodeRuntimeEvent::TurnStarted {
            turn_id: started.turn_id.clone(),
        }),
        EventMsg::TurnComplete(complete) => Some(VscodeRuntimeEvent::TurnComplete {
            turn_id: complete.turn_id.clone(),
            summary: complete.last_agent_message.clone().unwrap_or_default(),
        }),
        EventMsg::Error(error) => Some(VscodeRuntimeEvent::Error {
            message: error.message.clone(),
        }),
        EventMsg::TurnAborted(aborted) => Some(VscodeRuntimeEvent::Error {
            message: aborted.reason.clone(),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use pave_router::{PaveVector, ToolAccess};
    use protocol::{AgentMessageDeltaEvent, EventMsg};
    use status::RiskLevel;

    use super::*;

    #[test]
    fn maps_kernel_delta_to_vscode_event() {
        let event = Event {
            id: "sub-1".to_string(),
            msg: EventMsg::AgentMessageDelta(AgentMessageDeltaEvent {
                delta: "hello".to_string(),
            }),
        };

        assert_eq!(
            event_to_vscode(&event),
            Some(VscodeRuntimeEvent::Delta {
                text: "hello".to_string()
            })
        );
    }

    #[test]
    fn parses_initialize_request_with_api_key() {
        let raw = r#"{
            "id": 1,
            "type": "initialize",
            "workspace_root": "/workspace",
            "model": "gpt-test",
            "api_key": "sk-test",
            "base_url": "https://example.test/v1",
            "thinking_mode": "enabled",
            "reasoning_effort": "high",
            "max_tokens": 2048
        }"#;
        let parsed = serde_json::from_str::<VscodeRequestEnvelope>(raw).unwrap();
        assert_eq!(parsed.id, 1);
        match parsed.request {
            VscodeRequest::Initialize {
                workspace_root,
                model,
                api_key,
                base_url,
                thinking_mode,
                reasoning_effort,
                max_tokens,
                ..
            } => {
                assert_eq!(workspace_root, "/workspace");
                assert_eq!(model.as_deref(), Some("gpt-test"));
                assert_eq!(api_key.as_deref(), Some("sk-test"));
                assert_eq!(base_url.as_deref(), Some("https://example.test/v1"));
                assert_eq!(thinking_mode.as_deref(), Some("enabled"));
                assert_eq!(reasoning_effort.as_deref(), Some("high"));
                assert_eq!(max_tokens, Some(2048));
            }
            other => panic!("unexpected request: {other:?}"),
        }
    }

    #[test]
    fn parses_autonomy_tick_request() {
        let raw = r#"{
            "id": 7,
            "type": "autonomy_tick",
            "trigger": "heartbeat",
            "status": {},
            "agent_profiles": [{
                "id": "rust",
                "label": "Rust",
                "model": "gpt-test",
                "skills": ["rust-diagnostic-repair"],
                "skill_prompt": "Fix Rust code.",
                "tool_allowlist": ["read_file"],
                "pave": {"rust": 1.0},
                "default_approval": {"allow_workspace_write": false}
            }]
        }"#;
        let parsed = serde_json::from_str::<VscodeRequestEnvelope>(raw).unwrap();
        assert_eq!(parsed.id, 7);
        match parsed.request {
            VscodeRequest::AutonomyTick {
                trigger,
                agent_profiles,
                ..
            } => {
                assert_eq!(trigger, AutonomyTrigger::Heartbeat);
                assert_eq!(agent_profiles[0].id, "rust");
            }
            other => panic!("unexpected request: {other:?}"),
        }
    }

    #[test]
    fn serializes_autonomy_suggestion_response() {
        let route = RouteDecision {
            suggestion_id: "task:agent".to_string(),
            task: pave_router::TaskCandidate {
                id: "task".to_string(),
                title: "Fix diagnostic".to_string(),
                prompt: "Fix it.".to_string(),
                agent_id: Some("agent".to_string()),
                evidence: vec!["diagnostic".to_string()],
                files: Vec::new(),
                risk_level: RiskLevel::Medium,
                needs_write: true,
                desired_tools: vec!["apply_patch".to_string()],
                pave: PaveVector::new([("rust", 1.0)]),
            },
            agent: pave_router::AgentProfile {
                id: "agent".to_string(),
                label: "Agent".to_string(),
                model: "gpt-test".to_string(),
                skills: vec!["rust-diagnostic-repair".to_string()],
                mcp_servers: Vec::new(),
                skill_prompt: "Patch carefully.".to_string(),
                tool_allowlist: vec!["apply_patch".to_string()],
                pave: PaveVector::new([("rust", 1.0)]),
                default_approval: ToolAccess::patch_and_checks(),
            },
            cosine_score: 1.0,
            final_score: 1.0,
            reason: "test".to_string(),
        };
        let response = VscodeResponseEnvelope::for_request(
            9,
            VscodeResponse::AutonomyDecision {
                decision: Box::new(AutonomyDecision::Suggest {
                    suggestion: AutonomySuggestion {
                        suggestion_id: "task:agent".to_string(),
                        snapshot_hash: "hash".to_string(),
                        created_at_ms: 1,
                        route,
                        required_approval: PromptApproval {
                            allow_workspace_write: true,
                            allow_shell: true,
                            allow_risky_shell: false,
                            allow_git_write: false,
                            allow_network: false,
                        },
                    },
                }),
            },
        );
        let value = serde_json::to_value(response).unwrap();
        assert_eq!(value["type"], "autonomy_decision");
        assert_eq!(value["decision"]["type"], "suggest");
        assert_eq!(
            value["decision"]["suggestion"]["route"]["agent"]["id"],
            "agent"
        );
    }
}
