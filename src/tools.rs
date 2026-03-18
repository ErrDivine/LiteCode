use anyhow::Context;
use serde_json::json;
use tokio::process::Command;

use crate::types::{FunctionDef, ToolDefinition};

const MAX_OUTPUT_LEN: usize = 10_000;

pub struct ToolResult {
    pub output: String,
}

pub fn tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            r#type: "function".into(),
            function: FunctionDef {
                name: "shell".into(),
                description: "Run a shell command and return its output (stdout and stderr \
                               combined). The command runs in the current working directory."
                    .into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "The shell command to execute"
                        }
                    },
                    "required": ["command"]
                }),
            },
        },
        ToolDefinition {
            r#type: "function".into(),
            function: FunctionDef {
                name: "write_file".into(),
                description: "Write content to a file at the given path. Creates parent \
                               directories if needed. Overwrites existing files."
                    .into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "File path to write to"
                        },
                        "content": {
                            "type": "string",
                            "description": "Content to write to the file"
                        }
                    },
                    "required": ["path", "content"]
                }),
            },
        },
    ]
}

pub async fn execute_tool(name: &str, input: &serde_json::Value) -> ToolResult {
    match name {
        "shell" => execute_shell(input).await,
        "write_file" => execute_write_file(input),
        _ => ToolResult {
            output: format!("Unknown tool: {name}"),
        },
    }
}

async fn execute_shell(input: &serde_json::Value) -> ToolResult {
    let command = match input.get("command").and_then(|v| v.as_str()) {
        Some(c) => c,
        None => {
            return ToolResult {
                output: "Missing 'command' parameter".into(),
            }
        }
    };

    eprintln!("\x1b[36m[shell]\x1b[0m $ {command}");

    let result = Command::new("sh")
        .arg("-c")
        .arg(command)
        .output()
        .await
        .context("Failed to spawn shell");

    match result {
        Ok(output) => {
            let mut combined = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !stderr.is_empty() {
                if !combined.is_empty() {
                    combined.push('\n');
                }
                combined.push_str(&stderr);
            }
            if combined.len() > MAX_OUTPUT_LEN {
                combined.truncate(MAX_OUTPUT_LEN);
                combined.push_str("\n... (output truncated)");
            }
            if combined.is_empty() {
                combined = format!("(exit code {})", output.status.code().unwrap_or(-1));
            }
            ToolResult { output: combined }
        }
        Err(e) => ToolResult {
            output: e.to_string(),
        },
    }
}

fn execute_write_file(input: &serde_json::Value) -> ToolResult {
    let path = match input.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => {
            return ToolResult {
                output: "Missing 'path' parameter".into(),
            }
        }
    };
    let content = match input.get("content").and_then(|v| v.as_str()) {
        Some(c) => c,
        None => {
            return ToolResult {
                output: "Missing 'content' parameter".into(),
            }
        }
    };

    eprintln!("\x1b[36m[write_file]\x1b[0m {path}");

    let file_path = std::path::Path::new(path);
    if let Some(parent) = file_path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            return ToolResult {
                output: format!("Failed to create directories: {e}"),
            };
        }
    }

    match std::fs::write(file_path, content) {
        Ok(()) => ToolResult {
            output: format!("Successfully wrote {} bytes to {path}", content.len()),
        },
        Err(e) => ToolResult {
            output: format!("Failed to write file: {e}"),
        },
    }
}
