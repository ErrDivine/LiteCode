use async_trait::async_trait;
use futures::StreamExt;
use openai_rs::{
    ChatCompletionRequest, ChatStreamEvent, Client, FunctionDef, Message, ThinkingConfig, ToolCall,
    ToolCallFunction, ToolDefinition,
};
use protocol::{AgentMessageDeltaEvent, ContentItem, DynamicToolSpec, ResponseItem};
use session_kernel::{
    EventEmitter, KernelError, Result, Scheduler, SchedulerOutput, ToolExecutionResult, TurnRequest,
};

pub struct OpenAiScheduler {
    client: Client,
    base_url: String,
    request_options: ModelRequestOptions,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAiCompatibleConfig {
    pub api_key: String,
    pub base_url: String,
    pub request_options: ModelRequestOptions,
}

impl OpenAiCompatibleConfig {
    pub fn new(api_key: impl Into<String>, base_url: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: base_url.into(),
            request_options: ModelRequestOptions::default(),
        }
    }

    pub fn with_request_options(mut self, request_options: ModelRequestOptions) -> Self {
        self.request_options = request_options;
        self
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ThinkingMode {
    #[default]
    Auto,
    Disabled,
    Enabled,
    ProviderDefault,
}

impl ThinkingMode {
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "" | "auto" => Ok(Self::Auto),
            "disabled" | "disable" | "off" | "none" | "non-thinking" => Ok(Self::Disabled),
            "enabled" | "enable" | "on" | "thinking" => Ok(Self::Enabled),
            "provider_default" | "provider-default" | "default" | "omit" => {
                Ok(Self::ProviderDefault)
            }
            other => Err(KernelError::InvalidRequest(format!(
                "invalid thinking mode `{other}`; expected auto, disabled, enabled, or provider_default"
            ))),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModelRequestOptions {
    pub thinking_mode: ThinkingMode,
    pub reasoning_effort: Option<String>,
}

impl ModelRequestOptions {
    pub fn new(
        thinking_mode: impl AsRef<str>,
        reasoning_effort: Option<impl AsRef<str>>,
    ) -> Result<Self> {
        let thinking_mode = ThinkingMode::parse(thinking_mode.as_ref())?;
        let reasoning_effort = reasoning_effort
            .and_then(|value| {
                let value = value.as_ref().trim().to_ascii_lowercase();
                (!value.is_empty()).then_some(value)
            })
            .map(|value| match value.as_str() {
                "low" | "medium" | "high" => Ok(value),
                other => Err(KernelError::InvalidRequest(format!(
                    "invalid reasoning effort `{other}`; expected low, medium, or high"
                ))),
            })
            .transpose()?;
        Ok(Self {
            thinking_mode,
            reasoning_effort,
        })
    }

    pub fn for_chat_request(&self, base_url: &str) -> ChatRequestOptions {
        let thinking = match self.thinking_mode {
            ThinkingMode::Auto => {
                if provider_uses_deepseek_thinking_default(base_url) {
                    Some(ThinkingConfig::disabled())
                } else {
                    None
                }
            }
            ThinkingMode::Disabled => Some(ThinkingConfig::disabled()),
            ThinkingMode::Enabled => Some(ThinkingConfig::enabled()),
            ThinkingMode::ProviderDefault => None,
        };
        let reasoning_effort = if matches!(self.thinking_mode, ThinkingMode::Disabled) {
            None
        } else {
            self.reasoning_effort.clone()
        };
        ChatRequestOptions {
            thinking,
            reasoning_effort,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChatRequestOptions {
    pub thinking: Option<ThinkingConfig>,
    pub reasoning_effort: Option<String>,
}

fn provider_uses_deepseek_thinking_default(base_url: &str) -> bool {
    base_url.to_ascii_lowercase().contains("deepseek")
}

impl OpenAiScheduler {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            base_url: String::new(),
            request_options: ModelRequestOptions::default(),
        }
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
            .base_url(config.base_url.clone())
            .build()
            .map_err(|err| KernelError::InvalidRequest(err.to_string()))?;
        Ok(Self {
            client,
            base_url: config.base_url,
            request_options: config.request_options,
        })
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

            let request_options = self.request_options.for_chat_request(&self.base_url);
            let chat_request = ChatCompletionRequest {
                model: request.model.clone(),
                max_tokens: request.max_tokens,
                messages: messages.clone(),
                tools: tools.clone(),
                thinking: request_options.thinking,
                reasoning_effort: request_options.reasoning_effort,
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
            let mut reasoning_content = String::new();
            let mut tool_call_builders: Vec<(String, String, String)> = Vec::new();
            let mut finish_reason: Option<String> = None;

            while let Some(event) = stream.next().await {
                if request.cancellation.is_cancelled() {
                    return Ok(SchedulerOutput::default());
                }

                match event {
                    Ok(ChatStreamEvent::Delta {
                        content: delta_content,
                        reasoning_content: delta_reasoning_content,
                        tool_calls: delta_tool_calls,
                        finish_reason: delta_finish,
                    }) => {
                        if let Some(text) = delta_reasoning_content {
                            reasoning_content.push_str(&text);
                        }

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
                    response_items.push(ResponseItem::Message {
                        id: None,
                        role: "assistant".to_string(),
                        content: vec![ContentItem::OutputText {
                            text: message.clone(),
                        }],
                        end_turn: None,
                        phase: None,
                        reasoning_content: (!reasoning_content.is_empty())
                            .then_some(reasoning_content.clone()),
                    });
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
            let reasoning_content = (!reasoning_content.is_empty()).then_some(reasoning_content);
            if reasoning_content.is_some() {
                let last = messages
                    .last_mut()
                    .expect("assistant tool-call message was just pushed");
                last.reasoning_content = reasoning_content.clone();
            }

            for (index, call) in tool_calls.iter().enumerate() {
                response_items.push(ResponseItem::FunctionCall {
                    id: (!call.id.is_empty()).then_some(call.id.clone()),
                    name: call.function.name.clone(),
                    arguments: call.function.arguments.clone(),
                    call_id: call.id.clone(),
                    reasoning_content: (index == 0).then(|| reasoning_content.clone()).flatten(),
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
            ResponseItem::Message {
                role,
                content,
                reasoning_content,
                ..
            } => {
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
                    "assistant" => messages.push(Message::assistant_with_reasoning(
                        Some(text),
                        reasoning_content.clone(),
                        None,
                    )),
                    _ => {}
                }
            }
            ResponseItem::FunctionCallOutput { call_id, output } => {
                messages.push(Message::tool_result(call_id.clone(), output.clone()));
            }
            ResponseItem::FunctionCall { .. } => {
                let mut calls = Vec::new();
                let mut reasoning_content = None;
                while let Some(ResponseItem::FunctionCall {
                    name,
                    arguments,
                    call_id,
                    reasoning_content: call_reasoning_content,
                    ..
                }) = history.get(index)
                {
                    if reasoning_content.is_none() {
                        reasoning_content = call_reasoning_content.clone();
                    }
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
                    messages.push(Message::assistant_with_reasoning(
                        None,
                        reasoning_content,
                        Some(calls),
                    ));
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
    fn auto_thinking_disables_deepseek_and_omits_other_providers() {
        let options = ModelRequestOptions::default();
        let deepseek = options.for_chat_request("https://api.deepseek.com/v1");
        assert_eq!(deepseek.thinking, Some(ThinkingConfig::disabled()));

        let openai = options.for_chat_request("https://api.openai.com/v1");
        assert_eq!(openai.thinking, None);
    }

    #[test]
    fn enabled_thinking_passes_reasoning_effort() {
        let options = ModelRequestOptions::new("enabled", Some("HIGH")).unwrap();
        let chat = options.for_chat_request("https://api.deepseek.com/v1");
        assert_eq!(chat.thinking, Some(ThinkingConfig::enabled()));
        assert_eq!(chat.reasoning_effort.as_deref(), Some("high"));
    }

    #[test]
    fn history_preserves_tool_calls_before_outputs() {
        let messages = history_to_messages(&[
            ResponseItem::message("user", "inspect"),
            ResponseItem::FunctionCall {
                id: Some("call-1".to_string()),
                name: "read_file".to_string(),
                arguments: "{\"path\":\"src/lib.rs\"}".to_string(),
                call_id: "call-1".to_string(),
                reasoning_content: Some("Need to inspect the file.".to_string()),
            },
            ResponseItem::FunctionCall {
                id: Some("call-2".to_string()),
                name: "git_status".to_string(),
                arguments: "{}".to_string(),
                call_id: "call-2".to_string(),
                reasoning_content: None,
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
        assert_eq!(
            messages[1].reasoning_content.as_deref(),
            Some("Need to inspect the file.")
        );
        assert_eq!(messages[2].role, Role::Tool);
    }
}
