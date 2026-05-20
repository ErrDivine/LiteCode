mod thread_id;

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub use thread_id::ThreadId;

pub mod protocol {
    pub use crate::{
        AgentMessageDeltaEvent, AgentMessageEvent, AskForApproval, ContentItem, DynamicToolSpec,
        ErrorEvent, Event, EventMsg, ForkedFrom, Op, ResponseItem, RolloutItem, SandboxPolicy,
        SessionConfiguredEvent, SessionMeta, SessionMetaLine, SessionSource, Submission,
        ThreadNameUpdatedEvent, TokenCountEvent, TokenUsage, TokenUsageInfo, ToolCallBeginEvent,
        ToolCallEndEvent, TurnAbortedEvent, TurnCompleteEvent, TurnStartedEvent, UserMessageEvent,
        W3cTraceContext, WarningEvent,
    };
}

pub mod user_input {
    pub use crate::{ByteRange, TextElement, UserInput};
}

pub mod models {
    pub use crate::{ContentItem, MessagePhase, ResponseItem};
}

pub mod dynamic_tools {
    pub use crate::DynamicToolSpec;
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct W3cTraceContext {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub traceparent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tracestate: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Submission {
    pub id: String,
    pub op: Op,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace: Option<W3cTraceContext>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Op {
    Interrupt,
    UserInput {
        items: Vec<UserInput>,
        #[serde(skip_serializing_if = "Option::is_none")]
        final_output_json_schema: Option<serde_json::Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        responsesapi_client_metadata: Option<HashMap<String, String>>,
    },
    UserTurn {
        items: Vec<UserInput>,
        cwd: PathBuf,
        approval_policy: AskForApproval,
        sandbox_policy: SandboxPolicy,
        model: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        final_output_json_schema: Option<serde_json::Value>,
    },
    InjectResponseItems {
        items: Vec<ResponseItem>,
    },
    Synthetic {
        message: String,
    },
}

impl Op {
    pub fn user_text(text: impl Into<String>) -> Self {
        Self::UserInput {
            items: vec![UserInput::text(text)],
            final_output_json_schema: None,
            responsesapi_client_metadata: None,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AskForApproval {
    Never,
    #[default]
    OnRequest,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SandboxPolicy {
    #[default]
    WorkspaceWrite,
    ReadOnly,
    DangerFullAccess,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct DynamicToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UserInput {
    Text {
        text: String,
        #[serde(default)]
        text_elements: Vec<TextElement>,
    },
    Image {
        image_url: String,
    },
    LocalImage {
        path: PathBuf,
    },
    Skill {
        name: String,
        path: PathBuf,
    },
    Mention {
        name: String,
        path: String,
    },
}

impl UserInput {
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text {
            text: text.into(),
            text_elements: Vec::new(),
        }
    }

    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text { text, .. } => Some(text),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct TextElement {
    pub byte_range: ByteRange,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
pub struct ByteRange {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentItem {
    InputText { text: String },
    OutputText { text: String },
}

impl ContentItem {
    pub fn text(&self) -> &str {
        match self {
            Self::InputText { text } | Self::OutputText { text } => text,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessagePhase {
    Commentary,
    FinalAnswer,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseItem {
    Message {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        role: String,
        content: Vec<ContentItem>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        end_turn: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        phase: Option<MessagePhase>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reasoning_content: Option<String>,
    },
    FunctionCall {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        name: String,
        arguments: String,
        call_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reasoning_content: Option<String>,
    },
    FunctionCallOutput {
        call_id: String,
        output: String,
    },
}

impl ResponseItem {
    pub fn message(role: impl Into<String>, text: impl Into<String>) -> Self {
        let role = role.into();
        let text = text.into();
        let content = if role == "assistant" {
            vec![ContentItem::OutputText { text }]
        } else {
            vec![ContentItem::InputText { text }]
        };
        Self::Message {
            id: None,
            role,
            content,
            end_turn: None,
            phase: None,
            reasoning_content: None,
        }
    }

    pub fn role(&self) -> Option<&str> {
        match self {
            Self::Message { role, .. } => Some(role.as_str()),
            _ => None,
        }
    }

    pub fn text(&self) -> Option<String> {
        match self {
            Self::Message { content, .. } => Some(
                content
                    .iter()
                    .map(ContentItem::text)
                    .collect::<Vec<_>>()
                    .join(""),
            ),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Event {
    pub id: String,
    pub msg: EventMsg,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventMsg {
    Error(ErrorEvent),
    Warning(WarningEvent),
    SessionConfigured(SessionConfiguredEvent),
    ThreadNameUpdated(ThreadNameUpdatedEvent),
    TurnStarted(TurnStartedEvent),
    TurnComplete(TurnCompleteEvent),
    TurnAborted(TurnAbortedEvent),
    UserMessage(UserMessageEvent),
    AgentMessage(AgentMessageEvent),
    AgentMessageDelta(AgentMessageDeltaEvent),
    ToolCallBegin(ToolCallBeginEvent),
    ToolCallEnd(ToolCallEndEvent),
    TokenCount(TokenCountEvent),
    ShutdownComplete,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ErrorEvent {
    pub message: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct WarningEvent {
    pub message: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct SessionConfiguredEvent {
    pub session_id: ThreadId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forked_from_id: Option<ThreadId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_name: Option<String>,
    pub model: String,
    pub model_provider_id: String,
    pub approval_policy: AskForApproval,
    pub sandbox_policy: SandboxPolicy,
    pub cwd: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollout_path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_messages: Option<Vec<EventMsg>>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ThreadNameUpdatedEvent {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct TurnStartedEvent {
    pub turn_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_context_window: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct TurnCompleteEvent {
    pub turn_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_agent_message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct TurnAbortedEvent {
    pub reason: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct UserMessageEvent {
    pub message: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct AgentMessageEvent {
    pub message: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct AgentMessageDeltaEvent {
    pub delta: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct ToolCallBeginEvent {
    pub call_id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct ToolCallEndEvent {
    pub call_id: String,
    pub name: String,
    pub output: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq, Eq)]
pub struct TokenUsage {
    pub input_tokens: i64,
    pub cached_input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_output_tokens: i64,
    pub total_tokens: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct TokenUsageInfo {
    pub total_token_usage: TokenUsage,
    pub last_token_usage: TokenUsage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_context_window: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct TokenCountEvent {
    pub info: TokenUsageInfo,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionSource {
    Cli,
    #[default]
    Web,
    Custom(String),
    Unknown,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct SessionMeta {
    pub id: ThreadId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forked_from_id: Option<ThreadId>,
    pub timestamp: String,
    pub cwd: PathBuf,
    pub originator: String,
    pub cli_version: String,
    #[serde(default)]
    pub source: SessionSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dynamic_tools: Option<Vec<DynamicToolSpec>>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct SessionMetaLine {
    #[serde(flatten)]
    pub meta: SessionMeta,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum RolloutItem {
    SessionMeta(SessionMetaLine),
    ResponseItem(ResponseItem),
    EventMsg(EventMsg),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForkedFrom {
    ExistingThread,
    Rollout,
}

#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error("invalid thread id: {0}")]
    InvalidThreadId(String),
}
