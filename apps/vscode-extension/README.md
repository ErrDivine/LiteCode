# Marvis VSCode Extension

This is the product shell for Marvis. It talks to the Rust runtime through:

```bash
lite-code --vscode-stdio
```

## Run In VSCode

1. Build the Rust binary from the repo root:

   ```bash
   cargo build
   ```

2. Open `apps/vscode-extension` in VSCode.
3. Press `F5` to launch an Extension Development Host.
4. In the development host, run `Marvis: Start Marvis`.

Development mode can use `target/debug/lite-code`. Published extension packages should include a runtime binary under `bin/` or set `marvis.runtimePath`; the extension does not invoke `cargo run` as a product fallback.

Set `MARVIS_API_KEY` before asking Marvis to call a model. Set `MARVIS_BASE_URL` if your provider does not use `https://api.openai.com/v1`.

The extension does not use a fake production model fallback. If `MARVIS_API_KEY` is missing, Marvis returns a configuration error instead of pretending to answer.

Useful commands include `Marvis: Check Autonomy Now` for a manual wake-up check, plus the status, ask, diagnostic, terminal, command, and task commands declared in `package.json`.

## What It Collects

- active editor, cursor, selection, visible ranges
- open editors and recent saves
- VSCode Problems diagnostics
- task start/end state
- debug session start/end state
- command results run or recorded through Marvis

The extension sends these snapshots to the Rust status store. The runtime returns deterministic segments, stuckness hints, and streamed agent events.

## Autonomous Suggestions

With `marvis.autonomy.enabled` on, the extension sends debounced `autonomy_tick` requests after diagnostics, saves, command/task results, debug termination, and quiet editor changes. It also sends a heartbeat while the runtime is active.

The Rust runtime uses the configured OpenAI-compatible model to segment actionable problems, routes them to configured `marvis.agentProfiles` with PAVE cosine scoring, and returns a suggestion only when one passes thresholds. Suggestions are suggest-first: Marvis does not run tools, edit files, or execute shell commands until you accept the suggestion.

Agent profiles configure model names, skill ids, MCP server ids, tool allowlists, approval defaults, and PAVE vectors. They do not configure API credentials; all model calls still use `MARVIS_API_KEY` and `MARVIS_BASE_URL`.

The runtime resolves bundled Codex-style skill packages plus workspace skills from `.marvis/skills` and `.agents/skills`. MCP tools are exposed only after a configured stdio server is discovered successfully; missing or failing MCP servers produce an error instead of invented tools.
