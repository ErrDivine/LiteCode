# Module Index

This index lists every source module and its design responsibility.

## Root Binary Crate

| File | Responsibility |
| --- | --- |
| `src/main.rs` | Binary entrypoint, CLI parsing, mode selection, runtime composition, CLI loop, shared system prompt. |
| `src/web.rs` | Axum web harness, static HTML serving, chat endpoint, SSE event stream, web thread setup. |
| `src/vscode.rs` | VSCode stdio server, request dispatch, status store integration, context capsule submission, streamed VSCode events. |
| `src/tools.rs` | Dynamic local tool definitions and executor implementation. |
| `static/index.html` | Temporary web harness frontend bundled into `src/web.rs`. |

## VSCode Application

| File | Responsibility |
| --- | --- |
| `apps/vscode-extension/package.json` | Extension manifest, activation events, commands, menus, settings, scripts. |
| `apps/vscode-extension/extension.js` | Extension controller, runtime child process client, status collection, webview panel, code actions. |
| `apps/vscode-extension/README.md` | Development launch guide. |

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
| `crates/scheduler/src/lib.rs` | OpenAI-compatible scheduler, synthetic scheduler, dynamic tool conversion, response history conversion. |

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

## Persistence

| File | Responsibility |
| --- | --- |
| `crates/rollout/src/lib.rs` | JSONL session metadata and response history recording, reading, listing, archive path support. |
| `crates/thread-store/src/lib.rs` | Thread persistence trait, local rollout-backed store, remote stub, append helper. |
| `crates/state-store/src/lib.rs` | Local metadata/log appenders and migration marker. |

## `crates/ui-bridge`

| File | Responsibility |
| --- | --- |
| `crates/ui-bridge/src/lib.rs` | VSCode request/response envelopes, runtime event mapping, web event mapping, CLI event mapping, user text operation helper. |
