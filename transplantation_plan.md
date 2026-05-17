# Transplantation Plan

This document is an execution spec for a coding agent.

Follow it as written unless a later user instruction explicitly overrides it.

## 1. Mission

Transplant the stable infrastructure borrowed from the upstream Codex Rust workspace into `lite-code` as neutral local crates, then build a local runtime layer on top of them that:

- preserves the exposed runtime API shape of the original Codex project as closely as practical
- removes Codex branding from the final local crate architecture
- keeps model orchestration in a separate local `scheduler` crate
- keeps frontend and event adaptation in a separate local `ui-bridge` crate

This is not a whole-project port of `codex-core`.

## 2. Hard Constraints

These are mandatory.

### 2.1 Public Architecture Constraints

- The final workspace must not expose Codex-branded crate names.
- Each borrowed stable service layer must remain a separate crate.
- The live runtime must be implemented as a local forked crate named `session-kernel`.
- The public runtime API must align with the original Codex API surface at the manager and thread-handle level.
- Scheduler behavior must live outside `session-kernel`.
- UI and frontend adaptation must live outside `session-kernel`.

### 2.2 Scope Constraints

- Do not transplant `codex-core` wholesale.
- Do not merge `protocol`, `rollout`, `thread-store`, and `state-store` into one vendor crate.
- Do not pull MCP, skills, plugins, guardian, realtime, multi-agent, analytics, or network-proxy logic into phase 1 of `session-kernel`.
- Do not rewrite protocol payloads unless required by branding removal or local compilation.

### 2.3 Naming Constraints

- No new crate name may contain `codex`.
- Public type and function names should stay as close as possible to upstream unless the upstream name is explicitly Codex-branded.
- When a Codex-branded top-level type must be renamed, preserve its method set and behavior.

## 3. Final Target Layout

The target workspace layout is:

```text
crates/
  openai-rs/
  protocol/
  rollout/
  thread-store/
  state-store/
  session-kernel/
  scheduler/
  ui-bridge/
src/
static/
transplantation_plan.md
```

The target workspace manifest should contain these members:

```toml
[workspace]
members = [
  ".",
  "crates/openai-rs",
  "crates/protocol",
  "crates/rollout",
  "crates/thread-store",
  "crates/state-store",
  "crates/session-kernel",
  "crates/scheduler",
  "crates/ui-bridge",
]
resolver = "3"
```

## 4. Required Crates

### 4.1 `protocol`

Purpose:

- shared runtime-neutral types
- operations
- events
- thread ids
- history payloads
- config payloads needed by the runtime boundary

Source:

- upstream `codex-rs/protocol`

Rules:

- rename the package to `protocol`
- keep the semantics of upstream payloads as intact as possible
- only make changes required for local dependency rewrites, branding cleanup, or compilation

### 4.2 `rollout`

Purpose:

- transcript persistence
- rollout recorder
- thread history file layout
- metadata capture and replay support

Source:

- upstream `codex-rs/rollout`

Rules:

- rename the package to `rollout`
- preserve upstream persistence behavior
- do not move scheduler or UI behavior into this crate

### 4.3 `thread-store`

Purpose:

- storage-neutral thread persistence boundary
- local thread store implementation
- archive, list, read, resume, and metadata update operations

Source:

- upstream `codex-rs/thread-store`

Rules:

- rename the package to `thread-store`
- preserve the upstream trait boundary and behavior
- keep local and remote backends separated inside the crate

### 4.4 `state-store`

Purpose:

- SQLite-backed metadata storage
- migrations
- thread metadata runtime
- logs metadata support

Source:

- upstream `codex-rs/state`

Rules:

- rename the package to `state-store`
- preserve schema behavior in the first pass
- only make changes required for package renaming, dependency rewrites, or clear local integration needs

### 4.5 `session-kernel`

Purpose:

- live session state
- active turn state
- pending input handling
- submission loop
- event ordering and emission
- rollout replay for resume and fork
- coordination with persistence and the scheduler

Source:

- selected code transplanted from upstream `codex-rs/core`

Rules:

- this is the main local fork
- it must stay smaller and narrower than upstream `codex-core`
- it must not recreate a giant `SessionServices`-style dependency bucket

### 4.6 `scheduler`

Purpose:

- model selection
- reasoning effort selection
- turn orchestration policy
- continuation policy
- prewarm policy if needed

Source:

- local crate

Rules:

- keep all scheduling and model-routing policy here
- do not bury scheduling logic inside `session-kernel`

### 4.7 `ui-bridge`

Purpose:

- adapt runtime events to CLI and web UI behavior
- format output deltas and tool lifecycle updates
- translate thread-handle event streams into current frontend behavior

Source:

- local crate

Rules:

- keep all frontend-facing adaptation here
- do not place persistence or scheduling logic here

## 5. Upstream Name Mapping

Use this mapping unless a local implementation detail requires a narrower private name.

| Upstream name | Local name |
| --- | --- |
| `codex-protocol` | `protocol` |
| `codex-rollout` | `rollout` |
| `codex-thread-store` | `thread-store` |
| `codex-state` | `state-store` |
| `CodexThread` | `ThreadHandle` |
| `CodexSpawnOk` | `StartThreadOk` |
| `SessionConfiguration` | `SessionConfig` |
| internal `Codex` runner | `KernelRuntime` or `RuntimeCore` |

Keep `ThreadManager` as `ThreadManager`.

## 6. Public Runtime API Contract

The local runtime must preserve the upstream Codex interaction model.

That means:

- callers start or resume threads via a manager
- callers interact with one thread through a thread handle
- callers send operations into the thread
- callers consume an event stream back out of the thread

### 6.1 Manager API

The local manager must expose equivalents of these upstream capabilities:

- start a new thread
- start a new thread with tools
- resume a thread from rollout
- fork a thread
- list thread ids
- get a thread by id

The preferred shape is:

```rust
pub struct ThreadManager;
pub struct StartThreadOk {
    pub thread_id: ThreadId,
    pub thread: ThreadHandle,
}

impl ThreadManager {
    pub async fn start_thread(&self, config: Config) -> Result<StartThreadOk>;

    pub async fn start_thread_with_tools(
        &self,
        config: Config,
        dynamic_tools: Vec<DynamicToolSpec>,
        persist_extended_history: bool,
    ) -> Result<StartThreadOk>;

    pub async fn resume_thread_from_rollout(
        &self,
        config: Config,
        rollout_path: PathBuf,
    ) -> Result<StartThreadOk>;

    pub async fn fork_thread<S>(
        &self,
        source_thread_id: ThreadId,
        snapshot: S,
        config: Config,
    ) -> Result<StartThreadOk>
    where
        S: Into<ForkSnapshot>;
}
```

### 6.2 Thread Handle API

The local thread handle must expose equivalents of these upstream capabilities:

- submit an operation
- submit with explicit submission id
- submit with trace metadata
- consume the next event
- steer additional input into an active turn
- inject response items
- flush rollout
- inspect config snapshot
- inspect token usage snapshot if retained

The preferred shape is:

```rust
pub struct ThreadHandle;

impl ThreadHandle {
    pub async fn submit(&self, op: Op) -> Result<String>;

    pub async fn submit_with_id(&self, sub: Submission) -> Result<()>;

    pub async fn submit_with_trace(
        &self,
        op: Op,
        trace: Option<W3cTraceContext>,
    ) -> Result<String>;

    pub async fn next_event(&self) -> Result<Event>;

    pub async fn steer_input(
        &self,
        input: Vec<UserInput>,
        expected_turn_id: Option<&str>,
        client_metadata: Option<HashMap<String, String>>,
    ) -> Result<String, SteerInputError>;

    pub async fn inject_response_items(&self, items: Vec<ResponseItem>) -> Result<()>;

    pub async fn flush_rollout(&self) -> std::io::Result<()>;

    pub async fn config_snapshot(&self) -> ThreadConfigSnapshot;
}
```

### 6.3 Alignment Rules

Apply these rules when the upstream API and the local neutral naming requirement conflict:

1. Preserve method names when they are not branded.
2. Preserve payload shapes whenever practical.
3. Rename only top-level branded container types.
4. Prefer adapters over protocol rewrites.

## 7. `session-kernel` Intake Boundary

Do not copy all of upstream `core`.

Use the following intake rules.

### 7.1 Copy Early

These files or logic regions should be transplanted early into `session-kernel`:

- `core/src/context_manager/*`
- `core/src/state/session.rs`
- `core/src/state/turn.rs`
- `core/src/codex/rollout_reconstruction.rs`
- pending-work lifecycle logic from `core/src/tasks/mod.rs`
- regular-turn task shell from `core/src/tasks/regular.rs`
- selected session methods from `core/src/codex.rs`
- selected submission dispatch logic from `core/src/codex/handlers.rs`

### 7.2 Mine, Do Not Copy Whole

These files should be used as references and mined selectively:

- `core/src/codex/session.rs`
- `core/src/codex/turn.rs`
- `core/src/codex/turn_context.rs`
- `core/src/thread_manager.rs`
- `core/src/codex_thread.rs`

### 7.3 Explicitly Exclude In Phase 1

Do not import these subsystems into the first working version of `session-kernel`:

- MCP
- skills
- plugins
- guardian and review workflow
- realtime conversation
- managed network proxy
- sandbox orchestration
- multi-agent runtime
- analytics
- auth-specific app flow

If any of these are needed later, reintroduce them behind narrow traits. Do not re-expand the kernel into a second `codex-core`.

## 8. Internal Boundaries Inside `session-kernel`

`session-kernel` must not own everything directly.

Define narrow traits for collaboration points.

### 8.1 Required Traits

At minimum, introduce these boundaries:

```rust
pub trait HistoryStore: Send + Sync {
    // wraps rollout + thread-store + state-store
}

pub trait ModelBackend: Send + Sync {
    // low-level model execution and streaming
}

pub trait Scheduler: Send + Sync {
    // turn orchestration policy and model selection
}

pub trait ToolExecutor: Send + Sync {
    // tool call execution and tool result shaping
}

pub trait EventSink: Send + Sync {
    // event delivery to thread consumers
}
```

### 8.2 Ownership Rules

- `session-kernel` owns thread lifecycle, turn lifecycle, history mutation, persistence triggering, and event ordering.
- `scheduler` owns orchestration policy.
- `ui-bridge` owns presentation adaptation.
- `rollout`, `thread-store`, and `state-store` own persistence and storage behavior.

## 9. Execution Phases

Complete these phases in order.

Each phase has a clear exit condition.

### Phase 0. Freeze Upstream Source

Actions:

- record the exact upstream commit being transplanted
- write a provenance note listing the source location of each borrowed crate
- create a migration branch
- add the target workspace members to `Cargo.toml`

Outputs:

- a fixed upstream commit reference
- a provenance document
- updated workspace skeleton

Exit criteria:

- the upstream source snapshot is fixed
- the workspace layout is in place

### Phase 1. Vendor Stable Service Crates

Actions:

- copy upstream `protocol` into `crates/protocol`
- copy upstream `rollout` into `crates/rollout`
- copy upstream `thread-store` into `crates/thread-store`
- copy upstream `state` into `crates/state-store`
- rename package names and internal path dependencies
- make each crate compile in the local workspace

Rules:

- keep each service layer as its own crate
- keep semantic changes minimal
- avoid feature additions in this phase

Outputs:

- four compiling local service crates

Exit criteria:

- `protocol`, `rollout`, `thread-store`, and `state-store` compile
- none of the new crate package names contain `codex`

### Phase 2. Build the `session-kernel` Skeleton

Actions:

- create `crates/session-kernel`
- define the core public types: `ThreadManager`, `ThreadHandle`, `StartThreadOk`, `SessionConfig`, `ForkSnapshot`
- implement submission and event channels
- port the upstream submission-loop pattern
- add placeholder trait boundaries for history, model, tools, and event delivery

Outputs:

- a minimal working runtime shell

Exit criteria:

- a thread can be started
- `submit(Op)` works for a synthetic no-model operation
- `next_event()` yields ordered events from the synthetic operation path

### Phase 3. Port State, History, and Replay

Actions:

- port `ContextManager`
- port `SessionState`
- port `ActiveTurn` and `TurnState`
- port pending-input semantics
- port idle-turn wakeup semantics
- port rollout reconstruction for resume and fork
- connect history mutation to rollout persistence

Outputs:

- functional local session state and replay behavior

Exit criteria:

- resume works
- fork works
- queued next-turn input works
- interrupt and pending-input behavior match the intended upstream semantics

