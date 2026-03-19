# lite-code

A minimal one-turn vibe coding CLI that uses a language model (via OpenRouter) with access to shell and file writing tools to accomplish coding tasks.

## Features

- **One-turn interaction**: Provide a prompt and let the model use tools iteratively until the task is complete.
- **Tool usage**: The model can execute shell commands and write files to work on your project.
- **Streaming responses**: See the model's reasoning and output in real-time.
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

Run the CLI with your prompt:

```bash
./target/release/lite-code "Your prompt here"
```

### Options

- `--model`, `-m`: Specify the model to use (default: `nvidia/nemotron-3-super-120b-a12b:free`)
- `--max-tokens`: Maximum tokens for each API response (default: `4096`)

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

lite-code uses the OpenRouter API to chat with a language model, providing it with two tools:
- `shell`: Execute a shell command
- `write_file`: Write content to a file

The system prompt instructs the model to:
- Use tools to accomplish the user's request
- Work step by step, verifying progress
- Provide a brief summary when finished

The CLI streams the model's responses and executes any tool calls it makes, feeding the results back into the conversation until the model indicates completion.

## Project Structure

```
src/
├── main.rs      # CLI entry point and agent loop
├── api.rs       # OpenRouter API client with SSE streaming
├── tools.rs     # Tool definitions and execution (shell, write_file)
├── types.rs     # Shared types for messages, tools, etc.
└── ...
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