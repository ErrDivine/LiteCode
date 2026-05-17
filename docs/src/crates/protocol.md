# Protocol Crate

`crates/protocol` is the shared runtime vocabulary. It contains no scheduling, UI, transport, or storage behavior. Its job is to make process messages, rollout lines, and internal runtime operations explicit and serializable.

## Modules

| Module | Purpose |
| --- | --- |
| `lib.rs` | Defines operations, user inputs, response items, events, session metadata, tool specs, rollout items, and compatibility re-export modules. |
| `thread_id.rs` | Defines `ThreadId`, generation, parsing, display, and serde behavior. |

Compatibility modules in `lib.rs` re-export the current types:

- `protocol` re-exports the broad event and operation surface.
- `user_input` re-exports `UserInput`, `TextElement`, and `ByteRange`.
- `models` re-exports `ContentItem`, `MessagePhase`, and `ResponseItem`.
- `dynamic_tools` re-exports `DynamicToolSpec`.

These modules let older call sites keep using grouped paths while the source definitions remain in one file.

## Operation Model

`Submission` wraps an `Op` with:

- `id`: caller-provided or generated submission id.
- `op`: requested runtime operation.
- `trace`: optional `W3cTraceContext`.

`Op` variants:

| Variant | Design role |
| --- | --- |
| `Interrupt` | Cancels the active turn or reports that the thread is idle. |
| `UserInput` | Normal user input. Supports multiple `UserInput` items, optional final JSON schema, and optional client metadata. |
| `UserTurn` | User input plus per-turn overrides for cwd, approval policy, sandbox policy, and model. |
| `InjectResponseItems` | Directly adds prior response items to thread history. Used by web history injection and resume-style workflows. |
| `Synthetic` | Convenience operation that turns a message into text input. Useful for tests and harnesses. |

`Op::user_text` is a helper for simple text turns.

## User Input Model

`UserInput` supports:

- `Text { text, text_elements }`
- `Image { image_url }`
- `LocalImage { path }`
- `Skill { name, path }`
- `Mention { name, path }`

The current scheduler only uses text inputs. The broader shape allows future UI shells to attach images, skills, and connector mentions without changing the kernel submission model.

`TextElement` uses a `ByteRange` and optional placeholder. This gives the protocol a way to represent structured selections or inline mentions inside text.

## Response Item Model

`ResponseItem` is the persisted conversation unit:

| Variant | Meaning |
| --- | --- |
| `Message` | User, assistant, or system-style content. Content is a list of `ContentItem`s. |
| `FunctionCall` | A model tool call record. Defined in the protocol, but not currently persisted by the scheduler. |
| `FunctionCallOutput` | Tool output associated with a call id. |

`ResponseItem::message(role, text)` picks `InputText` for non-assistant roles and `OutputText` for assistant role. `role()` and `text()` are convenience accessors used by the kernel and scheduler.

`MessagePhase` distinguishes `commentary` from `final_answer` for future surfaces that need channel-aware rendering.

## Event Model

`Event` contains a submission id and an `EventMsg`.

`EventMsg` variants:

- `Error`
- `Warning`
- `SessionConfigured`
- `ThreadNameUpdated`
- `TurnStarted`
- `TurnComplete`
- `TurnAborted`
- `UserMessage`
- `AgentMessage`
- `AgentMessageDelta`
- `ToolCallBegin`
- `ToolCallEnd`
- `TokenCount`
- `ShutdownComplete`

Event structs are intentionally small and UI-neutral. For example, `ToolCallBeginEvent` contains `call_id`, `name`, and serialized `arguments`; presentation is handled in `ui-bridge`.

## Session Metadata And Rollout Records

`SessionMeta` captures persistent thread metadata:

- `id`
- `forked_from_id`
- `timestamp`
- `cwd`
- `originator`
- `cli_version`
- `source`
- `model_provider`
- `dynamic_tools`

`RolloutItem` wraps the JSONL line types:

- `SessionMeta`
- `ResponseItem`
- `EventMsg`

Today the runtime writes session metadata and response items. The event variant exists for richer trace persistence.

## ThreadId

`ThreadId` is a newtype over `String`.

Key properties:

- `ThreadId::new()` produces non-empty id strings using time and an atomic counter.
- `from_string` trims and rejects empty ids.
- `Display`, `From<ThreadId> for String`, `TryFrom<&str>`, `Serialize`, and `Deserialize` are implemented.

This type protects internal APIs from accepting arbitrary empty thread ids while staying JSON-friendly.
