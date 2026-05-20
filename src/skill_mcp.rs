use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use pave_router::{AgentProfile, PaveVector, ToolAccess, normalize_profiles};
use protocol::DynamicToolSpec;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdout, Command};
use tokio::time::timeout;

const MAX_TOOL_NAME_LENGTH: usize = 64;
const MCP_TOOL_NAME_PREFIX: &str = "mcp";
const MCP_TOOL_NAME_DELIMITER: &str = "__";
const DEFAULT_MCP_STARTUP_TIMEOUT_SECS: u64 = 30;
const DEFAULT_MCP_TOOL_TIMEOUT_SECS: u64 = 120;

pub use crate::skills::{SkillDescriptor, SkillScope, SkillToolDependency};
use crate::skills::{load_skill_packages, render_selected_skills_section};

#[derive(Debug, Clone, Default)]
pub struct SkillRegistry {
    skills: BTreeMap<String, SkillDescriptor>,
    mcp_servers: BTreeMap<String, McpServerConfig>,
    errors: Vec<String>,
}

impl SkillRegistry {
    pub fn load(workspace_root: &Path) -> Self {
        let mut registry = Self::default();
        let skill_outcome = load_skill_packages(workspace_root);
        registry.errors.extend(skill_outcome.errors);
        for skill in skill_outcome.skills {
            registry.skills.insert(skill.id.clone(), skill);
        }
        registry.load_mcp_config(&workspace_root.join(".marvis/mcp.json"));
        registry.load_mcp_config(&workspace_root.join(".mcp.json"));
        registry.merge_skill_mcp_dependencies();
        registry
    }

    pub fn errors(&self) -> &[String] {
        &self.errors
    }

    pub fn auto_agent_profiles(&self, default_model: &str) -> Vec<AgentProfile> {
        let mut profiles = Vec::new();
        for skill in self.skills.values() {
            if skill
                .policy
                .as_ref()
                .and_then(|policy| policy.allow_implicit_invocation)
                == Some(false)
            {
                continue;
            }
            profiles.push(agent_profile_for_skill(skill, default_model));
        }
        profiles.extend(toolset_agent_profiles(default_model));
        normalize_profiles(profiles)
    }

    pub fn resolve_agent(&self, agent: &AgentProfile) -> AgentSkillSelection {
        let mut skills = Vec::new();
        let mut missing_skills = Vec::new();
        let mut skill_local_tools = BTreeSet::new();
        let mut mcp_server_names = BTreeSet::new();

        for skill_id in &agent.skills {
            match self.skills.get(skill_id) {
                Some(skill) => {
                    skill_local_tools.extend(skill.local_tools.iter().cloned());
                    for dependency in &skill.mcp_dependencies {
                        if dependency.kind.eq_ignore_ascii_case("mcp") {
                            mcp_server_names.insert(dependency.value.clone());
                        }
                    }
                    skills.push(skill.clone());
                }
                None => missing_skills.push(skill_id.clone()),
            }
        }

        mcp_server_names.extend(agent.mcp_servers.iter().cloned());

        let profile_allowlist = agent
            .tool_allowlist
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        let local_tool_allowlist =
            match (profile_allowlist.is_empty(), skill_local_tools.is_empty()) {
                (true, true) => Vec::new(),
                (true, false) => skill_local_tools.into_iter().collect(),
                (false, true) => profile_allowlist.into_iter().collect(),
                (false, false) => profile_allowlist
                    .intersection(&skill_local_tools)
                    .cloned()
                    .collect(),
            };

        let mut mcp_servers = Vec::new();
        let mut missing_mcp_servers = Vec::new();
        for server_name in mcp_server_names {
            match self.mcp_servers.get(&server_name) {
                Some(server) => {
                    let mut server = server.clone();
                    server.required = true;
                    mcp_servers.push(server);
                }
                None => missing_mcp_servers.push(server_name),
            }
        }

        AgentSkillSelection {
            skills,
            local_tool_allowlist,
            mcp_servers,
            missing_skills,
            missing_mcp_servers,
        }
    }

    fn load_mcp_config(&mut self, path: &Path) {
        if !path.is_file() {
            return;
        }
        let contents = match std::fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(err) => {
                self.errors.push(format!(
                    "failed to read MCP config {}: {err}",
                    path.display()
                ));
                return;
            }
        };
        let parsed = match serde_json::from_str::<McpConfigFile>(&contents) {
            Ok(parsed) => parsed,
            Err(err) => {
                self.errors
                    .push(format!("invalid MCP config {}: {err}", path.display()));
                return;
            }
        };
        let base_dir = path.parent().unwrap_or_else(|| Path::new("."));
        for (name, server) in parsed.mcp_servers {
            match McpServerConfig::from_file(name.clone(), server, base_dir) {
                Ok(config) => {
                    self.mcp_servers.insert(name, config);
                }
                Err(err) => self.errors.push(format!(
                    "invalid MCP server {name} in {}: {err}",
                    path.display()
                )),
            }
        }
    }

    fn merge_skill_mcp_dependencies(&mut self) {
        let mut additions = Vec::new();
        for skill in self.skills.values() {
            for dependency in &skill.mcp_dependencies {
                if !dependency.kind.eq_ignore_ascii_case("mcp")
                    || self.mcp_servers.contains_key(&dependency.value)
                {
                    continue;
                }
                if let Some(config) = dependency.to_server_config() {
                    additions.push((config.name.clone(), config));
                }
            }
        }
        for (name, config) in additions {
            self.mcp_servers.insert(name, config);
        }
    }
}

