use protocol::protocol::{Event, EventMsg};
use protocol::{Op, UserInput};
use serde::Serialize;

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
