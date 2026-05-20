# Module Index

This index lists every source module and its design responsibility.

## Root Binary Crate

| File | Responsibility |
| --- | --- |
| `build.rs` | Generates the binary-safe bundled skill catalog from `skills/system/**/SKILL.md`. |
| `src/main.rs` | Binary entrypoint, CLI parsing, mode selection, runtime composition, CLI loop, shared system prompt. |
| `src/autonomy.rs` | Autonomous wake-up coordinator, LLM problem segmentation, suggestion cooldowns, PAVE routing, accepted suggestion prompt building. |
| `src/skills.rs` | Codex-style skill package loading, bundled skill materialization, resource indexing, and selected skill prompt rendering. |
| `src/skill_mcp.rs` | Skill registry, selected skill rendering, stdio MCP discovery, tool qualification, and MCP tool calls. |
| `src/web.rs` | Axum web harness, static HTML serving, chat endpoint, SSE event stream, web thread setup. |
| `src/vscode.rs` | VSCode stdio server, request dispatch, status store integration, context capsule submission, streamed VSCode events. |
| `src/tools.rs` | Dynamic local/MCP tool definitions, executor implementation, policy checks, and rollback snapshots. |
| `static/index.html` | Temporary web harness frontend bundled into `src/web.rs`. |

## VSCode Application

| File | Responsibility |
| --- | --- |
| `apps/vscode-extension/package.json` | Extension manifest, activation events, commands, menus, settings, scripts. |
| `apps/vscode-extension/extension.js` | Extension controller, runtime child process client, status collection, webview panel, code actions. |
| `apps/vscode-extension/README.md` | Development launch guide. |

## Demo Workspaces

| File | Responsibility |
| --- | --- |
| `demos/autonomy-showcase/launch.sh` | Builds the debug runtime and opens VSCode with the Marvis and demo-driver development extensions. |
| `demos/autonomy-showcase/scripts/verify-traps.js` | Headless verifier that ensures the clean demo workspace passes and each timed trap produces a failing task signal. |
| `demos/autonomy-showcase/driver-extension/extension.js` | Drives the timed autonomy showcase by resetting files, focusing editor context, running tasks, and waiting for Marvis. |
| `demos/autonomy-showcase/workspace` | Disposable JavaScript and documentation project used by the showcase traps. |

## `crates/protocol`

| File | Responsibility |
| --- | --- |
| `crates/protocol/src/lib.rs` | Operations, inputs, response items, events, metadata, rollout item enum, compatibility re-exports. |
| `crates/protocol/src/thread_id.rs` | Thread id generation, validation, display, conversion, and serde. |

## `crates/session-kernel`

| File | Responsibility |
| --- | --- |
| `crates/session-kernel/src/lib.rs` | Runtime errors, config, traits, thread manager, thread handle, event emitter, cancellation, submission loop, history store. |

## `crates/scheduler`

| File | Responsibility |
| --- | --- |
| `crates/scheduler/src/lib.rs` | OpenAI-compatible scheduler config, model turn loop, tool budget enforcement, synthetic test scheduler, dynamic tool conversion, response history conversion. |

## `crates/openai-rs`

| File | Responsibility |
| --- | --- |
| `crates/openai-rs/src/lib.rs` | Module declarations and top-level re-exports. |
| `crates/openai-rs/src/auth.rs` | Auth provider abstraction and bearer auth. |
| `crates/openai-rs/src/client.rs` | Client builder and chat namespace access. |
| `crates/openai-rs/src/provider.rs` | Provider URL construction, headers, retry config, stream timeout. |
| `crates/openai-rs/src/request.rs` | Transport-neutral request and response types. |
| `crates/openai-rs/src/transport.rs` | HTTP transport trait and reqwest transport. |
| `crates/openai-rs/src/retry.rs` | Retry predicates and backoff. |
| `crates/openai-rs/src/sse.rs` | SSE stream decoding task. |
| `crates/openai-rs/src/error.rs` | Transport, stream, and API errors. |
| `crates/openai-rs/src/chat/mod.rs` | Chat namespace. |
| `crates/openai-rs/src/chat/completions.rs` | Streaming and non-streaming chat completions. |
| `crates/openai-rs/src/types/mod.rs` | Type module re-exports. |
| `crates/openai-rs/src/types/chat.rs` | Chat request, messages, roles, tool calls, responses. |
| `crates/openai-rs/src/types/common.rs` | Tool definitions, function definitions, usage, finish reasons. |
| `crates/openai-rs/src/types/stream.rs` | Streaming response chunks and deltas. |

## `crates/status`

| File | Responsibility |
| --- | --- |
| `crates/status/src/lib.rs` | Workspace status model, VSCode status model, git parsing, segmentation, stuckness detection, context capsules. |

## `crates/pave-router`

| File | Responsibility |
| --- | --- |
| `crates/pave-router/src/lib.rs` | Sparse PAVE vectors, tool access flags, generated agent identities, task candidates, and route scoring. |

## Persistence

| File | Responsibility |
| --- | --- |
| `crates/rollout/src/lib.rs` | JSONL session metadata, response history, event trace recording, reading, listing, archive path support. |
| `crates/thread-store/src/lib.rs` | Thread persistence trait, local rollout-backed store, metadata sidecars, explicit unavailable-store adapter, append helper. |
| `crates/state-store/src/lib.rs` | Local metadata/log appenders and migration marker. |

## `crates/ui-bridge`

| File | Responsibility |
| --- | --- |
| `crates/ui-bridge/src/lib.rs` | VSCode request/response envelopes, runtime event mapping, web event mapping, CLI event mapping, user text operation helper. |
