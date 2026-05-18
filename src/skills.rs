use std::collections::{BTreeSet, VecDeque};
use std::fmt;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};

pub const SKILL_FILE_NAME: &str = "SKILL.md";

const AGENTS_DIR_NAME: &str = "agents";
const OPENAI_METADATA_FILE_NAME: &str = "openai.yaml";
const OPENAI_METADATA_JSON_FILE_NAME: &str = "openai.json";
const MAX_SCAN_DEPTH: usize = 6;
const MAX_SKILL_DIRS_PER_ROOT: usize = 2000;
const MAX_RESOURCE_DEPTH: usize = 6;
const MAX_RESOURCES_PER_SKILL: usize = 512;
const MAX_NAME_LEN: usize = 64;
const MAX_DESCRIPTION_LEN: usize = 1024;
const MAX_DEPENDENCY_VALUE_LEN: usize = 1024;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum SkillScope {
    System,
    Workspace,
}

impl fmt::Display for SkillScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::System => f.write_str("system"),
            Self::Workspace => f.write_str("workspace"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillDescriptor {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub short_description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interface: Option<SkillInterface>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<SkillPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    pub package_root: PathBuf,
    pub scope: SkillScope,
    pub body: String,
    #[serde(default)]
    pub local_tools: Vec<String>,
    #[serde(default)]
    pub mcp_dependencies: Vec<SkillToolDependency>,
    #[serde(default)]
    pub resources: Vec<SkillResource>,
}

impl SkillDescriptor {
    pub fn resource_path(&self, relative_path: &str) -> Result<PathBuf> {
        let relative = normalize_relative_path(relative_path)?;
        let allowed = self
            .resources
            .iter()
            .any(|resource| resource.relative_path == relative);
        if !allowed {
            return Err(anyhow!(
                "resource `{}` is not declared in skill `{}`",
                relative.display(),
                self.id
            ));
        }
        let candidate = self.package_root.join(relative);
        ensure_child_path(&self.package_root, &candidate)?;
        Ok(candidate)
    }

    pub fn script_path(&self, relative_path: &str) -> Result<PathBuf> {
        let relative = normalize_relative_path(relative_path)?;
        let allowed = self.resources.iter().any(|resource| {
            resource.kind == SkillResourceKind::Script && resource.relative_path == relative
        });
        if !allowed {
            return Err(anyhow!(
                "script `{}` is not declared in skill `{}`",
                relative.display(),
                self.id
            ));
        }
        let candidate = self.package_root.join(relative);
        ensure_child_path(&self.package_root, &candidate)?;
        Ok(candidate)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillPolicy {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_implicit_invocation: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillInterface {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub short_description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_small: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_large: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brand_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_prompt: Option<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillResource {
    pub kind: SkillResourceKind,
    pub relative_path: PathBuf,
    pub bytes: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SkillResourceKind {
    Script,
    Reference,
    Asset,
}

#[derive(Debug, Default)]
pub struct SkillLoadOutcome {
    pub skills: Vec<SkillDescriptor>,
    pub errors: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct SkillFrontmatter {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    metadata: SkillFrontmatterMetadata,
}

#[derive(Debug, Default, Deserialize)]
struct SkillFrontmatterMetadata {
    #[serde(default, rename = "short-description")]
    short_description: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct SkillMetadataFile {
    #[serde(default, alias = "tool_capabilities")]
    capabilities: Vec<String>,
    #[serde(default)]
    interface: Option<InterfaceFile>,
    #[serde(default)]
    dependencies: Option<DependenciesFile>,
    #[serde(default)]
    policy: Option<PolicyFile>,
}

#[derive(Debug, Default, Deserialize)]
struct InterfaceFile {
    display_name: Option<String>,
    short_description: Option<String>,
    icon_small: Option<PathBuf>,
    icon_large: Option<PathBuf>,
    brand_color: Option<String>,
    default_prompt: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct DependenciesFile {
    #[serde(default)]
    tools: Vec<DependencyToolFile>,
}

#[derive(Debug, Deserialize)]
struct DependencyToolFile {
    #[serde(rename = "type")]
    kind: Option<String>,
    value: Option<String>,
    description: Option<String>,
    transport: Option<String>,
    command: Option<String>,
    #[serde(default)]
    args: Vec<String>,
    url: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct PolicyFile {
    #[serde(default)]
    allow_implicit_invocation: Option<bool>,
}

struct BundledSkillFile {
    relative_path: &'static str,
    contents: &'static str,
}

struct BundledSkillPackage {
    id: &'static str,
    files: &'static [BundledSkillFile],
}

const BUNDLED_SKILLS: &[BundledSkillPackage] = &[
    BundledSkillPackage {
        id: "rust-diagnostic-repair",
        files: &[
            BundledSkillFile {
                relative_path: "SKILL.md",
                contents: include_str!("../skills/system/rust-diagnostic-repair/SKILL.md"),
            },
            BundledSkillFile {
                relative_path: "agents/openai.yaml",
                contents: include_str!(
                    "../skills/system/rust-diagnostic-repair/agents/openai.yaml"
                ),
            },
            BundledSkillFile {
                relative_path: "references/diagnostic-workflow.md",
                contents: include_str!(
                    "../skills/system/rust-diagnostic-repair/references/diagnostic-workflow.md"
                ),
            },
            BundledSkillFile {
                relative_path: "scripts/summarize-rust-diagnostics.py",
                contents: include_str!(
                    "../skills/system/rust-diagnostic-repair/scripts/summarize-rust-diagnostics.py"
                ),
            },
        ],
    },
    BundledSkillPackage {
        id: "test-failure-triage",
        files: &[
            BundledSkillFile {
                relative_path: "SKILL.md",
                contents: include_str!("../skills/system/test-failure-triage/SKILL.md"),
            },
            BundledSkillFile {
                relative_path: "agents/openai.yaml",
                contents: include_str!("../skills/system/test-failure-triage/agents/openai.yaml"),
            },
            BundledSkillFile {
                relative_path: "references/triage-checklist.md",
                contents: include_str!(
                    "../skills/system/test-failure-triage/references/triage-checklist.md"
                ),
            },
            BundledSkillFile {
                relative_path: "scripts/extract-failing-tests.py",
                contents: include_str!(
                    "../skills/system/test-failure-triage/scripts/extract-failing-tests.py"
                ),
            },
        ],
    },
    BundledSkillPackage {
        id: "repo-explainer",
        files: &[
            BundledSkillFile {
                relative_path: "SKILL.md",
                contents: include_str!("../skills/system/repo-explainer/SKILL.md"),
            },
            BundledSkillFile {
                relative_path: "agents/openai.yaml",
                contents: include_str!("../skills/system/repo-explainer/agents/openai.yaml"),
            },
            BundledSkillFile {
                relative_path: "references/repo-map.md",
                contents: include_str!("../skills/system/repo-explainer/references/repo-map.md"),
            },
            BundledSkillFile {
                relative_path: "scripts/file-type-summary.py",
                contents: include_str!(
                    "../skills/system/repo-explainer/scripts/file-type-summary.py"
                ),
            },
        ],
    },
];

pub fn load_skill_packages(workspace_root: &Path) -> SkillLoadOutcome {
    let mut outcome = SkillLoadOutcome::default();
    let bundled_root = install_bundled_skills(workspace_root, &mut outcome.errors);
    if let Some(root) = bundled_root {
        load_skill_root(&root, SkillScope::System, &mut outcome);
    }
    load_skill_root(
        &workspace_root.join(".marvis/skills"),
        SkillScope::Workspace,
        &mut outcome,
    );
    load_skill_root(
        &workspace_root.join(".agents/skills"),
        SkillScope::Workspace,
        &mut outcome,
    );
    dedupe_and_sort_skills(&mut outcome.skills, &mut outcome.errors);
    outcome
}

pub fn render_selected_skills_section(skills: &[SkillDescriptor]) -> Option<String> {
    if skills.is_empty() {
        return None;
    }

    let mut out = String::from(
        "## Skills\nA skill is a local package rooted at a `SKILL.md` file. The selected agent profile has equipped these skills for this turn. Read the package references and scripts only when needed; resolve relative paths against the skill package root.\n\n",
    );
    out.push_str("### Equipped Skills\n");
    for skill in skills {
        let path = skill
            .path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "<embedded>".to_string());
        out.push_str(&format!(
            "- {}: {} (file: {})\n",
            skill.id, skill.description, path
        ));
    }
    out.push_str("\n### Skill Instructions\n");
    for skill in skills {
        out.push_str("<skill>\n");
        out.push_str(&format!("<name>{}</name>\n", skill.id));
        if let Some(path) = &skill.path {
            out.push_str(&format!("<path>{}</path>\n", path.display()));
        }
        out.push_str(&format!("<scope>{}</scope>\n", skill.scope));
        out.push_str("<contents>\n");
        out.push_str(&skill.body);
        if !skill.body.ends_with('\n') {
            out.push('\n');
        }
        out.push_str("</contents>\n");
        if !skill.resources.is_empty() {
            out.push_str("<resources>\n");
            for resource in &skill.resources {
                out.push_str(&format!(
                    "- {:?}: {} ({} bytes)\n",
                    resource.kind,
                    resource.relative_path.display(),
                    resource.bytes
                ));
            }
            out.push_str("</resources>\n");
        }
        out.push_str("</skill>\n");
    }
    out.push_str(
        "\n### How To Use Equipped Skills\n- Use `read_skill_resource` for references/assets and `run_skill_script` for script utilities when the tool is available.\n- Do not assume every resource is relevant; load only the specific file needed for the current task.\n- Skill packages guide task execution. Local tool functions and MCP tools remain the execution layer and are separately gated by the routed profile.\n",
    );
    Some(out)
}

fn install_bundled_skills(workspace_root: &Path, errors: &mut Vec<String>) -> Option<PathBuf> {
    let primary_root = workspace_root.join(".lite-code/skills/.system");
    match write_bundled_skills_to_root(&primary_root) {
        Ok(()) => Some(primary_root),
        Err(primary_err) => {
            let fallback_root = std::env::temp_dir().join(format!(
                "marvis-bundled-skills-{}",
                env!("CARGO_PKG_VERSION")
            ));
            match write_bundled_skills_to_root(&fallback_root) {
                Ok(()) => {
                    errors.push(format!(
                        "failed to install bundled skills under {}: {primary_err}; using {}",
                        primary_root.display(),
                        fallback_root.display()
                    ));
                    Some(fallback_root)
                }
                Err(fallback_err) => {
                    errors.push(format!(
                        "failed to install bundled skills under {}: {primary_err}; fallback {} also failed: {fallback_err}",
                        primary_root.display(),
                        fallback_root.display()
                    ));
                    None
                }
            }
        }
    }
}

fn write_bundled_skills_to_root(root: &Path) -> Result<()> {
    for package in BUNDLED_SKILLS {
        let package_root = root.join(package.id);
        for file in package.files {
            let target = package_root.join(file.relative_path);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("create {}", parent.display()))?;
            }
            let needs_write = std::fs::read_to_string(&target)
                .map(|existing| existing != file.contents)
                .unwrap_or(true);
            if needs_write {
                std::fs::write(&target, file.contents)
                    .with_context(|| format!("write {}", target.display()))?;
            }
        }
    }
    Ok(())
}

fn load_skill_root(root: &Path, scope: SkillScope, outcome: &mut SkillLoadOutcome) {
    if !root.is_dir() {
        return;
    }

    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut visited = BTreeSet::new();
    let mut queue = VecDeque::from([(root.clone(), 0usize)]);
    visited.insert(root.clone());
    let mut truncated = false;

    while let Some((dir, depth)) = queue.pop_front() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(err) => {
                outcome.errors.push(format!(
                    "failed to read skills dir {}: {err}",
                    dir.display()
                ));
                continue;
            }
        };
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(err) => {
                    outcome.errors.push(format!(
                        "failed to read skill entry in {}: {err}",
                        dir.display()
                    ));
                    continue;
                }
            };
            let file_name = entry.file_name();
            if file_name.to_string_lossy().starts_with('.') {
                continue;
            }
            let path = entry.path();
            let metadata = match std::fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(err) => {
                    outcome.errors.push(format!(
                        "failed to stat skill path {}: {err}",
                        path.display()
                    ));
                    continue;
                }
            };
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                if depth >= MAX_SCAN_DEPTH {
                    continue;
                }
                if visited.len() >= MAX_SKILL_DIRS_PER_ROOT {
                    truncated = true;
                    continue;
                }
                let canonical = path.canonicalize().unwrap_or(path);
                if visited.insert(canonical.clone()) {
                    queue.push_back((canonical, depth + 1));
                }
                continue;
            }
            if metadata.is_file() && file_name == SKILL_FILE_NAME {
                match parse_skill_file(&path, scope) {
                    Ok(skill) => outcome.skills.push(skill),
                    Err(err) => outcome
                        .errors
                        .push(format!("failed to load skill {}: {err}", path.display())),
                }
            }
        }
    }

    if truncated {
        outcome.errors.push(format!(
            "skills scan truncated after {MAX_SKILL_DIRS_PER_ROOT} directories under {}",
            root.display()
        ));
    }
}

fn parse_skill_file(path: &Path, scope: SkillScope) -> Result<SkillDescriptor> {
    let body = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let frontmatter = extract_frontmatter(&body).ok_or_else(|| {
        anyhow!(
            "missing YAML frontmatter delimited by --- in {}",
            path.display()
        )
    })?;
    let frontmatter = parse_skill_frontmatter_yaml(&frontmatter)
        .with_context(|| format!("parse frontmatter in {}", path.display()))?;
    let skill_dir = path
        .parent()
        .ok_or_else(|| anyhow!("skill path has no parent: {}", path.display()))?;
    let name = frontmatter
        .name
        .as_deref()
        .map(sanitize_single_line)
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| default_skill_name(path));
    let id = frontmatter
        .id
        .as_deref()
        .map(normalize_identifier)
        .filter(|id| !id.is_empty())
        .unwrap_or_else(|| normalize_identifier(&name));
    let description = frontmatter
        .description
        .as_deref()
        .map(sanitize_single_line)
        .filter(|description| !description.is_empty())
        .ok_or_else(|| anyhow!("missing field `description`"))?;
    validate_len(&id, MAX_NAME_LEN, "id")?;
    validate_len(&name, MAX_NAME_LEN, "name")?;
    validate_len(&description, MAX_DESCRIPTION_LEN, "description")?;
    let short_description = frontmatter
        .metadata
        .short_description
        .as_deref()
        .map(sanitize_single_line)
        .filter(|value| !value.is_empty());
    if let Some(short_description) = &short_description {
        validate_len(
            short_description,
            MAX_DESCRIPTION_LEN,
            "metadata.short-description",
        )?;
    }

    let metadata = load_skill_metadata(skill_dir)?;
    let (local_tools, mcp_dependencies) = resolve_tool_dependencies(&metadata)?;
    let interface = resolve_interface(metadata.interface, skill_dir)?;
    let policy = metadata.policy.map(|policy| SkillPolicy {
        allow_implicit_invocation: policy.allow_implicit_invocation,
    });
    let package_root = skill_dir
        .canonicalize()
        .unwrap_or_else(|_| skill_dir.to_path_buf());
    let resources = collect_skill_resources(&package_root)?;

    Ok(SkillDescriptor {
        id,
        name,
        description,
        short_description,
        interface,
        policy,
        path: Some(path.canonicalize().unwrap_or_else(|_| path.to_path_buf())),
        package_root,
        scope,
        body,
        local_tools,
        mcp_dependencies,
        resources,
    })
}

