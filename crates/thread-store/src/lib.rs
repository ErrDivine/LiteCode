use std::path::{Path, PathBuf};

use async_trait::async_trait;
use protocol::protocol::{RolloutItem, SessionSource};
use protocol::{ResponseItem, ThreadId};
use rollout::{RolloutConfig, RolloutRecorder, RolloutRecorderParams};
use serde::{Deserialize, Serialize};

const THREAD_METADATA_SUBDIR: &str = "thread_metadata";

#[derive(Debug, thiserror::Error)]
pub enum ThreadStoreError {
    #[error("thread was not found: {0}")]
    NotFound(ThreadId),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Rollout(#[from] anyhow::Error),
}

pub type ThreadStoreResult<T> = Result<T, ThreadStoreError>;

#[derive(Debug, Clone)]
pub struct CreateThreadParams {
    pub thread_id: ThreadId,
    pub forked_from_id: Option<ThreadId>,
    pub cwd: PathBuf,
    pub source: SessionSource,
    pub model_provider: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AppendThreadItemsParams {
    pub thread_id: ThreadId,
    pub items: Vec<RolloutItem>,
}

#[derive(Debug, Clone)]
pub struct ReadThreadParams {
    pub thread_id: ThreadId,
}

#[derive(Debug, Clone)]
pub struct LoadThreadHistoryParams {
    pub thread_id: ThreadId,
}

#[derive(Debug, Clone)]
pub struct ArchiveThreadParams {
    pub thread_id: ThreadId,
}

#[derive(Debug, Clone, Default)]
pub struct ListThreadsParams {
    pub include_archived: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ThreadMetadataPatch {
    pub name: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThreadMetadata {
    pub name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct UpdateThreadMetadataParams {
    pub thread_id: ThreadId,
    pub metadata: ThreadMetadataPatch,
}

#[derive(Debug, Clone)]
pub struct StoredThread {
    pub thread_id: ThreadId,
    pub rollout_path: PathBuf,
    pub metadata: ThreadMetadata,
}

#[derive(Debug, Clone)]
pub struct StoredThreadHistory {
    pub thread_id: ThreadId,
    pub items: Vec<RolloutItem>,
    pub response_items: Vec<ResponseItem>,
    pub metadata: ThreadMetadata,
}

#[derive(Debug, Clone, Default)]
pub struct ThreadPage {
    pub threads: Vec<StoredThread>,
}

#[async_trait]
pub trait ThreadStore: Send + Sync {
    async fn create_thread(&self, params: CreateThreadParams) -> ThreadStoreResult<StoredThread>;
    async fn append_items(&self, params: AppendThreadItemsParams) -> ThreadStoreResult<()>;
    async fn read_thread(&self, params: ReadThreadParams) -> ThreadStoreResult<StoredThread>;
    async fn load_history(
        &self,
        params: LoadThreadHistoryParams,
    ) -> ThreadStoreResult<StoredThreadHistory>;
    async fn list_threads(&self, params: ListThreadsParams) -> ThreadStoreResult<ThreadPage>;
    async fn archive_thread(&self, params: ArchiveThreadParams) -> ThreadStoreResult<()>;
    async fn update_metadata(&self, params: UpdateThreadMetadataParams) -> ThreadStoreResult<()>;
}

#[derive(Debug, Clone)]
pub struct LocalThreadStore {
    root: PathBuf,
}

impl LocalThreadStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn rollout_path(&self, thread_id: &ThreadId) -> PathBuf {
        rollout::rollout_path_for_thread(&self.root, thread_id)
    }

    fn archived_rollout_path(&self, thread_id: &ThreadId) -> PathBuf {
        self.root
            .join(rollout::ARCHIVED_SESSIONS_SUBDIR)
            .join(format!("{thread_id}.jsonl"))
    }

    fn metadata_path(&self, thread_id: &ThreadId) -> PathBuf {
        self.root
            .join(THREAD_METADATA_SUBDIR)
            .join(format!("{thread_id}.json"))
    }

    fn thread_exists(&self, thread_id: &ThreadId) -> bool {
        self.rollout_path(thread_id).exists() || self.archived_rollout_path(thread_id).exists()
    }

    async fn read_metadata(&self, thread_id: &ThreadId) -> ThreadStoreResult<ThreadMetadata> {
        let path = self.metadata_path(thread_id);
        match tokio::fs::read_to_string(&path).await {
            Ok(contents) => Ok(serde_json::from_str(&contents)?),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(ThreadMetadata::default()),
            Err(err) => Err(err.into()),
        }
    }

    async fn write_metadata(
        &self,
        thread_id: &ThreadId,
        metadata: &ThreadMetadata,
    ) -> ThreadStoreResult<()> {
        let path = self.metadata_path(thread_id);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let bytes = serde_json::to_vec_pretty(metadata)?;
        tokio::fs::write(path, bytes).await?;
        Ok(())
    }

    async fn stored_thread(
        &self,
        thread_id: ThreadId,
        rollout_path: PathBuf,
    ) -> ThreadStoreResult<StoredThread> {
        let metadata = self.read_metadata(&thread_id).await?;
        Ok(StoredThread {
            thread_id,
            rollout_path,
            metadata,
        })
    }
}

#[async_trait]
impl ThreadStore for LocalThreadStore {
    async fn create_thread(&self, params: CreateThreadParams) -> ThreadStoreResult<StoredThread> {
        let recorder = RolloutRecorder::new(RolloutRecorderParams {
            config: RolloutConfig::new(&self.root),
            thread_id: params.thread_id.clone(),
            forked_from_id: params.forked_from_id,
            cwd: params.cwd,
            originator: "lite-code".to_string(),
            cli_version: env!("CARGO_PKG_VERSION").to_string(),
            source: params.source,
            model_provider: params.model_provider,
            dynamic_tools: None,
        })
        .await?;

        self.stored_thread(params.thread_id, recorder.path().to_path_buf())
            .await
    }

    async fn append_items(&self, params: AppendThreadItemsParams) -> ThreadStoreResult<()> {
        let path = self.rollout_path(&params.thread_id);
        for item in params.items {
            rollout::append_rollout_item_to_path(&path, &item).await?;
        }
        Ok(())
    }

    async fn read_thread(&self, params: ReadThreadParams) -> ThreadStoreResult<StoredThread> {
        let path = self.rollout_path(&params.thread_id);
        if !path.exists() {
            return Err(ThreadStoreError::NotFound(params.thread_id));
        }
        self.stored_thread(params.thread_id, path).await
    }

    async fn load_history(
        &self,
        params: LoadThreadHistoryParams,
    ) -> ThreadStoreResult<StoredThreadHistory> {
        let path = self.rollout_path(&params.thread_id);
        if !path.exists() {
            return Err(ThreadStoreError::NotFound(params.thread_id));
        }

        let items = rollout::read_rollout_items(&path).await?;
        let response_items = items
            .iter()
            .filter_map(|item| match item {
                RolloutItem::ResponseItem(response_item) => Some(response_item.clone()),
                _ => None,
            })
            .collect();

        Ok(StoredThreadHistory {
            metadata: self.read_metadata(&params.thread_id).await?,
            thread_id: params.thread_id,
            items,
            response_items,
        })
    }

    async fn list_threads(&self, params: ListThreadsParams) -> ThreadStoreResult<ThreadPage> {
        let page = rollout::get_threads_in_root(
            &self.root,
            rollout::ThreadListConfig {
                include_archived: params.include_archived,
            },
        )
        .await?;

        let mut threads = Vec::new();
        for item in page.items {
            threads.push(self.stored_thread(item.thread_id, item.path).await?);
        }

        Ok(ThreadPage { threads })
    }

    async fn archive_thread(&self, params: ArchiveThreadParams) -> ThreadStoreResult<()> {
        let source = self.rollout_path(&params.thread_id);
        if !source.exists() {
            return Err(ThreadStoreError::NotFound(params.thread_id));
        }
        let target_dir = self.root.join(rollout::ARCHIVED_SESSIONS_SUBDIR);
        tokio::fs::create_dir_all(&target_dir).await?;
        let target = target_dir.join(format!("{}.jsonl", params.thread_id));
        tokio::fs::rename(source, target).await?;
        Ok(())
    }

    async fn update_metadata(&self, params: UpdateThreadMetadataParams) -> ThreadStoreResult<()> {
        if !self.thread_exists(&params.thread_id) {
            return Err(ThreadStoreError::NotFound(params.thread_id));
        }
        let mut metadata = self.read_metadata(&params.thread_id).await?;
        if let Some(name) = params.metadata.name {
            metadata.name = Some(name);
        }
        self.write_metadata(&params.thread_id, &metadata).await
    }
}

#[derive(Debug, Clone)]
pub struct UnavailableThreadStore {
    reason: String,
}

impl UnavailableThreadStore {
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }

    fn error<T>(&self) -> ThreadStoreResult<T> {
        Err(ThreadStoreError::Io(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            self.reason.clone(),
        )))
    }
}

#[async_trait]
impl ThreadStore for UnavailableThreadStore {
    async fn create_thread(&self, _params: CreateThreadParams) -> ThreadStoreResult<StoredThread> {
        self.error()
    }

