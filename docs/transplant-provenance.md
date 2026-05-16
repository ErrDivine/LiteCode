# Transplant Provenance

This file records the fixed upstream source used for the local runtime transplant.

- Upstream checkout: `/Users/errdivine/ErrDivine/Rust/codex`
- Upstream Rust workspace: `/Users/errdivine/ErrDivine/Rust/codex/codex-rs`
- Upstream commit: `dae0608c06bf61a356209fd11243aec1ef816547`
- Local migration branch: `codex/session-kernel-transplant`

## Borrowed Crate Sources

| Local crate | Upstream source |
| --- | --- |
| `protocol` | `codex-rs/protocol` |
| `rollout` | `codex-rs/rollout` |
| `thread-store` | `codex-rs/thread-store` |
| `state-store` | `codex-rs/state` |
| `session-kernel` | selected manager, thread-handle, session-state, and turn-loop ideas from `codex-rs/core` |

## Phase 1 Note

The first local copy keeps the public shape and behavior needed by `lite-code`, but does not copy the whole upstream crate bodies. The upstream service crates depend on many Codex-specific helper crates and systems that the plan explicitly excludes from phase 1. The local crates are neutral, compile inside this workspace, and leave room to port more upstream internals later.