fn load_skill_metadata(skill_dir: &Path) -> Result<SkillMetadataFile> {
    let json_path = skill_dir
        .join(AGENTS_DIR_NAME)
        .join(OPENAI_METADATA_JSON_FILE_NAME);
    if json_path.is_file() {
        let contents = std::fs::read_to_string(&json_path)
            .with_context(|| format!("read {}", json_path.display()))?;
        return serde_json::from_str::<SkillMetadataFile>(&contents)
            .with_context(|| format!("parse {}", json_path.display()));
    }

    let yaml_path = skill_dir
        .join(AGENTS_DIR_NAME)
        .join(OPENAI_METADATA_FILE_NAME);
    if yaml_path.is_file() {
        let contents = std::fs::read_to_string(&yaml_path)
            .with_context(|| format!("read {}", yaml_path.display()))?;
        return parse_skill_metadata_yaml(&contents)
            .with_context(|| format!("parse {}", yaml_path.display()));
    }

    Ok(SkillMetadataFile::default())
}

fn resolve_tool_dependencies(
    metadata: &SkillMetadataFile,
) -> Result<(Vec<String>, Vec<SkillToolDependency>)> {
    let mut local_tools = BTreeSet::new();
    for capability in &metadata.capabilities {
        let capability = capability.trim();
        if !capability.is_empty() {
            local_tools.insert(capability.to_string());
        }
    }

    let mut mcp_dependencies = Vec::new();
    if let Some(dependencies) = &metadata.dependencies {
        for raw in &dependencies.tools {
            let kind = raw
                .kind
                .as_deref()
                .map(sanitize_single_line)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| anyhow!("dependencies.tools entry missing `type`"))?;
            let value = raw
                .value
                .as_deref()
                .map(sanitize_single_line)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| anyhow!("dependencies.tools entry missing `value`"))?;
            validate_len(&kind, MAX_NAME_LEN, "dependencies.tools.type")?;
            validate_len(&value, MAX_DEPENDENCY_VALUE_LEN, "dependencies.tools.value")?;

            if is_local_tool_dependency_kind(&kind) {
                local_tools.insert(value);
                continue;
            }

            if kind.eq_ignore_ascii_case("mcp") {
                mcp_dependencies.push(SkillToolDependency {
                    kind,
                    value,
                    description: optional_single_line(raw.description.as_deref()),
                    transport: optional_single_line(raw.transport.as_deref()),
                    command: optional_single_line(raw.command.as_deref()),
                    args: normalize_string_list(raw.args.clone()),
                    url: optional_single_line(raw.url.as_deref()),
                });
            }
        }
    }

    Ok((local_tools.into_iter().collect(), mcp_dependencies))
}

