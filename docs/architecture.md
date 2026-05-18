# Marvis Runtime Architecture

This repo is now organized around one product target: the VSCode extension. The CLI and web UI remain useful harnesses for testing the runtime, but they are not the main product shape.

## Product Shell

- `apps/vscode-extension/` is the VSCode shell.
- It collects live IDE status: active editor, cursor, selections, visible ranges, diagnostics, task/debug state, and command results.
- It starts the Rust binary with `lite-code --vscode-stdio`.
- It sends newline-delimited JSON requests and receives status reports plus streamed agent events.

## Rust Runtime

- `crates/session-kernel` owns thread lifecycle, submissions, event channels, history, replay, and fork basics.
- `crates/scheduler` runs model turns against an OpenAI-compatible chat completions endpoint configured by `MARVIS_API_KEY` and `MARVIS_BASE_URL`.
- `crates/status` owns `CodebaseStatus`, `VscodeStatus`, git state, command-result state, deterministic segments, stuckness signals, and prompt context capsules.
- `crates/pave-router` owns Rust JSON PAVE vectors, agent profiles, task candidates, and route scoring.
- `crates/ui-bridge` maps runtime events into CLI, web, and VSCode-facing event formats.
- `crates/rollout` records thread history in JSONL files under `.lite-code`.
- `crates/state-store` is the current local metadata/log store shell.
- `crates/protocol` holds runtime-neutral operations, events, thread ids, and response items.
- `src/autonomy.rs` owns suggest-first wake-up decisions, LLM task segmentation, suggestion cooldowns, and PAVE route selection.
- `src/skill_mcp.rs` resolves selected skills and stdio MCP tools for routed VSCode agents.
- `src/tools.rs` is still the local tool executor. It now enforces runtime policy for read, write, shell, git, network-like actions, MCP calls, and rollback snapshots; it should move into a tool-gateway crate once the API stabilizes.

## Current VSCode Loop

```text
VSCode event -> VscodeStatus -> status crate -> deterministic segments
Autonomy tick -> LLM task segmentation -> PAVE route -> accept/dismiss suggestion
User prompt -> context capsule -> session-kernel thread -> scheduler turn
Tool/model events -> ui-bridge -> VSCode panel/output
Command/task result -> status crate -> failure/stuckness segment
```

## Current Boundaries

- VSCode owns editor-native state.
- Rust owns status normalization, model/tool turns, traces, and deterministic status logic.
- The bridge uses typed JSON so the extension can stay thin and the runtime can be tested without VSCode.
