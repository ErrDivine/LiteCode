# Design Contracts

The codebase has a small set of contracts that matter more than individual modules.

## Stable Serialization Contract

Types in `crates/protocol` are the serialization layer for runtime operations, events, response items, and rollout records. They should remain backward compatible whenever possible because rollout files and external shells depend on them.

Design rules:

- Prefer additive fields with `#[serde(default)]`.
- Keep enum tag names stable once external surfaces use them.
- Keep process messages explicit and typed rather than ad hoc JSON.
- Use `ThreadId` instead of raw strings at Rust boundaries.

## Thread Contract

`ThreadHandle` is the operational interface for a single conversation thread.

It guarantees:

- Submissions are accepted through an async channel.
- Events are consumed from a single event receiver.
- User and assistant response items are appended to history.
- Rollout persistence happens if the thread has a rollout path.
- Interrupts emit a terminal abort event.

It does not guarantee:

- Parallel turns on one thread. The active turn and pending input model is sequential.
- Durable persistence of every transient event. The main persisted history is response items.
- A stable token usage implementation yet.

## Scheduler Contract

`Scheduler::run_turn` receives a complete `TurnRequest` and an `EventEmitter`.

It owns:

- Model message construction.
- Model streaming.
- Tool call execution ordering.
- Final assistant message extraction.
- Returning persisted response items.

It should not own:

- Thread creation or lookup.
- Rollout file paths.
- UI-specific event formats.
- VSCode status collection.

## Tool Contract

`ToolExecutor` is intentionally minimal:

```rust,ignore
async fn execute_tool(&self, name: &str, input: &serde_json::Value) -> ToolExecutionResult;
```

The current tool definitions are dynamic JSON schema records. The model receives the schema, then calls a tool by name with JSON arguments.

Design implications:

- The scheduler can stay provider-neutral.
- Tool policy can be introduced behind `ToolExecutor`.
- Tool schema and tool execution must stay aligned manually until a stronger tool registry exists.

## Status Contract

The status crate turns external state into a deterministic `StatusReport` and `ContextCapsule`.

It should remain:

- Pure for segmentation and stuckness functions where possible.
- Conservative about token volume.
- Independent from OpenAI-specific types.
- Testable without VSCode or a model.

## UI Contract

The UI bridge maps protocol events into surface-specific formats. It should be a formatting adapter, not a business-logic layer.

Surface behavior:

- CLI consumes deltas, tool start/end, errors, aborts, and turn completion.
- Web exposes SSE events named `delta`, `tool_start`, `tool_end`, `error`, and `done`.
- VSCode exposes `VscodeRuntimeEvent` variants for deltas, final messages, tools, turn lifecycle, and errors.