fn is_local_tool_dependency_kind(kind: &str) -> bool {
    matches!(
        kind.to_ascii_lowercase().as_str(),
        "local" | "local_tool" | "tool" | "function"
    )
}

fn resolve_interface(
    interface: Option<InterfaceFile>,
    skill_dir: &Path,
) -> Result<Option<SkillInterface>> {
    let Some(interface) = interface else {
        return Ok(None);
    };
    let resolved = SkillInterface {
        display_name: optional_limited(interface.display_name.as_deref(), MAX_NAME_LEN),
        short_description: optional_limited(
            interface.short_description.as_deref(),
            MAX_DESCRIPTION_LEN,
        ),
        icon_small: resolve_asset_path(skill_dir, interface.icon_small.as_ref())?,
        icon_large: resolve_asset_path(skill_dir, interface.icon_large.as_ref())?,
        brand_color: resolve_brand_color(interface.brand_color.as_deref()),
        default_prompt: optional_limited(interface.default_prompt.as_deref(), MAX_DESCRIPTION_LEN),
    };
    let has_fields = resolved.display_name.is_some()
        || resolved.short_description.is_some()
        || resolved.icon_small.is_some()
        || resolved.icon_large.is_some()
        || resolved.brand_color.is_some()
        || resolved.default_prompt.is_some();
    Ok(has_fields.then_some(resolved))
}

