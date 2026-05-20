# Binary, CLI, Web, And VSCode Stdio

The root binary crate composes all runtime pieces and exposes three execution modes.

## `src/main.rs`

Modules:

- `tools`
- `autonomy`
- `vscode`
- `web`

Main responsibilities:

- Parse CLI flags.
- Select VSCode stdio, web, or CLI mode.
- Read OpenAI-compatible provider configuration.
- Build `OpenAiScheduler`, `LocalToolExecutor`, and `ThreadManager`.
- Start a thread with policy-filtered local tool definitions.
- Consume runtime events and adapt them for the selected UI.
- In VSCode mode, evaluate autonomous status ticks and route suggestions.

### CLI Flags

`Cli` fields:

- `--web`: launch web UI instead of CLI mode.
- `--vscode-stdio`: run the VSCode extension JSON stdio bridge.
- `--print-trace <path>`: print a rollout trace summary and exit.
- `--model`, `-m`: model name, default `gpt-4.1-mini`.
- `--base-url`: OpenAI-compatible base URL override.
- `--max-tokens`: maximum tokens per response, default `4096`.
- `--allow-workspace-write`: allow file writes in CLI/web harnesses.
- `--allow-risky-shell`: allow broader shell/git/network-like commands in CLI/web harnesses.

### System Prompt

`SYSTEM_PROMPT` tells the model it is a coding assistant inside the user's project directory, with tools for shell, read/write/edit, directory listing, and search. It asks the model to inspect real project contents, prefer targeted edits, verify progress, and summarize briefly.

### Runtime Config

`runtime_config(cli, history_root)` builds `SessionConfig`:

- model from CLI
- cwd from current directory
- system prompt from `SYSTEM_PROMPT`
- max tokens from CLI
- history root from `.lite-code`

## CLI Mode

Flow:

1. Require `MARVIS_API_KEY`.
2. Start an OpenAI-compatible scheduler.
3. Start a thread with policy-filtered local dynamic tools.
4. Read stdin lines.
5. Submit each line as text input.
6. Print streamed deltas, tool logs, errors, and completion.

Typing `exit` ends the loop.

## `src/web.rs`

The web harness uses Axum.

Routes:

- `GET /`: serves `static/index.html`.
- `POST /api/chat`: accepts JSON messages and returns SSE.

`AppState` contains:

- `ThreadManager`
- history root
- model
- max tokens
- tool policy

`run_thread_loop`:

1. Validates latest user message.
2. Starts a new web thread.
3. Converts prior web messages into `ResponseItem` history.
4. Injects history into the thread.
5. Submits latest user text.
6. Converts runtime events through `event_to_web`.
7. Sends `[DONE]` at the end.

`start_web_thread` sets `SessionSource::Web` and uses the root system prompt.

## `src/vscode.rs`

The VSCode stdio server is the product runtime surface.

### Public Entrypoint

`serve_stdio(default_model, default_base_url, default_max_tokens)`:

- Reads lines from stdin.
- Parses `VscodeRequestEnvelope`.
- Sends an error notification for invalid JSON.
- Dispatches to `VscodeServer::handle_request`.
- Writes one JSON response per line to stdout.

### VscodeServer State

Fields:

- workspace root
- `StatusStore`
- optional OpenAI-compatible provider config
- model
- base URL
- max tokens
- next process id
- auto-detected agent profiles
- autonomy coordinator

### Request Handling

`Initialize`:

- Sets workspace root, model, and max tokens.
- Creates a new `StatusStore`.
- Loads `marvis.apiKey` and base URL from the initialize payload.
- Loads bundled and workspace skills, then generates same-model agent identities from skills and built-in toolsets.
- Returns `Ready`.

`StatusUpdate`:

- Updates VSCode status.
- Refreshes git state.
- Returns `StatusReport`.

`CommandResult`:

- Ingests command result.
- Refreshes git state.
- Returns `StatusReport`.

`UserPrompt`:

- Optionally updates status.
- Applies per-prompt approval to build a tool policy.
- Builds a context capsule.
- Starts a new thread with policy-filtered tools.
- Emits process updates.
- Submits the capsule text.
- Streams mapped runtime events as `AgentEvent`.
- Refreshes status and returns `Complete`.

`AutonomyTick`:

- Updates VSCode status and git state.
- Skips in-flight, unchanged, busy, ambiguous, or merely dirty/risky state.
- Calls the OpenAI-compatible model for strict JSON task segmentation when VSCode status points to a focused, useful next step; the segmenter prompt biases toward specific suggest-first action rather than waiting for perfect certainty.
- Requires the segmenter to choose an agent id from the auto-detected skill/toolset agents before permission is requested.
- Uses PAVE matching only as a compatibility fallback when older task payloads do not include an agent id.
- Returns `AutonomyDecision::Idle`, `Suggest`, or `Suppressed`.

`RunSuggestedTask`:

- Looks up a stored suggestion.
- Caps the request approval to the stored routed approval.
- Resolves the selected agent's skills and MCP servers.
- Adds selected skill instructions to the system prompt.
- Filters visible tools by runtime policy, selected skill package local-tool dependencies, and the generated agent tool allowlist.
- Exposes discovered stdio MCP tools only when MCP discovery succeeds.
- Runs the existing bounded VSCode prompt flow.

`DismissSuggestion`:

- Records a cooldown for the suggestion id.
- Returns a suppressed autonomy decision.

`Shutdown`:

- Returns `ShutdownComplete`.
- Stops the stdio loop.

### VSCode System Prompt

`VSCODE_SYSTEM_PROMPT` tells the model to treat VSCode status as data, pay attention to active editor, cursor, diagnostics, command failures, and git state, infer the immediate user intent implied by that status, stay focused on that intent, use no more than 15 tool calls in one turn, and be cautious with risky edits.

## Design Notes

The binary crate is currently the composition root and tool owner. That is acceptable for the current size, but moving `src/tools.rs` into a tool-gateway crate would make the runtime boundary cleaner.
