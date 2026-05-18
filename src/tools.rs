use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use protocol::DynamicToolSpec;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::json;
use session_kernel::{ToolExecutionResult, ToolExecutor};
use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::time::timeout;

use crate::skill_mcp::{McpToolRuntime, SkillDescriptor, SkillScope};

const MAX_OUTPUT_LEN: usize = 10_000;
const MAX_SEARCH_MATCHES: usize = 100;
const MAX_FIND_RESULTS: usize = 200;
const MAX_WALK_DEPTH: usize = 20;
const MAX_READ_MANY_FILES: usize = 12;
const MAX_SKILL_RESOURCE_BYTES: u64 = 256_000;
const MAX_SKILL_SCRIPT_STDIN_BYTES: usize = 64_000;
const ROLLBACK_ROOT: &str = ".marvis/rollback";

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

#[derive(Clone, Debug)]
pub struct ToolPolicy {
    pub cwd: PathBuf,
    pub allow_workspace_write: bool,
    pub allow_shell: bool,
    pub allow_risky_shell: bool,
    pub allow_git_write: bool,
    pub allow_network: bool,
    pub command_timeout_secs: u64,
    pub selected_skills: Vec<SkillDescriptor>,
}

impl Default for ToolPolicy {
    fn default() -> Self {
        Self {
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            allow_workspace_write: false,
            allow_shell: true,
            allow_risky_shell: false,
            allow_git_write: false,
            allow_network: false,
            command_timeout_secs: 120,
            selected_skills: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct LocalToolExecutor {
    policy: ToolPolicy,
    mcp_runtime: Option<Arc<McpToolRuntime>>,
}

impl LocalToolExecutor {
    pub fn new(policy: ToolPolicy) -> Self {
        Self {
            policy,
            mcp_runtime: None,
        }
    }

    pub fn with_mcp(policy: ToolPolicy, mcp_runtime: Arc<McpToolRuntime>) -> Self {
        Self {
            policy,
            mcp_runtime: Some(mcp_runtime),
        }
    }
}

#[async_trait]
impl ToolExecutor for LocalToolExecutor {
    async fn execute_tool(&self, name: &str, input: &serde_json::Value) -> ToolExecutionResult {
        if McpToolRuntime::is_mcp_tool_name(name) {
            let output = match &self.mcp_runtime {
                Some(runtime) => match runtime.call_tool(name, input, &self.policy.cwd).await {
                    Ok(output) => output,
                    Err(err) => format!("MCP tool failed: {err}"),
                },
                None => format!("Unknown MCP tool: {name}"),
            };
            return ToolExecutionResult { output };
        }
        let result = execute_tool_with_policy(&self.policy, name, input).await;
        ToolExecutionResult {
            output: result.output,
        }
    }
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
    input
        .get(key)
        .and_then(|v| v.as_bool().or_else(|| v.as_str().map(|s| s == "true")))
}

// ─── Tool Definitions ───────────────────────────────────────────────────────

pub fn tool_definitions_for_policy(policy: &ToolPolicy) -> Vec<DynamicToolSpec> {
    let mut tools = vec![
        tool(
            "read_file",
            "Read a workspace file and return its contents with line numbers. Use offset and limit to read a specific range of lines.",
            json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Workspace-relative file path to read"
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
        ),
        tool(
            "read_many_files",
            "Read several workspace text files in one call. Each file is returned with line numbers and a path header.",
            json!({
                "type": "object",
                "properties": {
                    "paths": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Workspace-relative file paths to read. Maximum 12."
                    },
                    "limit_per_file": {
                        "type": "integer",
                        "description": "Maximum lines per file. Defaults to 200."
                    }
                },
                "required": ["paths"]
            }),
        ),
        tool(
            "list_directory",
            "List files and directories inside the workspace. Directories are shown with a trailing /.",
            json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Workspace-relative directory path to list. Defaults to the workspace root."
                    }
                },
                "required": []
            }),
        ),
        tool(
            "search_files",
            "Search for a text pattern in workspace files recursively. Returns matching lines with file paths and line numbers.",
            json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Text or regex pattern to search for"
                    },
                    "path": {
                        "type": "string",
                        "description": "Workspace-relative directory to search in. Defaults to the workspace root."
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
        ),
        tool(
            "list_symbols",
            "Return a lightweight symbol outline for a workspace source file. Supports common Rust, Python, JavaScript, TypeScript, and Markdown declarations.",
            json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Workspace-relative source file path"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum symbols to return. Defaults to 120."
                    }
                },
                "required": ["path"]
            }),
        ),
        tool(
            "find_files",
            "Find workspace files matching a glob pattern. Use **/*.rs for recursive matching or *.rs for current directory only.",
            json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Glob pattern to match (e.g. '**/*.rs', 'src/**/*.py', 'Cargo.*')"
                    },
                    "path": {
                        "type": "string",
                        "description": "Workspace-relative base directory to search from. Defaults to the workspace root."
                    }
                },
                "required": ["pattern"]
            }),
        ),
        tool(
            "git_status",
            "Show git status for the workspace.",
            json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        ),
        tool(
            "git_diff",
            "Show git diff for the workspace. Use path to limit the diff to one workspace file.",
            json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Optional workspace-relative file path"
                    }
                },
                "required": []
            }),
        ),
        tool(
            "run_test",
            "Run a safe test command from the workspace. Defaults to cargo test.",
            json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "Safe test command, e.g. cargo test --workspace"
                    }
                },
                "required": []
            }),
        ),
        tool(
            "run_build",
            "Run a safe build/typecheck command from the workspace. Defaults to cargo check --workspace.",
            json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "Safe build command, e.g. cargo check --workspace"
                    }
                },
                "required": []
            }),
        ),
        tool(
            "run_formatter",
            "Run a safe formatter check from the workspace. Defaults to cargo fmt --all -- --check.",
            json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "Safe formatter command"
                    }
                },
                "required": []
            }),
        ),
        tool(
            "list_rollbacks",
            "List rollback snapshots created before workspace write tools changed files.",
            json!({
                "type": "object",
                "properties": {
                    "limit": {
                        "type": "integer",
                        "description": "Maximum snapshots to return. Defaults to 20."
                    }
                },
                "required": []
            }),
        ),
        tool(
            "list_skills",
            "List the skill packages equipped for this agent turn, including package roots and resource counts.",
            json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        ),
        tool(
            "list_skill_resources",
            "List scripts, references, and assets declared by an equipped skill package.",
            json!({
                "type": "object",
                "properties": {
                    "skill": {
                        "type": "string",
                        "description": "Equipped skill id or name"
                    }
                },
                "required": ["skill"]
            }),
        ),
        tool(
            "read_skill_resource",
            "Read a text resource from an equipped skill package. The path must be one of the package resources.",
            json!({
                "type": "object",
                "properties": {
                    "skill": {
                        "type": "string",
                        "description": "Equipped skill id or name"
                    },
                    "path": {
                        "type": "string",
                        "description": "Resource path relative to the skill package root"
                    }
                },
                "required": ["skill", "path"]
            }),
        ),
    ];

    if policy.allow_shell {
        tools.push(tool(
            "shell",
            "Run a shell command from the workspace. Risky commands are blocked unless the user granted risky-shell permission.",
            json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The shell command to execute"
                    }
                },
                "required": ["command"]
            }),
        ));
    }

    if policy.allow_risky_shell && !policy.selected_skills.is_empty() {
        tools.push(tool(
            "run_skill_script",
            "Run a script declared by an equipped skill package. Requires risky-shell approval because scripts are executable code.",
            json!({
                "type": "object",
                "properties": {
                    "skill": {
                        "type": "string",
                        "description": "Equipped skill id or name"
                    },
                    "path": {
                        "type": "string",
                        "description": "Script path relative to the skill package root, usually under scripts/"
                    },
                    "args": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Arguments passed directly to the script"
                    },
                    "stdin": {
                        "type": "string",
                        "description": "Optional stdin text passed to the script"
                    }
                },
                "required": ["skill", "path"]
            }),
        ));
    }

    if policy.allow_workspace_write {
        tools.extend([
            tool(
                "write_file",
                "Write content to a workspace file. Creates parent directories if needed. Overwrites existing files.",
                json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Workspace-relative file path to write to"
                        },
                        "content": {
                            "type": "string",
                            "description": "Content to write to the file"
                        }
                    },
                    "required": ["path", "content"]
                }),
            ),
            tool(
                "edit_file",
                "Edit a workspace file by replacing an exact string match with new content. The old_string must match exactly one location in the file.",
                json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Workspace-relative file path to edit"
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
            ),
            tool(
                "apply_patch",
                "Apply a unified diff patch to the workspace using git apply. Requires workspace write permission.",
                json!({
                    "type": "object",
                    "properties": {
                        "patch": {
                            "type": "string",
                            "description": "Unified diff patch text"
                        }
                    },
                    "required": ["patch"]
                }),
            ),
            tool(
                "restore_rollback",
                "Restore workspace files from a rollback snapshot id. Requires workspace write permission and creates a new undo snapshot first.",
                json!({
                    "type": "object",
                    "properties": {
                        "id": {
                            "type": "string",
                            "description": "Rollback snapshot id from list_rollbacks"
                        }
                    },
                    "required": ["id"]
                }),
            ),
        ]);
    }

    tools
}

