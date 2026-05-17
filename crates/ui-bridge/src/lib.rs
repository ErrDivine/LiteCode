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
        max_tokens: Option<u32>,
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
    },
    Shutdown,
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
        using_synthetic_model: bool,
        report: StatusReport,
    },
    StatusReport {
        report: StatusReport,
    },
    AgentEvent {
        event: VscodeRuntimeEvent,
    },
    Complete {
        report: StatusReport,
    },
    Error {
        message: String,
    },
    ShutdownComplete,
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
    use protocol::{AgentMessageDeltaEvent, EventMsg};

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
}
