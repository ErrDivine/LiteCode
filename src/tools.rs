use anyhow::{Context, anyhow};
use regex::Regex;
use serde_json::json;
use std::path::{Path, PathBuf};
use tokio::process::Command;

use crate::types::{FunctionDef, ToolDefinition};

const MAX_OUTPUT_LEN: usize = 10_000;
const MAX_SEARCH_MATCHES: usize = 100;
const MAX_FIND_RESULTS: usize = 200;
const MAX_WALK_DEPTH: usize = 20;

const SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "__pycache__",
    ".venv",
    "dist",
    "build",
    ".next",
    ".mypy_cache",
    ".pytest_cache",
];

pub struct ToolResult {
    pub output: String,
}

fn truncate_output(mut output: String) -> String {
    if output.len() > MAX_OUTPUT_LEN {
        output.truncate(MAX_OUTPUT_LEN);
        output.push_str("\n... (output truncated)");
    }
    output
}

/// Parse a numeric parameter robustly — handles both JSON numbers and stringified numbers.
fn parse_usize_param(input: &serde_json::Value, key: &str) -> Option<usize> {
    input.get(key).and_then(|v| {
        v.as_u64()
            .map(|n| n as usize)
            .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
    })
}

fn parse_bool_param(input: &serde_json::Value, key: &str) -> Option<bool> {
    input.get(key).and_then(|v| {
        v.as_bool()
            .or_else(|| v.as_str().map(|s| s == "true"))
    })
}

// ─── Tool Definitions ───────────────────────────────────────────────────────

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
        ToolDefinition {
            r#type: "function".into(),
            function: FunctionDef {
                name: "read_file".into(),
                description: "Read a file and return its contents with line numbers. \
                               Use offset and limit to read a specific range of lines."
                    .into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "File path to read"
                        },
                        "offset": {
                            "type": "integer",
                            "description": "Line number to start reading from (1-based). Defaults to 1."
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum number of lines to read. Defaults to the entire file."
                        }
                    },
                    "required": ["path"]
                }),
            },
        },
        ToolDefinition {
            r#type: "function".into(),
            function: FunctionDef {
                name: "edit_file".into(),
                description: "Edit a file by replacing an exact string match with new content. \
                               The old_string must match exactly one location in the file. \
                               Include surrounding context lines in old_string to ensure uniqueness."
                    .into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "File path to edit"
                        },
                        "old_string": {
                            "type": "string",
                            "description": "The exact string to find (must match exactly once)"
                        },
                        "new_string": {
                            "type": "string",
                            "description": "The replacement string"
                        }
                    },
                    "required": ["path", "old_string", "new_string"]
                }),
            },
        },
        ToolDefinition {
            r#type: "function".into(),
            function: FunctionDef {
                name: "list_directory".into(),
                description: "List files and directories at the given path. \
                               Directories are shown with a trailing /."
                    .into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Directory path to list. Defaults to the current directory."
                        }
                    },
                    "required": []
                }),
            },
        },
        ToolDefinition {
            r#type: "function".into(),
            function: FunctionDef {
                name: "search_files".into(),
                description: "Search for a text pattern in files recursively. Returns matching \
                               lines with file paths and line numbers (grep-style). Skips binary \
                               files and common non-source directories."
                    .into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "pattern": {
                            "type": "string",
                            "description": "Text or regex pattern to search for"
                        },
                        "path": {
                            "type": "string",
                            "description": "Directory to search in. Defaults to current directory."
                        },
                        "regex": {
                            "type": "boolean",
                            "description": "Treat pattern as a regex. Defaults to false (literal match)."
                        },
                        "include": {
                            "type": "string",
                            "description": "Glob pattern to filter files (e.g. '*.rs', '*.py')"
                        }
                    },
                    "required": ["pattern"]
                }),
            },
        },
        ToolDefinition {
            r#type: "function".into(),
            function: FunctionDef {
                name: "find_files".into(),
                description: "Find files matching a glob pattern. Use **/*.rs for recursive \
                               matching or *.rs for current directory only."
                    .into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "pattern": {
                            "type": "string",
                            "description": "Glob pattern to match (e.g. '**/*.rs', 'src/**/*.py', 'Cargo.*')"
                        },
                        "path": {
                            "type": "string",
                            "description": "Base directory to search from. Defaults to current directory."
                        }
                    },
                    "required": ["pattern"]
                }),
            },
        },
    ]
}

