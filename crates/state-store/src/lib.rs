use std::path::{Path, PathBuf};

use protocol::ThreadId;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;

pub const SQLITE_HOME_ENV: &str = "LITE_CODE_STATE_HOME";
pub const STATE_DB_FILENAME: &str = "state.jsonl";
pub const STATE_DB_VERSION: u32 = 1;
pub const LOGS_DB_FILENAME: &str = "logs.jsonl";
pub const LOGS_DB_VERSION: u32 = 1;

#[derive(Debug, Clone)]
pub struct StateRuntime {
    root: PathBuf,
}

impl StateRuntime {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub async fn run_migrations(&self) -> std::io::Result<()> {
        tokio::fs::create_dir_all(&self.root).await?;
        let marker = MigrationMarker {
            state_db_version: STATE_DB_VERSION,
            logs_db_version: LOGS_DB_VERSION,
        };
        let bytes = serde_json::to_vec_pretty(&marker).map_err(json_error)?;
        tokio::fs::write(self.root.join("migrations.json"), bytes).await
    }

    pub async fn upsert_thread_metadata(&self, metadata: ThreadMetadata) -> std::io::Result<()> {
        self.run_migrations().await?;
        let path = state_db_path(&self.root);
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .await?;
        let line = serde_json::to_string(&metadata).map_err(json_error)?;
        file.write_all(line.as_bytes()).await?;
        file.write_all(b"\n").await?;
        Ok(())
    }

    pub async fn append_log(&self, entry: LogEntry) -> std::io::Result<()> {
        self.run_migrations().await?;
        let path = logs_db_path(&self.root);
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .await?;
        let line = serde_json::to_string(&entry).map_err(json_error)?;
        file.write_all(line.as_bytes()).await?;
        file.write_all(b"\n").await
    }
}

fn json_error(err: serde_json::Error) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, err)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThreadMetadata {
    pub thread_id: ThreadId,
    pub name: Option<String>,
    pub archived: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ThreadMetadataBuilder {
    thread_id: Option<ThreadId>,
    name: Option<String>,
    archived: bool,
}

impl ThreadMetadataBuilder {
    pub fn thread_id(mut self, thread_id: ThreadId) -> Self {
        self.thread_id = Some(thread_id);
        self
    }

    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn archived(mut self, archived: bool) -> Self {
        self.archived = archived;
        self
    }

    pub fn build(self) -> Option<ThreadMetadata> {
        Some(ThreadMetadata {
            thread_id: self.thread_id?,
            name: self.name,
            archived: self.archived,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LogEntry {
    pub level: String,
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub struct LogQuery {
    pub level: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LogRow {
    pub entry: LogEntry,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct MigrationMarker {
    state_db_version: u32,
    logs_db_version: u32,
}

pub fn state_db_filename() -> &'static str {
    STATE_DB_FILENAME
}

pub fn logs_db_filename() -> &'static str {
    LOGS_DB_FILENAME
}

pub fn state_db_path(root: impl AsRef<Path>) -> PathBuf {
    root.as_ref().join(STATE_DB_FILENAME)
}

pub fn logs_db_path(root: impl AsRef<Path>) -> PathBuf {
    root.as_ref().join(LOGS_DB_FILENAME)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn migrations_run_cleanly() {
        let temp = tempfile::tempdir().unwrap();
        let runtime = StateRuntime::new(temp.path());
        runtime.run_migrations().await.unwrap();

        assert!(temp.path().join("migrations.json").exists());

        runtime
            .upsert_thread_metadata(
                ThreadMetadataBuilder::default()
                    .thread_id(ThreadId::new())
                    .name("test")
                    .build()
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(state_db_path(temp.path()).exists());
    }
}
