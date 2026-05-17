# Scheduler

`crates/scheduler` implements the `session-kernel::Scheduler` trait. It turns kernel `TurnRequest` values into OpenAI-compatible chat-completion calls, streams assistant deltas, executes tool calls, and returns final response items.

## Public Types

| Type | Purpose |
| --- | --- |
| `OpenAiScheduler` | Real model scheduler backed by `openai-rs::Client`. |
| `SyntheticScheduler` | Deterministic fallback scheduler for bridge and status testing. |

## OpenAiScheduler

Constructors:

- `OpenAiScheduler::new(client)`
- `OpenAiScheduler::openrouter(api_key)`

`openrouter` builds a client with base URL:

```text
https://openrouter.ai/api/v1
```

and maps client construction errors into `KernelError::InvalidRequest`.

## Turn Execution

`run_turn` performs this loop:

1. Convert prior `ResponseItem` history into chat messages.
2. Insert the configured system prompt as the first message.
3. Concatenate text inputs from the current turn and append one user message.
4. Convert dynamic tool specs into OpenAI function tool definitions.
5. Send a streaming `chat/completions` request.
6. Emit `AgentMessageDelta` events as content deltas arrive.
7. Accumulate streamed tool-call deltas by index.
8. If there are no tool calls, return final assistant message as `SchedulerOutput`.
9. If there are tool calls, emit tool begin/end events, execute each tool, append tool results to the chat messages, and continue the loop.

The scheduler checks `request.cancellation.is_cancelled()` before starting a request and while reading stream events.

## Tool Call Assembly

OpenAI-compatible streaming tool calls arrive as indexed deltas. The scheduler accumulates three strings per index:

- tool call id
- function name
- function arguments

Only builders with a non-empty function name become `ToolCall` values. Function arguments are parsed as JSON. If parsing fails, an empty JSON object is passed to the tool executor.

## History Conversion

`history_to_messages` maps:

- `ResponseItem::Message` with role `system`, `user`, or `assistant` to corresponding chat messages.
- `ResponseItem::FunctionCallOutput` to a tool result message.
- `ResponseItem::FunctionCall` is ignored today.

This keeps current model requests simple but means persisted tool call records are not replayed as assistant tool-call messages yet.

## SyntheticScheduler

The synthetic scheduler is used when the VSCode runtime starts without `OPENROUTER_API_KEY`.

Behavior:

- Join text input items with newlines.
- Return `synthetic: <text>` or `ok` if empty.
- Emit one `AgentMessageDelta`.
- Return a single assistant `ResponseItem`.

This allows extension development, status collection, and bridge tests without a real model.

## Design Notes

The scheduler is the only crate that knows both kernel turn semantics and OpenAI-compatible chat types. That is the correct place for message mapping, stream parsing, and tool-loop behavior.

Provider-specific behavior is kept in `openai-rs`, while product-specific context construction stays in `status` and `src/vscode.rs`.