fn agent_profile_for_skill(skill: &SkillDescriptor, default_model: &str) -> AgentProfile {
    let tool_allowlist = skill_tool_allowlist(skill);
    let default_approval = approval_for_tools(&tool_allowlist, has_mcp_dependency(skill));
    AgentProfile {
        id: skill.id.clone(),
        label: skill
            .interface
            .as_ref()
            .and_then(|interface| interface.display_name.clone())
            .unwrap_or_else(|| skill.name.clone()),
        model: default_model.to_string(),
        skills: vec![skill.id.clone()],
        mcp_servers: Vec::new(),
        skill_prompt: skill_agent_instruction(skill),
        tool_allowlist: tool_allowlist.clone(),
        pave: pave_for_skill(skill, &tool_allowlist, default_approval),
        default_approval,
    }
}

fn toolset_agent_profiles(default_model: &str) -> Vec<AgentProfile> {
    vec![
        AgentProfile {
            id: "tools-read-only".to_string(),
            label: "Read-Only Tools".to_string(),
            model: default_model.to_string(),
            skills: Vec::new(),
            mcp_servers: Vec::new(),
            skill_prompt:
                "Use workspace read/search, symbol, git status, and git diff tools to inspect and explain focused codebase questions."
                    .to_string(),
            tool_allowlist: read_only_tools()
                .into_iter()
                .map(str::to_string)
                .collect(),
            pave: PaveVector::new([
                ("explanation", 1.0),
                ("docs", 0.55),
                ("git", 0.35),
                ("risk_low", 0.7),
            ]),
            default_approval: ToolAccess::read_only(),
        },
        AgentProfile {
            id: "tools-verification".to_string(),
            label: "Verification Tools".to_string(),
            model: default_model.to_string(),
            skills: Vec::new(),
            mcp_servers: Vec::new(),
            skill_prompt:
                "Use build, test, formatter, read/search, and git inspection tools to verify a focused failure or recently changed code."
                    .to_string(),
            tool_allowlist: verification_tools()
                .into_iter()
                .map(str::to_string)
                .collect(),
            pave: PaveVector::new([
                ("tests", 1.0),
                ("diagnostics", 0.65),
                ("shell", 0.55),
                ("infra", 0.4),
                ("risk_low", 0.45),
            ]),
            default_approval: ToolAccess {
                allow_workspace_write: false,
                allow_shell: true,
                allow_risky_shell: false,
                allow_git_write: false,
                allow_network: false,
            },
        },
        AgentProfile {
            id: "tools-workspace-patch".to_string(),
            label: "Workspace Patch Tools".to_string(),
            model: default_model.to_string(),
            skills: Vec::new(),
            mcp_servers: Vec::new(),
            skill_prompt:
                "Use read/search, patch, rollback, build, test, formatter, and git inspection tools for focused workspace edits after user approval."
                    .to_string(),
            tool_allowlist: patch_tools()
                .into_iter()
                .map(str::to_string)
                .collect(),
            pave: PaveVector::new([
                ("patch", 1.0),
                ("refactor", 0.65),
                ("diagnostics", 0.55),
                ("tests", 0.45),
                ("risk_medium", 0.6),
            ]),
            default_approval: ToolAccess::patch_and_checks(),
        },
    ]
}

fn skill_agent_instruction(skill: &SkillDescriptor) -> String {
    let mut parts = vec![format!("Use the `{}` skill package.", skill.id)];
    if let Some(default_prompt) = skill
        .interface
        .as_ref()
        .and_then(|interface| interface.default_prompt.as_ref())
        .map(|prompt| prompt.trim())
        .filter(|prompt| !prompt.is_empty())
    {
        parts.push(default_prompt.to_string());
    } else if !skill.description.trim().is_empty() {
        parts.push(skill.description.trim().to_string());
    }
    parts.join(" ")
}

fn skill_tool_allowlist(skill: &SkillDescriptor) -> Vec<String> {
    let mut tools = BTreeSet::new();
    if skill.local_tools.is_empty() {
        tools.extend(read_only_tools().into_iter().map(str::to_string));
    } else {
        tools.extend(skill.local_tools.iter().cloned());
    }
    if !skill.resources.is_empty() {
        tools.extend(skill_resource_tools().into_iter().map(str::to_string));
    }
    tools.into_iter().collect()
}

fn has_mcp_dependency(skill: &SkillDescriptor) -> bool {
    skill
        .mcp_dependencies
        .iter()
        .any(|dependency| dependency.kind.eq_ignore_ascii_case("mcp"))
}

