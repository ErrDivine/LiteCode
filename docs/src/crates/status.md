# Status Engine

`crates/status` is the deterministic context engine for Marvis. It accepts IDE state, git state, and command results, then produces status reports, segments, stuckness signals, and prompt-ready context capsules.

## Data Model

### Workspace

`WorkspaceMeta` contains:

- `root`
- `name`
- `primary_language`

`WorkspaceMeta::new` infers Rust from `Cargo.toml` and JavaScript from `package.json`.

### Freshness, Risk, And Segment Kind

Enums:

- `Freshness`: `hot`, `warm`, `cold`, `stale`, `unknown`
- `RiskLevel`: `low`, `medium`, `high`, `critical`
- `SegmentKind`: `user_focus`, `recent_diff`, `command_failure`, `failing_test`, `diagnostic_cluster`, `risk`, `unknown`

These fields allow segments to be ranked and rendered without model interpretation.

### StatusSegment

Each segment contains:

- stable `id`
- `kind`
- human summary
- evidence labels
- related files
- related diagnostic ids
- token estimate
- freshness
- confidence
- importance
- risk level

Segments are sorted by descending importance.

## VSCode Status Types

The VSCode status snapshot includes:

- `EditorRef`
- `VisibleRange`
- `SelectionState`
- `CursorContext`
- `DiagnosticEvent`
- `TerminalSessionState`
- `VscodeTaskState`
- `DebugSessionState`
- `ClipboardHint`
- `VscodeStatus`

`VscodeStatus` is the aggregate sent by the extension. It includes active editor, open editors, visible ranges, selections, cursor context, recent files, Problems diagnostics, terminal sessions, tasks, debug sessions, trust state, remote name, and profile/app name.

## Git And Command State

`GitState` captures:

- whether the workspace is a repository
- branch
- head
- dirty files
- untracked files
- staged files
- deleted files
- optional diff summary

`read_git_state(root)` shells out to git:

- `rev-parse --abbrev-ref HEAD`
- `rev-parse HEAD`
- `status --porcelain=v1`
- `diff --shortstat`

Parser helpers:

- `parse_git_status_porcelain`
- `parse_diff_shortstat`

`CommandResult` records a command, cwd, output tail, exit code, and timestamp. `failed()` returns true for non-zero exit codes or failure words in output.

## CodebaseStatus And StatusReport

`CodebaseStatus` is the mutable snapshot:

- workspace metadata
- timestamp
- VSCode status
- git state
- command state
- active segments

`StatusReport` is the UI-facing view:

- `snapshot_hash`
- summary string
- active segments
- optional stuckness signal
- optional proactive suggestion

`summarize_status` produces a compact single-line summary with workspace, active editor, cursor, branch, dirty count, untracked count, diagnostics count, and segment count.

## ContextCapsule

`ContextCapsule` is the prompt-facing context object. It includes:

- original user prompt
- status summary
- active segments
- active editor
- cursor context
- filtered diagnostics
- recent commands
- git state

`to_prompt_context()` renders a structured plain-text prompt section. The VSCode runtime submits this text as the user input to the normal thread system.

## StatusStore

`StatusStore` owns the current `CodebaseStatus` and retention limits.

Public methods:

| Method | Behavior |
| --- | --- |
| `new(root)` | Starts a status store for a workspace root. |
| `snapshot()` | Returns the current status clone. |
| `report()` | Builds a `StatusReport` from the current status. |
| `update_vscode_status(vscode)` | Stores VSCode state, truncates recent file lists, resegments, reports. |
| `ingest_command_result(result)` | Stores command result, caps history, resegments, reports. |
| `refresh_git_state()` | Reads git state, resegments, reports. |
| `build_context_capsule(prompt)` | Builds the model context capsule. |

## Segmentation Rules

`segment_status` builds:

1. A user-focus segment when there is an active editor.
2. Diagnostic-cluster segments grouped by file.
3. A recent-diff segment when dirty or untracked files exist.
4. Command-failure or failing-test segments for recent failed commands.
5. A risk segment for deleted files, lock/config changes, or broad change sets.

Stuckness detection currently handles:

- repeated failure of the same normalized command
- three or more active VSCode error diagnostics

## Design Notes

The status engine is intentionally deterministic. It should explain why a segment exists through evidence labels and related ids, rather than relying on a model to infer state from raw editor dumps.

The status crate does call git through subprocesses. That keeps implementation small, but git behavior should remain isolated behind `read_git_state` and parser helpers.
