mod api;
mod tools;
mod types;

// use std::str::FromStr;

use anyhow::Result;
use clap::Parser;

use api::ApiClient;
use tools::{execute_tool, tool_definitions};
use types::*;
use std::io::{self, Write};

const SYSTEM_PROMPT: &str = "\
You are a coding assistant operating inside the user's project directory. \
You have access to tools for running shell commands, reading and writing files, \
editing files, listing directories, and searching code. \
When the task depends on project contents, use read_file, list_directory, or search_files \
to inspect the workspace and base your answer on real results. \
Prefer edit_file for targeted changes instead of rewriting entire files with write_file. \
Use find_files to locate files by glob pattern (e.g. **/*.rs). \
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
        .expect("OPENROUTER_API_KEY environment variable not set");

    let client = ApiClient::new(api_key);
    let tools = tool_definitions();

    let mut messages: Vec<Message> = vec![
        Message::system(SYSTEM_PROMPT),
    ];

    let mut tool_loop_flag = false;
    let mut first_turn = true;
    let mut ask_for_permissions = true;

    loop {
        if !tool_loop_flag {
            // Before execution, read user prompt for this turn.
            if first_turn {
                messages.push(Message::user(&cli.prompt));
                first_turn = false;
            } else {
                let user_stdin_input = read_trimmed_line()?;

                // Naive exiting for the present.
                if user_stdin_input == "exit" {
                    break;
                }

                // Append user prompt to message list.
                messages.push(Message::user(user_stdin_input));
            }
        }

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
            tool_loop_flag = false;
            println!();
            continue;
        }

        if let Some(calls) = tool_calls {
            if ask_for_permissions {
                print!("Allow tools to execute and do not ask me again? [y/n]: ");
                io::stdout().flush()?;
                let permission = read_trimmed_line()?;
                ask_for_permissions = match permission.trim().to_ascii_lowercase().as_str() {
                    "y" => false,
                    "n" => true,
                    _ => {
                        println!("Invalid input, defaulting to n(no)");
                        true
                    }
                };
            }

            let permission_flag;
            if ask_for_permissions {
                println!("LiteCode wants to execute a command:\n");
                permission_flag = ask_tool_permission(&calls)?;
            } else {
                permission_flag = true;
            }

            // Execute each tool call and append results.
            if permission_flag {
                tool_loop_flag = true;
                for tc in &calls {
                    let input: serde_json::Value = serde_json::from_str(&tc.function.arguments)
                        .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
                    let result = execute_tool(&tc.function.name, &input).await;
                    messages.push(Message::tool_result(&tc.id, result.output));
                }
            } else {
                // Do not execute tools and let the model rewrite
                tool_loop_flag = true;
                messages.push(Message::user(
                    "I do not approve these tool calls. Please revise your tool plan.\
                    Choose a safer and more appropriate sequence of tools, reduce unnecessary actions, and explain briefly why the new plan is better. \
                    Do not execute any tools in this reply."
                ));
                println!();
                continue;
            }
        }
        println!();
    }

    Ok(())
}

fn read_trimmed_line() -> Result<String> {
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_string())
}

// List commands with tools to the user and ask for permission
fn ask_tool_permission(commands: &[ToolCall]) -> Result<bool> {
    for command in commands {
        println!("{:?}", command.function);
    }

    loop {
        print!("\nExecute these commands? [y/n]: ");
        io::stdout().flush()?;
        let input = read_trimmed_line()?;
        match input.trim().to_ascii_lowercase().as_str() {
            "y" => return Ok(true),
            "n" => return Ok(false),
            _ => {
                println!("Please type y or n.");
            }
        }
    }
}