    async fn append_items(&self, _params: AppendThreadItemsParams) -> ThreadStoreResult<()> {
        self.error()
    }

    async fn read_thread(&self, _params: ReadThreadParams) -> ThreadStoreResult<StoredThread> {
        self.error()
    }

    async fn load_history(
        &self,
        _params: LoadThreadHistoryParams,
    ) -> ThreadStoreResult<StoredThreadHistory> {
        self.error()
    }

    async fn list_threads(&self, _params: ListThreadsParams) -> ThreadStoreResult<ThreadPage> {
        self.error()
    }

    async fn archive_thread(&self, _params: ArchiveThreadParams) -> ThreadStoreResult<()> {
        self.error()
    }

    async fn update_metadata(&self, _params: UpdateThreadMetadataParams) -> ThreadStoreResult<()> {
        self.error()
    }
}

#[derive(Debug, Clone)]
pub struct ThreadRecorder {
    store: LocalThreadStore,
    thread_id: ThreadId,
}

impl ThreadRecorder {
    pub fn new(store: LocalThreadStore, thread_id: ThreadId) -> Self {
        Self { store, thread_id }
    }

    pub async fn append(&self, item: RolloutItem) -> ThreadStoreResult<()> {
        self.store
            .append_items(AppendThreadItemsParams {
                thread_id: self.thread_id.clone(),
                items: vec![item],
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use protocol::ResponseItem;

    use super::*;

    #[tokio::test]
    async fn local_store_lists_reads_updates_and_archives() {
        let temp = tempfile::tempdir().unwrap();
        let store = LocalThreadStore::new(temp.path());
        let thread_id = ThreadId::new();

        store
            .create_thread(CreateThreadParams {
                thread_id: thread_id.clone(),
                forked_from_id: None,
                cwd: temp.path().to_path_buf(),
                source: SessionSource::Cli,
                model_provider: None,
            })
            .await
            .unwrap();

        store
            .append_items(AppendThreadItemsParams {
                thread_id: thread_id.clone(),
                items: vec![RolloutItem::ResponseItem(ResponseItem::message(
                    "user", "hello",
                ))],
            })
            .await
            .unwrap();

        assert_eq!(
            store
                .list_threads(ListThreadsParams::default())
                .await
                .unwrap()
                .threads
                .len(),
            1
        );
        assert_eq!(
            store
                .load_history(LoadThreadHistoryParams {
                    thread_id: thread_id.clone()
                })
                .await
                .unwrap()
                .response_items
                .len(),
            1
        );

        store
            .update_metadata(UpdateThreadMetadataParams {
                thread_id: thread_id.clone(),
                metadata: ThreadMetadataPatch {
                    name: Some("name".to_string()),
                },
            })
            .await
            .unwrap();
        assert_eq!(
            store
                .read_thread(ReadThreadParams {
                    thread_id: thread_id.clone()
                })
                .await
                .unwrap()
                .metadata
                .name
                .as_deref(),
            Some("name")
        );
        store
            .archive_thread(ArchiveThreadParams {
                thread_id: thread_id.clone(),
            })
            .await
            .unwrap();

        let archived = store
            .list_threads(ListThreadsParams {
                include_archived: true,
            })
            .await
            .unwrap();
        assert_eq!(archived.threads.len(), 1);
        assert_eq!(archived.threads[0].metadata.name.as_deref(), Some("name"));
    }
}