// ─── Tool Dispatch ──────────────────────────────────────────────────────────

pub async fn execute_tool(name: &str, input: &serde_json::Value) -> ToolResult {
    match name {
        "shell" => execute_shell(input).await,
        "write_file" => execute_write_file(input),
        "read_file" => execute_read_file(input),
        "edit_file" => execute_edit_file(input),
        "list_directory" => execute_list_directory(input),
        "search_files" => execute_search_files(input),
        "find_files" => execute_find_files(input),
        _ => ToolResult {
            output: format!("Unknown tool: {name}"),
        },
    }
}

// ─── shell ──────────────────────────────────────────────────────────────────

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

    let result = if cfg!(target_os = "windows") {
        let primary = Command::new("powershell")
            .arg("-NoProfile")
            .arg("-Command")
            .arg(command)
            .output()
            .await;

        match primary {
            Ok(output) => Ok(output),
            Err(primary_err) => {
                let fallback = Command::new("cmd")
                    .arg("/C")
                    .arg(command)
                    .output()
                    .await;
                match fallback {
                    Ok(output) => Ok(output),
                    Err(fallback_err) => Err(anyhow!(
                        "Failed to spawn shell. powershell: {primary_err}; cmd fallback: {fallback_err}"
                    )),
                }
            }
        }
    } else {
        Command::new("sh")
            .arg("-c")
            .arg(command)
            .output()
            .await
            .context("Failed to spawn shell")
    };

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
            if combined.is_empty() {
                combined = format!("(exit code {})", output.status.code().unwrap_or(-1));
            }
            ToolResult {
                output: truncate_output(combined),
            }
        }
        Err(e) => ToolResult {
            output: e.to_string(),
        },
    }
}

// ─── write_file ─────────────────────────────────────────────────────────────

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

    let file_path = Path::new(path);
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

// ─── read_file ──────────────────────────────────────────────────────────────

fn execute_read_file(input: &serde_json::Value) -> ToolResult {
    let path = match input.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => {
            return ToolResult {
                output: "Missing 'path' parameter".into(),
            }
        }
    };

    let offset = parse_usize_param(input, "offset").unwrap_or(1).max(1);
    let limit = parse_usize_param(input, "limit");

    eprintln!(
        "\x1b[36m[read_file]\x1b[0m {path}{}",
        match limit {
            Some(l) => format!(" (lines {offset}-{})", offset + l - 1),
            None => String::new(),
        }
    );

    // Check for binary content
    let file_path = Path::new(path);
    match std::fs::read(file_path) {
        Ok(bytes) => {
            let check_len = bytes.len().min(8192);
            if bytes[..check_len].contains(&0) {
                return ToolResult {
                    output: format!("File appears to be binary ({} bytes)", bytes.len()),
                };
            }

            let content = match String::from_utf8(bytes) {
                Ok(s) => s,
                Err(_) => {
                    return ToolResult {
                        output: "File is not valid UTF-8 (possibly binary)".into(),
                    }
                }
            };

            if content.is_empty() {
                return ToolResult {
                    output: format!("File is empty (0 bytes): {path}"),
                };
            }

            let lines: Vec<&str> = content.lines().collect();
            let total_lines = lines.len();

            if offset > total_lines {
                return ToolResult {
                    output: format!("File has only {total_lines} lines (requested offset {offset})"),
                };
            }

            let start = offset - 1;
            let end = match limit {
                Some(l) => (start + l).min(total_lines),
                None => total_lines,
            };

            let width = format!("{}", end).len();
            let mut output = String::new();
            for (i, line) in lines[start..end].iter().enumerate() {
                let line_num = start + i + 1;
                output.push_str(&format!("{line_num:>width$} | {line}\n"));
            }

            ToolResult {
                output: truncate_output(output),
            }
        }
        Err(e) => ToolResult {
            output: format!("Failed to read file: {e}"),
        },
    }
}

