# Persistence Crates

Persistence is split across three crates:

- `rollout`: concrete JSONL session history.
- `thread-store`: storage-neutral thread interface over rollout-style history.
- `state-store`: local metadata and log appenders.

## Rollout

`crates/rollout` writes and reads thread history as line-delimited JSON.

Constants:

- `SESSIONS_SUBDIR = "sessions"`
- `ARCHIVED_SESSIONS_SUBDIR = "archived_sessions"`

### RolloutConfig

`RolloutConfig` stores the root directory and derives:

- `sessions_dir()`
- `archived_sessions_dir()`

The normal layout is:

```text
<root>/sessions/<thread-id>.jsonl
<root>/archived_sessions/<thread-id>.jsonl
```

### RolloutRecorderParams

Thread metadata used when opening a recorder:

- config
- thread id
- optional fork source
- cwd
- originator
- CLI version
- session source
- model provider
- dynamic tools

### RolloutRecorder

`RolloutRecorder::new(params)`:

1. Creates the sessions directory.
2. Opens the thread JSONL file in append mode.
3. Writes a `SessionMeta` line if the file is empty.

Other methods:

- `path()`
- `append_item(item)`
- `flush()`

### Rollout Free Functions

| Function | Purpose |
| --- | --- |
| `append_rollout_item_to_path` | Append one `RolloutItem` to any JSONL path, creating parents. |
| `read_rollout_items` | Read and parse non-empty JSONL lines into rollout items. |
| `get_threads_in_root` | List session files, optionally including archived sessions. |
| `find_thread_path_by_id_str` | Resolve a live session path by string id. |
| `find_archived_thread_path_by_id_str` | Resolve an archived session path by string id. |
| `rollout_path_for_thread` | Build the live session path for a `ThreadId`. |

## Thread Store

`crates/thread-store` defines a persistence boundary for thread operations.

### Errors And Results

`ThreadStoreError` includes:

- `NotFound(ThreadId)`
- IO errors
- rollout errors

`ThreadStoreResult<T>` aliases `Result<T, ThreadStoreError>`.

### Parameter Types

- `CreateThreadParams`
- `AppendThreadItemsParams`
- `ReadThreadParams`
- `LoadThreadHistoryParams`
- `ArchiveThreadParams`
- `ListThreadsParams`
- `ThreadMetadataPatch`
- `UpdateThreadMetadataParams`

### Result Types

- `StoredThread`
- `StoredThreadHistory`
- `ThreadPage`

### ThreadStore Trait

The interface:

- `create_thread`
- `append_items`
- `read_thread`
- `load_history`
- `list_threads`
- `archive_thread`
- `update_metadata`

### LocalThreadStore

`LocalThreadStore` implements `ThreadStore` through rollout files.

Implemented behavior:

- Creates session metadata with `RolloutRecorder`.
- Appends rollout items.
- Reads thread file existence.
- Loads rollout items and extracts response items.
- Lists sessions through `rollout::get_threads_in_root`.
- Archives by renaming the JSONL file into `archived_sessions`.

`update_metadata` currently returns `Ok(())` without modifying a file.

### RemoteThreadStore

`RemoteThreadStore` is a stub. It returns `NotFound` for thread-specific operations and an empty page for `list_threads`.

### ThreadRecorder

`ThreadRecorder` is a thin append helper around a `LocalThreadStore` and a fixed `ThreadId`.

## State Store

`crates/state-store` is a local metadata/log shell. It currently writes JSONL files and a migrations marker.

Constants:

- `SQLITE_HOME_ENV = "LITE_CODE_STATE_HOME"`
- `STATE_DB_FILENAME = "state.jsonl"`
- `STATE_DB_VERSION = 1`
- `LOGS_DB_FILENAME = "logs.jsonl"`
- `LOGS_DB_VERSION = 1`

The `SQLITE_HOME_ENV` name is historical; the implementation is JSONL today.

### StateRuntime

`StateRuntime` owns a root path.

Methods:

- `new(root)`
- `root()`
- `run_migrations()`
- `upsert_thread_metadata(metadata)`
- `append_log(entry)`

`run_migrations` creates the root and writes `migrations.json` with state/log versions. Metadata and logs are appended as JSON lines.

### Metadata And Logs

Types:

- `ThreadMetadata`
- `ThreadMetadataBuilder`
- `LogEntry`
- `LogQuery`
- `LogRow`

Path helpers:

- `state_db_filename()`
- `logs_db_filename()`
- `state_db_path(root)`
- `logs_db_path(root)`

## Design Notes

Rollout is the runtime-critical persistence path. `thread-store` and `state-store` define future direction, but the active kernel writes through `LocalHistoryStore` in `session-kernel`, which itself uses `rollout`.

The long-term design should avoid two competing persistence APIs. Either the kernel should depend directly on `thread-store`, or `thread-store` should remain a higher-level API around rollout files for UI and management features.
