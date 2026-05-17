# Architecture

The workspace uses a layered runtime architecture. The top layer is product-specific. The middle layer is runtime orchestration. The lower layers are protocol, transport, status, and persistence support.

## Layer Map

```text
VSCode extension
  -> ui-bridge JSON request/response types
  -> src/vscode.rs stdio server
  -> status crate context capsule
  -> session-kernel thread
  -> scheduler model/tool turn
  -> openai-rs client
  -> local tools
  -> rollout history
```

The CLI and web surfaces enter lower in the stack:

```text
CLI stdin/stdout -> session-kernel -> scheduler -> tools -> rollout
Web HTTP/SSE    -> session-kernel -> scheduler -> tools -> rollout
```

## Workspace Crate Dependencies

The important dependency direction is one-way:

| Crate | Depends on | Design note |
| --- | --- | --- |
| `protocol` | `serde`, `serde_json`, `thiserror` | Lowest shared vocabulary. No runtime ownership. |
| `openai-rs` | HTTP, SSE, serde, retry dependencies | Reusable API client. No knowledge of Marvis thread state. |
| `rollout` | `protocol`, `tokio`, `serde_json` | JSONL persistence of protocol records. |
| `thread-store` | `protocol`, `rollout` | Store abstraction over rollout persistence. |
| `state-store` | `protocol`, `tokio`, `serde_json` | Local metadata/log appenders. |
| `session-kernel` | `protocol`, `rollout`, `thread-store` | Runtime core. Depends on traits for scheduler and tools. |
| `scheduler` | `session-kernel`, `protocol`, `openai-rs` | Implements `session-kernel::Scheduler`. |
| `status` | `serde`, git subprocesses | Deterministic workspace status model. Independent from scheduler. |
| `ui-bridge` | `protocol`, `status` | Surface-specific event/request shapes. |
| `lite-code` binary | all relevant crates | Composition root. |

## Composition Root

`src/main.rs` composes:

- `OpenAiScheduler::openrouter(api_key)` as the model scheduler.
- `LocalToolExecutor` as the tool executor.
- `ThreadManager::new(...)` as the thread runtime.
- `tool_definitions()` as the dynamic tool surface.
- `SessionConfig` as the per-thread runtime configuration.

`src/vscode.rs` composes the same pieces, but selects `SyntheticScheduler` when `OPENROUTER_API_KEY` is missing. That fallback allows VSCode status collection and bridge testing without a live model.

## Runtime Boundaries

### Protocol Boundary

The protocol crate is the serialization boundary. Anything sent across process boundaries or written to rollout JSONL should be expressible as protocol types. This keeps UI shells and persistence from depending on internal scheduler structs.

### Kernel Boundary

The kernel is responsible for turn lifecycle, not model policy. It receives an `Op`, emits lifecycle events, persists user and assistant `ResponseItem`s, and delegates model execution to a `Scheduler`.

### Scheduler Boundary

The scheduler is responsible for mapping a `TurnRequest` into model calls and tool calls. It sees history, dynamic tools, the tool executor, and a cancellation flag. It returns `SchedulerOutput`.

### Tool Boundary

The kernel only knows the `ToolExecutor` trait. The current implementation is `LocalToolExecutor` in the binary crate. Moving this into a dedicated crate would let policy, tracing, and permissions evolve without changing the kernel.

### Status Boundary

The status crate produces deterministic summaries and context capsules. The model receives that capsule as text through the normal user-input path. This keeps context construction testable and prevents VSCode-specific types from leaking into the scheduler.

## Event Model

The kernel emits `protocol::Event` values. Each event has:

- `id`: the submission id.
- `msg`: an `EventMsg` variant.

Surface adapters map these events:

- CLI prints deltas and tool logs.
- Web converts selected events into SSE names and JSON payloads.
- VSCode converts selected events into `VscodeRuntimeEvent`.

This design makes the kernel unaware of UI presentation while preserving a single runtime event stream.