fn approval_for_tools(tools: &[String], starts_mcp: bool) -> ToolAccess {
    let contains = |names: &[&str]| tools.iter().any(|tool| names.contains(&tool.as_str()));
    ToolAccess {
        allow_workspace_write: contains(&[
            "write_file",
            "edit_file",
            "apply_patch",
            "restore_rollback",
        ]),
        allow_shell: starts_mcp
            || contains(&[
                "shell",
                "run_test",
                "run_build",
                "run_formatter",
                "run_skill_script",
            ]),
        allow_risky_shell: contains(&["run_skill_script"]),
        allow_git_write: contains(&["git_write", "git_commit", "git_apply"]),
        allow_network: contains(&["network", "web_search", "fetch_url"]),
    }
}

fn pave_for_skill(
    skill: &SkillDescriptor,
    tool_allowlist: &[String],
    approval: ToolAccess,
) -> PaveVector {
    let mut dims = BTreeMap::new();
    let text = format!(
        "{} {} {} {} {}",
        skill.id,
        skill.name,
        skill.description,
        skill.short_description.clone().unwrap_or_default(),
        tool_allowlist.join(" ")
    )
    .to_ascii_lowercase();

    add_keywords(
        &mut dims,
        &text,
        "rust",
        &["rust", "cargo", "borrow", "clippy", ".rs"],
        1.0,
    );
    add_keywords(
        &mut dims,
        &text,
        "javascript",
        &["javascript", "typescript", "node", "npm", ".js", ".ts"],
        0.85,
    );
    add_keywords(
        &mut dims,
        &text,
        "tests",
        &["test", "tests", "failing", "assert", "panic"],
        1.0,
    );
    add_keywords(
        &mut dims,
        &text,
        "diagnostics",
        &["diagnostic", "compiler", "error", "failure", "warning"],
        0.9,
    );
    add_keywords(
        &mut dims,
        &text,
        "explanation",
        &[
            "explain",
            "explainer",
            "inspect",
            "relationship",
            "structure",
        ],
        0.9,
    );
    add_keywords(
        &mut dims,
        &text,
        "docs",
        &["doc", "docs", "markdown", "readme", "reference"],
        0.65,
    );
    add_keywords(
        &mut dims,
        &text,
        "frontend",
        &["frontend", "vscode", "webview", "ui", "extension"],
        0.75,
    );
    add_keywords(
        &mut dims,
        &text,
        "infra",
        &["build", "ci", "mcp", "server", "runtime"],
        0.55,
    );

    if tool_allowlist.iter().any(|tool| {
        matches!(
            tool.as_str(),
            "write_file" | "edit_file" | "apply_patch" | "restore_rollback"
        )
    }) {
        dims.insert("patch".to_string(), 0.9);
        dims.entry("refactor".to_string()).or_insert(0.45);
    }
    if tool_allowlist.iter().any(|tool| {
        matches!(
            tool.as_str(),
            "run_test" | "run_build" | "run_formatter" | "shell"
        )
    }) {
        dims.entry("shell".to_string()).or_insert(0.45);
    }
    if tool_allowlist
        .iter()
        .any(|tool| matches!(tool.as_str(), "git_status" | "git_diff"))
    {
        dims.entry("git".to_string()).or_insert(0.35);
    }
    if approval.allow_workspace_write {
        dims.insert("risk_medium".to_string(), 0.55);
    } else {
        dims.insert("risk_low".to_string(), 0.55);
    }
    if dims.is_empty() {
        dims.insert("explanation".to_string(), 0.35);
        dims.insert("risk_low".to_string(), 0.35);
    }
    PaveVector { dims }
}

fn add_keywords(
    dims: &mut BTreeMap<String, f32>,
    text: &str,
    dimension: &str,
    keywords: &[&str],
    value: f32,
) {
    if keywords.iter().any(|keyword| text.contains(keyword)) {
        dims.insert(dimension.to_string(), value);
    }
}

fn read_only_tools() -> Vec<&'static str> {
    vec![
        "read_file",
        "read_many_files",
        "search_files",
        "find_files",
        "list_directory",
        "list_symbols",
        "git_status",
        "git_diff",
    ]
}

fn skill_resource_tools() -> Vec<&'static str> {
    vec!["list_skills", "list_skill_resources", "read_skill_resource"]
}

fn verification_tools() -> Vec<&'static str> {
    let mut tools = read_only_tools();
    tools.extend(["run_test", "run_build", "run_formatter"]);
    tools
}

fn patch_tools() -> Vec<&'static str> {
    let mut tools = verification_tools();
    tools.extend([
        "write_file",
        "edit_file",
        "apply_patch",
        "list_rollbacks",
        "restore_rollback",
    ]);
    tools
}

#[derive(Debug, Clone, Default)]
pub struct AgentSkillSelection {
    pub skills: Vec<SkillDescriptor>,
    pub local_tool_allowlist: Vec<String>,
    pub mcp_servers: Vec<McpServerConfig>,
    pub missing_skills: Vec<String>,
    pub missing_mcp_servers: Vec<String>,
}

