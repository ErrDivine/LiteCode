# Runtime Flows

This page describes the major execution paths and where state changes occur.

## CLI Turn

```text
stdin line
  -> ui_bridge::user_text_op
  -> ThreadHandle::submit
  -> submission_loop
  -> run_user_turn
  -> Scheduler::run_turn
  -> EventEmitter events
  -> ThreadHandle::next_event
  -> ui_bridge::event_to_cli
  -> stdout/stderr
```

The CLI waits for `TurnComplete` or an error before accepting the next input line. It consumes `SessionConfigured` once immediately after creating the thread.

## Web Chat Turn

```text
POST /api/chat
  -> web::run_thread_loop
  -> start a new web thread
  -> inject prior web messages as ResponseItem history
  -> submit latest user message
  -> convert kernel events with event_to_web
  -> stream Server-Sent Events
```

The web harness is intentionally temporary. It uses `static/index.html` as a bundled frontend and starts a fresh thread for each request while injecting prior web messages into history.

## VSCode Prompt Turn

```text
VSCode command
  -> collectStatus()
  -> RuntimeClient request: user_prompt
  -> src/vscode.rs VscodeServer::run_prompt
  -> StatusStore::build_context_capsule
  -> ContextCapsule::to_prompt_context
  -> ThreadManager::start_thread_with_tools
  -> ThreadHandle::submit
  -> streamed AgentEvent responses
  -> final StatusReport refresh
```

The user prompt is wrapped in a deterministic context capsule before it enters the model. The model sees editor focus, cursor bubble, diagnostics, command failures, git state, and the original user request.

## Status Refresh

```text
VSCode event
  -> scheduleStatusRefresh debounce
  -> collectStatus()
  -> VscodeRequest::StatusUpdate
  -> StatusStore::update_vscode_status
  -> read_git_state
  -> segment_status
  -> StatusReport
  -> webview state update
```

The extension debounces editor and diagnostic changes by 500 ms. The Rust runtime resegments every time it receives new VSCode status, command results, or refreshed git state.

## Model And Tool Loop

```text
TurnRequest
  -> history_to_messages
  -> insert system prompt
  -> append user text
  -> tool_spec_to_openai
  -> streaming chat/completions
  -> emit AgentMessageDelta events
  -> accumulate tool call deltas
  -> execute each tool call
  -> append tool results to model messages
  -> repeat until no tool calls or finish_reason == stop
```

Tool calls are not persisted as `ResponseItem::FunctionCall` values today. The scheduler emits tool begin/end events and feeds tool results back to the model as chat tool messages. Final assistant text is returned as a `ResponseItem::Message` and persisted by the kernel.

## Persistence Flow

```text
ThreadManager::create_thread
  -> LocalHistoryStore::create_thread_record
  -> RolloutRecorder::new
  -> SessionMeta JSONL line

run_user_turn
  -> append user ResponseItem
  -> scheduler output
  -> append assistant ResponseItem
```

Rollout files live under:

```text
.lite-code/sessions/<thread-id>.jsonl
.lite-code/archived_sessions/<thread-id>.jsonl
```

Each line is a `RolloutItem`, either session metadata, a response item, or an event message.

## Cancellation And Steering

`ThreadHandle::submit(Op::Interrupt)` cancels an active turn by setting the active `CancellationFlag`. If the thread is idle, the kernel emits a `TurnAborted` event with reason `idle`.

`ThreadHandle::steer_input(...)` supports adding input while a turn is active. If a turn is active, input is queued in `pending_input`; otherwise it starts a new user turn. After the active turn finishes, queued input is submitted as the next `UserInput` operation.