fn resolve_asset_path(skill_dir: &Path, path: Option<&PathBuf>) -> Result<Option<PathBuf>> {
    let Some(path) = path else {
        return Ok(None);
    };
    let relative = normalize_relative_path(path.to_string_lossy().as_ref())?;
    let mut components = relative.components();
    match components.next() {
        Some(Component::Normal(component)) if component == "assets" => {}
        _ => {
            return Err(anyhow!(
                "interface icon paths must be relative paths under assets/"
            ));
        }
    }
    Ok(Some(skill_dir.join(relative)))
}

fn collect_skill_resources(skill_dir: &Path) -> Result<Vec<SkillResource>> {
    let mut resources = Vec::new();
    for (folder, kind) in [
        ("scripts", SkillResourceKind::Script),
        ("references", SkillResourceKind::Reference),
        ("assets", SkillResourceKind::Asset),
    ] {
        collect_resources_under(
            skill_dir,
            Path::new(folder),
            kind,
            &mut resources,
            0,
            &mut BTreeSet::new(),
        )?;
    }
    resources.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    if resources.len() > MAX_RESOURCES_PER_SKILL {
        resources.truncate(MAX_RESOURCES_PER_SKILL);
    }
    Ok(resources)
}

fn collect_resources_under(
    skill_dir: &Path,
    relative_dir: &Path,
    kind: SkillResourceKind,
    resources: &mut Vec<SkillResource>,
    depth: usize,
    visited: &mut BTreeSet<PathBuf>,
) -> Result<()> {
    if depth > MAX_RESOURCE_DEPTH || resources.len() >= MAX_RESOURCES_PER_SKILL {
        return Ok(());
    }
    let dir = skill_dir.join(relative_dir);
    if !dir.is_dir() {
        return Ok(());
    }
    let canonical = dir.canonicalize().unwrap_or_else(|_| dir.clone());
    if !visited.insert(canonical) {
        return Ok(());
    }

    for entry in std::fs::read_dir(&dir).with_context(|| format!("read {}", dir.display()))? {
        let entry = entry.with_context(|| format!("read entry in {}", dir.display()))?;
        let file_name = entry.file_name();
        if file_name.to_string_lossy().starts_with('.') {
            continue;
        }
        let path = entry.path();
        let metadata =
            std::fs::symlink_metadata(&path).with_context(|| format!("stat {}", path.display()))?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        let relative = relative_dir.join(file_name);
        if metadata.is_dir() {
            collect_resources_under(skill_dir, &relative, kind, resources, depth + 1, visited)?;
        } else if metadata.is_file() {
            resources.push(SkillResource {
                kind,
                relative_path: relative,
                bytes: metadata.len(),
            });
        }
    }
    Ok(())
}

