# Configuration And Data Locations

## Environment Variables

| Variable | Used by | Meaning |
| --- | --- | --- |
| `MARVIS_API_KEY` | CLI, web | API key for any OpenAI-compatible chat completions provider. Required for real CLI/web model turns. |
| `MARVIS_BASE_URL` | CLI, web, VSCode runtime fallback | Optional OpenAI-compatible base URL. Defaults to `https://api.openai.com/v1`. |
| `LITE_CODE_STATE_HOME` | `state-store` constant | Intended state root override. The current implementation exposes the constant but does not wire global discovery through it. |

## CLI Flags

| Flag | Meaning |
| --- | --- |
| `--web` | Start the Axum web harness on `127.0.0.1:3000`. |
| `--vscode-stdio` | Start the JSON stdio server for the VSCode extension. |
| `--model`, `-m` | Select the model name. |
| `--base-url` | Override `MARVIS_BASE_URL` for this run. |
| `--max-tokens` | Select maximum response tokens. |
| `--allow-workspace-write` | Allow model-requested file writes in CLI/web harnesses. |
| `--allow-risky-shell` | Allow broader shell/git/network-like commands in CLI/web harnesses. |
| `--print-trace <path>` | Print a replayable trace summary from a rollout JSONL file. |

## VSCode Settings

| Setting | Meaning |
| --- | --- |
| `marvis.runtimePath` | Absolute or workspace-relative binary path to run instead of auto-detected runtime. |
| `marvis.model` | Model passed to `Initialize`. |
| `marvis.apiKey` | API key passed to `Initialize` for the OpenAI-compatible provider. |
| `marvis.baseUrl` | Optional OpenAI-compatible base URL passed to `Initialize`. |
| `marvis.thinkingMode` | `auto`, `disabled`, `enabled`, or `provider_default`; `auto` disables DeepSeek thinking and omits the parameter elsewhere. |
| `marvis.reasoningEffort` | Optional `low`, `medium`, or `high` reasoning effort for compatible providers. |
| `marvis.maxTokens` | Max tokens passed to `Initialize`. |
| `marvis.autonomy.enabled` | Enables VSCode status wake-up checks and suggest-first autonomous routing. |
| `marvis.autonomy.idleDelayMs` | Debounce delay before sending an autonomy tick after status changes. |
| `marvis.autonomy.heartbeatIntervalMs` | Heartbeat interval for low-frequency status checks while the runtime is active. |

VSCode no longer accepts manually configured agent profiles. Store `marvis.apiKey` in User or Machine Settings rather than committed workspace settings; agent identities are synthesized inside the runtime from discovered skills, declared tool dependencies, MCP dependencies, and built-in toolsets. Every generated agent uses the shared `marvis.model`, `marvis.apiKey`, and `marvis.baseUrl` provider settings.

## Workspace Skill And MCP Config

Workspace skills are loaded from:

```text
.marvis/skills/**/SKILL.md
.agents/skills/**/SKILL.md
```

Bundled system skills are built from `skills/system/**/SKILL.md`, including imported Anthropic and local Codex skill packages, then materialized under:

```text
.lite-code/skills/.system/**/SKILL.md
```

To add a project-specific skill, use a workspace skill path. To ship a new built-in skill with Marvis itself, add a package under `skills/system`; the build script embeds its resources automatically.

Stdio MCP servers are loaded from:

```text
.marvis/mcp.json
.mcp.json
```

MCP config follows the `mcpServers` JSON shape documented in [Skill And MCP Runtime](../runtime/skills-mcp.md).

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

### Rollback Snapshots

Workspace write tools store rollback preimages under:

```text
.marvis/rollback/<snapshot-id>/manifest.json
.marvis/rollback/<snapshot-id>/files/*.bin
```

Use the `list_rollbacks` and `restore_rollback` tools to inspect and restore snapshots.

## Network Behavior

All product/runtime modes use `OpenAiScheduler` with an OpenAI-compatible provider config:

- `marvis.apiKey` supplies the bearer token in VSCode.
- `MARVIS_API_KEY` supplies the bearer token in CLI/web modes.
- `MARVIS_BASE_URL` supplies the provider URL in CLI/web modes and as a VSCode fallback, or defaults to `https://api.openai.com/v1`.
- `marvis.thinkingMode=auto` sends disabled thinking for DeepSeek-compatible endpoints so tool-call loops do not require reasoning content by default.

If the relevant API key is missing, the runtime returns a clear configuration error. It does not use a production fake-model fallback.

## Ports

The web harness binds:

```text
127.0.0.1:3000
```

There is no dynamic port fallback in the current implementation.
