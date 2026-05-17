# VSCode Extension

`apps/vscode-extension` is the primary product shell for Marvis. It collects IDE state, starts the Rust runtime, renders status and trace information, and exposes user commands.

## Files

| File | Purpose |
| --- | --- |
| `package.json` | VSCode contribution manifest, commands, configuration, activation events, and check script. |
| `extension.js` | Extension implementation. |
| `README.md` | Development instructions for launching the extension. |

## Commands

The extension contributes:

- `Marvis: Start Marvis`
- `Marvis: Show Status`
- `Marvis: Refresh Status`
- `Marvis: Ask Marvis`
- `Marvis: Ask About Selection`
- `Marvis: Fix Near Cursor`
- `Marvis: Explain Diagnostic`
- `Marvis: Record Terminal Failure`
- `Marvis: Run Command and Record Result`
- `Marvis: Run VSCode Task and Record Result`

The editor context menu includes ask-about-selection and fix-near-cursor.

## Configuration

Settings:

| Setting | Default | Meaning |
| --- | --- | --- |
| `marvis.runtimePath` | empty | Optional path to the `lite-code` binary. |
| `marvis.model` | `nvidia/nemotron-3-super-120b-a12b:free` | Model passed to the Rust runtime. |
| `marvis.maxTokens` | `4096` | Maximum response tokens per model turn. |

## Activation

The extension activates on startup and on each Marvis command. `activate(context)` creates:

- a `Marvis` output channel
- one `MarvisController`
- command registrations
- a quick-fix code action provider

`deactivate()` disposes the active controller.

## MarvisController

`MarvisController` is the main extension coordinator.

State:

- extension context
- output channel
- runtime client
- status webview panel
- last status report
- last raw status
- debounce timer
- recent opened/saved files
- command results
- running task map
- debug session map
- agent log

Major methods:

| Method | Purpose |
| --- | --- |
| `watchWorkspace` | Registers listeners for editor, diagnostics, save, task, and debug changes. |
| `start` | Ensures runtime, refreshes status, and shows the panel. |
| `showStatus` | Creates or reveals the webview panel. |
| `refreshStatus` | Collects status and sends `status_update`. |
| `ask` | Prompts the user for a request. |
| `askAboutSelection` | Builds a prompt around selected code or cursor context. |
| `fixNearCursor` | Confirms and asks Marvis to inspect and fix nearby issues. |
| `explainDiagnostic` | Builds a prompt from a diagnostic. |
| `recordTerminalFailure` | Manually records a failed command and output tail. |
| `runCommandAndRecord` | Runs a shell command from workspace root and records result. |
| `runTaskAndRecord` | Runs a VSCode task and records exit status. |
| `runPrompt` | Sends `user_prompt` with current status and handles streamed events. |
| `sendCommandResult` | Sends command result to runtime if available. |
| `ensureClient` | Starts and initializes the Rust runtime. |
| `collectStatus` | Builds the `VscodeStatus` JSON payload. |
| `scheduleStatusRefresh` | Debounces status refresh after workspace changes. |
| `updatePanel` | Sends current state to the webview. |

## RuntimeClient

`RuntimeClient` owns the child process and request/response correlation.

Fields:

- runtime command, args, cwd
- output channel
- message callback
- child process
- next request id
- pending request map
- stdout buffer

Behavior:

- `start()` spawns the runtime with piped stdio.
- `request(type, payload, terminalTypes, timeoutMs)` writes one JSON line and waits for a terminal response type.
- `handleStdout(text)` buffers newline-delimited JSON, dispatches messages, and resolves pending requests.
- `rejectAll(error)` rejects every pending request.
- `dispose()` writes a shutdown request, then kills the process after 500 ms if needed.

This client treats stdout as protocol JSON and stderr as diagnostic output.

## Code Actions

`MarvisCodeActionProvider` creates two quick fixes for diagnostics:

- ask Marvis to explain this diagnostic
- ask Marvis to fix near this diagnostic

## Status Collection

Helpers:

- `collectOpenEditors`
- `collectDiagnostics`
- `editorRef`
- `cursorContextFor`
- `position`
- `textRange`
- `severity`
- `selectedText`
- `taskKind`
- `rangeLabel`
- `isPathInside`
- `truncate`
- `truncateTail`

`collectStatus` sends active editor, open editors, visible ranges, selections, cursor bubble, recent files, diagnostics, terminal-like command results, running tasks, debug sessions, trust state, remote name, and VSCode app/profile name.

## Webview Panel

`renderPanelHtml()` returns a self-contained HTML document with:

- toolbar buttons for ask, refresh, run command, record failure
- status summary section
- active segments section
- suggestion section
- trace log section

The panel uses VSCode theme variables and receives state through `postMessage`.

## Runtime Resolution

`resolveRuntime(context, workspaceRoot)` chooses:

1. Configured `marvis.runtimePath` with `--vscode-stdio`.
2. `target/debug/lite-code` from the repo root.
3. `cargo run --quiet -- --vscode-stdio` from the repo root.

## Design Notes

The extension is intentionally thin with respect to agent behavior. It gathers high-quality IDE state and sends typed requests. Rust decides how to segment state, build the prompt context, run the model, and execute tools.