fn dedupe_and_sort_skills(skills: &mut Vec<SkillDescriptor>, errors: &mut Vec<String>) {
    skills.sort_by(|left, right| {
        scope_rank(left.scope)
            .cmp(&scope_rank(right.scope))
            .then_with(|| left.id.cmp(&right.id))
            .then_with(|| left.path.cmp(&right.path))
    });
    let mut seen = BTreeSet::new();
    skills.retain(|skill| {
        let inserted = seen.insert(skill.id.clone());
        if !inserted {
            errors.push(format!(
                "duplicate skill id `{}` ignored at {}",
                skill.id,
                skill
                    .path
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "<unknown>".to_string())
            ));
        }
        inserted
    });
}

fn scope_rank(scope: SkillScope) -> u8 {
    match scope {
        SkillScope::Workspace => 0,
        SkillScope::System => 1,
    }
}

fn extract_frontmatter(contents: &str) -> Option<String> {
    let mut lines = contents.lines();
    if !matches!(lines.next(), Some(line) if line.trim() == "---") {
        return None;
    }
    let mut frontmatter_lines = Vec::new();
    for line in lines.by_ref() {
        if line.trim() == "---" {
            return Some(frontmatter_lines.join("\n"));
        }
        frontmatter_lines.push(line);
    }
    None
}