impl AgentSkillSelection {
    pub fn ensure_available(&self) -> Result<()> {
        if !self.missing_skills.is_empty() {
            return Err(anyhow!(
                "selected agent references unknown skills: {}",
                self.missing_skills.join(", ")
            ));
        }
        if !self.missing_mcp_servers.is_empty() {
            return Err(anyhow!(
                "selected agent references unknown MCP servers: {}",
                self.missing_mcp_servers.join(", ")
            ));
        }
        Ok(())
    }

    pub fn render_skills_section(&self) -> Option<String> {
        render_selected_skills_section(&self.skills)
    }
}

impl SkillToolDependency {
    fn to_server_config(&self) -> Option<McpServerConfig> {
        if !self.kind.eq_ignore_ascii_case("mcp") {
            return None;
        }
        let transport = self
            .transport
            .as_deref()
            .unwrap_or("stdio")
            .to_ascii_lowercase();
        if transport != "stdio" {
            return None;
        }
        let command = self.command.as_ref()?.trim();
        if command.is_empty() {
            return None;
        }
        Some(McpServerConfig {
            name: self.value.clone(),
            transport: McpTransport::Stdio,
            command: command.to_string(),
            args: self.args.clone(),
            env: BTreeMap::new(),
            cwd: None,
            enabled: true,
            required: true,
            enabled_tools: Vec::new(),
            disabled_tools: Vec::new(),
            startup_timeout_secs: DEFAULT_MCP_STARTUP_TIMEOUT_SECS,
            tool_timeout_secs: DEFAULT_MCP_TOOL_TIMEOUT_SECS,
        })
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum McpTransport {
    #[default]
    Stdio,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpServerConfig {
    pub name: String,
    #[serde(default)]
    pub transport: McpTransport,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<PathBuf>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub enabled_tools: Vec<String>,
    #[serde(default)]
    pub disabled_tools: Vec<String>,
    #[serde(default = "default_startup_timeout")]
    pub startup_timeout_secs: u64,
    #[serde(default = "default_tool_timeout")]
    pub tool_timeout_secs: u64,
}

impl McpServerConfig {
    fn from_file(name: String, file: McpServerFile, base_dir: &Path) -> Result<Self> {
        let transport = file.transport.unwrap_or_default();
        let command = file
            .command
            .map(|command| command.trim().to_string())
            .filter(|command| !command.is_empty())
            .ok_or_else(|| anyhow!("stdio MCP server requires command"))?;
        let cwd = file.cwd.map(|cwd| {
            if cwd.is_absolute() {
                cwd
            } else {
                base_dir.join(cwd)
            }
        });
        Ok(Self {
            name,
            transport,
            command,
            args: file.args.unwrap_or_default(),
            env: file.env.unwrap_or_default(),
            cwd,
            enabled: file.enabled.unwrap_or(true),
            required: file.required.unwrap_or(false),
            enabled_tools: file.enabled_tools.unwrap_or_default(),
            disabled_tools: file.disabled_tools.unwrap_or_default(),
            startup_timeout_secs: file
                .startup_timeout_secs
                .or(file.startup_timeout_sec)
                .unwrap_or(DEFAULT_MCP_STARTUP_TIMEOUT_SECS),
            tool_timeout_secs: file
                .tool_timeout_secs
                .or(file.tool_timeout_sec)
                .unwrap_or(DEFAULT_MCP_TOOL_TIMEOUT_SECS),
        })
    }

    fn allows_tool(&self, name: &str) -> bool {
        (self.enabled_tools.is_empty() || self.enabled_tools.iter().any(|tool| tool == name))
            && !self.disabled_tools.iter().any(|tool| tool == name)
    }
}

#[derive(Debug, Clone, Default)]
pub struct McpToolRuntime {
    tools: BTreeMap<String, McpToolBinding>,
    servers: BTreeMap<String, McpServerConfig>,
}

impl McpToolRuntime {
    pub async fn discover(workspace_root: &Path, servers: Vec<McpServerConfig>) -> Result<Self> {
        if servers.is_empty() {
            return Ok(Self::default());
        }

        let mut runtime = Self::default();
        let mut used_names = BTreeSet::new();
        for server in servers {
            if !server.enabled {
                if server.required {
                    return Err(anyhow!(
                        "required MCP server `{}` is disabled in configuration",
                        server.name
                    ));
                }
                continue;
            }
            let result = list_mcp_tools(workspace_root, &server).await;
            let tools = match result {
                Ok(tools) => tools,
                Err(err) if server.required => {
                    return Err(anyhow!(
                        "required MCP server `{}` failed during tool discovery: {err}",
                        server.name
                    ));
                }
                Err(_) => continue,
            };
            let mut exposed_count = 0usize;
            for tool in tools {
                if !server.allows_tool(&tool.name) {
                    continue;
                }
                let qualified_name =
                    qualify_mcp_tool_name(&server.name, &tool.name, &mut used_names);
                runtime.tools.insert(
                    qualified_name.clone(),
                    McpToolBinding {
                        qualified_name,
                        server_name: server.name.clone(),
                        raw_tool_name: tool.name,
                        description: tool.description,
                        input_schema: tool.input_schema,
                    },
                );
                exposed_count += 1;
            }
            if server.required && exposed_count == 0 {
                return Err(anyhow!(
                    "required MCP server `{}` exposed no usable tools",
                    server.name
                ));
            }
            runtime.servers.insert(server.name.clone(), server);
        }
        Ok(runtime)
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    pub fn is_mcp_tool_name(name: &str) -> bool {
        name.starts_with(&format!("{MCP_TOOL_NAME_PREFIX}{MCP_TOOL_NAME_DELIMITER}"))
    }

    pub fn tool_specs(&self) -> Vec<DynamicToolSpec> {
        self.tools
            .values()
            .map(|binding| DynamicToolSpec {
                name: binding.qualified_name.clone(),
                description: format!(
                    "MCP tool `{}` from server `{}`. {}",
                    binding.raw_tool_name,
                    binding.server_name,
                    binding
                        .description
                        .as_deref()
                        .unwrap_or("No description provided.")
                ),
                parameters: binding
                    .input_schema
                    .clone()
                    .unwrap_or_else(|| json!({"type": "object", "properties": {}, "required": []})),
            })
            .collect()
    }

    pub async fn call_tool(
        &self,
        name: &str,
        arguments: &Value,
        workspace_root: &Path,
    ) -> Result<String> {
        let binding = self
            .tools
            .get(name)
            .ok_or_else(|| anyhow!("unknown MCP tool: {name}"))?;
        let server = self
            .servers
            .get(&binding.server_name)
            .ok_or_else(|| anyhow!("missing MCP server config: {}", binding.server_name))?;
        let result = call_mcp_tool(
            workspace_root,
            server,
            &binding.raw_tool_name,
            arguments.clone(),
        )
        .await?;
        Ok(format_mcp_call_result(result))
    }
}

#[derive(Debug, Clone)]
struct McpToolBinding {
    qualified_name: String,
    server_name: String,
    raw_tool_name: String,
    description: Option<String>,
    input_schema: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
struct McpConfigFile {
    #[serde(default, alias = "mcpServers")]
    mcp_servers: BTreeMap<String, McpServerFile>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct McpServerFile {
    #[serde(default)]
    transport: Option<McpTransport>,
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    args: Option<Vec<String>>,
    #[serde(default)]
    env: Option<BTreeMap<String, String>>,
    #[serde(default)]
    cwd: Option<PathBuf>,
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    required: Option<bool>,
    #[serde(default)]
    enabled_tools: Option<Vec<String>>,
    #[serde(default)]
    disabled_tools: Option<Vec<String>>,
    #[serde(default)]
    startup_timeout_secs: Option<u64>,
    #[serde(default)]
    startup_timeout_sec: Option<u64>,
    #[serde(default)]
    tool_timeout_secs: Option<u64>,
    #[serde(default)]
    tool_timeout_sec: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
struct JsonRpcResponse {
    id: Option<Value>,
    result: Option<Value>,
    error: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
struct ToolsListResult {
    #[serde(default)]
    tools: Vec<McpToolDefinition>,
}

#[derive(Debug, Clone, Deserialize)]
struct McpToolDefinition {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default, alias = "inputSchema")]
    input_schema: Option<Value>,
}

fn default_true() -> bool {
    true
}

fn default_startup_timeout() -> u64 {
    DEFAULT_MCP_STARTUP_TIMEOUT_SECS
}

fn default_tool_timeout() -> u64 {
    DEFAULT_MCP_TOOL_TIMEOUT_SECS
}

fn sanitize_responses_api_tool_name(name: &str) -> String {
    let mut sanitized = String::with_capacity(name.len());
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            sanitized.push(ch);
        } else {
            sanitized.push('_');
        }
    }
    if sanitized.is_empty() {
        "_".to_string()
    } else {
        sanitized
    }
}

fn qualified_mcp_tool_name_prefix(server_name: &str) -> String {
    sanitize_responses_api_tool_name(&format!(
        "{MCP_TOOL_NAME_PREFIX}{MCP_TOOL_NAME_DELIMITER}{server_name}{MCP_TOOL_NAME_DELIMITER}"
    ))
}

fn qualify_mcp_tool_name(
    server_name: &str,
    tool_name: &str,
    used_names: &mut BTreeSet<String>,
) -> String {
    let raw_identity = format!("{server_name}\0{tool_name}");
    let mut candidate = format!(
        "{}{}",
        qualified_mcp_tool_name_prefix(server_name),
        sanitize_responses_api_tool_name(tool_name)
    );
    if candidate.len() > MAX_TOOL_NAME_LENGTH {
        candidate = with_hash_suffix(&candidate, &raw_identity, 0);
    }
    let mut attempt = 0_u32;
    while used_names.contains(&candidate) {
        attempt = attempt.saturating_add(1);
        candidate = with_hash_suffix(&candidate, &raw_identity, attempt);
    }
    used_names.insert(candidate.clone());
    candidate
}

fn with_hash_suffix(value: &str, raw_identity: &str, attempt: u32) -> String {
    let suffix = format!(
        "_{:012x}",
        fnv1a_64(&format!("{raw_identity}\0{attempt}")) & 0x0000_ffff_ffff_ffff
    );
    let keep = MAX_TOOL_NAME_LENGTH.saturating_sub(suffix.len());
    format!("{}{}", value.chars().take(keep).collect::<String>(), suffix)
}

fn fnv1a_64(value: &str) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in value.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    hash
}

async fn list_mcp_tools(
    workspace_root: &Path,
    server: &McpServerConfig,
) -> Result<Vec<McpToolDefinition>> {
    let result = stdio_mcp_request(
        workspace_root,
        server,
        "tools/list",
        json!({}),
        Duration::from_secs(server.startup_timeout_secs.max(1)),
    )
    .await?;
    let result: ToolsListResult = serde_json::from_value(result).with_context(|| {
        format!(
            "invalid tools/list result from MCP server `{}`",
            server.name
        )
    })?;
    Ok(result.tools)
}

async fn call_mcp_tool(
    workspace_root: &Path,
    server: &McpServerConfig,
    tool_name: &str,
    arguments: Value,
) -> Result<Value> {
    stdio_mcp_request(
        workspace_root,
        server,
        "tools/call",
        json!({
            "name": tool_name,
            "arguments": arguments,
        }),
        Duration::from_secs(server.tool_timeout_secs.max(1)),
    )
    .await
}

async fn stdio_mcp_request(
    workspace_root: &Path,
    server: &McpServerConfig,
    method: &str,
    params: Value,
    request_timeout: Duration,
) -> Result<Value> {
    match server.transport {
        McpTransport::Stdio => {}
    }
    let mut child = spawn_mcp_server(workspace_root, server)?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("MCP server `{}` stdout was not piped", server.name))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow!("MCP server `{}` stdin was not piped", server.name))?;
    let mut lines = BufReader::new(stdout).lines();

    write_json_line(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "marvis",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }
        }),
    )
    .await?;
    let _ = read_json_rpc_response(&mut lines, 1, request_timeout).await?;
    write_json_line(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {}
        }),
    )
    .await?;
    write_json_line(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": method,
            "params": params,
        }),
    )
    .await?;
    let response = read_json_rpc_response(&mut lines, 2, request_timeout).await;
    cleanup_child(&mut child).await;
    response
}

