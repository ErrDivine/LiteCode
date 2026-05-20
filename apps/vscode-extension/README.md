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

Set `marvis.apiKey` in VSCode Settings before asking Marvis to call a model. Set `marvis.baseUrl` if your provider does not use `https://api.openai.com/v1`.

The extension does not use a fake production model fallback. If `marvis.apiKey` is missing, Marvis returns a configuration error instead of pretending to answer.

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

The Rust runtime uses the configured OpenAI-compatible model when VSCode status shows a focused, useful next step. The segmenter prompt is intentionally proactive: it should infer user intent from editor, cursor, diagnostics, recent saves, command failures, and status segments without waiting for a broken build or perfect certainty. The runtime auto-detects skills and local toolsets, turns them into same-model agent identities, and asks the segmenter to choose one before permission is requested. Suggestions are suggest-first: Marvis does not run tools, edit files, or execute shell commands until you accept the suggestion.

Agent identities are derived from discovered `SKILL.md` packages, declared local tool functions, declared MCP dependencies, and built-in read-only/verification/patch toolsets. They do not configure API credentials; all VSCode model calls use `marvis.apiKey` and `marvis.baseUrl`.

The runtime resolves bundled Codex-style skill packages, imported Anthropic/local Codex skills, and workspace skills from `.marvis/skills` and `.agents/skills`. MCP tools are exposed only after a configured stdio server is discovered successfully; missing or failing MCP servers produce an error instead of invented tools.
