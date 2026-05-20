# lite-code / Marvis Runtime Design

This book documents the current `lite-code` workspace from a design point of view. It is written for maintainers who need to understand how the runtime is shaped, where each boundary sits, and which interfaces are stable enough to build against.

The main product target is Marvis inside VSCode. The CLI and web harnesses still exist, but they are supporting surfaces around the same Rust runtime.

## What The System Is

`lite-code` is a local coding-agent runtime. It accepts user turns, builds a model request from prior thread history and live workspace context, streams model output, executes local tools when the model requests them, emits normalized runtime events, and records thread history as JSONL.

At the top level the project is split into:

| Area | Role |
| --- | --- |
| `apps/vscode-extension` | VSCode product shell. It collects editor context and speaks newline-delimited JSON to the Rust binary. |
| `src/main.rs` | Binary entrypoint. It selects CLI, web, or VSCode stdio mode. |
| `src/tools.rs` | Current local tool executor for shell, file read/write/edit, directory listing, search, glob discovery, safe test/build helpers, and runtime policy gates. |
| `src/skills.rs` | Codex-style skill package loading, bundled skill materialization, and resource indexing. |
| `src/skill_mcp.rs` | Skill registry, selected skill injection, stdio MCP discovery, and MCP tool execution. |
| `src/web.rs` | Temporary HTTP/SSE web harness. |
| `src/vscode.rs` | Runtime side of the VSCode stdio bridge. |
| `crates/protocol` | Runtime-neutral operations, events, ids, user inputs, response items, and rollout records. |
| `crates/session-kernel` | Thread lifecycle, submissions, event channels, history, replay, forking, and turn orchestration boundary. |
| `crates/scheduler` | Model turn execution against OpenAI-compatible chat completions plus a synthetic scheduler used only by tests/dev code. |
| `crates/openai-rs` | Reusable OpenAI-compatible HTTP and streaming client. |
| `crates/status` | VSCode/codebase status model, deterministic segments, stuckness detection, git state, and context capsules. |
| `crates/pave-router` | Rust JSON PAVE vectors, generated agent identities, task candidates, and route scoring. |
| `crates/ui-bridge` | Adapters from kernel events into CLI, web SSE, and VSCode runtime event shapes. |
| `crates/rollout` | JSONL session recording and listing. |
| `crates/thread-store` | Storage-neutral thread persistence interface with local JSONL history and sidecar metadata. |
| `crates/state-store` | Local JSONL metadata/log shell for future state persistence. |
| `src/autonomy.rs` | Autonomous wake-up coordinator, LLM problem segmentation, suggestion cooldowns, and PAVE routing. |

## Reading Path

Start with [Quick Start](quick-start.md) if you want to run Marvis in VSCode. Use [Autonomy Showcase Demo](apps/autonomy-showcase.md) when you need a screen-recordable walkthrough of the autonomous scheduler. For implementation context, read [Product And Runtime Overview](design/overview.md), then [Architecture](design/architecture.md), then [Runtime Flows](design/flows.md). After that, each crate page can be read independently.

For interface-level lookup, use:

- [Module Index](appendix/module-index.md) for file-by-file responsibilities.
- [Interface Reference](appendix/interface-reference.md) for the public types, traits, and message contracts.

## Building The Web View

The source of truth is this markdown tree under `docs/src`. The web site is generated with mdBook:

```bash
target/mdbook-bin/bin/mdbook build docs
```

The generated site is written to `docs/site`. Open `docs/site/index.html` in a browser or serve `docs/site` from any static file server.