fn spawn_mcp_server(workspace_root: &Path, server: &McpServerConfig) -> Result<Child> {
    let cwd = resolve_mcp_cwd(workspace_root, server)?;
    let mut command = Command::new(&server.command);
    command
        .args(&server.args)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    for (key, value) in &server.env {
        command.env(key, value);
    }
    command.spawn().with_context(|| {
        format!(
            "spawn MCP server `{}` command `{}`",
            server.name, server.command
        )
    })
}

fn resolve_mcp_cwd(workspace_root: &Path, server: &McpServerConfig) -> Result<PathBuf> {
    let cwd = match &server.cwd {
        Some(path) if path.is_absolute() => path.clone(),
        Some(path) => workspace_root.join(path),
        None => workspace_root.to_path_buf(),
    };
    if path_contains_parent_dir(&cwd) {
        return Err(anyhow!("MCP cwd must not contain '..': {}", cwd.display()));
    }
    let workspace_root = workspace_root
        .canonicalize()
        .with_context(|| format!("canonicalize workspace root {}", workspace_root.display()))?;
    let cwd = cwd
        .canonicalize()
        .with_context(|| format!("canonicalize MCP cwd {}", cwd.display()))?;
    if !cwd.starts_with(&workspace_root) {
        return Err(anyhow!(
            "MCP cwd must stay inside workspace {}: {}",
            workspace_root.display(),
            cwd.display()
        ));
    }
    Ok(cwd)
}

