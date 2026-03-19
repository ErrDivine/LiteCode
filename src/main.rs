mod api;
mod tools;
mod types;

// use std::str::FromStr;

use anyhow::{Context, Result};
use clap::Parser;

use api::ApiClient;
use tools::{execute_tool, tool_definitions};
use types::*;

const SYSTEM_PROMPT: &str = "\
You are a coding assistant operating inside the user's project directory. You have access to tools for running shell commands and writing files. \
When the task depends on project contents, inspect the workspace first with shell commands and base your answer on real results. \
Do not claim you cannot access files or run commands when these tools are available. \
Use tools to accomplish the user's request, work step by step, verify progress, then provide a brief summary.";

#[derive(Parser)]
#[command(name = "lite-code", about = "Minimal one-turn vibe coding CLI")]
struct Cli {
    /// The prompt to send to the LLM
    prompt: String,

    /// Model to use
    #[arg(short, long, default_value = "nvidia/nemotron-3-super-120b-a12b:free")]
    model: String,

    /// Max tokens for each API response
    #[arg(long, default_value = "4096")]
    max_tokens: u32,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let api_key = std::env::var("OPENROUTER_API_KEY")
        .context("Missing OPENROUTER_API_KEY environment variable. Set it before running lite-code.")?;

    let client = ApiClient::new(api_key);
    let tools = tool_definitions();

    let mut messages: Vec<Message> = vec![
        Message::system(SYSTEM_PROMPT),
        Message::user(&cli.prompt),
    ];

    loop {
        let request = ChatRequest {
            model: cli.model.clone(),
            max_tokens: cli.max_tokens,
            messages: messages.clone(),
            tools: tools.clone(),
            stream: true,
        };

        let (content, tool_calls, finish_reason) = client.send_message(&request).await?;

        // Append assistant turn to conversation
        messages.push(Message {
            role: Role::Assistant,
            content,
            tool_calls: tool_calls.clone(),
            tool_call_id: None,
            name: None,
        });

        // Done when model has no tool calls or explicitly stopped
        if tool_calls.is_none() || finish_reason.as_deref() == Some("stop") {
            println!();
            break;
        }

        // Execute each tool call and append results
        if let Some(calls) = tool_calls {
            for tc in &calls {
                let input: serde_json::Value = serde_json::from_str(&tc.function.arguments)
                    .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
                let result = execute_tool(&tc.function.name, &input).await;
                messages.push(Message::tool_result(&tc.id, result.output));
            }
        }

        println!();
    }

    Ok(())
}