### Phase 4. Insert the Scheduler Boundary

Actions:

- create `crates/scheduler`
- define the `Scheduler` trait if not already created
- move model selection and turn execution policy behind the scheduler boundary
- implement a default scheduler that is behaviorally close to upstream regular-turn flow

Outputs:

- a replaceable orchestration layer

Exit criteria:

- user turns run through `submit(Op::UserTurn | Op::UserInput)` without hardcoded orchestration logic living directly inside `session-kernel`
- scheduler policy can be changed without touching persistence or thread lifecycle code

### Phase 5. Add the Compatibility Layer

Actions:

- finish porting the useful parts of upstream `ThreadManager`
- finish porting the useful parts of upstream `CodexThread` under the local `ThreadHandle` name
- preserve method names and semantics where branding does not force a rename

Outputs:

- a stable local runtime interface aligned with the upstream interaction model

Exit criteria:

- the manager and thread-handle APIs match the intended contract in Section 6

### Phase 6. Add `ui-bridge`

Actions:

- create `crates/ui-bridge`
- map thread events into CLI output behavior
- map thread events into web SSE behavior
- remove the duplicated direct model loops from `src/main.rs` and `src/web.rs`

Outputs:

- frontends driven by thread events rather than bespoke model loops

Exit criteria:

- both CLI and web entrypoints use the thread manager and thread handle APIs
- no direct duplicated agent loop remains in the top-level frontend code

### Phase 7. Stabilize With Tests

Actions:

- add event-order tests
- add resume and fork tests
- add interrupt and pending-input tests
- add persistence tests for rollout and thread-store integration
- add integration tests for CLI and web event adaptation

Outputs:

- a protected behavioral baseline

Exit criteria:

- representative flows pass end-to-end
- major divergences from upstream behavior are documented intentionally

## 10. Required Tests

The following test cases are mandatory.

### 10.1 Runtime Flow Tests

- start thread -> submit user op -> consume event stream
- resume thread from rollout
- fork thread from an existing thread
- interrupt an active turn
- inject response items into a thread
- queue next-turn input and wake an idle thread

### 10.2 Persistence Tests

- rollout is written in the expected order
- thread-store can list, read, update, and archive threads
- state-store migrations run cleanly in the local workspace

### 10.3 Compatibility Tests

- manager API behavior matches the intended upstream flow
- thread-handle API behavior matches the intended upstream flow
- event ordering for a normal turn is stable
- event ordering for an interrupted turn is stable

### 10.4 Frontend Integration Tests

- CLI path is driven by `ThreadManager` plus `ThreadHandle`
- web path is driven by `ThreadHandle` event streaming

## 11. Change Rules For Borrowed Crates

When editing `protocol`, `rollout`, `thread-store`, or `state-store`, follow these rules.

### Allowed Changes

- package renaming
- dependency path rewriting
- visibility adjustments required by local use
- documentation cleanup
- branding cleanup in crate-level naming and local docs

### Changes To Avoid In Early Phases

- persistence behavior changes
- schema changes
- event ordering changes
- payload shape rewrites
- scheduler logic inside service crates
- UI logic inside service crates

### Preferred Placement For Local Divergence

- runtime divergence goes in `session-kernel`
- orchestration divergence goes in `scheduler`
- presentation divergence goes in `ui-bridge`

## 12. Deliverables

At the end of the transplant, the workspace must contain:

- four separate neutral service crates: `protocol`, `rollout`, `thread-store`, `state-store`
- one forked runtime crate: `session-kernel`
- one orchestration crate: `scheduler`
- one presentation adapter crate: `ui-bridge`
- a manager and thread-handle API aligned with the original Codex interaction model
- tests covering resume, fork, event ordering, interrupt behavior, and persistence

## 13. Definition of Done

The transplant is complete only when all of the following are true:

1. The stable borrowed service layers exist as separate local crates.
2. No new crate package name contains `codex`.
3. The local runtime is exposed through `ThreadManager` and `ThreadHandle`.
4. The local runtime can start, resume, fork, submit, and stream events.
5. Scheduler logic is outside `session-kernel`.
6. Frontend adaptation is outside `session-kernel`.
7. CLI and web frontends no longer own duplicated direct model loops.
8. The required tests in Section 10 pass.
