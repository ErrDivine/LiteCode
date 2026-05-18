use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use pave_router::AgentProfile;
use protocol::DynamicToolSpec;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdout, Command};
use tokio::time::timeout;

const SKILL_FILE_NAME: &str = "SKILL.md";
const MAX_SKILL_SCAN_DEPTH: usize = 6;
const MAX_TOOL_NAME_LENGTH: usize = 64;
const MCP_TOOL_NAME_PREFIX: &str = "mcp";
const MCP_TOOL_NAME_DELIMITER: &str = "__";
const DEFAULT_MCP_STARTUP_TIMEOUT_SECS: u64 = 30;
const DEFAULT_MCP_TOOL_TIMEOUT_SECS: u64 = 120;

#[derive(Debug, Clone, Default)]
pub struct SkillRegistry {
    skills: BTreeMap<String, SkillDescriptor>,
    mcp_servers: BTreeMap<String, McpServerConfig>,
    errors: Vec<String>,
}

impl SkillRegistry {
    pub fn load(workspace_root: &Path) -> Self {
        let mut registry = Self::default();
        for skill in built_in_skills() {
            registry.skills.insert(skill.id.clone(), skill);
        }
        registry.load_mcp_config(&workspace_root.join(".marvis/mcp.json"));
        registry.load_mcp_config(&workspace_root.join(".mcp.json"));
        registry.load_skills_root(&workspace_root.join(".marvis/skills"));
        registry.load_skills_root(&workspace_root.join(".agents/skills"));
        registry.merge_skill_mcp_dependencies();
        registry
    }

    pub fn errors(&self) -> &[String] {
        &self.errors
    }