fn parse_skill_frontmatter_yaml(contents: &str) -> Result<SkillFrontmatter> {
    let mut frontmatter = SkillFrontmatter {
        id: None,
        name: None,
        description: None,
        metadata: SkillFrontmatterMetadata::default(),
    };
    let mut section: Option<&str> = None;
    for raw in contents.lines() {
        let line = strip_yaml_comment(raw).trim_end();
        if line.trim().is_empty() {
            continue;
        }
        let indent = raw.chars().take_while(|ch| *ch == ' ').count();
        let trimmed = line.trim();
        if indent == 0 {
            let (key, value) = split_yaml_key_value(trimmed)?;
            section = if value.is_empty() { Some(key) } else { None };
            match key {
                "id" => frontmatter.id = Some(parse_yaml_scalar(value)),
                "name" => frontmatter.name = Some(parse_yaml_scalar(value)),
                "description" => frontmatter.description = Some(parse_yaml_scalar(value)),
                "metadata" if value.is_empty() => {}
                "metadata" => return Err(anyhow!("metadata must be a mapping")),
                _ => {}
            }
            continue;
        }

        match section {
            Some("metadata") => {
                let (key, value) = split_yaml_key_value(trimmed)?;
                if key == "short-description" {
                    frontmatter.metadata.short_description = Some(parse_yaml_scalar(value));
                }
            }
            _ => return Err(anyhow!("unexpected indented frontmatter entry `{trimmed}`")),
        }
    }
    Ok(frontmatter)
}

fn parse_skill_metadata_yaml(contents: &str) -> Result<SkillMetadataFile> {
    let mut metadata = SkillMetadataFile::default();
    let mut section = MetadataYamlSection::None;
    let mut current_tool: Option<DependencyToolFile> = None;
    let mut pending_tool_list_field: Option<&str> = None;

    for raw in contents.lines() {
        let line = strip_yaml_comment(raw).trim_end();
        if line.trim().is_empty() {
            continue;
        }
        let indent = raw.chars().take_while(|ch| *ch == ' ').count();
        let trimmed = line.trim();

        if indent == 0 {
            finish_dependency_tool(&mut metadata, &mut current_tool);
            pending_tool_list_field = None;
            let (key, value) = split_yaml_key_value(trimmed)?;
            section = match key {
                "capabilities" if value.is_empty() => MetadataYamlSection::Capabilities,
                "dependencies" if value.is_empty() => MetadataYamlSection::Dependencies,
                "interface" if value.is_empty() => {
                    metadata
                        .interface
                        .get_or_insert_with(InterfaceFile::default);
                    MetadataYamlSection::Interface
                }
                "policy" if value.is_empty() => {
                    metadata.policy.get_or_insert_with(PolicyFile::default);
                    MetadataYamlSection::Policy
                }
                "capabilities" | "dependencies" | "interface" | "policy" => {
                    return Err(anyhow!("{key} must be a mapping or list"));
                }
                _ => MetadataYamlSection::None,
            };
            continue;
        }

        match section {
            MetadataYamlSection::Capabilities => {
                let Some(value) = trimmed.strip_prefix("- ") else {
                    return Err(anyhow!("capabilities entries must use `- value`"));
                };
                metadata.capabilities.push(parse_yaml_scalar(value));
            }
            MetadataYamlSection::Dependencies => {
                let (key, value) = split_yaml_key_value(trimmed)?;
                if key != "tools" || !value.is_empty() {
                    return Err(anyhow!("dependencies only supports a nested tools list"));
                }
                section = MetadataYamlSection::Tools;
            }
            MetadataYamlSection::Tools => {
                if pending_tool_list_field == Some("args")
                    && let Some(value) = trimmed.strip_prefix("- ")
                {
                    let Some(tool) = current_tool.as_mut() else {
                        return Err(anyhow!("args entry found before a tool list entry"));
                    };
                    tool.args.push(parse_yaml_scalar(value));
                    continue;
                }
                if let Some(value) = trimmed.strip_prefix("- ") {
                    finish_dependency_tool(&mut metadata, &mut current_tool);
                    pending_tool_list_field = None;
                    let mut tool = DependencyToolFile {
                        kind: None,
                        value: None,
                        description: None,
                        transport: None,
                        command: None,
                        args: Vec::new(),
                        url: None,
                    };
                    if !value.trim().is_empty() {
                        let (key, value) = split_yaml_key_value(value.trim())?;
                        assign_dependency_yaml_field(&mut tool, key, value)?;
                    }
                    current_tool = Some(tool);
                } else {
                    let Some(tool) = current_tool.as_mut() else {
                        return Err(anyhow!("tool field found before a tool list entry"));
                    };
                    let (key, value) = split_yaml_key_value(trimmed)?;
                    assign_dependency_yaml_field(tool, key, value)?;
                    pending_tool_list_field = (key == "args" && value.is_empty()).then_some("args");
                }
            }
            MetadataYamlSection::Interface => {
                let interface = metadata
                    .interface
                    .get_or_insert_with(InterfaceFile::default);
                let (key, value) = split_yaml_key_value(trimmed)?;
                match key {
                    "display_name" => interface.display_name = Some(parse_yaml_scalar(value)),
                    "short_description" => {
                        interface.short_description = Some(parse_yaml_scalar(value));
                    }
                    "icon_small" => {
                        interface.icon_small = Some(PathBuf::from(parse_yaml_scalar(value)))
                    }
                    "icon_large" => {
                        interface.icon_large = Some(PathBuf::from(parse_yaml_scalar(value)))
                    }
                    "brand_color" => interface.brand_color = Some(parse_yaml_scalar(value)),
                    "default_prompt" => interface.default_prompt = Some(parse_yaml_scalar(value)),
                    _ => {}
                }
            }
            MetadataYamlSection::Policy => {
                let policy = metadata.policy.get_or_insert_with(PolicyFile::default);
                let (key, value) = split_yaml_key_value(trimmed)?;
                if key == "allow_implicit_invocation" {
                    policy.allow_implicit_invocation = Some(parse_yaml_bool(value)?);
                }
            }
            MetadataYamlSection::None => {}
        }
    }

    finish_dependency_tool(&mut metadata, &mut current_tool);
    Ok(metadata)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MetadataYamlSection {
    None,
    Capabilities,
    Dependencies,
    Tools,
    Interface,
    Policy,
}

fn finish_dependency_tool(
    metadata: &mut SkillMetadataFile,
    current_tool: &mut Option<DependencyToolFile>,
) {
    let Some(tool) = current_tool.take() else {
        return;
    };
    metadata
        .dependencies
        .get_or_insert_with(DependenciesFile::default)
        .tools
        .push(tool);
}

fn assign_dependency_yaml_field(
    tool: &mut DependencyToolFile,
    key: &str,
    value: &str,
) -> Result<()> {
    match key {
        "type" => tool.kind = Some(parse_yaml_scalar(value)),
        "value" => tool.value = Some(parse_yaml_scalar(value)),
        "description" => tool.description = Some(parse_yaml_scalar(value)),
        "transport" => tool.transport = Some(parse_yaml_scalar(value)),
        "command" => tool.command = Some(parse_yaml_scalar(value)),
        "args" => tool.args = parse_yaml_array(value)?,
        "url" => tool.url = Some(parse_yaml_scalar(value)),
        _ => {}
    }
    Ok(())
}

fn split_yaml_key_value(line: &str) -> Result<(&str, &str)> {
    let Some((key, value)) = line.split_once(':') else {
        return Err(anyhow!("expected `key: value`, got `{line}`"));
    };
    let key = key.trim();
    if key.is_empty() {
        return Err(anyhow!("YAML key must not be empty"));
    }
    Ok((key, value.trim()))
}

fn parse_yaml_scalar(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim()
        .to_string()
}

fn parse_yaml_array(value: &str) -> Result<Vec<String>> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(Vec::new());
    }
    if !value.starts_with('[') || !value.ends_with(']') {
        return Err(anyhow!("expected inline YAML array"));
    }
    Ok(value[1..value.len() - 1]
        .split(',')
        .map(parse_yaml_scalar)
        .filter(|value| !value.is_empty())
        .collect())
}