#[allow(dead_code)]
fn legacy_tool_definitions() -> Vec<DynamicToolSpec> {
    vec![
        tool(
            "shell",
            "Run a shell command and return its output (stdout and stderr combined). The command runs in the current working directory.",
            json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The shell command to execute"
                    }
                },
                "required": ["command"]
            }),
        ),
        tool(
            "write_file",
            "Write content to a file at the given path. Creates parent directories if needed. Overwrites existing files.",
            json!({
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
        ),
        tool(
            "read_file",
            "Read a file and return its contents with line numbers. Use offset and limit to read a specific range of lines.",
            json!({
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
        ),
        tool(
            "edit_file",
            "Edit a file by replacing an exact string match with new content. The old_string must match exactly one location in the file. Include surrounding context lines in old_string to ensure uniqueness.",
            json!({
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
        ),
        tool(
            "list_directory",
            "List files and directories at the given path. Directories are shown with a trailing /.",
            json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Directory path to list. Defaults to the current directory."
                    }
                },
                "required": []
            }),
        ),
        tool(
            "search_files",
            "Search for a text pattern in files recursively. Returns matching lines with file paths and line numbers (grep-style). Skips binary files and common non-source directories.",
            json!({
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
        ),
        tool(
            "find_files",
            "Find files matching a glob pattern. Use **/*.rs for recursive matching or *.rs for current directory only.",
            json!({
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
        ),
    ]
}

fn tool(name: &str, description: &str, parameters: serde_json::Value) -> DynamicToolSpec {
    DynamicToolSpec {
        name: name.to_string(),
        description: description.to_string(),
        parameters,
    }
}

// ─── Tool Dispatch ──────────────────────────────────────────────────────────

pub async fn execute_tool_with_policy(
    policy: &ToolPolicy,
    name: &str,
    input: &serde_json::Value,
) -> ToolResult {
    match name {
        "shell" => execute_shell(policy, input).await,
        "write_file" => execute_write_file(policy, input),
        "read_file" => execute_read_file(policy, input),
        "read_many_files" => execute_read_many_files(policy, input),
        "edit_file" => execute_edit_file(policy, input),
        "apply_patch" => execute_apply_patch(policy, input).await,
        "list_rollbacks" => execute_list_rollbacks(policy, input),
        "restore_rollback" => execute_restore_rollback(policy, input),
        "list_directory" => execute_list_directory(policy, input),
        "search_files" => execute_search_files(policy, input),
        "list_symbols" => execute_list_symbols(policy, input),
        "find_files" => execute_find_files(policy, input),
        "list_skills" => execute_list_skills(policy),
        "list_skill_resources" => execute_list_skill_resources(policy, input),
        "read_skill_resource" => execute_read_skill_resource(policy, input),
        "run_skill_script" => execute_run_skill_script(policy, input).await,
        "git_status" => execute_git_status(policy).await,
        "git_diff" => execute_git_diff(policy, input).await,
        "run_test" => {
            execute_safe_command(
                policy,
                input
                    .get("command")
                    .and_then(|v| v.as_str())
                    .unwrap_or("cargo test"),
                SafeCommandKind::Test,
            )
            .await
        }
        "run_build" => {
            execute_safe_command(
                policy,
                input
                    .get("command")
                    .and_then(|v| v.as_str())
                    .unwrap_or("cargo check --workspace"),
                SafeCommandKind::Build,
            )
            .await
        }
        "run_formatter" => {
            execute_safe_command(
                policy,
                input
                    .get("command")
                    .and_then(|v| v.as_str())
                    .unwrap_or("cargo fmt --all -- --check"),
                SafeCommandKind::Formatter,
            )
            .await
        }
        _ => ToolResult {
            output: format!("Unknown tool: {name}"),
        },
    }
}

// ─── shell ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
enum SafeCommandKind {
    Test,
    Build,
    Formatter,
}

fn blocked(message: impl Into<String>) -> ToolResult {
    ToolResult {
        output: format!("Blocked by Marvis policy: {}", message.into()),
    }
}

fn resolve_workspace_path(policy: &ToolPolicy, raw: &str) -> Result<PathBuf> {
    let path = Path::new(raw);
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(anyhow!("paths containing '..' are not allowed"));
    }

    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        policy.cwd.join(path)
    };
    let canonical_cwd = policy
        .cwd
        .canonicalize()
        .unwrap_or_else(|_| policy.cwd.clone());
    let canonical_parent = joined
        .parent()
        .and_then(|parent| parent.canonicalize().ok())
        .unwrap_or_else(|| canonical_cwd.clone());

    if !canonical_parent.starts_with(&canonical_cwd) {
        return Err(anyhow!(
            "path must stay inside workspace {}",
            canonical_cwd.display()
        ));
    }

    Ok(joined)
}

fn reject_unsafe_write_path(path: &Path) -> Option<String> {
    let components = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => value.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>();
    if components.contains(&".git") {
        return Some("writes inside .git are not allowed".to_string());
    }
    if std::fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Some("writes through symlinks are not allowed".to_string());
    }
    if components
        .windows(2)
        .any(|window| window == [".marvis", "rollback"])
    {
        return Some("writes inside .marvis/rollback are not allowed".to_string());
    }
    if components.iter().any(|value| SKIP_DIRS.contains(value)) {
        return Some(
            "writes inside dependency/build cache directories are not allowed".to_string(),
        );
    }
    None
}

fn command_has_shell_metacharacters(command: &str) -> bool {
    command
        .chars()
        .any(|ch| matches!(ch, ';' | '|' | '&' | '>' | '<' | '`'))
}

fn command_words(command: &str) -> Vec<&str> {
    command.split_whitespace().collect()
}

fn is_safe_shell_command(command: &str) -> bool {
    let words = command_words(command);
    let Some(first) = words.first().copied() else {
        return false;
    };
    match first {
        "cargo" => matches!(
            words.get(1).copied(),
            Some("test" | "check" | "fmt" | "clippy" | "build")
        ),
        "npm" => matches!(words.get(1).copied(), Some("test" | "run" | "exec")),
        "pnpm" | "yarn" => matches!(
            words.get(1).copied(),
            Some("test" | "run" | "exec" | "lint" | "build")
        ),
        "git" => matches!(
            words.get(1).copied(),
            Some("status" | "diff" | "show" | "log" | "branch")
        ),
        "rg" | "grep" | "ls" | "pwd" | "cat" | "sed" | "awk" | "find" => true,
        _ => false,
    }
}

fn is_risky_shell_command(command: &str) -> bool {
    let words = command_words(command);
    let Some(first) = words.first().copied() else {
        return true;
    };
    let lower = command.to_ascii_lowercase();
    let risky_first = matches!(
        first,
        "rm" | "rmdir"
            | "mv"
            | "cp"
            | "chmod"
            | "chown"
            | "sudo"
            | "su"
            | "curl"
            | "wget"
            | "ssh"
            | "scp"
            | "rsync"
            | "brew"
            | "pip"
            | "pip3"
            | "python"
            | "python3"
            | "node"
            | "npx"
            | "npm"
            | "pnpm"
            | "yarn"
            | "git"
    );
    if first == "git" {
        return !matches!(
            words.get(1).copied(),
            Some("status" | "diff" | "show" | "log" | "branch")
        );
    }
    if matches!(first, "npm" | "pnpm" | "yarn") {
        return !is_safe_shell_command(command);
    }
    risky_first
        || lower.contains("://")
        || lower.contains(" --force")
        || lower.contains(" -f")
        || lower.contains("delete")
        || lower.contains("destroy")
}

async fn run_shell_command(policy: &ToolPolicy, command: &str) -> Result<std::process::Output> {
    let timeout_duration = Duration::from_secs(policy.command_timeout_secs.max(1));
    let child = if cfg!(target_os = "windows") {
        Command::new("powershell")
            .arg("-NoProfile")
            .arg("-Command")
            .arg(command)
            .current_dir(&policy.cwd)
            .output()
    } else {
        Command::new("sh")
            .arg("-c")
            .arg(command)
            .current_dir(&policy.cwd)
            .output()
    };
    timeout(timeout_duration, child)
        .await
        .map_err(|_| {
            anyhow!(
                "command timed out after {} seconds",
                policy.command_timeout_secs
            )
        })?
        .context("Failed to spawn shell")
}

fn output_to_tool_result(output: std::process::Output) -> ToolResult {
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
    } else if !output.status.success() {
        combined.push_str(&format!(
            "\n(exit code {})",
            output.status.code().unwrap_or(-1)
        ));
    }
    ToolResult {
        output: truncate_output(combined),
    }
}