    pub fn resolve_agent(&self, agent: &AgentProfile) -> AgentSkillSelection {
        let mut skills = Vec::new();
        let mut missing_skills = Vec::new();
        let mut skill_capabilities = BTreeSet::new();
        let mut mcp_server_names = BTreeSet::new();

        for skill_id in &agent.skills {
            match self.skills.get(skill_id) {
                Some(skill) => {
                    skill_capabilities.extend(skill.capabilities.iter().cloned());
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
            match (profile_allowlist.is_empty(), skill_capabilities.is_empty()) {
                (true, true) => Vec::new(),
                (true, false) => skill_capabilities.into_iter().collect(),
                (false, true) => profile_allowlist.into_iter().collect(),
                (false, false) => profile_allowlist
                    .intersection(&skill_capabilities)
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

    fn load_skills_root(&mut self, root: &Path) {
        if !root.is_dir() {
            return;
        }
        self.discover_skills(root, 0);
    }

    fn discover_skills(&mut self, dir: &Path, depth: usize) {
        if depth > MAX_SKILL_SCAN_DEPTH {
            return;
        }
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(err) => {
                self.errors.push(format!(
                    "failed to read skills dir {}: {err}",
                    dir.display()
                ));
                return;
            }
        };
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(err) => {
                    self.errors.push(format!(
                        "failed to read skill entry in {}: {err}",
                        dir.display()
                    ));
                    continue;
                }
            };
            let path = entry.path();
            let file_name = entry.file_name();
            if file_name.to_string_lossy().starts_with('.') {
                continue;
            }
            if path.is_dir() {
                self.discover_skills(&path, depth + 1);
            } else if path.is_file() && file_name == SKILL_FILE_NAME {
                match parse_skill_file(&path) {
                    Ok(skill) => {
                        self.skills.insert(skill.id.clone(), skill);
                    }
                    Err(err) => self
                        .errors
                        .push(format!("failed to load skill {}: {err}", path.display())),
                }
            }
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
        if self.skills.is_empty() {
            return None;
        }
        let mut out = String::from("Selected Marvis skills:\n");
        for skill in &self.skills {
            out.push_str("<skill>\n");
            out.push_str(&format!("<name>{}</name>\n", skill.id));
            if let Some(path) = &skill.path {
                out.push_str(&format!("<path>{}</path>\n", path.display()));
            }
            out.push_str(&skill.body);
            if !skill.body.ends_with('\n') {
                out.push('\n');
            }
            out.push_str("</skill>\n");
        }
        Some(out)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillDescriptor {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    pub body: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub mcp_dependencies: Vec<SkillToolDependency>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillToolDependency {
    #[serde(rename = "type")]
    pub kind: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
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
        for server in servers.into_iter().filter(|server| server.enabled) {
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

#[derive(Debug, Clone, Default, Deserialize)]
struct SkillMetadataFile {
    #[serde(default)]
    capabilities: Vec<String>,
    #[serde(default)]
    dependencies: SkillDependenciesFile,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct SkillDependenciesFile {
    #[serde(default)]
    tools: Vec<SkillToolDependency>,
}

#[derive(Debug, Clone)]
struct SkillFrontmatter {
    id: Option<String>,
    name: Option<String>,
    description: Option<String>,
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

fn built_in_skills() -> Vec<SkillDescriptor> {
    vec![
        SkillDescriptor {
            id: "rust-diagnostic-repair".to_string(),
            name: "Rust Diagnostic Repair".to_string(),
            description: "Repair Rust compiler diagnostics and failing tests with small verified patches.".to_string(),
            path: None,
            body: "Focus on Rust diagnostics, borrow checker messages, failing tests, and small localized patches. Prefer read_file/search_files before editing, apply_patch for edits, and run_build/run_test for verification.".to_string(),
            capabilities: vec![
                "read_file",
                "search_files",
                "find_files",
                "list_directory",
                "apply_patch",
                "run_test",
                "run_build",
                "git_diff",
                "git_status",
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
            mcp_dependencies: Vec::new(),
        },
        SkillDescriptor {
            id: "test-failure-triage".to_string(),
            name: "Test Failure Triage".to_string(),
            description: "Inspect failing behavior and choose the smallest useful verification path.".to_string(),
            path: None,
            body: "Focus on recent command failures, test output, and the smallest reproducible check. Avoid edits unless the user or routed profile explicitly grants workspace writes.".to_string(),
            capabilities: vec![
                "read_file",
                "search_files",
                "find_files",
                "list_directory",
                "run_test",
                "git_diff",
                "git_status",
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
            mcp_dependencies: Vec::new(),
        },
        SkillDescriptor {
            id: "repo-explainer".to_string(),
            name: "Repo Explainer".to_string(),
            description: "Explain repository structure and code relationships without editing.".to_string(),
            path: None,
            body: "Read the relevant files, summarize concrete relationships, and avoid edits or shell commands unless the user changes the task.".to_string(),
            capabilities: vec![
                "read_file",
                "search_files",
                "find_files",
                "list_directory",
                "git_diff",
                "git_status",
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
            mcp_dependencies: Vec::new(),
        },
    ]
}

fn parse_skill_file(path: &Path) -> Result<SkillDescriptor> {
    let body = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let (frontmatter, _body_without_frontmatter) =
        extract_frontmatter(&body).ok_or_else(|| anyhow!("missing YAML frontmatter"))?;
    let frontmatter = parse_skill_frontmatter(frontmatter)?;
    let name = frontmatter
        .name
        .clone()
        .or_else(|| {
            path.parent()
                .and_then(Path::file_name)
                .map(|name| name.to_string_lossy().to_string())
        })
        .ok_or_else(|| anyhow!("missing skill name"))?;
    let id = frontmatter
        .id
        .clone()
        .unwrap_or_else(|| normalize_identifier(&name));
    let description = frontmatter
        .description
        .clone()
        .ok_or_else(|| anyhow!("missing skill description"))?;
    let metadata = load_skill_metadata(path)?;
    Ok(SkillDescriptor {
        id,
        name,
        description,
        path: Some(path.to_path_buf()),
        body,
        capabilities: normalize_string_list(metadata.capabilities),
        mcp_dependencies: metadata.dependencies.tools,
    })
}

fn load_skill_metadata(skill_path: &Path) -> Result<SkillMetadataFile> {
    let Some(skill_dir) = skill_path.parent() else {
        return Ok(SkillMetadataFile::default());
    };
    let json_path = skill_dir.join("agents/openai.json");
    if json_path.is_file() {
        let contents = std::fs::read_to_string(&json_path)
            .with_context(|| format!("read {}", json_path.display()))?;
        let parsed = serde_json::from_str::<SkillMetadataFile>(&contents)
            .with_context(|| format!("parse {}", json_path.display()))?;
        return Ok(parsed);
    }
    let yaml_path = skill_dir.join("agents/openai.yaml");
    if yaml_path.is_file() {
        let contents = std::fs::read_to_string(&yaml_path)
            .with_context(|| format!("read {}", yaml_path.display()))?;
        return Ok(parse_skill_metadata_yaml(&contents));
    }
    Ok(SkillMetadataFile::default())
}

fn extract_frontmatter(contents: &str) -> Option<(&str, &str)> {
    let rest = contents.strip_prefix("---\n")?;
    let closing = rest.find("\n---")?;
    let frontmatter = &rest[..closing];
    let body = rest[closing + "\n---".len()..]
        .strip_prefix('\n')
        .unwrap_or(&rest[closing + "\n---".len()..]);
    Some((frontmatter, body))
}

fn parse_skill_frontmatter(frontmatter: &str) -> Result<SkillFrontmatter> {
    let mut parsed = SkillFrontmatter {
        id: None,
        name: None,
        description: None,
    };
    for line in frontmatter.lines() {
        let Some((key, value)) = split_key_value(line) else {
            continue;
        };
        match key {
            "id" => parsed.id = Some(value),
            "name" => parsed.name = Some(value),
            "description" => parsed.description = Some(value),
            _ => {}
        }
    }
    Ok(parsed)
}

fn parse_skill_metadata_yaml(contents: &str) -> SkillMetadataFile {
    let mut metadata = SkillMetadataFile::default();
    let mut in_capabilities = false;
    let mut in_tools = false;
    let mut current_tool: Option<SkillToolDependency> = None;

    for raw in contents.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "capabilities:" {
            in_capabilities = true;
            in_tools = false;
            continue;
        }
        if line == "dependencies:" {
            in_capabilities = false;
            continue;
        }
        if line == "tools:" {
            in_tools = true;
            in_capabilities = false;
            continue;
        }
        if in_capabilities && line.starts_with("- ") {
            metadata.capabilities.push(parse_scalar(&line[2..]));
            continue;
        }
        if in_tools && line.starts_with("- ") {
            if let Some(tool) = current_tool.take() {
                metadata.dependencies.tools.push(tool);
            }
            let mut tool = SkillToolDependency {
                kind: String::new(),
                value: String::new(),
                description: None,
                transport: None,
                command: None,
                args: Vec::new(),
                url: None,
            };
            if let Some((key, value)) = split_key_value(&line[2..]) {
                assign_dependency_field(&mut tool, key, value);
            }
            current_tool = Some(tool);
            continue;
        }
        if in_tools
            && let Some(tool) = current_tool.as_mut()
            && let Some((key, value)) = split_key_value(line)
        {
            assign_dependency_field(tool, key, value);
        }
    }
    if let Some(tool) = current_tool.take() {
        metadata.dependencies.tools.push(tool);
    }
    metadata
        .dependencies
        .tools
        .retain(|tool| !tool.kind.trim().is_empty() && !tool.value.trim().is_empty());
    metadata
}

fn split_key_value(line: &str) -> Option<(&str, String)> {
    let line = line.trim();
    let (key, value) = line.split_once(':')?;
    Some((key.trim(), parse_scalar(value.trim())))
}

fn parse_scalar(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim()
        .to_string()
}

fn parse_array(value: &str) -> Vec<String> {
    let value = value.trim();
    if !value.starts_with('[') || !value.ends_with(']') {
        return Vec::new();
    }
    value[1..value.len() - 1]
        .split(',')
        .map(parse_scalar)
        .filter(|value| !value.is_empty())
        .collect()
}

fn assign_dependency_field(tool: &mut SkillToolDependency, key: &str, value: String) {
    match key {
        "type" => tool.kind = value,
        "value" => tool.value = value,
        "description" => tool.description = Some(value),
        "transport" => tool.transport = Some(value),
        "command" => tool.command = Some(value),
        "args" => tool.args = parse_array(&value),
        "url" => tool.url = Some(value),
        _ => {}
    }
}

fn normalize_identifier(value: &str) -> String {
    let mut out = String::new();
    let mut previous_dash = false;
    for ch in value.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            previous_dash = false;
        } else if !previous_dash {
            out.push('-');
            previous_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

fn normalize_string_list(values: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .filter(|value| seen.insert(value.clone()))
        .collect()
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
            skill_dir.join(SKILL_FILE_NAME),
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
        assert_eq!(skill.capabilities, vec!["read_file", "run_build"]);
        assert_eq!(skill.mcp_dependencies[0].value, "docs");
        assert!(registry.mcp_servers.contains_key("docs"));
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

    #[test]
    fn resolves_agent_tools_as_profile_capped_skill_capabilities() {
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
