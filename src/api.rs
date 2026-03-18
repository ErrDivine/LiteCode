use anyhow::{bail, Context, Result};
use reqwest::Client;
use std::io::Write;

use crate::types::*;

const API_URL: &str = "https://openrouter.ai/api/v1/chat/completions";

pub struct ApiClient {
    client: Client,
    api_key: String,
}

impl ApiClient {
    pub fn new(api_key: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
        }
    }

    pub async fn send_message(&self, request: &ChatRequest) -> Result<(Option<String>, Option<Vec<ToolCall>>, Option<String>)> {
        let resp = self
            .client
            .post(API_URL)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("content-type", "application/json")
            .json(request)
            .send()
            .await
            .context("Failed to send API request")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!("API error {status}: {body}");
        }

        let body = resp.text().await.context("Failed to read response body")?;
        self.parse_sse_stream(&body)
    }

    fn parse_sse_stream(&self, body: &str) -> Result<(Option<String>, Option<Vec<ToolCall>>, Option<String>)> {
        let mut text = String::new();
        // tool_calls accumulated by index: (id, name, arguments_buf)
        let mut tool_call_builders: Vec<(String, String, String)> = Vec::new();
        let mut finish_reason: Option<String> = None;

        for line in body.lines() {
            let Some(data) = line.strip_prefix("data: ") else {
                continue;
            };
            if data == "[DONE]" {
                break;
            }

            let chunk: StreamChunk = match serde_json::from_str(data) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let Some(choice) = chunk.choices.into_iter().next() else {
                continue;
            };

            if let Some(fr) = choice.finish_reason {
                if !fr.is_empty() {
                    finish_reason = Some(fr);
                }
            }

            let delta = choice.delta;

            if let Some(content) = delta.content {
                print!("{content}");
                std::io::stdout().flush().ok();
                text.push_str(&content);
            }

            if let Some(tc_deltas) = delta.tool_calls {
                for tc in tc_deltas {
                    while tool_call_builders.len() <= tc.index {
                        tool_call_builders.push((String::new(), String::new(), String::new()));
                    }
                    let builder = &mut tool_call_builders[tc.index];
                    if let Some(id) = tc.id {
                        builder.0 = id;
                    }
                    if let Some(func) = tc.function {
                        if let Some(name) = func.name {
                            builder.1 = name;
                        }
                        if let Some(args) = func.arguments {
                            builder.2.push_str(&args);
                        }
                    }
                }
            }
        }

        let tool_calls = if tool_call_builders.is_empty() {
            None
        } else {
            Some(
                tool_call_builders
                    .into_iter()
                    .map(|(id, name, arguments)| ToolCall {
                        id,
                        r#type: "function".into(),
                        function: ToolCallFunction { name, arguments },
                    })
                    .collect(),
            )
        };

        let content = if text.is_empty() { None } else { Some(text) };
        // match &content {
        //     Some(s) => println!("The content is {}",s),
        //     None => println!("It is empty"),
        // }

        Ok((content, tool_calls, finish_reason))
    }
}