async fn execute_shell(policy: &ToolPolicy, input: &serde_json::Value) -> ToolResult {
    let command = match input.get("command").and_then(|v| v.as_str()) {
        Some(c) => c,
        None => {
            return ToolResult {
                output: "Missing 'command' parameter".into(),
            };
        }
    };

    if !policy.allow_shell {
        return blocked("shell execution is not allowed for this turn");
    }
    if !policy.allow_risky_shell && command_has_shell_metacharacters(command) {
        return blocked("shell metacharacters require risky-shell permission");
    }
    if !policy.allow_risky_shell && !is_safe_shell_command(command) {
        return blocked(format!(
            "`{command}` is not on the safe command allowlist; use run_test/run_build/git_diff/git_status or ask for risky-shell permission"
        ));
    }
    if !policy.allow_risky_shell && is_risky_shell_command(command) {
        return blocked(format!("`{command}` requires risky-shell permission"));
    }
    if !policy.allow_network
        && (command.contains("://")
            || command_words(command)
                .iter()
                .any(|word| matches!(*word, "curl" | "wget" | "ssh" | "scp")))
    {
        return blocked("network-like shell commands are not allowed for this turn");
    }
    if command_words(command).first() == Some(&"git")
        && !policy.allow_git_write
        && matches!(
            command_words(command).get(1).copied(),
            Some(
                "add"
                    | "commit"
                    | "push"
                    | "reset"
                    | "checkout"
                    | "switch"
                    | "merge"
                    | "rebase"
                    | "clean"
                    | "restore"
            )
        )
    {
        return blocked("git write commands require git-write permission");
    }

    eprintln!("\x1b[36m[shell]\x1b[0m $ {command}");

    match run_shell_command(policy, command).await {
        Ok(output) => output_to_tool_result(output),
        Err(e) => ToolResult {
            output: e.to_string(),
        },
    }
}

// ─── write_file ─────────────────────────────────────────────────────────────

fn execute_write_file(policy: &ToolPolicy, input: &serde_json::Value) -> ToolResult {
    if !policy.allow_workspace_write {
        return blocked("workspace write permission was not granted for this turn");
    }
    let path = match input.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => {
            return ToolResult {
                output: "Missing 'path' parameter".into(),
            };
        }
    };
    let content = match input.get("content").and_then(|v| v.as_str()) {
        Some(c) => c,
        None => {
            return ToolResult {
                output: "Missing 'content' parameter".into(),
            };
        }
    };

    eprintln!("\x1b[36m[write_file]\x1b[0m {path}");

    let file_path = match resolve_workspace_path(policy, path) {
        Ok(path) => path,
        Err(err) => return blocked(err.to_string()),
    };
    if let Some(reason) = reject_unsafe_write_path(&file_path) {
        return blocked(reason);
    }
    let rollback =
        match create_rollback_snapshot(policy, "write_file", std::slice::from_ref(&file_path)) {
            Ok(snapshot) => snapshot,
            Err(err) => {
                return ToolResult {
                    output: format!("Failed to create rollback snapshot: {err}"),
                };
            }
        };
    if let Some(parent) = file_path.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        return ToolResult {
            output: format!("Failed to create directories: {e}"),
        };
    }

    match std::fs::write(&file_path, content) {
        Ok(()) => ToolResult {
            output: format!(
                "Successfully wrote {} bytes to {}\nRollback snapshot: {}",
                content.len(),
                file_path.display(),
                rollback.id
            ),
        },
        Err(e) => ToolResult {
            output: format!("Failed to write file: {e}"),
        },
    }
}

// ─── read_file ──────────────────────────────────────────────────────────────

