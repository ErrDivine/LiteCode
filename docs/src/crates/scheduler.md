# Scheduler

`crates/scheduler` implements the `session-kernel::Scheduler` trait. It turns kernel `TurnRequest` values into OpenAI-compatible chat-completion calls, streams assistant deltas, executes tool calls, and returns final response items.

## Public Types

| Type | Purpose |
| --- | --- |
| `OpenAiScheduler` | Real model scheduler backed by `openai-rs::Client`. |
| `OpenAiCompatibleConfig` | API key and base URL for an OpenAI-compatible provider. |
| `ModelRequestOptions` | Thinking mode and optional reasoning effort applied to chat-completion requests. |
| `SyntheticScheduler` | Deterministic scheduler used by tests/dev code, not by the VSCode product fallback path. |

## OpenAiScheduler

Constructors:

- `OpenAiScheduler::new(client)`
- `OpenAiScheduler::openai_compatible(config)`

`openai_compatible` builds a client from:

- `config.api_key`
- `config.base_url`
- `config.request_options`

and maps client construction errors into `KernelError::InvalidRequest`.

`ModelRequestOptions` defaults to `auto`: DeepSeek-compatible base URLs receive `{"thinking":{"type":"disabled"}}`, while other providers omit the `thinking` parameter. Explicit `enabled`, `disabled`, and `provider_default` modes are also supported.

## Turn Execution

`run_turn` performs this loop:

1. Convert prior `ResponseItem` history into chat messages.
2. Insert the configured system prompt as the first message.
3. Concatenate text inputs from the current turn and append one user message.
4. Convert dynamic tool specs into OpenAI function tool definitions.
5. Send a streaming `chat/completions` request.
6. Emit `AgentMessageDelta` events as content deltas arrive.
7. Accumulate streamed reasoning-content and tool-call deltas by index.
8. If there are no tool calls, return final assistant message as `SchedulerOutput`.
9. If there are tool calls, emit tool begin/end events, execute each tool, append tool results to the chat messages, and continue the loop.

The scheduler checks `request.cancellation.is_cancelled()` before starting a request and while reading stream events. It also stops execution when `request.max_tool_calls` is exceeded.

## Tool Call Assembly

OpenAI-compatible streaming tool calls arrive as indexed deltas. The scheduler accumulates three strings per index:

- tool call id
- function name
- function arguments

Only builders with a non-empty function name become `ToolCall` values. Function arguments are parsed as JSON. If parsing fails, an empty JSON object is passed to the tool executor.

## History Conversion

`history_to_messages` maps:

- `ResponseItem::Message` with role `system`, `user`, or `assistant` to corresponding chat messages.
- Consecutive `ResponseItem::FunctionCall` values to one assistant message with tool calls.
- `ResponseItem::FunctionCallOutput` to a tool result message.

The scheduler returns function-call and function-call-output response items alongside the final assistant message, so future turns can replay tool history without malformed tool-result-only chat history.

When a thinking-enabled provider streams `reasoning_content`, the scheduler stores it on assistant messages. For tool-call assistant messages, the content is stored on the first `FunctionCall` response item and restored when replaying history.

## SyntheticScheduler

The synthetic scheduler is used by tests and explicit development code only.

Behavior:

- Join text input items with newlines.
- Return `synthetic: <text>` or `ok` if empty.
- Emit one `AgentMessageDelta`.
- Return a single assistant `ResponseItem`.

It is not selected automatically by the VSCode product when provider config is missing.

## Design Notes

The scheduler is the only crate that knows both kernel turn semantics and OpenAI-compatible chat types. That is the correct place for message mapping, stream parsing, and tool-loop behavior.

Provider-specific behavior is kept in `openai-rs`, while product-specific context construction stays in `status` and `src/vscode.rs`.
