# Session Kernel

`crates/session-kernel` is the runtime core. It owns conversation threads, submission processing, event emission, history snapshots, cancellation, replay, fork basics, and integration with rollout persistence.

The kernel deliberately avoids OpenAI-specific types. Model execution is delegated through the `Scheduler` trait, and tool execution is delegated through the `ToolExecutor` trait.

## Public Error Types

| Type | Purpose |
| --- | --- |
| `KernelError` | Main runtime error: closed channels, missing threads, invalid requests, IO errors, and rollout errors. |
| `SteerInputError` | Error returned by `steer_input`, including wrong active turn id. |
| `Result<T>` | Alias for `std::result::Result<T, KernelError>`. |

## Configuration Types

### `SessionConfig`

Thread runtime configuration:

- `model`
- `model_provider_id`
- `cwd`
- `system_prompt`
- `max_tokens`
- `max_tool_calls`
- `approval_policy`
- `sandbox_policy`
- `dynamic_tools`
- `persist_history`
- `history_root`
- `session_source`

`SessionConfig::new(model, cwd)` defaults provider to `openai-compatible`, max tokens to `4096`, max tool calls to `32`, approval to `on_request`, sandbox to `workspace_write`, history persistence to true, and source to CLI.

### `ThreadConfigSnapshot`

Read-only configuration view returned by `ThreadHandle::config_snapshot`. It includes model, provider, approval, sandbox, cwd, ephemeral flag, and session source.

### `ForkSnapshot`

Controls fork history selection:

- `Interrupted`: preserve current history.
- `TruncateBeforeNthUserMessage(n)`: keep history up to but not including the nth user message.

## Core Traits

### `ToolExecutor`

The tool boundary:

```rust,ignore
async fn execute_tool(&self, name: &str, input: &serde_json::Value) -> ToolExecutionResult;
```

The kernel does not inspect tool schemas or tool policy. It passes the executor into `TurnRequest`.

### `Scheduler`

The model-turn boundary:

```rust,ignore
async fn run_turn(
    &self,
    request: TurnRequest,
    events: EventEmitter,
) -> Result<SchedulerOutput>;
```

The scheduler owns model interaction and calls the tool executor when appropriate.

### `HistoryStore`

Persistence abstraction used by `ThreadManager`:

- `create_thread_record`
- `append_items`
- `read_rollout`
- `list_thread_ids`

`LocalHistoryStore` implements this trait using the `rollout` crate.

### `EventSink`

A general async event sink trait. It is currently defined as a simple event-emission boundary for future integration.

## Turn Types

### `TurnRequest`

The complete scheduler input:

- thread identity: `thread_id`, `submission_id`, `turn_id`
- model settings: `model`, `max_tokens`, `system_prompt`
- conversation state: `history`, `input`
- tool state: `dynamic_tools`, `tool_executor`
- output constraints: `final_output_json_schema`
- cancellation: `CancellationFlag`

### `SchedulerOutput`

The scheduler returns:

- `response_items`: items the kernel should append to thread history.
- `final_message`: optional final assistant text for `AgentMessage` and `TurnComplete`.
- `token_usage`: optional `TokenUsageInfo`.

### `ToolExecutionResult`

Currently contains only `output: String`. This keeps the executor simple but leaves room for richer status in a future tool gateway.

## EventEmitter

`EventEmitter` attaches the submission id to emitted `EventMsg` values and sends them over the thread event channel.

Convenience methods:

- `emit(msg)`
- `tool_begin(call_id, name, arguments)`
- `tool_end(call_id, name, output)`

Schedulers should emit deltas and tool events through this object rather than writing to UI adapters directly.

## CancellationFlag

`CancellationFlag` wraps an atomic bool and is cloned into scheduler code. Interrupts call `cancel()`. Scheduler loops should periodically check `is_cancelled()` and return early.

## ThreadManager

`ThreadManager` owns:

- active thread handles in an `RwLock<HashMap<ThreadId, ThreadHandle>>`
- a shared scheduler
- a shared tool executor
- a shared history store

Public constructors:

- `new(scheduler, tool_executor, history_root)`
- `with_history_store(scheduler, tool_executor, history_store)`

Public operations:

| Method | Design role |
| --- | --- |
| `start_thread(config)` | Starts a thread with config dynamic tools. |
| `start_thread_with_tools(config, tools, persist_extended_history)` | Sets dynamic tools and persistence behavior before start. |
| `resume_thread_from_rollout(config, rollout_path)` | Reads a rollout file, extracts session id and response history, and starts a live handle. |
| `fork_thread(source_thread_id, snapshot, config)` | Creates a new thread from a source history snapshot. |
| `list_thread_ids()` | Merges ids from persisted history and active handles. |
| `get_thread(thread_id)` | Returns a live handle if active. |

On thread creation, the manager emits `SessionConfigured` before any user turns.

## ThreadHandle

`ThreadHandle` is the operational handle used by CLI, web, and VSCode code.

Public methods:

| Method | Behavior |
| --- | --- |
| `submit(op)` | Generates a submission id and sends the operation. Interrupts are handled immediately. |
| `submit_with_id(submission)` | Sends a caller-provided submission. |
| `submit_with_trace(op, trace)` | Sends an operation with W3C trace context. |
| `next_event()` | Receives the next runtime event. |
| `steer_input(input, expected_turn_id, metadata)` | Queues input during an active turn or starts a new turn while idle. |
| `inject_response_items(items)` | Adds non-empty response items to history and persistence. |
| `flush_rollout()` | Calls `sync_all` on the rollout file when persistence is enabled. |
| `config_snapshot()` | Returns a thread configuration snapshot. |
| `token_usage_info()` | Currently returns `None`. |
| `rollout_path()` | Returns the rollout path if persistence is enabled. |

Internal behavior:

- `submission_loop` serially processes submitted operations.
- `run_user_turn` emits `TurnStarted`, `UserMessage`, scheduler events, `AgentMessage`, and `TurnComplete`.
- User response items are persisted before scheduling. Scheduler requests receive the prior history plus the current input, so the active user turn is not duplicated in the model request.
- Assistant, tool-call, and tool-output response items are persisted through the history store.
- Pending input is submitted as a new turn after the active turn completes.

## Design Notes

The kernel is intentionally channel-driven. That makes it easy for UI surfaces to consume one event stream while the runtime performs scheduler and tool work asynchronously.

The biggest future boundary is tool policy. Today the scheduler gets direct access to a `ToolExecutor`; a policy-aware gateway can be introduced behind the same trait or through a richer trait without changing the UI bridge.