fn execute_read_file(policy: &ToolPolicy, input: &serde_json::Value) -> ToolResult {
    let path = match input.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => {
            return ToolResult {
                output: "Missing 'path' parameter".into(),
            };
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
    let file_path = match resolve_workspace_path(policy, path) {
        Ok(path) => path,
        Err(err) => return blocked(err.to_string()),
    };
    match std::fs::read(&file_path) {
        Ok(bytes) => {
            let check_len = bytes.len().min(8192);
            if bytes[..check_len].contains(&0) {
                return ToolResult {
                    output: format!(
                        "File appears to be binary ({} bytes): {}",
                        bytes.len(),
                        file_path.display()
                    ),
                };
            }

            let content = match String::from_utf8(bytes) {
                Ok(s) => s,
                Err(_) => {
                    return ToolResult {
                        output: "File is not valid UTF-8 (possibly binary)".into(),
                    };
                }
            };

            if content.is_empty() {
                return ToolResult {
                    output: format!("File is empty (0 bytes): {}", file_path.display()),
                };
            }

            let lines: Vec<&str> = content.lines().collect();
            let total_lines = lines.len();

            if offset > total_lines {
                return ToolResult {
                    output: format!(
                        "File has only {total_lines} lines (requested offset {offset})"
                    ),
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

fn execute_read_many_files(policy: &ToolPolicy, input: &serde_json::Value) -> ToolResult {
    let paths = match input.get("paths").and_then(|value| value.as_array()) {
        Some(paths) if !paths.is_empty() => paths,
        _ => {
            return ToolResult {
                output: "Missing 'paths' parameter".into(),
            };
        }
    };
    if paths.len() > MAX_READ_MANY_FILES {
        return blocked(format!(
            "read_many_files accepts at most {MAX_READ_MANY_FILES} paths"
        ));
    }
    let limit = parse_usize_param(input, "limit_per_file").unwrap_or(200);
    let mut output = String::new();
    for path_value in paths {
        let Some(path) = path_value.as_str() else {
            return ToolResult {
                output: "All paths must be strings".into(),
            };
        };
        output.push_str(&format!("===== {path} =====\n"));
        let result = execute_read_file(policy, &json!({ "path": path, "limit": limit }));
        output.push_str(&result.output);
        if !output.ends_with('\n') {
            output.push('\n');
        }
    }
    ToolResult {
        output: truncate_output(output),
    }
}

fn execute_list_symbols(policy: &ToolPolicy, input: &serde_json::Value) -> ToolResult {
    let path = match input.get("path").and_then(|value| value.as_str()) {
        Some(path) => path,
        None => {
            return ToolResult {
                output: "Missing 'path' parameter".into(),
            };
        }
    };
    let limit = parse_usize_param(input, "limit").unwrap_or(120).max(1);
    let file_path = match resolve_workspace_path(policy, path) {
        Ok(path) => path,
        Err(err) => return blocked(err.to_string()),
    };
    let content = match std::fs::read_to_string(&file_path) {
        Ok(content) => content,
        Err(err) => {
            return ToolResult {
                output: format!("Failed to read file: {err}"),
            };
        }
    };

    let extension = file_path.extension().and_then(|value| value.to_str());
    let mut symbols = Vec::new();
    for (index, line) in content.lines().enumerate() {
        if let Some(symbol) = symbol_from_line(extension, line) {
            symbols.push(format!("{}:{}: {}", path, index + 1, symbol));
            if symbols.len() >= limit {
                break;
            }
        }
    }

    if symbols.is_empty() {
        ToolResult {
            output: "No symbols found.".to_string(),
        }
    } else {
        ToolResult {
            output: symbols.join("\n"),
        }
    }
}

fn symbol_from_line(extension: Option<&str>, line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    if trimmed.is_empty() {
        return None;
    }
    if matches!(extension, Some("md" | "markdown")) && trimmed.starts_with('#') {
        return Some(trimmed.to_string());
    }

    let patterns = match extension {
        Some("rs") => &[
            r"^(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)",
            r"^(?:pub(?:\([^)]*\))?\s+)?(?:struct|enum|trait|mod|type|const|static)\s+([A-Za-z_][A-Za-z0-9_]*)",
            r"^impl(?:<[^>]+>)?\s+(.+)",
        ][..],
        Some("py") => &[
            r"^(?:async\s+)?def\s+([A-Za-z_][A-Za-z0-9_]*)",
            r"^class\s+([A-Za-z_][A-Za-z0-9_]*)",
        ][..],
        Some("js" | "jsx" | "ts" | "tsx") => &[
            r"^(?:export\s+)?(?:async\s+)?function\s+([A-Za-z_$][A-Za-z0-9_$]*)",
            r"^(?:export\s+)?(?:class|interface|type)\s+([A-Za-z_$][A-Za-z0-9_$]*)",
            r"^(?:export\s+)?(?:const|let|var)\s+([A-Za-z_$][A-Za-z0-9_$]*)",
        ][..],
        _ => &[
            r"^(?:function|class|def|struct|enum|trait|interface|type)\s+([A-Za-z_][A-Za-z0-9_]*)",
        ][..],
    };

    for pattern in patterns {
        let regex = Regex::new(pattern).ok()?;
        if let Some(captures) = regex.captures(trimmed) {
            let name = captures
                .get(1)
                .map(|value| value.as_str())
                .unwrap_or(trimmed);
            return Some(name.trim().to_string());
        }
    }
    None
}

fn execute_list_skills(policy: &ToolPolicy) -> ToolResult {
    let skills = policy
        .selected_skills
        .iter()
        .map(|skill| {
            let scripts = skill
                .resources
                .iter()
                .filter(|resource| {
                    resource.kind == crate::skills::SkillResourceKind::Script
                })
                .count();
            let references = skill
                .resources
                .iter()
                .filter(|resource| {
                    resource.kind == crate::skills::SkillResourceKind::Reference
                })
                .count();
            let assets = skill
                .resources
                .iter()
                .filter(|resource| resource.kind == crate::skills::SkillResourceKind::Asset)
                .count();
            json!({
                "id": skill.id,
                "name": skill.name,
                "description": skill.description,
                "scope": skill.scope,
                "path": skill.path,
                "package_root": skill.package_root,
                "local_tools": skill.local_tools,
                "mcp_servers": skill.mcp_dependencies.iter().map(|dependency| dependency.value.clone()).collect::<Vec<_>>(),
                "resources": {
                    "scripts": scripts,
                    "references": references,
                    "assets": assets
                }
            })
        })
        .collect::<Vec<_>>();
    ToolResult {
        output: serde_json::to_string_pretty(&skills).unwrap_or_else(|_| "[]".to_string()),
    }
}

fn execute_list_skill_resources(policy: &ToolPolicy, input: &serde_json::Value) -> ToolResult {
    let skill = match input.get("skill").and_then(|value| value.as_str()) {
        Some(skill) => skill,
        None => {
            return ToolResult {
                output: "Missing 'skill' parameter".into(),
            };
        }
    };
    let skill = match find_selected_skill(policy, skill) {
        Some(skill) => skill,
        None => return blocked(format!("skill `{skill}` is not equipped for this turn")),
    };
    let resources = skill
        .resources
        .iter()
        .map(|resource| {
            json!({
                "kind": resource.kind,
                "path": resource.relative_path,
                "bytes": resource.bytes,
            })
        })
        .collect::<Vec<_>>();
    ToolResult {
        output: serde_json::to_string_pretty(&resources).unwrap_or_else(|_| "[]".to_string()),
    }
}

fn execute_read_skill_resource(policy: &ToolPolicy, input: &serde_json::Value) -> ToolResult {
    let skill_name = match input.get("skill").and_then(|value| value.as_str()) {
        Some(skill) => skill,
        None => {
            return ToolResult {
                output: "Missing 'skill' parameter".into(),
            };
        }
    };
    let relative_path = match input.get("path").and_then(|value| value.as_str()) {
        Some(path) => path,
        None => {
            return ToolResult {
                output: "Missing 'path' parameter".into(),
            };
        }
    };
    let skill = match find_selected_skill(policy, skill_name) {
        Some(skill) => skill,
        None => {
            return blocked(format!(
                "skill `{skill_name}` is not equipped for this turn"
            ));
        }
    };
    let path = match skill.resource_path(relative_path) {
        Ok(path) => path,
        Err(err) => return blocked(err.to_string()),
    };
    let metadata = match std::fs::metadata(&path) {
        Ok(metadata) => metadata,
        Err(err) => {
            return ToolResult {
                output: format!("Failed to stat skill resource: {err}"),
            };
        }
    };
    if metadata.len() > MAX_SKILL_RESOURCE_BYTES {
        return blocked(format!(
            "skill resource is {} bytes; maximum readable size is {MAX_SKILL_RESOURCE_BYTES}",
            metadata.len()
        ));
    }
    match std::fs::read_to_string(&path) {
        Ok(content) => ToolResult {
            output: truncate_output(content),
        },
        Err(err) => ToolResult {
            output: format!("Failed to read skill resource as UTF-8 text: {err}"),
        },
    }
}

async fn execute_run_skill_script(policy: &ToolPolicy, input: &serde_json::Value) -> ToolResult {
    if !policy.allow_risky_shell {
        return blocked("run_skill_script requires risky-shell approval");
    }
    let skill_name = match input.get("skill").and_then(|value| value.as_str()) {
        Some(skill) => skill,
        None => {
            return ToolResult {
                output: "Missing 'skill' parameter".into(),
            };
        }
    };
    let relative_path = match input.get("path").and_then(|value| value.as_str()) {
        Some(path) => path,
        None => {
            return ToolResult {
                output: "Missing 'path' parameter".into(),
            };
        }
    };
    let skill = match find_selected_skill(policy, skill_name) {
        Some(skill) => skill,
        None => {
            return blocked(format!(
                "skill `{skill_name}` is not equipped for this turn"
            ));
        }
    };
    if skill.scope == SkillScope::Workspace && !policy.allow_risky_shell {
        return blocked("workspace skill scripts require risky-shell approval");
    }
    let script_path = match skill.script_path(relative_path) {
        Ok(path) => path,
        Err(err) => return blocked(err.to_string()),
    };
    let args = match input.get("args") {
        Some(value) => match value.as_array() {
            Some(values) => values
                .iter()
                .map(|value| value.as_str().map(str::to_string))
                .collect::<Option<Vec<_>>>(),
            None => None,
        },
        None => Some(Vec::new()),
    };
    let Some(args) = args else {
        return ToolResult {
            output: "args must be an array of strings".to_string(),
        };
    };
    let stdin_text = input
        .get("stdin")
        .and_then(|value| value.as_str())
        .map(str::to_string);
    if stdin_text
        .as_ref()
        .is_some_and(|text| text.len() > MAX_SKILL_SCRIPT_STDIN_BYTES)
    {
        return blocked(format!(
            "stdin exceeds {MAX_SKILL_SCRIPT_STDIN_BYTES} bytes"
        ));
    }

    let mut command = skill_script_command(&script_path);
    command
        .args(args)
        .current_dir(&policy.cwd)
        .env("MARVIS_WORKSPACE_ROOT", &policy.cwd)
        .env("MARVIS_SKILL_DIR", &skill.package_root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if stdin_text.is_some() {
        command.stdin(Stdio::piped());
    }

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(err) => {
            return ToolResult {
                output: format!("Failed to spawn skill script: {err}"),
            };
        }
    };
    if let Some(stdin_text) = stdin_text
        && let Some(mut stdin) = child.stdin.take()
        && let Err(err) = stdin.write_all(stdin_text.as_bytes()).await
    {
        return ToolResult {
            output: format!("Failed to write skill script stdin: {err}"),
        };
    }
    match timeout(
        Duration::from_secs(policy.command_timeout_secs.max(1)),
        child.wait_with_output(),
    )
    .await
    {
        Ok(Ok(output)) => output_to_tool_result(output),
        Ok(Err(err)) => ToolResult {
            output: format!("Failed to run skill script: {err}"),
        },
        Err(_) => ToolResult {
            output: format!(
                "skill script timed out after {} seconds",
                policy.command_timeout_secs
            ),
        },
    }
}

fn skill_script_command(script_path: &Path) -> Command {
    match script_path.extension().and_then(|value| value.to_str()) {
        Some("py") => {
            let mut command = Command::new("python3");
            command.arg(script_path);
            command
        }
        Some("js") => {
            let mut command = Command::new("node");
            command.arg(script_path);
            command
        }
        Some("sh") => {
            let mut command = Command::new("sh");
            command.arg(script_path);
            command
        }
        _ => Command::new(script_path),
    }
}

fn find_selected_skill<'a>(policy: &'a ToolPolicy, name: &str) -> Option<&'a SkillDescriptor> {
    let normalized = name.trim();
    policy.selected_skills.iter().find(|skill| {
        skill.id == normalized
            || skill.name == normalized
            || skill
                .interface
                .as_ref()
                .and_then(|interface| interface.display_name.as_deref())
                == Some(normalized)
    })
}

// ─── edit_file ──────────────────────────────────────────────────────────────

fn execute_edit_file(policy: &ToolPolicy, input: &serde_json::Value) -> ToolResult {
    if !policy.allow_workspace_write {
        return blocked("workspace write permission was not granted for this turn");
    }
    let path = match input.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => {
            return ToolResult {
                output: "Missing 'path' parameter".into(),
            };
        }
    };
    let old_string = match input.get("old_string").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => {
            return ToolResult {
                output: "Missing 'old_string' parameter".into(),
            };
        }
    };
    let new_string = match input.get("new_string").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => {
            return ToolResult {
                output: "Missing 'new_string' parameter".into(),
            };
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

    let file_path = match resolve_workspace_path(policy, path) {
        Ok(path) => path,
        Err(err) => return blocked(err.to_string()),
    };
    if let Some(reason) = reject_unsafe_write_path(&file_path) {
        return blocked(reason);
    }

    let content = match std::fs::read_to_string(&file_path) {
        Ok(c) => c,
        Err(e) => {
            return ToolResult {
                output: format!("Failed to read file: {e}"),
            };
        }
    };

    let count = content.matches(old_string).count();
    if count == 0 {
        return ToolResult {
            output: format!("old_string not found in {}", file_path.display()),
        };
    }
    if count > 1 {
        return ToolResult {
            output: format!(
                "old_string found {count} times in {}. Provide more surrounding context to make it unique.",
                file_path.display()
            ),
        };
    }

    let new_content = content.replacen(old_string, new_string, 1);

    let rollback =
        match create_rollback_snapshot(policy, "edit_file", std::slice::from_ref(&file_path)) {
            Ok(snapshot) => snapshot,
            Err(err) => {
                return ToolResult {
                    output: format!("Failed to create rollback snapshot: {err}"),
                };
            }
        };

    match std::fs::write(&file_path, &new_content) {
        Ok(()) => ToolResult {
            output: format!(
                "Edited {}: replaced {} bytes with {} bytes\nRollback snapshot: {}",
                file_path.display(),
                old_string.len(),
                new_string.len(),
                rollback.id
            ),
        },
        Err(e) => ToolResult {
            output: format!("Failed to write file: {e}"),
        },
    }
}

// ─── apply_patch ────────────────────────────────────────────────────────────

async fn execute_apply_patch(policy: &ToolPolicy, input: &serde_json::Value) -> ToolResult {
    if !policy.allow_workspace_write {
        return blocked("workspace write permission was not granted for this turn");
    }
    let patch = match input.get("patch").and_then(|v| v.as_str()) {
        Some(value) if !value.trim().is_empty() => value,
        _ => {
            return ToolResult {
                output: "Missing 'patch' parameter".into(),
            };
        }
    };
    if patch.contains("\n--- /") || patch.contains("\n+++ /") || patch.contains("../") {
        return blocked("patch paths must stay inside the workspace");
    }
    let affected_paths = match affected_paths_from_patch(policy, patch) {
        Ok(paths) => paths,
        Err(err) => return blocked(err.to_string()),
    };
    let rollback = match create_rollback_snapshot(policy, "apply_patch", &affected_paths) {
        Ok(snapshot) => snapshot,
        Err(err) => {
            return ToolResult {
                output: format!("Failed to create rollback snapshot: {err}"),
            };
        }
    };

    eprintln!("\x1b[36m[apply_patch]\x1b[0m {} bytes", patch.len());

    let mut child = match Command::new("git")
        .arg("apply")
        .arg("--whitespace=nowarn")
        .current_dir(&policy.cwd)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(err) => {
            return ToolResult {
                output: format!("Failed to spawn git apply: {err}"),
            };
        }
    };

    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        if let Err(err) = stdin.write_all(patch.as_bytes()).await {
            return ToolResult {
                output: format!("Failed to write patch to git apply: {err}"),
            };
        }
    }

    let result = timeout(
        Duration::from_secs(policy.command_timeout_secs.max(1)),
        child.wait_with_output(),
    )
    .await;
    match result {
        Ok(Ok(output)) => {
            let mut result = output_to_tool_result(output);
            result
                .output
                .push_str(&format!("\nRollback snapshot: {}", rollback.id));
            result
        }
        Ok(Err(err)) => ToolResult {
            output: format!("Failed to run git apply: {err}"),
        },
        Err(_) => ToolResult {
            output: format!(
                "git apply timed out after {} seconds",
                policy.command_timeout_secs
            ),
        },
    }
}

// ─── rollback snapshots ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RollbackManifest {
    id: String,
    created_at_ms: u64,
    operation: String,
    cwd: String,
    files: Vec<RollbackFileSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RollbackFileSnapshot {
    path: String,
    existed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    snapshot_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    size_bytes: Option<u64>,
}

fn create_rollback_snapshot(
    policy: &ToolPolicy,
    operation: &str,
    files: &[PathBuf],
) -> Result<RollbackManifest> {
    let rollback_root = policy.cwd.join(ROLLBACK_ROOT);
    std::fs::create_dir_all(&rollback_root).context("create rollback root")?;
    let id = allocate_rollback_id(&rollback_root, operation)?;
    let snapshot_dir = rollback_root.join(&id);
    let files_dir = snapshot_dir.join("files");
    std::fs::create_dir_all(&files_dir).context("create rollback files dir")?;

    let mut seen = BTreeSet::new();
    let mut snapshots = Vec::new();
    for file_path in files {
        if let Some(reason) = reject_unsafe_write_path(file_path) {
            return Err(anyhow!(reason));
        }
        let rel = workspace_relative_path(policy, file_path)?;
        if !seen.insert(rel.clone()) {
            continue;
        }
        let path_display = rel.to_string_lossy().replace('\\', "/");
        if file_path.exists() {
            if !file_path.is_file() {
                return Err(anyhow!(
                    "rollback target is not a file: {}",
                    file_path.display()
                ));
            }
            let bytes = std::fs::read(file_path)
                .with_context(|| format!("read preimage {}", file_path.display()))?;
            let snapshot_name = format!("{:04}.bin", snapshots.len());
            std::fs::write(files_dir.join(&snapshot_name), &bytes)
                .with_context(|| format!("write rollback preimage {snapshot_name}"))?;
            snapshots.push(RollbackFileSnapshot {
                path: path_display,
                existed: true,
                snapshot_path: Some(format!("files/{snapshot_name}")),
                size_bytes: Some(bytes.len() as u64),
            });
        } else {
            snapshots.push(RollbackFileSnapshot {
                path: path_display,
                existed: false,
                snapshot_path: None,
                size_bytes: None,
            });
        }
    }

    let manifest = RollbackManifest {
        id: id.clone(),
        created_at_ms: now_ms(),
        operation: operation.to_string(),
        cwd: policy.cwd.display().to_string(),
        files: snapshots,
    };
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
    std::fs::write(snapshot_dir.join("manifest.json"), manifest_bytes)
        .context("write rollback manifest")?;
    Ok(manifest)
}

fn execute_list_rollbacks(policy: &ToolPolicy, input: &serde_json::Value) -> ToolResult {
    let limit = parse_usize_param(input, "limit").unwrap_or(20).max(1);
    let rollback_root = policy.cwd.join(ROLLBACK_ROOT);
    let entries = match std::fs::read_dir(&rollback_root) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return ToolResult {
                output: "[]".to_string(),
            };
        }
        Err(err) => {
            return ToolResult {
                output: format!("Failed to read rollback root: {err}"),
            };
        }
    };

    let mut manifests = Vec::new();
    for entry in entries.flatten() {
        let manifest_path = entry.path().join("manifest.json");
        if !manifest_path.is_file() {
            continue;
        }
        if let Ok(contents) = std::fs::read_to_string(&manifest_path)
            && let Ok(manifest) = serde_json::from_str::<RollbackManifest>(&contents)
        {
            manifests.push(manifest);
        }
    }
    manifests.sort_by(|left, right| right.created_at_ms.cmp(&left.created_at_ms));
    manifests.truncate(limit);
    let summaries = manifests
        .into_iter()
        .map(|manifest| {
            json!({
                "id": manifest.id,
                "created_at_ms": manifest.created_at_ms,
                "operation": manifest.operation,
                "files": manifest.files.iter().map(|file| file.path.clone()).collect::<Vec<_>>(),
            })
        })
        .collect::<Vec<_>>();
    ToolResult {
        output: serde_json::to_string_pretty(&summaries).unwrap_or_else(|_| "[]".to_string()),
    }
}