fn path_contains_parent_dir(path: &Path) -> bool {
    path.components()
        .any(|component| matches!(component, Component::ParentDir))
}

async fn write_json_line(stdin: &mut tokio::process::ChildStdin, value: &Value) -> Result<()> {
    let line = serde_json::to_string(value)?;
    stdin.write_all(line.as_bytes()).await?;
    stdin.write_all(b"\n").await?;
    stdin.flush().await?;
    Ok(())
}

async fn read_json_rpc_response(
    lines: &mut Lines<BufReader<ChildStdout>>,
    id: u64,
    request_timeout: Duration,
) -> Result<Value> {
    loop {
        let line = timeout(request_timeout, lines.next_line())
            .await
            .map_err(|_| {
                anyhow!(
                    "MCP request timed out after {} seconds",
                    request_timeout.as_secs()
                )
            })??;
        let Some(line) = line else {
            return Err(anyhow!("MCP server closed stdout before response {id}"));
        };
        let parsed: JsonRpcResponse = match serde_json::from_str(&line) {
            Ok(parsed) => parsed,
            Err(_) => continue,
        };
        if parsed.id.as_ref().and_then(Value::as_u64) != Some(id) {
            continue;
        }
        if let Some(error) = parsed.error {
            return Err(anyhow!("MCP error response: {error}"));
        }
        return parsed
            .result
            .ok_or_else(|| anyhow!("MCP response {id} did not include result"));
    }
}

