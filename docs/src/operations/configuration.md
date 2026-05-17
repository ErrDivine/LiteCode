# Configuration And Data Locations

## Environment Variables

| Variable | Used by | Meaning |
| --- | --- | --- |
| `OPENROUTER_API_KEY` | CLI, web, VSCode runtime | API key for OpenRouter/OpenAI-compatible requests. Required for CLI and web modes. Optional for VSCode, which falls back to synthetic scheduling. |
| `LITE_CODE_STATE_HOME` | `state-store` constant | Intended state root override. The current implementation exposes the constant but does not wire global discovery through it. |

## CLI Flags

| Flag | Meaning |
| --- | --- |
| `--web` | Start the Axum web harness on `0.0.0.0:3000`. |
| `--vscode-stdio` | Start the JSON stdio server for the VSCode extension. |
| `--model`, `-m` | Select the model name. |
| `--max-tokens` | Select maximum response tokens. |

## VSCode Settings

| Setting | Meaning |
| --- | --- |
| `marvis.runtimePath` | Absolute or workspace-relative binary path to run instead of auto-detected runtime. |
| `marvis.model` | Model passed to `Initialize`. |
| `marvis.maxTokens` | Max tokens passed to `Initialize`. |

## Runtime Data

### Thread Rollouts

The binary and VSCode runtime use a `.lite-code` directory under the current workspace root.

```text
.lite-code/sessions/<thread-id>.jsonl
.lite-code/archived_sessions/<thread-id>.jsonl
```

Each JSONL line is a `protocol::RolloutItem`.

### State Store

`StateRuntime` writes:

```text
<root>/migrations.json
<root>/state.jsonl
<root>/logs.jsonl
```

This store is not yet the primary thread history path.

## Network Behavior

CLI and web modes require a live OpenRouter key and create `OpenAiScheduler`. VSCode stdio mode creates:

- `OpenAiScheduler` when `OPENROUTER_API_KEY` is present and non-empty.
- `SyntheticScheduler` otherwise.

## Ports

The web harness binds:

```text
0.0.0.0:3000
```

There is no dynamic port fallback in the current implementation.