fn execute_restore_rollback(policy: &ToolPolicy, input: &serde_json::Value) -> ToolResult {
    if !policy.allow_workspace_write {
        return blocked("workspace write permission was not granted for this turn");
    }
    let id = match input.get("id").and_then(|value| value.as_str()) {
        Some(id) if valid_rollback_id(id) => id,
        Some(_) => {
            return blocked("rollback id contains invalid characters");
        }
        None => {
            return ToolResult {
                output: "Missing 'id' parameter".into(),
            };
        }
    };
    let rollback_dir = policy.cwd.join(ROLLBACK_ROOT).join(id);
    let manifest_path = rollback_dir.join("manifest.json");
    let manifest = match std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("read {}", manifest_path.display()))
        .and_then(|contents| {
            serde_json::from_str::<RollbackManifest>(&contents).context("parse rollback manifest")
        }) {
        Ok(manifest) => manifest,
        Err(err) => {
            return ToolResult {
                output: format!("Failed to read rollback {id}: {err}"),
            };
        }
    };

    let mut targets = Vec::new();
    for file in &manifest.files {
        let target = match resolve_workspace_path(policy, &file.path) {
            Ok(path) => path,
            Err(err) => return blocked(err.to_string()),
        };
        if let Some(reason) = reject_unsafe_write_path(&target) {
            return blocked(reason);
        }
        targets.push(target);
    }
    let undo = match create_rollback_snapshot(
        policy,
        &format!("restore_rollback_{}", sanitize_rollback_component(id)),
        &targets,
    ) {
        Ok(snapshot) => snapshot,
        Err(err) => {
            return ToolResult {
                output: format!("Failed to create undo rollback snapshot: {err}"),
            };
        }
    };

    for (file, target) in manifest.files.iter().zip(targets.iter()) {
        if file.existed {
            let Some(snapshot_path) = &file.snapshot_path else {
                return ToolResult {
                    output: format!("Rollback {id} is missing preimage for {}", file.path),
                };
            };
            let bytes = match std::fs::read(rollback_dir.join(snapshot_path)) {
                Ok(bytes) => bytes,
                Err(err) => {
                    return ToolResult {
                        output: format!("Failed to read preimage for {}: {err}", file.path),
                    };
                }
            };
            if let Some(parent) = target.parent()
                && let Err(err) = std::fs::create_dir_all(parent)
            {
                return ToolResult {
                    output: format!("Failed to create restore parent dirs: {err}"),
                };
            }
            if let Err(err) = std::fs::write(target, bytes) {
                return ToolResult {
                    output: format!("Failed to restore {}: {err}", file.path),
                };
            }
        } else if target.exists() {
            if target.is_file() {
                if let Err(err) = std::fs::remove_file(target) {
                    return ToolResult {
                        output: format!("Failed to remove new file {}: {err}", file.path),
                    };
                }
            } else {
                return ToolResult {
                    output: format!("Rollback target is not a file: {}", file.path),
                };
            }
        }
    }

    ToolResult {
        output: format!(
            "Restored rollback {id} ({} files)\nUndo snapshot: {}",
            manifest.files.len(),
            undo.id
        ),
    }
}

