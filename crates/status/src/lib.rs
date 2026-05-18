use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

pub type SegmentId = String;

pub fn now_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceMeta {
    pub root: PathBuf,
    pub name: Option<String>,
    pub primary_language: Option<String>,
}

impl WorkspaceMeta {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let name = root
            .file_name()
            .and_then(|value| value.to_str())
            .map(ToOwned::to_owned);
        let primary_language = if root.join("Cargo.toml").exists() {
            Some("rust".to_string())
        } else if root.join("package.json").exists() {
            Some("javascript".to_string())
        } else {
            None
        };
        Self {
            root,
            name,
            primary_language,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Freshness {
    Hot,
    Warm,
    Cold,
    Stale,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    #[default]
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SegmentKind {
    UserFocus,
    RecentDiff,
    CommandFailure,
    FailingTest,
    DiagnosticCluster,
    Risk,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StatusSegment {
    pub id: SegmentId,
    pub kind: SegmentKind,
    pub summary: String,
    #[serde(default)]
    pub evidence: Vec<String>,
    #[serde(default)]
    pub files: Vec<PathBuf>,
    #[serde(default)]
    pub diagnostics: Vec<String>,
    pub token_estimate: usize,
    pub freshness: Freshness,
    pub confidence: f32,
    pub importance: f32,
    pub risk_level: RiskLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct TextRange {
    pub start: Position,
    pub end: Position,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EditorRef {
    pub path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language_id: Option<String>,
    #[serde(default)]
    pub is_dirty: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VisibleRange {
    pub path: PathBuf,
    pub range: TextRange,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SelectionState {
    pub path: PathBuf,
    pub anchor: Position,
    pub active: Position,
    #[serde(default)]
    pub is_reversed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CursorContext {
    pub path: PathBuf,
    pub line: u32,
    pub character: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol_hint: Option<String>,
    #[serde(default)]
    pub text_before: String,
    #[serde(default)]
    pub text_after: String,
    #[serde(default)]
    pub surrounding_text: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Information,
    Hint,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiagnosticEvent {
    pub id: String,
    pub path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<TextRange>,
    #[serde(default)]
    pub severity: DiagnosticSeverity,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TerminalSessionState {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_output_tail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_exit_code: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VscodeTaskState {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default)]
    pub is_running: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_exit_code: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_output_tail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DebugSessionState {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClipboardHint {
    pub kind: String,
    pub preview: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct VscodeStatus {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_editor: Option<EditorRef>,
    #[serde(default)]
    pub open_editors: Vec<EditorRef>,
    #[serde(default)]
    pub visible_ranges: Vec<VisibleRange>,
    #[serde(default)]
    pub selections: Vec<SelectionState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor_context: Option<CursorContext>,
    #[serde(default)]
    pub recently_opened_files: Vec<PathBuf>,
    #[serde(default)]
    pub recently_saved_files: Vec<PathBuf>,
    #[serde(default)]
    pub problems: Vec<DiagnosticEvent>,
    #[serde(default)]
    pub terminal_sessions: Vec<TerminalSessionState>,
    #[serde(default)]
    pub running_tasks: Vec<VscodeTaskState>,
    #[serde(default)]
    pub debug_sessions: Vec<DebugSessionState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clipboard_hint: Option<ClipboardHint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_trusted: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct GitDiffSummary {
    pub files_changed: usize,
    pub insertions: usize,
    pub deletions: usize,
    #[serde(default)]
    pub raw_shortstat: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct GitState {
    #[serde(default)]
    pub is_repository: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head: Option<String>,
    #[serde(default)]
    pub dirty_files: Vec<PathBuf>,
    #[serde(default)]
    pub untracked_files: Vec<PathBuf>,
    #[serde(default)]
    pub staged_files: Vec<PathBuf>,
    #[serde(default)]
    pub deleted_files: Vec<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff_summary: Option<GitDiffSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommandResult {
    pub command: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<PathBuf>,
    pub output_tail: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    pub timestamp_ms: u64,
}

impl CommandResult {
    pub fn failed(&self) -> bool {
        self.exit_code.is_some_and(|code| code != 0) || contains_failure_word(&self.output_tail)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CommandState {
    #[serde(default)]
    pub recent_results: Vec<CommandResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CodebaseStatus {
    pub workspace: WorkspaceMeta,
    pub timestamp_ms: u64,
    #[serde(default)]
    pub vscode: VscodeStatus,
    #[serde(default)]
    pub git: GitState,
    #[serde(default)]
    pub commands: CommandState,
    #[serde(default)]
    pub segments: Vec<StatusSegment>,
}

impl CodebaseStatus {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            workspace: WorkspaceMeta::new(root),
            timestamp_ms: now_timestamp_ms(),
            vscode: VscodeStatus::default(),
            git: GitState::default(),
            commands: CommandState::default(),
            segments: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProactiveSuggestion {
    pub message: String,
    pub action_type: String,
    pub confidence: f32,
    #[serde(default)]
    pub related_segment_ids: Vec<SegmentId>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StucknessSignal {
    pub score: f32,
    pub evidence: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub likely_problem: Option<String>,
    pub suggested_intervention: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StatusReport {
    pub snapshot_hash: String,
    pub summary: String,
    pub active_segments: Vec<StatusSegment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stuckness: Option<StucknessSignal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<ProactiveSuggestion>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextCapsule {
    pub user_prompt: String,
    pub status_summary: String,
    pub active_segments: Vec<StatusSegment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_editor: Option<EditorRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor_context: Option<CursorContext>,
    #[serde(default)]
    pub diagnostics: Vec<DiagnosticEvent>,
    #[serde(default)]
    pub recent_commands: Vec<CommandResult>,
    pub git: GitState,
}

impl ContextCapsule {
    pub fn to_prompt_context(&self) -> String {
        let mut out = String::new();
        out.push_str("Marvis structured VSCode/codebase status:\n");
        out.push_str(&self.status_summary);
        out.push_str("\n\nActive segments:\n");
        if self.active_segments.is_empty() {
            out.push_str("- none\n");
        } else {
            for segment in &self.active_segments {
                out.push_str(&format!(
                    "- {:?}: {} (risk: {:?}, files: {})\n",
                    segment.kind,
                    segment.summary,
                    segment.risk_level,
                    join_paths(&segment.files)
                ));
            }
        }

        if let Some(editor) = &self.active_editor {
            out.push_str(&format!("\nActive editor: {}\n", editor.path.display()));
        }
        if let Some(cursor) = &self.cursor_context {
            out.push_str(&format!(
                "Cursor: {}:{}:{}\n",
                cursor.path.display(),
                cursor.line + 1,
                cursor.character + 1
            ));
            if let Some(symbol) = &cursor.symbol_hint {
                out.push_str(&format!("Symbol hint: {symbol}\n"));
            }
            if !cursor.surrounding_text.trim().is_empty() {
                out.push_str("\nCursor bubble:\n```text\n");
                out.push_str(cursor.surrounding_text.trim());
                out.push_str("\n```\n");
            }
        }

        if !self.diagnostics.is_empty() {
            out.push_str("\nDiagnostics:\n");
            for diagnostic in self.diagnostics.iter().take(20) {
                out.push_str(&format!(
                    "- {:?} {}: {}\n",
                    diagnostic.severity,
                    diagnostic.path.display(),
                    diagnostic.message
                ));
            }
        }

        if !self.recent_commands.is_empty() {
            out.push_str("\nRecent command results:\n");
            for command in self.recent_commands.iter().take(8) {
                out.push_str(&format!(
                    "- `{}` exit={:?}\n",
                    command.command, command.exit_code
                ));
                if !command.output_tail.trim().is_empty() {
                    out.push_str("  output tail: ");
                    out.push_str(&collapse_ws(&command.output_tail));
                    out.push('\n');
                }
            }
        }

        out.push_str("\nUser request:\n");
        out.push_str(&self.user_prompt);
        out
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StatusError {
    #[error("git command failed: {0}")]
    Git(String),
}

#[derive(Debug, Clone)]
pub struct StatusStore {
    status: CodebaseStatus,
    max_recent_commands: usize,
    max_recent_files: usize,
}

impl StatusStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            status: CodebaseStatus::new(root),
            max_recent_commands: 20,
            max_recent_files: 20,
        }
    }

    pub fn snapshot(&self) -> CodebaseStatus {
        self.status.clone()
    }

    pub fn report(&self) -> StatusReport {
        let stuckness = detect_stuckness(&self.status);
        let suggestion = stuckness.as_ref().map(|signal| ProactiveSuggestion {
            message: signal.likely_problem.clone().unwrap_or_else(|| {
                "Marvis sees repeated friction in the current workspace.".into()
            }),
            action_type: if signal.score >= 0.75 {
                "ask".to_string()
            } else {
                "suggest".to_string()
            },
            confidence: signal.score,
            related_segment_ids: self
                .status
                .segments
                .iter()
                .filter(|segment| {
                    matches!(
                        segment.kind,
                        SegmentKind::CommandFailure
                            | SegmentKind::FailingTest
                            | SegmentKind::DiagnosticCluster
                    )
                })
                .map(|segment| segment.id.clone())
                .collect(),
        });

        StatusReport {
            snapshot_hash: status_hash(&self.status),
            summary: summarize_status(&self.status),
            active_segments: self.status.segments.clone(),
            stuckness,
            suggestion,
        }
    }

    pub fn update_vscode_status(&mut self, mut vscode: VscodeStatus) -> StatusReport {
        truncate_paths(&mut vscode.recently_opened_files, self.max_recent_files);
        truncate_paths(&mut vscode.recently_saved_files, self.max_recent_files);
        self.status.vscode = vscode;
        self.status.timestamp_ms = now_timestamp_ms();
        self.resegment();
        self.report()
    }

    pub fn ingest_command_result(&mut self, mut result: CommandResult) -> StatusReport {
        if result.timestamp_ms == 0 {
            result.timestamp_ms = now_timestamp_ms();
        }
        self.status.commands.recent_results.push(result);
        if self.status.commands.recent_results.len() > self.max_recent_commands {
            let drain_len = self.status.commands.recent_results.len() - self.max_recent_commands;
            self.status.commands.recent_results.drain(0..drain_len);
        }
        self.status.timestamp_ms = now_timestamp_ms();
        self.resegment();
        self.report()
    }

    pub fn refresh_git_state(&mut self) -> StatusReport {
        self.status.git = read_git_state(&self.status.workspace.root);
        self.status.timestamp_ms = now_timestamp_ms();
        self.resegment();
        self.report()
    }

    pub fn build_context_capsule(&self, user_prompt: impl Into<String>) -> ContextCapsule {
        let user_prompt = user_prompt.into();
        let active_editor = self.status.vscode.active_editor.clone();
        let cursor_path = self
            .status
            .vscode
            .cursor_context
            .as_ref()
            .map(|cursor| cursor.path.clone());
        let diagnostics = self
            .status
            .vscode
            .problems
            .iter()
            .filter(|diagnostic| {
                cursor_path
                    .as_ref()
                    .is_none_or(|path| same_path(path, &diagnostic.path))
                    || matches!(diagnostic.severity, DiagnosticSeverity::Error)
            })
            .take(40)
            .cloned()
            .collect();

        ContextCapsule {
            user_prompt,
            status_summary: summarize_status(&self.status),
            active_segments: self.status.segments.clone(),
            active_editor,
            cursor_context: self.status.vscode.cursor_context.clone(),
            diagnostics,
            recent_commands: self
                .status
                .commands
                .recent_results
                .iter()
                .rev()
                .take(8)
                .cloned()
                .collect(),
            git: self.status.git.clone(),
        }
    }

    fn resegment(&mut self) {
        self.status.segments = segment_status(&self.status);
    }
}

pub fn segment_status(status: &CodebaseStatus) -> Vec<StatusSegment> {
    let mut segments = Vec::new();
    if let Some(segment) = user_focus_segment(status) {
        segments.push(segment);
    }
    segments.extend(diagnostic_segments(status));
    if let Some(segment) = recent_diff_segment(status) {
        segments.push(segment);
    }
    segments.extend(command_failure_segments(status));
    if let Some(segment) = risk_segment(status) {
        segments.push(segment);
    }

    segments.sort_by(|left, right| {
        right
            .importance
            .partial_cmp(&left.importance)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    segments
}

fn user_focus_segment(status: &CodebaseStatus) -> Option<StatusSegment> {
    let editor = status.vscode.active_editor.as_ref()?;
    let cursor = status.vscode.cursor_context.as_ref();
    let diagnostic_count = status
        .vscode
        .problems
        .iter()
        .filter(|diagnostic| same_path(&diagnostic.path, &editor.path))
        .count();
    let cursor_summary = cursor
        .map(|cursor| format!("cursor at line {}", cursor.line + 1))
        .unwrap_or_else(|| "cursor location unknown".to_string());
    let summary = if diagnostic_count > 0 {
        format!(
            "User focus is {} with {diagnostic_count} visible diagnostic(s); {cursor_summary}.",
            editor.path.display()
        )
    } else {
        format!("User focus is {}; {cursor_summary}.", editor.path.display())
    };

    Some(StatusSegment {
        id: segment_id("user_focus", &editor.path),
        kind: SegmentKind::UserFocus,
        summary,
        evidence: vec![
            "active_editor".to_string(),
            "cursor".to_string(),
            "visible_ranges".to_string(),
        ],
        files: vec![editor.path.clone()],
        diagnostics: status
            .vscode
            .problems
            .iter()
            .filter(|diagnostic| same_path(&diagnostic.path, &editor.path))
            .map(|diagnostic| diagnostic.id.clone())
            .collect(),
        token_estimate: cursor
            .map(|cursor| estimate_tokens(&cursor.surrounding_text))
            .unwrap_or(64),
        freshness: Freshness::Hot,
        confidence: 0.98,
        importance: 0.95,
        risk_level: RiskLevel::Low,
    })
}

fn diagnostic_segments(status: &CodebaseStatus) -> Vec<StatusSegment> {
    let mut groups: Vec<(PathBuf, Vec<&DiagnosticEvent>)> = Vec::new();
    for diagnostic in &status.vscode.problems {
        if let Some((_, group)) = groups
            .iter_mut()
            .find(|(path, _)| same_path(path, &diagnostic.path))
        {
            group.push(diagnostic);
        } else {
            groups.push((diagnostic.path.clone(), vec![diagnostic]));
        }
    }

    groups
        .into_iter()
        .map(|(path, diagnostics)| {
            let errors = diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error)
                .count();
            let warnings = diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.severity == DiagnosticSeverity::Warning)
                .count();
            let summary = format!(
                "{} has {} diagnostic(s): {errors} error(s), {warnings} warning(s).",
                path.display(),
                diagnostics.len()
            );
            StatusSegment {
                id: segment_id("diagnostics", &path),
                kind: SegmentKind::DiagnosticCluster,
                summary,
                evidence: vec!["vscode_problems".to_string()],
                files: vec![path],
                diagnostics: diagnostics
                    .iter()
                    .map(|diagnostic| diagnostic.id.clone())
                    .collect(),
                token_estimate: diagnostics
                    .iter()
                    .map(|diagnostic| estimate_tokens(&diagnostic.message))
                    .sum(),
                freshness: Freshness::Hot,
                confidence: 0.95,
                importance: if errors > 0 { 0.9 } else { 0.7 },
                risk_level: if errors > 0 {
                    RiskLevel::Medium
                } else {
                    RiskLevel::Low
                },
            }
        })
        .collect()
}

fn recent_diff_segment(status: &CodebaseStatus) -> Option<StatusSegment> {
    let mut files = status.git.dirty_files.clone();
    for path in &status.git.untracked_files {
        if !files.iter().any(|existing| same_path(existing, path)) {
            files.push(path.clone());
        }
    }
    if files.is_empty() {
        return None;
    }
    Some(StatusSegment {
        id: "seg_recent_diff".to_string(),
        kind: SegmentKind::RecentDiff,
        summary: format!(
            "Git working tree has {} dirty file(s) and {} untracked file(s).",
            status.git.dirty_files.len(),
            status.git.untracked_files.len()
        ),
        evidence: vec!["git_status".to_string()],
        files,
        diagnostics: Vec::new(),
        token_estimate: 128,
        freshness: Freshness::Hot,
        confidence: 0.95,
        importance: 0.8,
        risk_level: RiskLevel::Medium,
    })
}

fn command_failure_segments(status: &CodebaseStatus) -> Vec<StatusSegment> {
    status
        .commands
        .recent_results
        .iter()
        .rev()
        .filter(|command| command.failed())
        .take(6)
        .map(|command| {
            let kind = if command.command.contains("cargo test") || command.command.contains("test")
            {
                SegmentKind::FailingTest
            } else {
                SegmentKind::CommandFailure
            };
            StatusSegment {
                id: format!("seg_command_{}", short_hash(&command.command)),
                kind,
                summary: format!(
                    "Command `{}` failed with exit {:?}.",
                    command.command, command.exit_code
                ),
                evidence: vec!["command_result".to_string()],
                files: Vec::new(),
                diagnostics: Vec::new(),
                token_estimate: estimate_tokens(&command.output_tail),
                freshness: Freshness::Hot,
                confidence: 0.9,
                importance: if matches!(kind, SegmentKind::FailingTest) {
                    0.88
                } else {
                    0.76
                },
                risk_level: RiskLevel::Medium,
            }
        })
        .collect()
}

fn risk_segment(status: &CodebaseStatus) -> Option<StatusSegment> {
    let deleted = status.git.deleted_files.len();
    let lock_or_config_files = status
        .git
        .dirty_files
        .iter()
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    name.ends_with(".lock")
                        || name.ends_with(".toml")
                        || name.ends_with(".json")
                        || name.ends_with(".yaml")
                        || name.ends_with(".yml")
                })
        })
        .count();
    let broad_change = status.git.dirty_files.len() + status.git.untracked_files.len() >= 12;
    if deleted == 0 && lock_or_config_files == 0 && !broad_change {
        return None;
    }

    let risk_level = if deleted > 0 || broad_change {
        RiskLevel::High
    } else {
        RiskLevel::Medium
    };
    Some(StatusSegment {
        id: "seg_risk".to_string(),
        kind: SegmentKind::Risk,
        summary: format!(
            "Working tree needs care: {deleted} deleted file(s), {lock_or_config_files} lock/config change(s)."
        ),
        evidence: vec!["git_status".to_string(), "risk_rules".to_string()],
        files: status.git.dirty_files.clone(),
        diagnostics: Vec::new(),
        token_estimate: 96,
        freshness: Freshness::Hot,
        confidence: 0.85,
        importance: 0.72,
        risk_level,
    })
}

pub fn detect_stuckness(status: &CodebaseStatus) -> Option<StucknessSignal> {
    let mut evidence = Vec::new();
    let failed_commands = status
        .commands
        .recent_results
        .iter()
        .rev()
        .filter(|command| command.failed())
        .collect::<Vec<_>>();

    if let Some(latest) = failed_commands.first() {
        let normalized = normalize_command(&latest.command);
        let repeat_count = failed_commands
            .iter()
            .filter(|command| normalize_command(&command.command) == normalized)
            .count();
        if repeat_count >= 2 {
            evidence.push(format!(
                "same command failed {repeat_count} time(s): `{}`",
                latest.command
            ));
            let score = (0.45 + repeat_count as f32 * 0.15).min(0.95);
            return Some(StucknessSignal {
                score,
                evidence,
                likely_problem: Some(format!(
                    "The recent `{}` failure repeated. I can inspect the focused code and propose a scoped fix.",
                    latest.command
                )),
                suggested_intervention: "ask_before_patch".to_string(),
            });
        }
    }

    let active_errors = status
        .vscode
        .problems
        .iter()
        .filter(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error)
        .count();
    if active_errors >= 3 {
        evidence.push(format!("{active_errors} active VSCode error diagnostic(s)"));
        return Some(StucknessSignal {
            score: 0.55,
            evidence,
            likely_problem: Some(
                "The Problems panel shows several active errors near the workspace.".to_string(),
            ),
            suggested_intervention: "passive_status".to_string(),
        });
    }

    None
}

pub fn summarize_status(status: &CodebaseStatus) -> String {
    let active = status
        .vscode
        .active_editor
        .as_ref()
        .map(|editor| editor.path.display().to_string())
        .unwrap_or_else(|| "none".to_string());
    let cursor = status
        .vscode
        .cursor_context
        .as_ref()
        .map(|cursor| format!("{}:{}", cursor.line + 1, cursor.character + 1))
        .unwrap_or_else(|| "unknown".to_string());
    let branch = status
        .git
        .branch
        .clone()
        .unwrap_or_else(|| "unknown".to_string());
    format!(
        "workspace={} active_editor={} cursor={} branch={} dirty_files={} untracked_files={} diagnostics={} segments={}",
        status.workspace.name.clone().unwrap_or_else(|| status
            .workspace
            .root
            .display()
            .to_string()),
        active,
        cursor,
        branch,
        status.git.dirty_files.len(),
        status.git.untracked_files.len(),
        status.vscode.problems.len(),
        status.segments.len(),
    )
}

pub fn status_hash(status: &CodebaseStatus) -> String {
    let bytes = serde_json::to_vec(status)
        .unwrap_or_else(|err| format!("status-serialization-error:{err}").into_bytes());
    short_hash(&bytes)
}

pub fn read_git_state(root: &Path) -> GitState {
    let Some(branch) = git_output(root, &["rev-parse", "--abbrev-ref", "HEAD"]) else {
        return GitState::default();
    };

    let head = git_output(root, &["rev-parse", "HEAD"]);
    let (status_output, status_error) =
        match git_output_raw_result(root, &["status", "--porcelain=v1"]) {
            Ok(output) => (output, None),
            Err(err) => (String::new(), Some(err)),
        };
    let diff_summary =
        git_output(root, &["diff", "--shortstat"]).and_then(|output| parse_diff_shortstat(&output));
    let parsed = parse_git_status_porcelain(&status_output);

    GitState {
        is_repository: true,
        status_error,
        branch: Some(branch),
        head,
        dirty_files: parsed.dirty_files,
        untracked_files: parsed.untracked_files,
        staged_files: parsed.staged_files,
        deleted_files: parsed.deleted_files,
        diff_summary,
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedGitStatus {
    pub dirty_files: Vec<PathBuf>,
    pub untracked_files: Vec<PathBuf>,
    pub staged_files: Vec<PathBuf>,
    pub deleted_files: Vec<PathBuf>,
}

pub fn parse_git_status_porcelain(output: &str) -> ParsedGitStatus {
    let mut parsed = ParsedGitStatus::default();
    for line in output.lines().filter(|line| line.len() >= 4) {
        let bytes = line.as_bytes();
        let index = bytes[0] as char;
        let worktree = bytes[1] as char;
        let path = parse_porcelain_path(&line[3..]);

        if index == '?' && worktree == '?' {
            parsed.untracked_files.push(path.clone());
            parsed.dirty_files.push(path);
            continue;
        }
        if index != ' ' {
            parsed.staged_files.push(path.clone());
        }
        if index == 'D' || worktree == 'D' {
            parsed.deleted_files.push(path.clone());
        }
        if index != ' ' || worktree != ' ' {
            parsed.dirty_files.push(path);
        }
    }
    dedup_paths(&mut parsed.dirty_files);
    dedup_paths(&mut parsed.untracked_files);
    dedup_paths(&mut parsed.staged_files);
    dedup_paths(&mut parsed.deleted_files);
    parsed
}

pub fn parse_diff_shortstat(output: &str) -> Option<GitDiffSummary> {
    let trimmed = output.trim();
    if trimmed.is_empty() {
        return None;
    }

    Some(GitDiffSummary {
        files_changed: number_before(trimmed, "file changed")
            .or_else(|| number_before(trimmed, "files changed"))
            .unwrap_or_default(),
        insertions: number_before(trimmed, "insertion")
            .or_else(|| number_before(trimmed, "insertions"))
            .unwrap_or_default(),
        deletions: number_before(trimmed, "deletion")
            .or_else(|| number_before(trimmed, "deletions"))
            .unwrap_or_default(),
        raw_shortstat: trimmed.to_string(),
    })
}

fn git_output(root: &Path, args: &[&str]) -> Option<String> {
    git_output_raw_result(root, args)
        .ok()
        .map(|output| output.trim().to_string())
}

fn git_output_raw_result(root: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|err| format!("git {} failed to start: {err}", args.join(" ")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "git {} exited with {}{}",
            args.join(" "),
            output.status,
            if stderr.trim().is_empty() {
                String::new()
            } else {
                format!(": {}", stderr.trim())
            }
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn parse_porcelain_path(raw: &str) -> PathBuf {
    let path = raw
        .rsplit_once(" -> ")
        .map(|(_, right)| right)
        .unwrap_or(raw)
        .trim_matches('"');
    PathBuf::from(path)
}

fn number_before(text: &str, marker: &str) -> Option<usize> {
    let idx = text.find(marker)?;
    text[..idx]
        .split(|ch: char| !ch.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .next_back()
        .and_then(|part| part.parse::<usize>().ok())
}

fn contains_failure_word(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("error:")
        || lower.contains("failed")
        || lower.contains("panicked")
        || lower.contains("test result: failed")
}

fn normalize_command(command: &str) -> String {
    command.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn collapse_ws(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn estimate_tokens(text: &str) -> usize {
    (text.len() / 4).max(1)
}

fn segment_id(prefix: &str, path: &Path) -> String {
    format!("seg_{prefix}_{}", short_hash(&path.display().to_string()))
}

fn short_hash<T: Hash + ?Sized>(value: &T) -> String {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn same_path(left: &Path, right: &Path) -> bool {
    left == right || left.ends_with(right) || right.ends_with(left)
}

fn join_paths(paths: &[PathBuf]) -> String {
    if paths.is_empty() {
        return "none".to_string();
    }
    paths
        .iter()
        .take(6)
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn dedup_paths(paths: &mut Vec<PathBuf>) {
    paths.sort();
    paths.dedup();
}

fn truncate_paths(paths: &mut Vec<PathBuf>, max_len: usize) {
    if paths.len() > max_len {
        let drain_len = paths.len() - max_len;
        paths.drain(0..drain_len);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_git_status_porcelain() {
        let parsed = parse_git_status_porcelain(
            " M src/main.rs\nA  crates/status/src/lib.rs\n?? apps/vscode-extension/package.json\n D old.rs\nR  before.rs -> after.rs\n",
        );

        assert!(parsed.dirty_files.contains(&PathBuf::from("src/main.rs")));
        assert!(
            parsed
                .dirty_files
                .contains(&PathBuf::from("crates/status/src/lib.rs"))
        );
        assert!(
            parsed
                .untracked_files
                .contains(&PathBuf::from("apps/vscode-extension/package.json"))
        );
        assert!(parsed.deleted_files.contains(&PathBuf::from("old.rs")));
        assert!(parsed.staged_files.contains(&PathBuf::from("after.rs")));
    }

    #[test]
    fn parses_git_diff_shortstat() {
        let parsed =
            parse_diff_shortstat(" 3 files changed, 120 insertions(+), 9 deletions(-)").unwrap();
        assert_eq!(parsed.files_changed, 3);
        assert_eq!(parsed.insertions, 120);
        assert_eq!(parsed.deletions, 9);
    }

    #[test]
    fn creates_focus_and_diagnostic_segments() {
        let root = PathBuf::from("/tmp/demo");
        let file = root.join("src/lib.rs");
        let mut store = StatusStore::new(&root);
        let report = store.update_vscode_status(VscodeStatus {
            active_editor: Some(EditorRef {
                path: file.clone(),
                language_id: Some("rust".to_string()),
                is_dirty: true,
            }),
            cursor_context: Some(CursorContext {
                path: file.clone(),
                line: 10,
                character: 4,
                symbol_hint: Some("run_turn".to_string()),
                text_before: "run".to_string(),
                text_after: "_turn".to_string(),
                surrounding_text: "fn run_turn() {}".to_string(),
            }),
            problems: vec![DiagnosticEvent {
                id: "d1".to_string(),
                path: file,
                range: None,
                severity: DiagnosticSeverity::Error,
                message: "borrowed value does not live long enough".to_string(),
                source: Some("rustc".to_string()),
                code: None,
            }],
            ..VscodeStatus::default()
        });

        assert!(
            report
                .active_segments
                .iter()
                .any(|segment| segment.kind == SegmentKind::UserFocus)
        );
        assert!(
            report
                .active_segments
                .iter()
                .any(|segment| segment.kind == SegmentKind::DiagnosticCluster)
        );
    }

    #[test]
    fn detects_repeated_failed_command_stuckness() {
        let mut store = StatusStore::new("/tmp/demo");
        store.ingest_command_result(CommandResult {
            command: "cargo test scheduler".to_string(),
            cwd: None,
            output_tail: "test result: FAILED".to_string(),
            exit_code: Some(101),
            timestamp_ms: 1,
        });
        let report = store.ingest_command_result(CommandResult {
            command: "cargo   test   scheduler".to_string(),
            cwd: None,
            output_tail: "test result: FAILED".to_string(),
            exit_code: Some(101),
            timestamp_ms: 2,
        });

        assert!(report.stuckness.is_some());
        assert!(report.suggestion.is_some());
    }
}
