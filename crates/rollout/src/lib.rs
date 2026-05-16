use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use protocol::protocol::{RolloutItem, SessionMeta, SessionMetaLine, SessionSource};
use protocol::{DynamicToolSpec, ThreadId};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::sync::Mutex;

pub const SESSIONS_SUBDIR: &str = "sessions";
pub const ARCHIVED_SESSIONS_SUBDIR: &str = "archived_sessions";

#[derive(Debug, Clone)]
pub struct RolloutConfig {
    pub root: PathBuf,
}

impl RolloutConfig {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn sessions_dir(&self) -> PathBuf {
        self.root.join(SESSIONS_SUBDIR)
    }

    pub fn archived_sessions_dir(&self) -> PathBuf {
        self.root.join(ARCHIVED_SESSIONS_SUBDIR)
    }
}

pub type Config = RolloutConfig;

#[derive(Debug, Clone)]
pub struct RolloutRecorderParams {
    pub config: RolloutConfig,
    pub thread_id: ThreadId,
    pub forked_from_id: Option<ThreadId>,
    pub cwd: PathBuf,
    pub originator: String,
    pub cli_version: String,
    pub source: SessionSource,
    pub model_provider: Option<String>,
    pub dynamic_tools: Option<Vec<DynamicToolSpec>>,
}

#[derive(Debug, Clone)]
pub struct RolloutRecorder {
    path: PathBuf,
    file: Arc<Mutex<tokio::fs::File>>,
}

impl RolloutRecorder {
    pub async fn new(params: RolloutRecorderParams) -> Result<Self> {
        tokio::fs::create_dir_all(params.config.sessions_dir())
            .await
            .with_context(|| format!("create {}", params.config.sessions_dir().display()))?;

        let path = rollout_path_for_thread(&params.config.root, &params.thread_id);
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await
            .with_context(|| format!("open rollout {}", path.display()))?;

        if file.metadata().await?.len() == 0 {
            let meta = SessionMeta {
                id: params.thread_id,
                forked_from_id: params.forked_from_id,
                timestamp: now_timestamp_string(),
                cwd: params.cwd,
                originator: params.originator,
                cli_version: params.cli_version,
                source: params.source,
                model_provider: params.model_provider,
                dynamic_tools: params.dynamic_tools,
            };
            append_item_to_file(
                &mut file,
                &RolloutItem::SessionMeta(SessionMetaLine { meta }),
            )
            .await?;
            file.flush().await?;
        }

        Ok(Self {
            path,
            file: Arc::new(Mutex::new(file)),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub async fn append_item(&self, item: &RolloutItem) -> Result<()> {
        let mut file = self.file.lock().await;
        append_item_to_file(&mut file, item).await
    }

    pub async fn flush(&self) -> std::io::Result<()> {
        self.file.lock().await.flush().await
    }
}

pub async fn append_rollout_item_to_path(path: impl AsRef<Path>, item: &RolloutItem) -> Result<()> {
    if let Some(parent) = path.as_ref().parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path.as_ref())
        .await?;
    append_item_to_file(&mut file, item).await
}

pub async fn read_rollout_items(path: impl AsRef<Path>) -> Result<Vec<RolloutItem>> {
    let file = tokio::fs::File::open(path.as_ref())
        .await
        .with_context(|| format!("open rollout {}", path.as_ref().display()))?;
    let mut lines = tokio::io::BufReader::new(file).lines();
    let mut items = Vec::new();

    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let item = serde_json::from_str::<RolloutItem>(&line)
            .with_context(|| format!("parse rollout line: {line}"))?;
        items.push(item);
    }

    Ok(items)
}

#[derive(Debug, Clone)]
pub struct ThreadItem {
    pub thread_id: ThreadId,
    pub path: PathBuf,
    pub archived: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ThreadListConfig {
    pub include_archived: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ThreadsPage {
    pub items: Vec<ThreadItem>,
}

pub async fn get_threads_in_root(
    root: impl AsRef<Path>,
    config: ThreadListConfig,
) -> Result<ThreadsPage> {
    let mut items = read_thread_items_from_dir(root.as_ref().join(SESSIONS_SUBDIR), false).await?;
    if config.include_archived {
        items.extend(
            read_thread_items_from_dir(root.as_ref().join(ARCHIVED_SESSIONS_SUBDIR), true).await?,
        );
    }
    items.sort_by(|left, right| left.thread_id.cmp(&right.thread_id));
    Ok(ThreadsPage { items })
}

pub async fn find_thread_path_by_id_str(root: impl AsRef<Path>, id: &str) -> Option<PathBuf> {
    let path = root
        .as_ref()
        .join(SESSIONS_SUBDIR)
        .join(format!("{id}.jsonl"));
    path.exists().then_some(path)
}

pub async fn find_archived_thread_path_by_id_str(
    root: impl AsRef<Path>,
    id: &str,
) -> Option<PathBuf> {
    let path = root
        .as_ref()
        .join(ARCHIVED_SESSIONS_SUBDIR)
        .join(format!("{id}.jsonl"));
    path.exists().then_some(path)
}

pub fn rollout_path_for_thread(root: impl AsRef<Path>, thread_id: &ThreadId) -> PathBuf {
    root.as_ref()
        .join(SESSIONS_SUBDIR)
        .join(format!("{thread_id}.jsonl"))
}

async fn append_item_to_file(file: &mut tokio::fs::File, item: &RolloutItem) -> Result<()> {
    let line = serde_json::to_string(item)?;
    file.write_all(line.as_bytes()).await?;
    file.write_all(b"\n").await?;
    Ok(())
}

async fn read_thread_items_from_dir(dir: PathBuf, archived: bool) -> Result<Vec<ThreadItem>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut entries = tokio::fs::read_dir(dir).await?;
    let mut items = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("jsonl") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
            continue;
        };
        if let Ok(thread_id) = ThreadId::from_string(stem) {
            items.push(ThreadItem {
                thread_id,
                path,
                archived,
            });
        }
    }
    Ok(items)
}

fn now_timestamp_string() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    seconds.to_string()
}

#[cfg(test)]
mod tests {
    use protocol::ResponseItem;
    use protocol::protocol::RolloutItem;

    use super::*;

    #[tokio::test]
    async fn writes_items_in_order() {
        let temp = tempfile::tempdir().unwrap();
        let thread_id = ThreadId::new();
        let recorder = RolloutRecorder::new(RolloutRecorderParams {
            config: RolloutConfig::new(temp.path()),
            thread_id: thread_id.clone(),
            forked_from_id: None,
            cwd: temp.path().to_path_buf(),
            originator: "test".to_string(),
            cli_version: "test".to_string(),
            source: SessionSource::Cli,
            model_provider: None,
            dynamic_tools: None,
        })
        .await
        .unwrap();

        recorder
            .append_item(&RolloutItem::ResponseItem(ResponseItem::message(
                "user", "hello",
            )))
            .await
            .unwrap();
        recorder.flush().await.unwrap();

        let items = read_rollout_items(recorder.path()).await.unwrap();
        assert!(matches!(items[0], RolloutItem::SessionMeta(_)));
        assert!(matches!(items[1], RolloutItem::ResponseItem(_)));
        assert_eq!(
            get_threads_in_root(temp.path(), ThreadListConfig::default())
                .await
                .unwrap()
                .items[0]
                .thread_id,
            thread_id
        );
    }
}
