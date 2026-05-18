# Product And Runtime Overview

Marvis is organized as a local agent runtime with a VSCode shell. The extension owns IDE-native observation and UI affordances; Rust owns the model/tool loop, status normalization, history, and cross-surface event contracts.

## Product Shape

The VSCode extension starts the Rust binary in stdio mode:

```text
target/debug/lite-code --vscode-stdio
```

Published extension packages load a bundled binary from `bin/`, and development mode can use `target/debug/lite-code`. Users can always set `marvis.runtimePath` to an explicit binary. The extension does not invoke `cargo run` as a product fallback.

The extension then sends newline-delimited JSON envelopes for initialization, status updates, command results, autonomy ticks, prompts, accepted suggestions, and shutdown. The Rust side answers with typed response envelopes, autonomous decisions, and streamed agent events.

## Runtime Shape

The runtime is built around four internal contracts:

| Contract | Owner | Purpose |
| --- | --- | --- |
| Protocol data | `crates/protocol` | Shared vocabulary for operations, user inputs, response items, events, session metadata, and rollout lines. |
| Thread runtime | `crates/session-kernel` | Owns submissions, active turns, cancellation, history, event emission, and persistence integration. |
| Scheduler | `crates/scheduler` | Converts thread state into model chat messages, streams deltas, executes tool calls, and returns final response items. |
| UI bridge | `crates/ui-bridge` | Converts kernel events into CLI, web, and VSCode-facing formats. |
| PAVE router | `crates/pave-router` | Scores LLM-segmented tasks against configured agent profiles. |

Everything else supports these contracts:

- `crates/openai-rs` provides the HTTP/SSE client used by the scheduler.
- `crates/status` turns IDE snapshots, git state, diagnostics, and command failures into deterministic context.
- `src/autonomy.rs` checks actionable VSCode status, asks the model to segment tasks, routes them through PAVE, and stores suggest-first decisions.
- `src/skill_mcp.rs` resolves routed agent skills and stdio MCP servers before a VSCode agent turn starts.
- `crates/rollout`, `thread-store`, and `state-store` provide persistence surfaces.
- `src/tools.rs` exposes the current local/MCP tool gateway.

## Design Intent

The codebase is moving toward a product where the editor is the primary context source and the Rust runtime is product-neutral. The bridge stays thin: it transports typed data but does not decide model behavior. The status crate similarly keeps its logic deterministic, so high-signal context can be tested without the model.

The current implementation favors a small number of explicit interfaces over a broad plugin system. That makes it easier to evolve storage, scheduling, and UI surfaces independently.

## Current Maturity

The runtime already supports:

- CLI, web, and VSCode entrypoints.
- Streamed model output.
- Dynamic tool definitions filtered by runtime policy.
- Local tool execution with write, shell, git, and network-like policy checks.
- Rollback preimage snapshots for workspace write tools.
- Skill package registry with bundled/workspace Codex-style skills and stdio MCP tool discovery for routed VSCode agents.
- JSONL history recording.
- Event trace recording in rollout JSONL.
- Thread resume and fork basics.
- VSCode status ingestion.
- Autonomous VSCode wake-up checks with suggest-first PAVE routed tasks.
- OpenAI-compatible provider configuration through `MARVIS_API_KEY` and `MARVIS_BASE_URL`.

Areas that are intentionally still simple:

- Tool policy is not yet factored into a dedicated tool-gateway crate.
- `thread-store` is local-rollout backed; remote persistence is not exposed as a fake implementation.
- `state-store` is a JSONL shell despite constants that still refer to database-style names.
- Token usage reporting is shaped in protocol types, but not fully wired through the scheduler.
- Autonomous execution is intentionally suggest-first; accepted suggestions run through the existing bounded prompt/process path.
