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

If `target/debug/lite-code` is not present, the extension falls back to:

```bash
cargo run --quiet -- --vscode-stdio
```

Set `OPENROUTER_API_KEY` to get real model replies. Without it, the Rust runtime uses the synthetic scheduler so status collection and bridge testing still work.

## What It Collects

- active editor, cursor, selection, visible ranges
- open editors and recent saves
- VSCode Problems diagnostics
- task start/end state
- debug session start/end state
- command results run or recorded through Marvis

The extension sends these snapshots to the Rust status store. The runtime returns deterministic segments, stuckness hints, and streamed agent events.
