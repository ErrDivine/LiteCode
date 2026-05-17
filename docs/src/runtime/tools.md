# Local Tool Gateway

`src/tools.rs` implements the current local tool executor and dynamic tool schema list. It is not a separate crate yet, but it is a major runtime interface because model tool calls pass through it.

## Public Types And Functions

| Item | Purpose |
| --- | --- |
| `ToolResult` | Internal result wrapper with `output: String`. |
| `LocalToolExecutor` | Implements `session_kernel::ToolExecutor`. |
| `tool_definitions()` | Returns dynamic JSON schemas for all exposed tools. |
| `execute_tool(name, input)` | Dispatches tool execution by name. |

## Constants

- `MAX_OUTPUT_LEN = 10_000`
- `MAX_SEARCH_MATCHES = 100`
- `MAX_FIND_RESULTS = 200`
- `MAX_WALK_DEPTH = 20`
- `SKIP_DIRS`: `.git`, `node_modules`, `target`, cache/build directories, and similar non-source directories.

## Exposed Tools

### `shell`

Runs a shell command in the current working directory.

Parameters:

- `command: string`

Behavior:

- On Windows, tries PowerShell first, then `cmd /C`.
- On other platforms, runs `sh -c`.
- Combines stdout and stderr.
- Returns exit code text if output is empty.
- Truncates long output.

Design note: this tool has broad local execution power. Policy and approval should eventually live in a dedicated tool gateway.

### `write_file`

Writes content to a file, creating parent directories when needed.

Parameters:

- `path: string`
- `content: string`

Behavior:

- Overwrites existing files.
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

This is the preferred targeted edit tool from the system prompt.

### `list_directory`

Lists one directory.

Parameters:

- optional `path: string`, default `.`

Behavior:

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
- Uses the `glob` crate.
- Limits result count.

## Helper Functions

- `truncate_output`
- `parse_usize_param`
- `parse_bool_param`
- `tool`
- `walk_files`

The parsers accept both JSON primitives and stringified values where useful. This makes tool execution more robust against imperfect model arguments.

## Design Notes

The current implementation values simplicity over policy. It should be considered a development tool gateway, not a final security boundary.

Recommended future direction:

- Move tools into a crate with explicit policy and tracing.
- Return structured status in addition to text output.
- Persist tool call records as response items or trace events.
- Separate read-only tools from mutating and shell tools.
