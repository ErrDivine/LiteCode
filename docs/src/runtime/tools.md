# Local Tool Gateway

`src/tools.rs` implements the current local tool executor, dynamic tool schema list, and runtime policy checks. It is not a separate crate yet, but it is a major runtime interface because model tool calls pass through it.

## Public Types And Functions

| Item | Purpose |
| --- | --- |
| `ToolResult` | Internal result wrapper with `output: String`. |
| `ToolPolicy` | Per-turn permissions for workspace writes, shell, risky shell, git writes, network-like commands, cwd, and timeout. |
| `LocalToolExecutor` | Implements `session_kernel::ToolExecutor` using a `ToolPolicy`, plus optional discovered MCP tools. |
| `tool_definitions_for_policy(policy)` | Returns only the dynamic JSON schemas allowed by the current policy. |
| `execute_tool_with_policy(policy, name, input)` | Dispatches tool execution by name and checks policy again at runtime. |

## Constants

- `MAX_OUTPUT_LEN = 10_000`
- `MAX_SEARCH_MATCHES = 100`
- `MAX_FIND_RESULTS = 200`
- `MAX_WALK_DEPTH = 20`
- `SKIP_DIRS`: `.git`, `node_modules`, `target`, cache/build directories, and similar non-source directories.

## Exposed Tools

### `shell`

Runs a shell command in the configured workspace.

Parameters:

- `command: string`

Behavior:

- On Windows, runs PowerShell.
- On other platforms, runs `sh -c`.
- Blocks unsafe commands unless risky-shell permission is granted.
- Blocks network-like commands unless network permission is granted.
- Blocks git write commands unless git-write permission is granted.
- Enforces command timeout.
- Combines stdout and stderr.
- Returns exit code text if output is empty.
- Truncates long output.

Design note: this tool still belongs in the binary crate, but it is no longer unrestricted.

### `write_file`

Writes content to a file, creating parent directories when needed.

Parameters:

- `path: string`
- `content: string`

Behavior:

- Overwrites existing files.
- Requires workspace-write permission.
- Rejects paths outside the workspace.
- Rejects writes inside unsafe internal/cache directories.
- Creates a rollback preimage snapshot before writing.
- Returns byte count or error text.

### `read_file`

Reads a UTF-8 text file with line numbers.

Parameters:

- `path: string`
- optional `offset: integer`
- optional `limit: integer`

Behavior:

- Detects binary content by looking for null bytes in the first 8192 bytes.
- Rejects invalid UTF-8.
- Supports 1-based line offsets.
- Truncates output.

### `edit_file`

Replaces an exact string with new content.

Parameters:

- `path: string`
- `old_string: string`
- `new_string: string`

Behavior:

- Rejects empty old strings.
- Rejects no-op replacements.
- Requires exactly one match.
- Writes the modified file.
- Requires workspace-write permission.
- Rejects paths outside the workspace.
- Creates a rollback preimage snapshot before writing.

This is the preferred targeted edit tool from the system prompt.

### `list_directory`

Lists one directory.

Parameters:

- optional `path: string`, default `.`

Behavior:

- Restricts paths to the workspace.
- Appends `/` to directories.
- Shows file sizes for files.
- Sorts results.

### `search_files`

Recursively searches text files.

Parameters:

- `pattern: string`
- optional `path: string`, default `.`
- optional `regex: boolean`
- optional `include: string` file-name glob

Behavior:

- Restricts search roots to the workspace.
- Skips common non-source directories.
- Supports literal search or regex search.
- Applies include glob to file names.
- Stops collecting after configured match limits.
- Skips unreadable or non-UTF-8 files.

### `find_files`

Finds paths with a glob pattern.

Parameters:

- `pattern: string`
- optional `path: string`, default `.`

Behavior:

- Joins `path` and `pattern`.
- Restricts the base path to the workspace and rejects `..` in glob patterns.
- Uses the `glob` crate.
- Limits result count.

### `apply_patch`

Applies a unified diff patch with `git apply`.

Parameters:

- `patch: string`

Behavior:

- Requires workspace-write permission.
- Rejects absolute/out-of-workspace patch paths.
- Creates rollback preimage snapshots for affected files before applying.
- Uses a timeout.

### `list_rollbacks`

Lists rollback snapshots under `.marvis/rollback`.

Parameters:

- optional `limit: integer`, default `20`

Behavior:

- Does not require workspace-write permission.
- Returns snapshot ids, creation times, operations, and affected files.

### `restore_rollback`

Restores files from a rollback snapshot id.

Parameters:

- `id: string`

Behavior:

- Requires workspace-write permission.
- Creates a new undo snapshot before restoring.
- Restores deleted/new-file state as well as file contents.

### MCP tools

When a routed VSCode agent selects configured MCP servers, `LocalToolExecutor` can also execute discovered stdio MCP tools. MCP tool names are exposed as `mcp__server__tool` and are only present after successful discovery.

### `run_test`, `run_build`, `run_formatter`

Run allowlisted local commands for verification.

Behavior:

- Require shell permission.
- Accept only safe command families for each helper.
- Enforce timeout and output truncation.

### `git_status`, `git_diff`

Read-only git helpers.

Behavior:

- Require shell permission.
- Use allowlisted git commands only.

## Helper Functions

- `truncate_output`
- `parse_usize_param`
- `parse_bool_param`
- `tool`
- `walk_files`

The parsers accept both JSON primitives and stringified values where useful. This makes tool execution more robust against imperfect model arguments.

## Design Notes

The current implementation now enforces policy in two places: it only exposes allowed tool schemas to the model, and it checks the policy again when a tool call arrives.

Recommended future direction:

- Move tools into a crate once the policy API is stable.
- Return structured status in addition to text output.
- Persist tool call records as response items in addition to trace events.
