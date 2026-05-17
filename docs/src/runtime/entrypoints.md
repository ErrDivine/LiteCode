# Binary, CLI, Web, And VSCode Stdio

The root binary crate composes all runtime pieces and exposes three execution modes.

## `src/main.rs`

Modules:

- `tools`
- `vscode`
- `web`

Main responsibilities:

- Parse CLI flags.
- Select VSCode stdio, web, or CLI mode.
- Read `OPENROUTER_API_KEY` for CLI and web modes.
- Build `OpenAiScheduler`, `LocalToolExecutor`, and `ThreadManager`.
- Start a thread with local tool definitions.
- Consume runtime events and adapt them for the selected UI.

### CLI Flags

`Cli` fields:

- `--web`: launch web UI instead of CLI mode.
- `--vscode-stdio`: run the VSCode extension JSON stdio bridge.
- `--model`, `-m`: model name, default `nvidia/nemotron-3-super-120b-a12b:free`.
- `--max-tokens`: maximum tokens per response, default `4096`.

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

1. Require `OPENROUTER_API_KEY`.
2. Start an OpenRouter scheduler.
3. Start a thread with local dynamic tools.
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

`serve_stdio(default_model, default_max_tokens)`:

- Reads lines from stdin.
- Parses `VscodeRequestEnvelope`.
- Sends an error notification for invalid JSON.
- Dispatches to `VscodeServer::handle_request`.
- Writes one JSON response per line to stdout.

### VscodeServer State

Fields:

- workspace root
- `StatusStore`
- optional `ThreadManager`
- model
- max tokens
- synthetic-model flag

### Request Handling

`Initialize`:

- Sets workspace root, model, and max tokens.
- Creates a new `StatusStore`.
- Uses `OpenAiScheduler` if `OPENROUTER_API_KEY` is set.
- Uses `SyntheticScheduler` otherwise.
- Creates a `ThreadManager`.
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
- Builds a context capsule.
- Starts a new thread with tools.
- Submits the capsule text.
- Streams mapped runtime events as `AgentEvent`.
- Refreshes status and returns `Complete`.

`Shutdown`:

- Returns `ShutdownComplete`.
- Stops the stdio loop.

### VSCode System Prompt

`VSCODE_SYSTEM_PROMPT` tells the model to treat VSCode status as data, pay attention to active editor, cursor, diagnostics, command failures, and git state, and to be cautious with risky edits.

## Design Notes

The binary crate is currently the composition root and tool owner. That is acceptable for the current size, but moving `src/tools.rs` into a tool-gateway crate would make the runtime boundary cleaner.