fn parse_yaml_bool(value: &str) -> Result<bool> {
    match parse_yaml_scalar(value).to_ascii_lowercase().as_str() {
        "true" => Ok(true),
        "false" => Ok(false),
        other => Err(anyhow!("expected boolean, got `{other}`")),
    }
}

fn strip_yaml_comment(line: &str) -> &str {
    let mut previous_was_space = true;
    for (index, ch) in line.char_indices() {
        if ch == '#' && previous_was_space {
            return &line[..index];
        }
        previous_was_space = ch.is_whitespace();
    }
    line
}

fn default_skill_name(path: &Path) -> String {
    path.parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .map(sanitize_single_line)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "skill".to_string())
}

fn sanitize_single_line(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn optional_single_line(raw: Option<&str>) -> Option<String> {
    raw.map(sanitize_single_line)
        .filter(|value| !value.is_empty())
}

fn optional_limited(raw: Option<&str>, max_len: usize) -> Option<String> {
    raw.and_then(|value| optional_single_line(Some(value)))
        .filter(|value| value.chars().count() <= max_len)
}

fn resolve_brand_color(raw: Option<&str>) -> Option<String> {
    let value = raw?.trim();
    let mut chars = value.chars();
    (value.len() == 7 && chars.next() == Some('#') && chars.all(|ch| ch.is_ascii_hexdigit()))
        .then(|| value.to_string())
}

fn validate_len(value: &str, max_len: usize, field_name: &'static str) -> Result<()> {
    if value.is_empty() {
        return Err(anyhow!("missing field `{field_name}`"));
    }
    if value.chars().count() > max_len {
        return Err(anyhow!(
            "`{field_name}` exceeds maximum length of {max_len} characters"
        ));
    }
    Ok(())
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

fn normalize_relative_path(raw: &str) -> Result<PathBuf> {
    let path = Path::new(raw);
    if path.as_os_str().is_empty() {
        return Err(anyhow!("path must not be empty"));
    }
    if path.is_absolute() {
        return Err(anyhow!("path must be relative"));
    }
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(value) => normalized.push(value),
            Component::ParentDir => return Err(anyhow!("path must not contain '..'")),
            _ => return Err(anyhow!("path contains unsupported component")),
        }
    }
    if normalized.as_os_str().is_empty() {
        return Err(anyhow!("path must not be empty"));
    }
    Ok(normalized)
}

