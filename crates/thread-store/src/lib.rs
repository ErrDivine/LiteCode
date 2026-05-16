use std::path::{Path, PathBuf};

use async_trait::async_trait;
use protocol::protocol::{RolloutItem, SessionSource};
use protocol::{ResponseItem, ThreadId};
use rollout::{RolloutConfig, RolloutRecorder, RolloutRecorderParams};
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum ThreadStoreError {
    #[error("thread was not found: {0}")]
    NotFound(ThreadId),
    #[error(transparent)]
    Io(#[from] std::io::Error),
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

#[derive(Debug, Clone)]
pub struct UpdateThreadMetadataParams {
    pub thread_id: ThreadId,
    pub metadata: ThreadMetadataPatch,
}

#[derive(Debug, Clone)]
pub struct StoredThread {
    pub thread_id: ThreadId,
    pub rollout_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct StoredThreadHistory {
    pub thread_id: ThreadId,
    pub items: Vec<RolloutItem>,
    pub response_items: Vec<ResponseItem>,
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

        Ok(StoredThread {
            thread_id: params.thread_id,
            rollout_path: recorder.path().to_path_buf(),
        })
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
        Ok(StoredThread {
            thread_id: params.thread_id,
            rollout_path: path,
        })
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

        Ok(ThreadPage {
            threads: page
                .items
                .into_iter()
                .map(|item| StoredThread {
                    thread_id: item.thread_id,
                    rollout_path: item.path,
                })
                .collect(),
        })
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

    async fn update_metadata(&self, _params: UpdateThreadMetadataParams) -> ThreadStoreResult<()> {
        Ok(())
    }
}

#[derive(Debug, Clone, Default)]
pub struct RemoteThreadStore;

#[async_trait]
impl ThreadStore for RemoteThreadStore {
    async fn create_thread(&self, params: CreateThreadParams) -> ThreadStoreResult<StoredThread> {
        Err(ThreadStoreError::NotFound(params.thread_id))
    }

    async fn append_items(&self, params: AppendThreadItemsParams) -> ThreadStoreResult<()> {
        Err(ThreadStoreError::NotFound(params.thread_id))
    }

    async fn read_thread(&self, params: ReadThreadParams) -> ThreadStoreResult<StoredThread> {
        Err(ThreadStoreError::NotFound(params.thread_id))
    }

    async fn load_history(
        &self,
        params: LoadThreadHistoryParams,
    ) -> ThreadStoreResult<StoredThreadHistory> {
        Err(ThreadStoreError::NotFound(params.thread_id))
    }

    async fn list_threads(&self, _params: ListThreadsParams) -> ThreadStoreResult<ThreadPage> {
        Ok(ThreadPage::default())
    }

    async fn archive_thread(&self, params: ArchiveThreadParams) -> ThreadStoreResult<()> {
        Err(ThreadStoreError::NotFound(params.thread_id))
    }

    async fn update_metadata(&self, params: UpdateThreadMetadataParams) -> ThreadStoreResult<()> {
        Err(ThreadStoreError::NotFound(params.thread_id))
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
        store
            .archive_thread(ArchiveThreadParams {
                thread_id: thread_id.clone(),
            })
            .await
            .unwrap();

        assert_eq!(
            store
                .list_threads(ListThreadsParams {
                    include_archived: true
                })
                .await
                .unwrap()
                .threads
                .len(),
            1
        );
    }
}