async fn cleanup_child(child: &mut Child) {
    let _ = child.kill().await;
    let _ = child.wait().await;
}

fn format_mcp_call_result(value: Value) -> String {
    let is_error = value
        .get("isError")
        .or_else(|| value.get("is_error"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut parts = Vec::new();
    if let Some(content) = value.get("content").and_then(Value::as_array) {
        for item in content {
            if let Some(text) = item.get("text").and_then(Value::as_str) {
                parts.push(text.to_string());
            } else {
                parts.push(item.to_string());
            }
        }
    }
    if parts.is_empty() {
        parts.push(value.to_string());
    }
    let text = parts.join("\n");
    if is_error {
        format!("MCP tool returned error:\n{text}")
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qualifies_mcp_tool_names_with_prefix_and_length_cap() {
        let mut used = BTreeSet::new();
        let name = qualify_mcp_tool_name(
            "server-with-dashes",
            "tool with spaces and a very very very very very very long name",
            &mut used,
        );
        assert!(name.starts_with("mcp__server_with_dashes__"));
        assert!(name.len() <= MAX_TOOL_NAME_LENGTH);
        let second = qualify_mcp_tool_name(
            "server-with-dashes",
            "tool with spaces and a very very very very very very long name",
            &mut used,
        );
        assert_ne!(name, second);
    }

    #[test]
    fn parses_workspace_skill_with_yaml_metadata() {
        let root = std::env::temp_dir().join(format!("marvis-skill-test-{}", std::process::id()));
        let skill_dir = root.join(".marvis/skills/rust");
        std::fs::create_dir_all(skill_dir.join("agents")).unwrap();
        std::fs::write(
            skill_dir.join(crate::skills::SKILL_FILE_NAME),
            "---\nname: Rust Helper\ndescription: Helps Rust work.\n---\nUse cargo carefully.\n",
        )
        .unwrap();
        std::fs::write(
            skill_dir.join("agents/openai.yaml"),
            "capabilities:\n  - read_file\n  - run_build\ndependencies:\n  tools:\n    - type: mcp\n      value: docs\n      transport: stdio\n      command: marvis-docs\n",
        )
        .unwrap();

        let registry = SkillRegistry::load(&root);
        let skill = registry.skills.get("rust-helper").unwrap();
        assert_eq!(skill.local_tools, vec!["read_file", "run_build"]);
        assert_eq!(skill.mcp_dependencies[0].value, "docs");
        assert!(registry.mcp_servers.contains_key("docs"));
    }

    #[test]
    fn auto_agent_profiles_are_generated_from_discovered_skills_and_tools() {
        let root =
            std::env::temp_dir().join(format!("marvis-auto-agent-test-{}", std::process::id()));
        let skill_dir = root.join(".marvis/skills/frontend-helper");
        std::fs::create_dir_all(skill_dir.join("agents")).unwrap();
        std::fs::create_dir_all(skill_dir.join("references")).unwrap();
        std::fs::write(
            skill_dir.join(crate::skills::SKILL_FILE_NAME),
            "---\nname: Frontend Helper\ndescription: Fix VSCode webview frontend issues.\n---\nUse UI context carefully.\n",
        )
        .unwrap();
        std::fs::write(
            skill_dir.join("agents/openai.yaml"),
            "capabilities:\n  - read_file\n  - apply_patch\n  - run_build\ninterface:\n  display_name: Frontend Helper Agent\npolicy:\n  allow_implicit_invocation: true\n",
        )
        .unwrap();
        std::fs::write(skill_dir.join("references/guide.md"), "Frontend guide.").unwrap();

        let registry = SkillRegistry::load(&root);
        let profiles = registry.auto_agent_profiles("same-model");
        let profile = profiles
            .iter()
            .find(|profile| profile.id == "frontend-helper")
            .expect("workspace skill should become an agent");

        assert_eq!(profile.model, "same-model");
        assert_eq!(profile.skills, vec!["frontend-helper"]);
        assert_eq!(profile.label, "Frontend Helper Agent");
        assert!(profile.tool_allowlist.contains(&"apply_patch".to_string()));
        assert!(
            profile
                .tool_allowlist
                .contains(&"read_skill_resource".to_string())
        );
        assert!(profile.default_approval.allow_workspace_write);
        assert!(profile.default_approval.allow_shell);
        assert!(profile.pave.dims.contains_key("frontend"));
        assert!(
            profiles
                .iter()
                .any(|profile| profile.id == "tools-workspace-patch"
                    && profile.model == "same-model")
        );
    }

    #[test]
    fn auto_agent_profiles_skip_skills_that_disallow_implicit_invocation() {
        let root = std::env::temp_dir().join(format!(
            "marvis-auto-agent-opt-out-test-{}",
            std::process::id()
        ));
        let skill_dir = root.join(".marvis/skills/manual-only");
        std::fs::create_dir_all(skill_dir.join("agents")).unwrap();
        std::fs::write(
            skill_dir.join(crate::skills::SKILL_FILE_NAME),
            "---\nname: manual-only\ndescription: Manual only.\n---\nUse only when explicitly selected.\n",
        )
        .unwrap();
        std::fs::write(
            skill_dir.join("agents/openai.yaml"),
            "policy:\n  allow_implicit_invocation: false\n",
        )
        .unwrap();

        let registry = SkillRegistry::load(&root);
        let profiles = registry.auto_agent_profiles("same-model");
        assert!(!profiles.iter().any(|profile| profile.id == "manual-only"));
    }

    #[test]
    fn bundled_imported_skills_become_auto_agent_profiles() {
        let root = std::env::temp_dir().join(format!(
            "marvis-bundled-imported-agent-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        let registry = SkillRegistry::load(&root);
        assert!(
            registry.errors().is_empty(),
            "skill load errors: {:?}",
            registry.errors()
        );
        let profiles = registry.auto_agent_profiles("shared-model");

        let frontend = profiles
            .iter()
            .find(|profile| profile.id == "frontend-design")
            .expect("frontend-design should be auto-routable");
        assert_eq!(frontend.model, "shared-model");
        assert!(frontend.skills.contains(&"frontend-design".to_string()));
        assert!(frontend.tool_allowlist.contains(&"apply_patch".to_string()));
        assert!(frontend.default_approval.allow_workspace_write);

        let docx = profiles
            .iter()
            .find(|profile| profile.id == "docx")
            .expect("docx should be auto-routable");
        assert!(
            docx.tool_allowlist
                .contains(&"run_skill_script".to_string())
        );
        assert!(docx.default_approval.allow_risky_shell);

        let transplant = profiles
            .iter()
            .find(|profile| profile.id == "transplant-code")
            .expect("transplant-code should be auto-routable");
        assert_eq!(transplant.label, "Transplant Code");
    }

    #[test]
    fn mcp_cwd_must_stay_inside_workspace() {
        let root = std::env::temp_dir().join(format!("marvis-mcp-cwd-test-{}", std::process::id()));
        let inside = root.join("tools");
        let outside =
            std::env::temp_dir().join(format!("marvis-mcp-cwd-outside-{}", std::process::id()));
        std::fs::create_dir_all(&inside).unwrap();
        std::fs::create_dir_all(&outside).unwrap();

        let mut server = McpServerConfig {
            name: "docs".to_string(),
            transport: McpTransport::Stdio,
            command: "docs".to_string(),
            args: Vec::new(),
            env: BTreeMap::new(),
            cwd: Some(PathBuf::from("tools")),
            enabled: true,
            required: true,
            enabled_tools: Vec::new(),
            disabled_tools: Vec::new(),
            startup_timeout_secs: DEFAULT_MCP_STARTUP_TIMEOUT_SECS,
            tool_timeout_secs: DEFAULT_MCP_TOOL_TIMEOUT_SECS,
        };
        assert!(resolve_mcp_cwd(&root, &server).unwrap().ends_with("tools"));

        server.cwd = Some(outside);
        assert!(resolve_mcp_cwd(&root, &server).is_err());
    }

    #[tokio::test]
    async fn required_disabled_mcp_server_fails_closed() {
        let root =
            std::env::temp_dir().join(format!("marvis-mcp-disabled-test-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let server = McpServerConfig {
            name: "docs".to_string(),
            transport: McpTransport::Stdio,
            command: "does-not-run".to_string(),
            args: Vec::new(),
            env: BTreeMap::new(),
            cwd: None,
            enabled: false,
            required: true,
            enabled_tools: Vec::new(),
            disabled_tools: Vec::new(),
            startup_timeout_secs: DEFAULT_MCP_STARTUP_TIMEOUT_SECS,
            tool_timeout_secs: DEFAULT_MCP_TOOL_TIMEOUT_SECS,
        };

        let err = McpToolRuntime::discover(&root, vec![server])
            .await
            .unwrap_err();
        assert!(err.to_string().contains("disabled"));
    }

    #[test]
    fn resolves_agent_tools_as_profile_capped_skill_local_tools() {
        let registry = SkillRegistry::load(Path::new("/definitely/not/a/workspace"));
        let agent = AgentProfile {
            id: "agent".to_string(),
            label: "Agent".to_string(),
            model: "model".to_string(),
            skill_prompt: String::new(),
            skills: vec!["rust-diagnostic-repair".to_string()],
            mcp_servers: Vec::new(),
            tool_allowlist: vec!["read_file".to_string(), "apply_patch".to_string()],
            pave: Default::default(),
            default_approval: Default::default(),
        };
        let selection = registry.resolve_agent(&agent);
        assert_eq!(
            selection.local_tool_allowlist,
            vec!["apply_patch", "read_file"]
        );
    }
}