fn affected_paths_from_patch(policy: &ToolPolicy, patch: &str) -> Result<Vec<PathBuf>> {
    let mut paths = BTreeSet::new();
    for line in patch.lines() {
        let Some(raw_path) = line
            .strip_prefix("--- ")
            .or_else(|| line.strip_prefix("+++ "))
        else {
            continue;
        };
        let raw_path = raw_path.split_whitespace().next().unwrap_or_default();
        if raw_path.is_empty() || raw_path == "/dev/null" {
            continue;
        }
        let path = raw_path
            .strip_prefix("a/")
            .or_else(|| raw_path.strip_prefix("b/"))
            .unwrap_or(raw_path);
        let resolved = resolve_workspace_path(policy, path)?;
        if let Some(reason) = reject_unsafe_write_path(&resolved) {
            return Err(anyhow!(reason));
        }
        paths.insert(resolved);
    }
    Ok(paths.into_iter().collect())
}

fn allocate_rollback_id(root: &Path, operation: &str) -> Result<String> {
    let base = format!(
        "{}-{}-{}",
        now_ms(),
        sanitize_rollback_component(operation),
        std::process::id()
    );
    for attempt in 0..1000_u16 {
        let id = if attempt == 0 {
            base.clone()
        } else {
            format!("{base}-{attempt}")
        };
        if !root.join(&id).exists() {
            return Ok(id);
        }
    }
    Err(anyhow!("could not allocate unique rollback id"))
}

