# Autonomy Showcase Demo

The repository includes a recording-oriented demo under `demos/autonomy-showcase`. It opens a disposable VSCode workspace, starts Marvis, introduces focused code and documentation traps, runs matching VSCode tasks, and waits so the normal autonomy debounce or heartbeat can trigger suggestions.

The demo does not fake runtime output. It drives the real VSCode extension commands and the real Rust `--vscode-stdio` runtime.

## Layout

| Path | Role |
| --- | --- |
| `demos/autonomy-showcase/launch.sh` | Builds `target/debug/lite-code` and opens VSCode with the Marvis extension plus the demo driver extension. |
| `demos/autonomy-showcase/driver-extension` | Small VSCode extension that resets the demo workspace, writes traps, focuses files, runs tasks, and waits between stages. |
| `demos/autonomy-showcase/workspace` | Disposable JavaScript/documentation project used for the recording. |

## Run

Set `marvis.apiKey` in VSCode User or Machine Settings first. Then run:

```bash
./demos/autonomy-showcase/launch.sh
```

On macOS the launcher prefers the official VSCode CLI at:

```text
/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code
```

Set `CODE_CMD=/absolute/path/to/code` if the launcher should use a different VSCode CLI.

## Verify

The demo includes a headless trap verifier:

```bash
npm --prefix demos/autonomy-showcase run verify
```

It copies the workspace to a temporary directory, checks that the baseline passes, applies each trap, and asserts that the corresponding task fails with useful output.

## Recording Flow

The driver starts automatically when `demos/autonomy-showcase/workspace` opens. If it does not, run `Marvis Demo: Run Autonomous Showcase`. Each stage starts from a clean baseline so only the current trap is active while Marvis observes the workspace.

The stages are:

| Stage | Status signal | Expected Marvis behavior |
| --- | --- | --- |
| Focused JavaScript bug | Active editor, saved file, JS diagnostic, failing VSCode task | Suggest a narrow repair near the cursor. |
| Refactor fallout | Deleted public module, moved implementation, active failing import, failing VSCode task | Suggest restoring the public import path or updating references. |
| Missing discount rule | Cursor at TODO plus failing test task | Suggest implementing the missing business rule. |
| Dashboard readiness | Active HTML TODO plus failing accessibility check task | Suggest an accessible UI patch with a frontend-oriented agent. |
| Release-note readiness | Active Markdown file plus failing documentation check | Suggest a documentation patch or verification step. |

Suggestions are still suggest-first. Accepting one shows the normal bounded execution path; dismissing lets the recording continue to the next trap quickly.

## Safety

The driver refuses to run unless the open workspace contains `.marvis-demo.json`. It only modifies files under `demos/autonomy-showcase/workspace`, and it restores the workspace at the beginning and end of the run.

If a recording is interrupted mid-stage, reset from the terminal:

```bash
npm --prefix demos/autonomy-showcase run reset
```
