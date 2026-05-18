use async_trait::async_trait;
use futures::StreamExt;
use openai_rs::{
    ChatCompletionRequest, ChatStreamEvent, Client, FunctionDef, Message, ToolCall,
    ToolCallFunction, ToolDefinition,
};
use protocol::{AgentMessageDeltaEvent, ContentItem, DynamicToolSpec, ResponseItem};
use session_kernel::{
    EventEmitter, KernelError, Result, Scheduler, SchedulerOutput, ToolExecutionResult, TurnRequest,
};

pub struct OpenAiScheduler {
    client: Client,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAiCompatibleConfig {
    pub api_key: String,
    pub base_url: String,
}

impl OpenAiCompatibleConfig {
    pub fn new(api_key: impl Into<String>, base_url: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: base_url.into(),
        }
    }
}

impl OpenAiScheduler {
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    pub fn openai_compatible(config: OpenAiCompatibleConfig) -> Result<Self> {
        if config.api_key.trim().is_empty() {
            return Err(KernelError::InvalidRequest(
                "MARVIS_API_KEY must not be empty".to_string(),
            ));
        }
        if config.base_url.trim().is_empty() {
            return Err(KernelError::InvalidRequest(
                "MARVIS_BASE_URL must not be empty".to_string(),
            ));
        }

        let client = Client::builder()
            .api_key(config.api_key)
            .base_url(config.base_url)
            .build()
            .map_err(|err| KernelError::InvalidRequest(err.to_string()))?;
        Ok(Self::new(client))
    }
}

#[async_trait]
impl Scheduler for OpenAiScheduler {
    async fn run_turn(
        &self,
        request: TurnRequest,
        events: EventEmitter,
    ) -> Result<SchedulerOutput> {
        let mut messages = history_to_messages(&request.history);
        if !request.system_prompt.trim().is_empty() {
            messages.insert(0, Message::system(request.system_prompt.clone()));
        }

        let user_text = request
            .input
            .iter()
            .filter_map(|item| item.as_text())
            .collect::<Vec<_>>()
            .join("\n");
        if !user_text.is_empty() {
            messages.push(Message::user(user_text));
        }

        let tools = request
            .dynamic_tools
            .iter()
            .map(tool_spec_to_openai)
            .collect::<Vec<_>>();
        let mut tool_calls_used = 0u32;
        let mut response_items = Vec::new();

        loop {
            if request.cancellation.is_cancelled() {
                return Ok(SchedulerOutput::default());
            }

            let chat_request = ChatCompletionRequest {
                model: request.model.clone(),
                max_tokens: request.max_tokens,
                messages: messages.clone(),
                tools: tools.clone(),
                stream: true,
            };

            let mut stream = self
                .client
                .chat()
                .completions()
                .create_stream(chat_request)
                .await
                .map_err(|err| {
                    KernelError::InvalidRequest(format!("model request failed: {err}"))
                })?;

            let mut content = String::new();
            let mut tool_call_builders: Vec<(String, String, String)> = Vec::new();
            let mut finish_reason: Option<String> = None;

            while let Some(event) = stream.next().await {
                if request.cancellation.is_cancelled() {
                    return Ok(SchedulerOutput::default());
                }

                match event {
                    Ok(ChatStreamEvent::Delta {
                        content: delta_content,
                        tool_calls: delta_tool_calls,
                        finish_reason: delta_finish,
                    }) => {
                        if let Some(text) = delta_content {
                            content.push_str(&text);
                            events
                                .emit(protocol::EventMsg::AgentMessageDelta(
                                    AgentMessageDeltaEvent { delta: text },
                                ))
                                .await?;
                        }

                        if let Some(reason) = delta_finish
                            && !reason.is_empty()
                        {
                            finish_reason = Some(reason);
                        }

                        if let Some(deltas) = delta_tool_calls {
                            for delta in deltas {
                                while tool_call_builders.len() <= delta.index {
                                    tool_call_builders.push((
                                        String::new(),
                                        String::new(),
                                        String::new(),
                                    ));
                                }
                                let builder = &mut tool_call_builders[delta.index];
                                if let Some(id) = delta.id {
                                    builder.0 = id;
                                }
                                if let Some(function) = delta.function {
                                    if let Some(name) = function.name {
                                        builder.1 = name;
                                    }
                                    if let Some(arguments) = function.arguments {
                                        builder.2.push_str(&arguments);
                                    }
                                }
                            }
                        }
                    }
                    Ok(ChatStreamEvent::Done) => break,
                    Err(err) => {
                        return Err(KernelError::InvalidRequest(format!(
                            "model stream failed: {err}"
                        )));
                    }
                }
            }

            let tool_calls = tool_call_builders
                .into_iter()
                .filter(|(_, name, _)| !name.is_empty())
                .map(|(id, name, arguments)| ToolCall {
                    id,
                    r#type: "function".to_string(),
                    function: ToolCallFunction { name, arguments },
                })
                .collect::<Vec<_>>();

            if tool_calls.is_empty() || finish_reason.as_deref() == Some("stop") {
                let final_message = (!content.is_empty()).then_some(content.clone());
                if let Some(message) = &final_message {
                    response_items.push(ResponseItem::message("assistant", message.clone()));
                }
                return Ok(SchedulerOutput {
                    response_items,
                    final_message,
                    token_usage: None,
                });
            }

            messages.push(Message::assistant(
                (!content.is_empty()).then_some(content),
                Some(tool_calls.clone()),
            ));

            for call in &tool_calls {
                response_items.push(ResponseItem::FunctionCall {
                    id: (!call.id.is_empty()).then_some(call.id.clone()),
                    name: call.function.name.clone(),
                    arguments: call.function.arguments.clone(),
                    call_id: call.id.clone(),
                });
            }

            for call in tool_calls {
                if tool_calls_used >= request.max_tool_calls {
                    return Err(KernelError::InvalidRequest(format!(
                        "tool-call budget exceeded: max {}",
                        request.max_tool_calls
                    )));
                }
                tool_calls_used += 1;
                events
                    .tool_begin(
                        call.id.clone(),
                        call.function.name.clone(),
                        call.function.arguments.clone(),
                    )
                    .await?;
                let input = serde_json::from_str::<serde_json::Value>(&call.function.arguments)
                    .unwrap_or_else(|_| serde_json::Value::Object(serde_json::Map::new()));
                let ToolExecutionResult { output } = request
                    .tool_executor
                    .execute_tool(&call.function.name, &input)
                    .await;
                events
                    .tool_end(call.id.clone(), call.function.name.clone(), output.clone())
                    .await?;
                response_items.push(ResponseItem::FunctionCallOutput {
                    call_id: call.id.clone(),
                    output: output.clone(),
                });
                messages.push(Message::tool_result(call.id, output));
            }
        }
    }
}