fn workspace_relative_path(policy: &ToolPolicy, path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        policy.cwd.join(path)
    };
    absolute
        .strip_prefix(&policy.cwd)
        .map(Path::to_path_buf)
        .with_context(|| {
            format!(
                "path {} is outside workspace {}",
                absolute.display(),
                policy.cwd.display()
            )
        })
}

fn sanitize_rollback_component(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
            out.push(ch);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

fn valid_rollback_id(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ─── safe command helpers ───────────────────────────────────────────────────

async fn execute_git_status(policy: &ToolPolicy) -> ToolResult {
    execute_safe_command(
        policy,
        "git status --short --branch",
        SafeCommandKind::Build,
    )
    .await
}

async fn execute_git_diff(policy: &ToolPolicy, input: &serde_json::Value) -> ToolResult {
    let command = if let Some(path) = input.get("path").and_then(|v| v.as_str()) {
        let path = match resolve_workspace_path(policy, path) {
            Ok(path) => path,
            Err(err) => return blocked(err.to_string()),
        };
        let relative = path.strip_prefix(&policy.cwd).unwrap_or(&path);
        format!(
            "git diff -- {}",
            shell_quote(&relative.display().to_string())
        )
    } else {
        "git diff --".to_string()
    };
    execute_safe_command(policy, &command, SafeCommandKind::Build).await
}

async fn execute_safe_command(
    policy: &ToolPolicy,
    command: &str,
    kind: SafeCommandKind,
) -> ToolResult {
    if !policy.allow_shell {
        return blocked("shell execution is not allowed for this turn");
    }
    if command_has_shell_metacharacters(command) {
        return blocked("shell metacharacters are not allowed for safe helper commands");
    }
    let words = command_words(command);
    let safe = match kind {
        SafeCommandKind::Test => {
            matches!(
                words.as_slice(),
                ["cargo", "test", ..]
                    | ["npm", "test", ..]
                    | ["pnpm", "test", ..]
                    | ["yarn", "test", ..]
            )
        }
        SafeCommandKind::Build => {
            matches!(
                words.as_slice(),
                ["cargo", "check", ..]
                    | ["cargo", "build", ..]
                    | ["git", "status", ..]
                    | ["git", "diff", ..]
            )
        }
        SafeCommandKind::Formatter => {
            matches!(
                words.as_slice(),
                ["cargo", "fmt", ..]
                    | ["npm", "run", "format", ..]
                    | ["pnpm", "run", "format", ..]
                    | ["yarn", "run", "format", ..]
            )
        }
    };
    if !safe {
        return blocked(format!("`{command}` is not allowed for this safe helper"));
    }

    eprintln!("\x1b[36m[safe]\x1b[0m $ {command}");
    match run_shell_command(policy, command).await {
        Ok(output) => output_to_tool_result(output),
        Err(err) => ToolResult {
            output: err.to_string(),
        },
    }
}

fn shell_quote(value: &str) -> String {
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '_' | '-' | '.'))
    {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

// ─── list_directory ─────────────────────────────────────────────────────────

fn execute_list_directory(policy: &ToolPolicy, input: &serde_json::Value) -> ToolResult {
    let path = input.get("path").and_then(|v| v.as_str()).unwrap_or(".");

    eprintln!("\x1b[36m[list_directory]\x1b[0m {path}");

    let dir_path = match resolve_workspace_path(policy, path) {
        Ok(path) => path,
        Err(err) => return blocked(err.to_string()),
    };
    if !dir_path.exists() {
        return ToolResult {
            output: format!("Directory not found: {}", dir_path.display()),
        };
    }
    if !dir_path.is_dir() {
        return ToolResult {
            output: format!("Not a directory: {}", dir_path.display()),
        };
    }

    let entries = match std::fs::read_dir(&dir_path) {
        Ok(e) => e,
        Err(e) => {
            return ToolResult {
                output: format!("Failed to read directory: {e}"),
            };
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

fn execute_search_files(policy: &ToolPolicy, input: &serde_json::Value) -> ToolResult {
    let pattern = match input.get("pattern").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => {
            return ToolResult {
                output: "Missing 'pattern' parameter".into(),
            };
        }
    };

    if pattern.is_empty() {
        return ToolResult {
            output: "Pattern cannot be empty".into(),
        };
    }

    let search_path = input.get("path").and_then(|v| v.as_str()).unwrap_or(".");
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
                };
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
                };
            }
        }
    } else {
        None
    };

    let search_path = match resolve_workspace_path(policy, search_path) {
        Ok(path) => path,
        Err(err) => return blocked(err.to_string()),
    };

    // Walk the directory tree
    let mut files = Vec::new();
    walk_files(&search_path, &mut files, 0);

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