// ─── edit_file ──────────────────────────────────────────────────────────────

fn execute_edit_file(input: &serde_json::Value) -> ToolResult {
    let path = match input.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => {
            return ToolResult {
                output: "Missing 'path' parameter".into(),
            }
        }
    };
    let old_string = match input.get("old_string").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => {
            return ToolResult {
                output: "Missing 'old_string' parameter".into(),
            }
        }
    };
    let new_string = match input.get("new_string").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => {
            return ToolResult {
                output: "Missing 'new_string' parameter".into(),
            }
        }
    };

    if old_string.is_empty() {
        return ToolResult {
            output: "old_string cannot be empty".into(),
        };
    }

    if old_string == new_string {
        return ToolResult {
            output: "old_string and new_string are identical, no changes made".into(),
        };
    }

    eprintln!("\x1b[36m[edit_file]\x1b[0m {path}");

    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            return ToolResult {
                output: format!("Failed to read file: {e}"),
            }
        }
    };

    let count = content.matches(old_string).count();
    if count == 0 {
        return ToolResult {
            output: format!("old_string not found in {path}"),
        };
    }
    if count > 1 {
        return ToolResult {
            output: format!(
                "old_string found {count} times in {path}. Provide more surrounding context to make it unique."
            ),
        };
    }

    let new_content = content.replacen(old_string, new_string, 1);

    match std::fs::write(path, &new_content) {
        Ok(()) => ToolResult {
            output: format!(
                "Edited {path}: replaced {} bytes with {} bytes",
                old_string.len(),
                new_string.len()
            ),
        },
        Err(e) => ToolResult {
            output: format!("Failed to write file: {e}"),
        },
    }
}

// ─── list_directory ─────────────────────────────────────────────────────────

fn execute_list_directory(input: &serde_json::Value) -> ToolResult {
    let path = input
        .get("path")
        .and_then(|v| v.as_str())
        .unwrap_or(".");

    eprintln!("\x1b[36m[list_directory]\x1b[0m {path}");

    let dir_path = Path::new(path);
    if !dir_path.exists() {
        return ToolResult {
            output: format!("Directory not found: {path}"),
        };
    }
    if !dir_path.is_dir() {
        return ToolResult {
            output: format!("Not a directory: {path}"),
        };
    }

    let entries = match std::fs::read_dir(dir_path) {
        Ok(e) => e,
        Err(e) => {
            return ToolResult {
                output: format!("Failed to read directory: {e}"),
            }
        }
    };

    let mut items: Vec<String> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let ft = entry.file_type();
        if let Ok(ft) = ft {
            if ft.is_dir() {
                items.push(format!("{name}/"));
            } else {
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                items.push(format!("{name}  ({size} bytes)"));
            }
        }
    }

    if items.is_empty() {
        return ToolResult {
            output: "Directory is empty".into(),
        };
    }

    items.sort();
    ToolResult {
        output: truncate_output(items.join("\n")),
    }
}

// ─── search_files ───────────────────────────────────────────────────────────