pub struct SyntheticScheduler;

#[async_trait]
impl Scheduler for SyntheticScheduler {
    async fn run_turn(
        &self,
        request: TurnRequest,
        events: EventEmitter,
    ) -> Result<SchedulerOutput> {
        let text = request
            .input
            .iter()
            .filter_map(|item| item.as_text())
            .collect::<Vec<_>>()
            .join("\n");
        let message = if text.is_empty() {
            "ok".to_string()
        } else {
            format!("synthetic: {text}")
        };
        events
            .emit(protocol::EventMsg::AgentMessageDelta(
                AgentMessageDeltaEvent {
                    delta: message.clone(),
                },
            ))
            .await?;
        Ok(SchedulerOutput {
            response_items: vec![ResponseItem::message("assistant", message.clone())],
            final_message: Some(message),
            token_usage: None,
        })
    }
}

fn tool_spec_to_openai(spec: &DynamicToolSpec) -> ToolDefinition {
    ToolDefinition {
        r#type: "function".to_string(),
        function: FunctionDef {
            name: spec.name.clone(),
            description: spec.description.clone(),
            parameters: spec.parameters.clone(),
        },
    }
}

fn history_to_messages(history: &[ResponseItem]) -> Vec<Message> {
    let mut messages = Vec::new();
    let mut index = 0usize;
    while index < history.len() {
        match &history[index] {
            ResponseItem::Message { role, content, .. } => {
                let text = content
                    .iter()
                    .map(|content| match content {
                        ContentItem::InputText { text } | ContentItem::OutputText { text } => {
                            text.as_str()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("");
                match role.as_str() {
                    "system" => messages.push(Message::system(text)),
                    "user" => messages.push(Message::user(text)),
                    "assistant" => messages.push(Message::assistant(Some(text), None)),
                    _ => {}
                }
            }
            ResponseItem::FunctionCallOutput { call_id, output } => {
                messages.push(Message::tool_result(call_id.clone(), output.clone()));
            }
            ResponseItem::FunctionCall { .. } => {
                let mut calls = Vec::new();
                while let Some(ResponseItem::FunctionCall {
                    name,
                    arguments,
                    call_id,
                    ..
                }) = history.get(index)
                {
                    calls.push(ToolCall {
                        id: call_id.clone(),
                        r#type: "function".to_string(),
                        function: ToolCallFunction {
                            name: name.clone(),
                            arguments: arguments.clone(),
                        },
                    });
                    index += 1;
                }
                if !calls.is_empty() {
                    messages.push(Message::assistant(None, Some(calls)));
                }
                continue;
            }
        }
        index += 1;
    }
    messages
}

#[cfg(test)]
mod tests {
    use openai_rs::Role;

    use super::*;

    #[test]
    fn history_preserves_tool_calls_before_outputs() {
        let messages = history_to_messages(&[
            ResponseItem::message("user", "inspect"),
            ResponseItem::FunctionCall {
                id: Some("call-1".to_string()),
                name: "read_file".to_string(),
                arguments: "{\"path\":\"src/lib.rs\"}".to_string(),
                call_id: "call-1".to_string(),
            },
            ResponseItem::FunctionCall {
                id: Some("call-2".to_string()),
                name: "git_status".to_string(),
                arguments: "{}".to_string(),
                call_id: "call-2".to_string(),
            },
            ResponseItem::FunctionCallOutput {
                call_id: "call-1".to_string(),
                output: "contents".to_string(),
            },
        ]);

        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].role, Role::User);
        assert_eq!(messages[1].role, Role::Assistant);
        assert_eq!(messages[1].tool_calls.as_ref().unwrap().len(), 2);
        assert_eq!(messages[2].role, Role::Tool);
    }
}