fn execute_find_files(policy: &ToolPolicy, input: &serde_json::Value) -> ToolResult {
    let pattern = match input.get("pattern").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => {
            return ToolResult {
                output: "Missing 'pattern' parameter".into(),
            };
        }
    };

    let base_path = input.get("path").and_then(|v| v.as_str()).unwrap_or(".");

    eprintln!("\x1b[36m[find_files]\x1b[0m pattern=\"{pattern}\" path={base_path}");

    let base_path = match resolve_workspace_path(policy, base_path) {
        Ok(path) => path,
        Err(err) => return blocked(err.to_string()),
    };
    if Path::new(pattern)
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return blocked("glob patterns containing '..' are not allowed");
    }
    let full_pattern = base_path.join(pattern);
    let full_pattern_str = full_pattern.to_string_lossy();

    let paths = match glob::glob(&full_pattern_str) {
        Ok(p) => p,
        Err(e) => {
            return ToolResult {
                output: format!("Invalid glob pattern: {e}"),
            };
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::{SkillResource, SkillResourceKind};

    fn test_policy(allow_workspace_write: bool) -> ToolPolicy {
        let root = std::env::temp_dir().join(format!("marvis-tools-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&root);
        ToolPolicy {
            cwd: root,
            allow_workspace_write,
            allow_shell: true,
            allow_risky_shell: false,
            allow_git_write: false,
            allow_network: false,
            command_timeout_secs: 10,
            selected_skills: Vec::new(),
        }
    }

    fn equipped_skill_policy() -> ToolPolicy {
        let mut policy = test_policy(false);
        let skill_root = policy.cwd.join("skill-package");
        std::fs::create_dir_all(skill_root.join("references")).unwrap();
        std::fs::create_dir_all(skill_root.join("scripts")).unwrap();
        std::fs::write(skill_root.join("references/guide.md"), "Read this guide.").unwrap();
        std::fs::write(skill_root.join("scripts/helper.py"), "print('ok')").unwrap();
        policy.selected_skills = vec![SkillDescriptor {
            id: "demo-skill".to_string(),
            name: "Demo Skill".to_string(),
            description: "Demo package.".to_string(),
            short_description: None,
            interface: None,
            policy: None,
            path: Some(skill_root.join("SKILL.md")),
            package_root: skill_root,
            scope: SkillScope::Workspace,
            body: "---\nname: demo-skill\ndescription: Demo package.\n---\nBody.".to_string(),
            local_tools: vec![
                "list_skill_resources".to_string(),
                "read_skill_resource".to_string(),
            ],
            mcp_dependencies: Vec::new(),
            resources: vec![
                SkillResource {
                    kind: SkillResourceKind::Reference,
                    relative_path: PathBuf::from("references/guide.md"),
                    bytes: 16,
                },
                SkillResource {
                    kind: SkillResourceKind::Script,
                    relative_path: PathBuf::from("scripts/helper.py"),
                    bytes: 11,
                },
            ],
        }];
        policy
    }

    #[tokio::test]
    async fn write_tool_is_hidden_and_blocked_without_permission() {
        let policy = test_policy(false);
        let tools = tool_definitions_for_policy(&policy);
        assert!(!tools.iter().any(|tool| tool.name == "write_file"));

        let result = execute_tool_with_policy(
            &policy,
            "write_file",
            &json!({
                "path": "demo.txt",
                "content": "hello"
            }),
        )
        .await;
        assert!(result.output.contains("Blocked by Marvis policy"));
    }

    #[test]
    fn read_many_files_and_list_symbols_work_inside_workspace() {
        let policy = test_policy(false);
        std::fs::write(
            policy.cwd.join("lib.rs"),
            "pub struct Demo;\n\nimpl Demo {\n    pub fn run(&self) {}\n}\n",
        )
        .unwrap();
        std::fs::write(policy.cwd.join("mod.rs"), "fn helper() {}\n").unwrap();

        let many = execute_read_many_files(
            &policy,
            &json!({
                "paths": ["lib.rs", "mod.rs"],
                "limit_per_file": 20
            }),
        );
        assert!(many.output.contains("===== lib.rs ====="));
        assert!(many.output.contains("pub struct Demo"));

        let symbols = execute_list_symbols(&policy, &json!({ "path": "lib.rs" }));
        assert!(symbols.output.contains("lib.rs:1: Demo"));
        assert!(symbols.output.contains("lib.rs:3: Demo {"));
    }

    #[tokio::test]
    async fn skill_resource_tools_are_scoped_to_equipped_skills() {
        let policy = equipped_skill_policy();
        let listed = execute_list_skill_resources(&policy, &json!({ "skill": "demo-skill" }));
        assert!(listed.output.contains("references/guide.md"));

        let read = execute_read_skill_resource(
            &policy,
            &json!({
                "skill": "Demo Skill",
                "path": "references/guide.md"
            }),
        );
        assert_eq!(read.output, "Read this guide.");

        let blocked = execute_read_skill_resource(
            &policy,
            &json!({
                "skill": "demo-skill",
                "path": "../Cargo.toml"
            }),
        );
        assert!(blocked.output.contains("Blocked by Marvis policy"));

        let script = execute_run_skill_script(
            &policy,
            &json!({
                "skill": "demo-skill",
                "path": "scripts/helper.py"
            }),
        )
        .await;
        assert!(script.output.contains("risky-shell approval"));
    }

    #[tokio::test]
    async fn risky_shell_is_blocked_without_permission() {
        let policy = test_policy(true);
        let result = execute_tool_with_policy(
            &policy,
            "shell",
            &json!({
                "command": "rm -rf target"
            }),
        )
        .await;
        assert!(result.output.contains("Blocked by Marvis policy"));
    }

    #[tokio::test]
    async fn safe_helpers_reject_shell_metacharacters() {
        let policy = test_policy(false);
        let result = execute_tool_with_policy(
            &policy,
            "run_test",
            &json!({
                "command": "cargo test -- ; touch injected"
            }),
        )
        .await;
        assert!(
            result
                .output
                .contains("shell metacharacters are not allowed")
        );
        assert!(!policy.cwd.join("injected").exists());
    }

    #[tokio::test]
    async fn read_file_cannot_escape_workspace() {
        let policy = test_policy(false);
        let result = execute_tool_with_policy(
            &policy,
            "read_file",
            &json!({
                "path": "../outside.txt"
            }),
        )
        .await;
        assert!(result.output.contains("Blocked by Marvis policy"));
    }

    #[tokio::test]
    async fn write_file_creates_rollback_and_restore_recovers_preimage() {
        let policy = test_policy(true);
        let path = policy.cwd.join("demo.txt");
        std::fs::write(&path, "before").unwrap();

        let write = execute_tool_with_policy(
            &policy,
            "write_file",
            &json!({
                "path": "demo.txt",
                "content": "after"
            }),
        )
        .await;
        assert!(write.output.contains("Rollback snapshot:"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "after");

        let list = execute_tool_with_policy(&policy, "list_rollbacks", &json!({})).await;
        let snapshots: serde_json::Value = serde_json::from_str(&list.output).unwrap();
        let id = snapshots[0]["id"].as_str().unwrap();

        let restore = execute_tool_with_policy(
            &policy,
            "restore_rollback",
            &json!({
                "id": id
            }),
        )
        .await;
        assert!(restore.output.contains("Restored rollback"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "before");
    }
}
