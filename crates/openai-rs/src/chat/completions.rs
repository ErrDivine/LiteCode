use crate::auth::add_auth_headers;
use crate::error::{ApiError, StreamError};
use crate::retry::run_with_retry;
use crate::sse::sse_stream;
use crate::transport::HttpTransport;
use crate::types::chat::{ChatCompletionRequest, ChatCompletionResponse};
use crate::types::stream::{StreamChunk, ToolCallDelta};
use futures::Stream;
use http::{HeaderValue, Method};
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::mpsc;

/// A streaming chat completion event.
#[derive(Debug)]
pub enum ChatStreamEvent {
    Delta {
        content: Option<String>,
        tool_calls: Option<Vec<ToolCallDelta>>,
        finish_reason: Option<String>,
    },
    Done,
}

/// An async stream of chat completion events.
pub struct ChatStream {
    rx: mpsc::Receiver<Result<ChatStreamEvent, ApiError>>,
}

impl Stream for ChatStream {
    type Item = Result<ChatStreamEvent, ApiError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.rx.poll_recv(cx)
    }
}

/// Handles chat completion requests (both streaming and non-streaming).
pub struct Completions<'a> {
    pub(crate) transport: &'a dyn HttpTransport,
    pub(crate) provider: &'a crate::provider::Provider,
    pub(crate) auth: &'a dyn crate::auth::AuthProvider,
}

impl Completions<'_> {
    /// Send a non-streaming chat completion request.
    pub async fn create(
        &self,
        mut request: ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse, ApiError> {
        request.stream = false;

        let body = serde_json::to_value(&request).map_err(|e| ApiError::InvalidRequest {
            message: e.to_string(),
        })?;

        let make_request = || {
            let mut req = self
                .provider
                .build_request(Method::POST, "chat/completions");
            req.body = Some(body.clone());
            add_auth_headers(self.auth, req)
        };

        let response = run_with_retry(self.provider.retry.to_policy(), make_request, |req| async {
            self.transport.execute(req).await
        })
        .await?;

        let parsed: ChatCompletionResponse = serde_json::from_slice(&response.body)
            .map_err(|e| ApiError::Stream(format!("failed to parse response: {e}")))?;

        Ok(parsed)
    }

    /// Send a streaming chat completion request, returning a `ChatStream`.
    pub async fn create_stream(
        &self,
        mut request: ChatCompletionRequest,
    ) -> Result<ChatStream, ApiError> {
        request.stream = true;

        let body = serde_json::to_value(&request).map_err(|e| ApiError::InvalidRequest {
            message: e.to_string(),
        })?;

        let make_request = || {
            let mut req = self
                .provider
                .build_request(Method::POST, "chat/completions");
            req.body = Some(body.clone());
            req.headers.insert(
                http::header::ACCEPT,
                HeaderValue::from_static("text/event-stream"),
            );
            add_auth_headers(self.auth, req)
        };

        let stream_response =
            run_with_retry(self.provider.retry.to_policy(), make_request, |req| async {
                self.transport.stream(req).await
            })
            .await?;

        let (tx_event, rx_event) = mpsc::channel::<Result<ChatStreamEvent, ApiError>>(256);
        let (tx_sse, mut rx_sse) = mpsc::channel::<Result<String, StreamError>>(256);

        sse_stream(
            stream_response.bytes,
            self.provider.stream_idle_timeout,
            tx_sse,
        );

        // Spawn a task to parse SSE data frames into ChatStreamEvents
        tokio::spawn(async move {
            while let Some(frame) = rx_sse.recv().await {
                match frame {
                    Ok(data) => {
                        if data == "[DONE]" {
                            let _ = tx_event.send(Ok(ChatStreamEvent::Done)).await;
                            return;
                        }

                        let chunk: StreamChunk = match serde_json::from_str(&data) {
                            Ok(c) => c,
                            Err(_) => continue,
                        };

                        let Some(choice) = chunk.choices.into_iter().next() else {
                            continue;
                        };

                        let event = ChatStreamEvent::Delta {
                            content: choice.delta.content,
                            tool_calls: choice.delta.tool_calls,
                            finish_reason: choice.finish_reason,
                        };

                        if tx_event.send(Ok(event)).await.is_err() {
                            return;
                        }
                    }
                    Err(e) => {
                        let _ = tx_event.send(Err(ApiError::Stream(e.to_string()))).await;
                        return;
                    }
                }
            }
        });

        Ok(ChatStream { rx: rx_event })
    }
}
