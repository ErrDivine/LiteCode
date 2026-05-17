# Product And Runtime Overview

Marvis is organized as a local agent runtime with a VSCode shell. The extension owns IDE-native observation and UI affordances; Rust owns the model/tool loop, status normalization, history, and cross-surface event contracts.

## Product Shape

The VSCode extension starts the Rust binary in stdio mode:

```text
target/debug/lite-code --vscode-stdio
```

If the binary is missing, the extension falls back to:

```text
cargo run --quiet -- --vscode-stdio
```

The extension then sends newline-delimited JSON envelopes for initialization, status updates, command results, prompts, and shutdown. The Rust side answers with typed response envelopes and streamed agent events.

## Runtime Shape

The runtime is built around four internal contracts:

| Contract | Owner | Purpose |
| --- | --- | --- |
| Protocol data | `crates/protocol` | Shared vocabulary for operations, user inputs, response items, events, session metadata, and rollout lines. |
| Thread runtime | `crates/session-kernel` | Owns submissions, active turns, cancellation, history, event emission, and persistence integration. |
| Scheduler | `crates/scheduler` | Converts thread state into model chat messages, streams deltas, executes tool calls, and returns final response items. |
| UI bridge | `crates/ui-bridge` | Converts kernel events into CLI, web, and VSCode-facing formats. |

Everything else supports these contracts:

- `crates/openai-rs` provides the HTTP/SSE client used by the scheduler.
- `crates/status` turns IDE snapshots, git state, diagnostics, and command failures into deterministic context.
- `crates/rollout`, `thread-store`, and `state-store` provide persistence surfaces.
- `src/tools.rs` exposes the current local tool gateway.

## Design Intent

The codebase is moving toward a product where the editor is the primary context source and the Rust runtime is product-neutral. The bridge stays thin: it transports typed data but does not decide model behavior. The status crate similarly keeps its logic deterministic, so high-signal context can be tested without the model.

The current implementation favors a small number of explicit interfaces over a broad plugin system. That makes it easier to evolve storage, scheduling, and UI surfaces independently.

## Current Maturity

The runtime already supports:

- CLI, web, and VSCode entrypoints.
- Streamed model output.
- Dynamic tool definitions.
- Local tool execution.
- JSONL history recording.
- Thread resume and fork basics.
- VSCode status ingestion.
- Synthetic scheduler fallback when `OPENROUTER_API_KEY` is absent.

Areas that are intentionally still simple:

- Tool policy is not yet factored into a dedicated tool-gateway crate.
- `thread-store` has a remote stub, but no remote implementation.
- `state-store` is a JSONL shell despite constants that still refer to database-style names.
- Token usage reporting is shaped in protocol types, but not fully wired through the scheduler.
