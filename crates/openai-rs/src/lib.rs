pub mod auth;
pub mod chat;
pub mod client;
pub mod error;
pub mod provider;
pub mod request;
pub mod retry;
pub mod sse;
pub mod transport;
pub mod types;

// Top-level re-exports for convenience
pub use auth::{AuthProvider, BearerAuth};
pub use chat::{ChatStream, ChatStreamEvent};
pub use client::{Client, ClientBuilder};
pub use error::{ApiError, StreamError, TransportError};
pub use provider::{Provider, RetryConfig};
pub use types::chat::{
    ChatCompletionRequest, ChatCompletionResponse, Message, Role, ToolCall, ToolCallFunction,
};
pub use types::common::{FinishReason, FunctionDef, ToolDefinition, Usage};
pub use types::stream::{Delta, FunctionDelta, StreamChoice, StreamChunk, ToolCallDelta};
