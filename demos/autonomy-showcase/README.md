# Marvis Autonomy Showcase

This demo opens a normal VSCode workspace, starts Marvis, then introduces timed traps that make the autonomous scheduler wake up through the real VSCode status path. It is designed for screen recording: the Marvis panel stays visible while the active editor, diagnostics, failing tasks, routed suggestion, and accept/dismiss prompt appear naturally.

The demo does not fake Marvis output. The only synthetic part is the demo workspace itself: it contains small intentional defects that the driver introduces one by one.

## What It Shows

- Marvis starts from the VSCode extension and initializes the Rust runtime.
- The heartbeat and debounce scheduler observe active editor, cursor, saves, diagnostics, and task failures.
- The scheduler asks the configured model for a focused next step only after a concrete trap is ready.
- Marvis routes the task to an auto-detected skill/tool agent and asks for permission before execution.
- Accepted suggestions run through the normal bounded process path with rollback-capable tools.

## Prerequisites

- VSCode, not Cursor, for the recording window.
- Rust toolchain available to build `target/debug/lite-code`.
- Node.js available for the demo workspace tests.
- `marvis.apiKey` set in VSCode User or Machine Settings.
- Optional: `marvis.baseUrl`, `marvis.model`, `marvis.thinkingMode`, and `marvis.reasoningEffort` set in VSCode Settings for your provider.

The launcher prefers the macOS VSCode CLI at:

```text
/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code
```

Set `CODE_CMD=/path/to/code` if your VSCode CLI is somewhere else.

## Run

From the repository root:

```bash
./demos/autonomy-showcase/launch.sh
```

The launcher builds the Rust runtime, then opens a fresh VSCode window with two development extensions loaded:

- `apps/vscode-extension` for Marvis
- `demos/autonomy-showcase/driver-extension` for the timed demo driver

The demo starts automatically when the workspace opens. If it does not, run:

```text
Marvis Demo: Run Autonomous Showcase
```

## Verify The Traps

The headless verifier checks that the clean workspace passes and every timed trap creates a real failing task signal:

```bash
npm --prefix demos/autonomy-showcase run verify
```

## Recording Flow

1. Start recording after the VSCode window opens.
2. Keep the Marvis panel visible on the side.
3. Let the driver advance through the traps. Each stage resets to a clean baseline, writes one real trap, focuses the relevant line, runs its VSCode task, then waits.
4. When Marvis suggests a task, record the agent choice and permission prompt. Dismiss the suggestion to continue quickly, or accept it to show execution.
5. The final stage restores the workspace to a clean baseline for another take.

The default stage delay is tuned for a live model call. To make the stage pacing slower or faster, edit `marvisDemo.stageDelayMs` in `workspace/.vscode/settings.json`.

## Demo Stages

| Stage | Signal | Intended Marvis behavior |
| --- | --- | --- |
| Focused JS bug | Active editor, saved file, JS diagnostic, failing test task | Suggest a narrow code repair near the cursor. |
| Refactor fallout | Removed public module, moved implementation, active failing import, failing test task | Suggest restoring the public import path or updating references. |
| Discount rule gap | Active editor at TODO, failing test task | Suggest implementing the missing business rule. |
| Dashboard readiness | Active HTML TODO, failing accessibility check task | Suggest an accessible UI patch using a frontend-oriented agent. |
| Release-note check | Active document context, failing documentation check task | Suggest a documentation patch or verification step. |

## Reset

The driver resets all demo files at the beginning and end of a run. You can also run:

```text
Marvis Demo: Reset Workspace
```

Or reset from the terminal:

```bash
npm --prefix demos/autonomy-showcase run reset
```

Only files under `demos/autonomy-showcase/workspace` are modified.
