# Quick Start

This guide gets Marvis running in VSCode against an OpenAI-compatible model provider.

## Prerequisites

- Rust toolchain with `cargo`.
- VSCode.
- Node.js for the extension development check.
- An API key for an OpenAI-compatible chat completions endpoint.

Marvis uses one provider configuration for every VSCode agent. Configure it in VSCode Settings:

```json
{
  "marvis.apiKey": "...",
  "marvis.baseUrl": "https://api.openai.com/v1",
  "marvis.thinkingMode": "auto"
}
```

`marvis.baseUrl` is optional when using the OpenAI default. For other compatible providers, set it to the provider's `/v1` base URL.

Put `marvis.apiKey` in User or Machine Settings. Do not commit it in a project's `.vscode/settings.json`.

## Build The Runtime

From the repository root:

```bash
cargo build
```

The VSCode extension can use `target/debug/lite-code` while developing this repository. Published extension packages should include a runtime binary under `apps/vscode-extension/bin/` or configure `marvis.runtimePath`.

## Launch The VSCode Extension

1. Open `apps/vscode-extension` in VSCode.
2. Press `F5` to start an Extension Development Host.
3. In the development host, open the workspace you want Marvis to inspect.
4. Run `Marvis: Start Marvis` from the command palette.

The extension starts:

```bash
lite-code --vscode-stdio
```

It sends editor state, cursor context, diagnostics, task status, debug status, and recorded command results to the Rust runtime.

## Basic Use

Use these commands first:

| Command | Use |
| --- | --- |
| `Marvis: Start Marvis` | Start the runtime and show the status panel. |
| `Marvis: Ask Marvis` | Ask a free-form question about the current workspace. |
| `Marvis: Ask About Selection` | Ask using the selected code as context. |
| `Marvis: Fix Near Cursor` | Ask Marvis to inspect and fix the nearby code. |
| `Marvis: Explain Diagnostic` | Explain the selected VSCode problem. |
| `Marvis: Check Autonomy Now` | Trigger an immediate autonomous status check. |

Marvis does not run autonomous edits or shell commands from a status tick. Autonomy is suggest-first: the runtime segments the current codebase and IDE state, routes a task through PAVE, and shows a suggestion. Work only starts after you accept the suggestion.

## Run The Demo

To record the autonomous scheduler behavior, use the showcase workspace:

```bash
./demos/autonomy-showcase/launch.sh
```

It opens VSCode with Marvis and a demo driver extension, introduces timed traps in a disposable workspace, and waits for the normal heartbeat/debounce path to suggest work. See [Autonomy Showcase Demo](apps/autonomy-showcase.md) for the recording flow.

## Recommended Settings

Set these in VSCode when the defaults do not match your environment:

```json
{
  "marvis.runtimePath": "/absolute/path/to/lite-code",
  "marvis.apiKey": "...",
  "marvis.model": "gpt-4.1-mini",
  "marvis.baseUrl": "https://api.openai.com/v1",
  "marvis.thinkingMode": "auto",
  "marvis.reasoningEffort": "",
  "marvis.maxTokens": 4096,
  "marvis.autonomy.enabled": true,
  "marvis.autonomy.idleDelayMs": 3000,
  "marvis.autonomy.heartbeatIntervalMs": 30000
}
```

Agents are generated automatically from bundled skills, workspace skills, and built-in toolsets. They all use the shared model/provider settings above; to add a project-specific agent identity, add a `SKILL.md` package under `.marvis/skills` or `.agents/skills`.

For DeepSeek-compatible endpoints, `auto` sends `{"thinking":{"type":"disabled"}}` by default. Set `marvis.thinkingMode` to `enabled` and optionally set `marvis.reasoningEffort` to `low`, `medium`, or `high` only when you want provider thinking output.

## Skills And MCP

Bundled Marvis, Anthropic, and local Codex skills are available automatically. Workspace skills can be added under:

```text
.marvis/skills/**/SKILL.md
.agents/skills/**/SKILL.md
```

MCP servers are configured from:

```text
.marvis/mcp.json
.mcp.json
```

Skills package reusable instructions, references, and scripts. MCP servers expose external tools. Both are gated by the selected agent profile and the runtime tool policy.

## Verify The Workspace

Run the main checks before publishing changes:

```bash
cargo fmt --all -- --check
cargo check --workspace --locked
cargo test --workspace --locked
cargo clippy --workspace --locked -- -D warnings
npm --prefix apps/vscode-extension run check
target/mdbook-bin/bin/mdbook build docs
```

If `marvis.apiKey` is missing, real VSCode model turns fail with a configuration error. The production runtime does not use a fake model fallback.
