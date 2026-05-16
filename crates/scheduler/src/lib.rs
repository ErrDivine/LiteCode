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

impl OpenAiScheduler {
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    pub fn openrouter(api_key: impl Into<String>) -> Result<Self> {
        let client = Client::builder()
            .api_key(api_key)
            .base_url("https://openrouter.ai/api/v1")
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

                        if let Some(reason) = delta_finish {
                            if !reason.is_empty() {
                                finish_reason = Some(reason);
                            }
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
                let response_items = final_message
                    .iter()
                    .map(|message| ResponseItem::message("assistant", message.clone()))
                    .collect();
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

            for call in tool_calls {
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
    history
        .iter()
        .filter_map(|item| match item {
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
                    "system" => Some(Message::system(text)),
                    "user" => Some(Message::user(text)),
                    "assistant" => Some(Message::assistant(Some(text), None)),
                    _ => None,
                }
            }
            ResponseItem::FunctionCallOutput { call_id, output } => {
                Some(Message::tool_result(call_id.clone(), output.clone()))
            }
            ResponseItem::FunctionCall { .. } => None,
        })
        .collect()
}
