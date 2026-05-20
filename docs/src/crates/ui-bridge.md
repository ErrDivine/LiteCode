# UI Bridge

`crates/ui-bridge` adapts protocol events and operations for CLI, web, and VSCode surfaces. It also defines the JSON envelopes used by the VSCode extension.

## VSCode Request Envelope

`VscodeRequestEnvelope` contains:

- `id: u64`
- flattened `VscodeRequest`

`VscodeRequest` variants:

| Variant | Payload |
| --- | --- |
| `Initialize` | workspace root, optional model, optional API key, optional base URL, optional thinking mode, optional reasoning effort, optional max tokens |
| `StatusUpdate` | `VscodeStatus` |
| `CommandResult` | `CommandResult` |
| `UserPrompt` | prompt plus optional `VscodeStatus` and per-prompt approval grants |
| `AutonomyTick` | `VscodeStatus` and trigger. Legacy `agent_profiles` payloads are accepted but ignored by the runtime. |
| `RunSuggestedTask` | suggestion id plus requested approval grants; runtime caps them to the stored route approval |
| `DismissSuggestion` | suggestion id |
| `Shutdown` | no payload |

These requests are sent as newline-delimited JSON over child-process stdio.

## VSCode Response Envelope

`VscodeResponseEnvelope` contains:

- optional `id`
- flattened `VscodeResponse`

Constructors:

- `for_request(id, response)`
- `notification(response)`

`VscodeResponse` variants:

- `Ready`
- `StatusReport`
- `AgentEvent`
- `ProcessUpdate`
- `AutonomyDecision`
- `Complete`
- `Error`
- `ShutdownComplete`

The `Ready` response includes workspace root, model, base URL, and initial status report.

`AutonomyDecision` is one of `idle`, `suggest`, or `suppressed`. A suggestion includes the selected route, task, agent profile, route explanation, and required approval flags.

## VSCode Runtime Events

`VscodeRuntimeEvent` variants:

- `Delta { text }`
- `AgentMessage { text }`
- `ToolStart { name, arguments }`
- `ToolEnd { name, output }`
- `TurnStarted { turn_id }`
- `TurnComplete { turn_id, summary }`
- `Error { message }`
- `Ignore`

`event_to_vscode` maps kernel protocol events into these runtime events.

## Web Events

`WebEvent` contains:

- `event`: static event name
- `data`: JSON payload

`event_to_web` maps:

- `AgentMessageDelta` -> `delta`
- `ToolCallBegin` -> `tool_start`
- `ToolCallEnd` -> `tool_end`
- `Error` -> `error`
- `TurnComplete` -> `done`
- `TurnAborted` -> `error`

The web harness writes these as Server-Sent Events.

## CLI Events

`CliEvent` variants:

- `Print`
- `ToolStart`
- `ToolEnd`
- `Error`
- `Done`
- `Ignore`

`event_to_cli` maps kernel events to terminal behavior. The CLI prints deltas to stdout, tool events to stderr, and stops a turn on `Done` or `Error`.

## Operation Helper

`user_text_op(text)` creates a protocol `Op::UserInput` with one text item and no final JSON schema or client metadata.

## Design Notes

The bridge is intentionally shallow. It should map and format, not decide runtime behavior. Business logic belongs in `session-kernel`, `scheduler`, `status`, `pave-router`, or the binary composition layer.