fn execute_search_files(input: &serde_json::Value) -> ToolResult {
    let pattern = match input.get("pattern").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => {
            return ToolResult {
                output: "Missing 'pattern' parameter".into(),
            }
        }
    };

    if pattern.is_empty() {
        return ToolResult {
            output: "Pattern cannot be empty".into(),
        };
    }

    let search_path = input
        .get("path")
        .and_then(|v| v.as_str())
        .unwrap_or(".");
    let use_regex = parse_bool_param(input, "regex").unwrap_or(false);
    let include_glob = input.get("include").and_then(|v| v.as_str());

    eprintln!(
        "\x1b[36m[search_files]\x1b[0m pattern=\"{pattern}\" path={search_path}{}",
        if use_regex { " (regex)" } else { "" }
    );

    // Compile regex if needed
    let re = if use_regex {
        match Regex::new(pattern) {
            Ok(r) => Some(r),
            Err(e) => {
                return ToolResult {
                    output: format!("Invalid regex pattern: {e}"),
                }
            }
        }
    } else {
        None
    };

    // Compile include glob filter
    let glob_filter = if let Some(glob_str) = include_glob {
        match glob::Pattern::new(glob_str) {
            Ok(p) => Some(p),
            Err(e) => {
                return ToolResult {
                    output: format!("Invalid include glob pattern: {e}"),
                }
            }
        }
    } else {
        None
    };

    // Walk the directory tree
    let mut files = Vec::new();
    walk_files(Path::new(search_path), &mut files, 0);

    let mut matches = Vec::new();
    let mut total_matches: usize = 0;

    'outer: for file_path in &files {
        // Apply glob filter on the file name
        if let Some(ref gf) = glob_filter {
            let file_name = file_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            if !gf.matches(&file_name) {
                continue;
            }
        }

        // Read file, skip binary/non-UTF-8
        let content = match std::fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        for (line_num, line) in content.lines().enumerate() {
            let is_match = match &re {
                Some(regex) => regex.is_match(line),
                None => line.contains(pattern),
            };

            if is_match {
                total_matches += 1;
                if matches.len() < MAX_SEARCH_MATCHES {
                    let display_path = file_path.to_string_lossy();
                    matches.push(format!("{display_path}:{}: {}", line_num + 1, line.trim()));
                }
                if total_matches >= MAX_SEARCH_MATCHES * 10 {
                    // Stop walking entirely if way too many matches
                    break 'outer;
                }
            }
        }
    }

    if matches.is_empty() {
        return ToolResult {
            output: "No matches found".into(),
        };
    }

    let mut output = matches.join("\n");
    if total_matches > MAX_SEARCH_MATCHES {
        output.push_str(&format!(
            "\n\n... ({total_matches} total matches, showing first {MAX_SEARCH_MATCHES})"
        ));
    }

    ToolResult {
        output: truncate_output(output),
    }
}

/// Recursively collect file paths, skipping SKIP_DIRS and respecting depth limit.
fn walk_files(dir: &Path, results: &mut Vec<PathBuf>, depth: usize) {
    if depth > MAX_WALK_DEPTH {
        return;
    }

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();

        if path.is_dir() {
            let dir_name = entry.file_name().to_string_lossy().to_string();
            if SKIP_DIRS.contains(&dir_name.as_str()) {
                continue;
            }
            walk_files(&path, results, depth + 1);
        } else if path.is_file() {
            results.push(path);
        }
    }
}

// ─── find_files ─────────────────────────────────────────────────────────────

fn execute_find_files(input: &serde_json::Value) -> ToolResult {
    let pattern = match input.get("pattern").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => {
            return ToolResult {
                output: "Missing 'pattern' parameter".into(),
            }
        }
    };

    let base_path = input
        .get("path")
        .and_then(|v| v.as_str())
        .unwrap_or(".");

    eprintln!("\x1b[36m[find_files]\x1b[0m pattern=\"{pattern}\" path={base_path}");

    let full_pattern = Path::new(base_path).join(pattern);
    let full_pattern_str = full_pattern.to_string_lossy();

    let paths = match glob::glob(&full_pattern_str) {
        Ok(p) => p,
        Err(e) => {
            return ToolResult {
                output: format!("Invalid glob pattern: {e}"),
            }
        }
    };

    let mut results: Vec<String> = Vec::new();
    let mut total = 0usize;

    for entry in paths.flatten() {
        total += 1;
        if results.len() < MAX_FIND_RESULTS {
            results.push(entry.to_string_lossy().to_string());
        }
    }

    if results.is_empty() {
        return ToolResult {
            output: format!("No files found matching pattern: {pattern}"),
        };
    }

    let mut output = results.join("\n");
    if total > MAX_FIND_RESULTS {
        output.push_str(&format!(
            "\n\n... ({total} total results, showing first {MAX_FIND_RESULTS})"
        ));
    }

    ToolResult {
        output: truncate_output(output),
    }
}
