# lite-code / Marvis

A Rust coding-agent runtime with a VSCode product shell. The original CLI and web UI still exist as harnesses, but the main product direction is Marvis inside VSCode.

## Features

- **VSCode status ingestion**: Collects active editor, cursor, selections, visible ranges, Problems diagnostics, task/debug state, and recorded command results.
- **Structured status**: Builds `CodebaseStatus`, git state, deterministic segments, stuckness hints, and context capsules.
- **Agent runtime**: Uses `session-kernel` and `scheduler` for streamed model/tool turns.
- **Tool usage**: The model can execute shell commands and read/write/edit files through local tools.
- **Streaming responses**: CLI, web, and VSCode bridge all receive runtime events.
- **Configurable model**: Choose any model available on OpenRouter (defaults to `nvidia/nemotron-3-super-120b-a12b:free`).
- **Built with Rust**: Fast, safe, and efficient.

## Installation

### Prerequisites

- Rust toolchain (version 1.70 or later)
- An OpenRouter API key (set it via `OPENROUTER_API_KEY`; see [Configuration](#configuration))

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
Get an OpenRouter API key from [OpenRouter](https://openrouter.ai/) and set `OPENROUTER_API_KEY` before running LiteCode.

### Option 1: Set for current terminal session

Windows (PowerShell):

```powershell
$env:OPENROUTER_API_KEY = "your_api_key_here"
```

macOS (zsh/bash):

```bash
export OPENROUTER_API_KEY="your_api_key_here"
```

### Option 2: Set permanently

Windows (PowerShell):

```powershell
[System.Environment]::SetEnvironmentVariable("OPENROUTER_API_KEY", "your_api_key_here", "User")
```

After setting it, open a new terminal.

macOS (zsh):

```bash
echo 'export OPENROUTER_API_KEY="your_api_key_here"' >> ~/.zshrc
source ~/.zshrc
```

Run the CLI harness:

```bash
./target/release/lite-code
```

### Options

- `--vscode-stdio`: Run the JSON stdio bridge used by the VSCode extension
- `--web`: Launch the temporary web harness
- `--model`, `-m`: Specify the model to use (default: `nvidia/nemotron-3-super-120b-a12b:free`)
- `--max-tokens`: Maximum tokens for each API response (default: `4096`)

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

If that binary is missing, it falls back to:

```bash
cargo run --quiet -- --vscode-stdio
```

Useful commands:

- `Marvis: Start Marvis`
- `Marvis: Show Status`
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

lite-code reads the API key from the `OPENROUTER_API_KEY` environment variable at runtime.

If this variable is not set, the CLI exits with a clear error message.

## How It Works

VSCode sends live editor status to the Rust stdio bridge. The Rust runtime turns that into segments such as user focus, diagnostics, recent diff, command failure, and risk. When the user asks Marvis for help, the runtime builds a context capsule from the active editor, cursor bubble, diagnostics, git state, and recent commands, then runs a normal `session-kernel` thread.

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
└── ui-bridge/        # CLI/web/VSCode event adapters
src/
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

- [OpenRouter](https://openrouter.ai/) for providing access to various language models
- The Rust community for excellent libraries like `clap`, `tokio`, `reqwest`, and `serde`