fn ensure_child_path(root: &Path, candidate: &Path) -> Result<()> {
    let canonical_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let canonical_parent = candidate
        .parent()
        .and_then(|parent| parent.canonicalize().ok())
        .unwrap_or_else(|| canonical_root.clone());
    if !canonical_parent.starts_with(&canonical_root) {
        return Err(anyhow!(
            "path must stay inside skill package {}",
            canonical_root.display()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("marvis-skills-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn loads_codex_style_skill_package_with_resources() {
        let root = temp_root("codex-style");
        let skill_dir = root.join(".marvis/skills/rust-helper");
        std::fs::create_dir_all(skill_dir.join("agents")).unwrap();
        std::fs::create_dir_all(skill_dir.join("references")).unwrap();
        std::fs::create_dir_all(skill_dir.join("scripts")).unwrap();
        std::fs::write(
            skill_dir.join(SKILL_FILE_NAME),
            "---\nname: Rust Helper\ndescription: Helps Rust work.\nmetadata:\n  short-description: Rust repairs\n---\nUse cargo carefully.\n",
        )
        .unwrap();
        std::fs::write(
            skill_dir.join("agents/openai.yaml"),
            "dependencies:\n  tools:\n    - type: local\n      value: read_file\n    - type: mcp\n      value: docs\n      transport: stdio\n      command: marvis-docs\n      args:\n        - --stdio\ninterface:\n  brand_color: '#336699'\n",
        )
        .unwrap();
        std::fs::write(skill_dir.join("references/checklist.md"), "check").unwrap();
        std::fs::write(skill_dir.join("scripts/helper.py"), "print('ok')").unwrap();

        let outcome = load_skill_packages(&root);
        let skill = outcome
            .skills
            .iter()
            .find(|skill| skill.id == "rust-helper")
            .unwrap();
        assert_eq!(skill.local_tools, vec!["read_file"]);
        assert_eq!(skill.mcp_dependencies[0].value, "docs");
        assert_eq!(skill.mcp_dependencies[0].args, vec!["--stdio"]);
        assert_eq!(
            skill
                .interface
                .as_ref()
                .and_then(|interface| interface.brand_color.as_deref()),
            Some("#336699")
        );
        assert!(skill.resources.iter().any(|resource| {
            resource.kind == SkillResourceKind::Reference
                && resource.relative_path == PathBuf::from("references/checklist.md")
        }));
        assert!(skill.resources.iter().any(|resource| {
            resource.kind == SkillResourceKind::Script
                && resource.relative_path == PathBuf::from("scripts/helper.py")
        }));
    }

    #[test]
    fn invalid_metadata_fails_skill_closed() {
        let root = temp_root("invalid-metadata");
        let skill_dir = root.join(".marvis/skills/broken");
        std::fs::create_dir_all(skill_dir.join("agents")).unwrap();
        std::fs::write(
            skill_dir.join(SKILL_FILE_NAME),
            "---\nname: Broken\ndescription: Broken metadata.\n---\nBody.\n",
        )
        .unwrap();
        std::fs::write(skill_dir.join("agents/openai.yaml"), "dependencies: [").unwrap();

        let outcome = load_skill_packages(&root);
        assert!(!outcome.skills.iter().any(|skill| skill.id == "broken"));
        assert!(outcome.errors.iter().any(|error| error.contains("parse")));
    }

    #[test]
    fn bundled_skills_are_file_backed() {
        let root = temp_root("bundled");
        let outcome = load_skill_packages(&root);
        let skill = outcome
            .skills
            .iter()
            .find(|skill| skill.id == "rust-diagnostic-repair")
            .unwrap();
        assert_eq!(skill.scope, SkillScope::System);
        assert!(skill.path.as_ref().unwrap().ends_with(SKILL_FILE_NAME));
        assert!(skill.resources.iter().any(|resource| {
            resource.kind == SkillResourceKind::Reference
                && resource
                    .relative_path
                    .ends_with("references/diagnostic-workflow.md")
        }));
    }
}
