# lite-code / Marvis

A Rust coding-agent runtime with a VSCode product shell. The original CLI and web UI still exist as harnesses, but the main product direction is Marvis inside VSCode.

## Features

- **VSCode status ingestion**: Collects active editor, cursor, selections, visible ranges, Problems diagnostics, task/debug state, and recorded command results.
- **Structured status**: Builds `CodebaseStatus`, git state, deterministic segments, stuckness hints, and context capsules.
- **Autonomous suggestions**: VSCode wake-up timers send status ticks; Rust segments actionable problems with an LLM and suggests useful agent actions.
- **PAVE routing**: Task vectors are matched against configured agent-profile vectors with cosine scoring.
- **Skill/MCP routing**: Routed agents resolve concrete built-in or workspace skills and can expose discovered stdio MCP tools without fake fallbacks.
- **Agent runtime**: Uses `session-kernel` and `scheduler` for streamed model/tool turns.
- **Tool usage**: The model can execute shell commands and read/write/edit files through policy-gated local tools with rollback snapshots before writes.
- **Streaming responses**: CLI, web, and VSCode bridge all receive runtime events.
- **Configurable model provider**: Use any OpenAI-compatible chat completions endpoint through `MARVIS_API_KEY` and `MARVIS_BASE_URL`.
- **Built with Rust**: Fast, safe, and efficient.

## Installation

### Prerequisites

- Rust toolchain (version 1.70 or later)
- An API key for an OpenAI-compatible provider (set it via `MARVIS_API_KEY`; see [Configuration](#configuration))

### Build from Source

```bash
# Clone the repository
git clone https://github.com/your-username/lite-code.git
# or use the SSH URL
# git clone git@github.com:your-username/lite-code.git

cd lite-code

# Build the project
cargo build --release

# The binary will be at `target/release/lite-code`
```

## Usage
Set `MARVIS_API_KEY` before running LiteCode. Set `MARVIS_BASE_URL` if your provider does not use `https://api.openai.com/v1`.

### Option 1: Set for current terminal session

Windows (PowerShell):

```powershell
$env:MARVIS_API_KEY = "your_api_key_here"
$env:MARVIS_BASE_URL = "https://api.openai.com/v1"
```

macOS (zsh/bash):

```bash
export MARVIS_API_KEY="your_api_key_here"
export MARVIS_BASE_URL="https://api.openai.com/v1"
```

### Option 2: Set permanently

Windows (PowerShell):

```powershell
[System.Environment]::SetEnvironmentVariable("MARVIS_API_KEY", "your_api_key_here", "User")
[System.Environment]::SetEnvironmentVariable("MARVIS_BASE_URL", "https://api.openai.com/v1", "User")
```

After setting it, open a new terminal.

macOS (zsh):

```bash
echo 'export MARVIS_API_KEY="your_api_key_here"' >> ~/.zshrc
echo 'export MARVIS_BASE_URL="https://api.openai.com/v1"' >> ~/.zshrc
source ~/.zshrc
```

Run the CLI harness:

```bash
./target/release/lite-code
```

### Options

- `--vscode-stdio`: Run the JSON stdio bridge used by the VSCode extension
- `--web`: Launch the temporary web harness
- `--model`, `-m`: Specify the model to use (default: `gpt-4.1-mini`)
- `--base-url`: Override `MARVIS_BASE_URL` for this run
- `--max-tokens`: Maximum tokens for each API response (default: `4096`)
- `--allow-workspace-write`: Allow model-requested file writes in CLI/web harnesses
- `--allow-risky-shell`: Allow broader shell/git/network-like commands in CLI/web harnesses
- `--print-trace <path>`: Print a replayable trace summary from a rollout JSONL file

## VSCode Extension

The VSCode product shell lives in `apps/vscode-extension`.

```bash
cargo build
```

Then open `apps/vscode-extension` in VSCode, press `F5`, and run `Marvis: Start Marvis` in the Extension Development Host.

The extension starts:

```bash
target/debug/lite-code --vscode-stdio
```

Published extension packages should include a runtime binary under `bin/`, or users can set `marvis.runtimePath`. The extension does not use `cargo run` as a product fallback.

Useful commands:

- `Marvis: Start Marvis`
- `Marvis: Show Status`
- `Marvis: Check Autonomy Now`
- `Marvis: Ask Marvis`
- `Marvis: Fix Near Cursor`
- `Marvis: Record Terminal Failure`
- `Marvis: Run Command and Record Result`
- `Marvis: Run VSCode Task and Record Result`

### Example

```bash
./target/release/lite-code "Create a simple REST API in Rust using Axum that returns 'Hello, World!'"
```

The model will:
1. Think about the task
2. Potentially run shell commands to explore the environment
3. Write files to implement the solution
4. Provide a summary when done

## Configuration

lite-code reads the API key from `MARVIS_API_KEY` at runtime.

`MARVIS_BASE_URL` defaults to:

```text
https://api.openai.com/v1
```

If `MARVIS_API_KEY` is not set, the CLI, web harness, and VSCode runtime return a clear configuration error. The VSCode product no longer falls back to a fake model.

## How It Works

VSCode sends live editor status to the Rust stdio bridge. The Rust runtime turns that into segments such as user focus, diagnostics, recent diff, command failure, and risk. When the user asks Marvis for help, the runtime builds a context capsule from the active editor, cursor bubble, diagnostics, git state, and recent commands, then runs a normal `session-kernel` thread.

When autonomy is enabled, the extension also sends debounced status ticks after meaningful VSCode activity and a low-frequency heartbeat while the runtime is active. Rust checks whether the status is actionable, asks the configured model to segment concrete tasks, routes each task through PAVE cosine scoring against configured agent profiles, and returns either `idle`, `suppressed`, or a suggestion. Suggestions do not run until the user accepts them.

Accepted suggestions execute with the routed agent's capped permissions. Agent profiles can name real skills and MCP servers; workspace skills live under `.marvis/skills/<skill>/SKILL.md` or `.agents/skills/<skill>/SKILL.md`, and stdio MCP servers are read from `.marvis/mcp.json` or `.mcp.json`. Write tools create restoreable preimage snapshots under `.marvis/rollback`.

## Project Structure

```
apps/
└── vscode-extension/ # VSCode product shell
crates/
├── protocol/         # shared operations, events, ids, and response items
├── rollout/          # JSONL thread history
├── session-kernel/   # thread/session runtime
├── scheduler/        # model turn execution
├── status/           # VSCode/codebase status, segments, stuckness, capsules
├── pave-router/      # PAVE vectors, agent profiles, and route scoring
└── ui-bridge/        # CLI/web/VSCode event adapters
src/
├── autonomy.rs       # autonomous wake-up, LLM segmentation, PAVE routing
├── skill_mcp.rs      # skill registry and stdio MCP tool runtime
├── main.rs           # CLI/web/vscode-stdio entry point
├── tools.rs          # current local tool executor
├── vscode.rs         # VSCode stdio bridge
└── web.rs            # temporary web harness
```

## Contributing

Contributions are welcome! Please follow these steps:

1. Fork the repository
2. Create a new branch (`git checkout -b feature/your-feature`)
3. Make your changes
4. Commit your changes (`git commit -am 'Add new feature'`)
5. Push to the branch (`git push origin feature/your-feature`)
6. Open a Pull Request

Please ensure your code follows the existing style and includes tests where appropriate.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

- OpenAI-compatible API providers for model access
- The Rust community for excellent libraries like `clap`, `tokio`, `reqwest`, and `serde`
