mod tools;
mod web;

use anyhow::{Context, Result};
use clap::Parser;
use std::io::{self, Write};
use std::sync::Arc;

use scheduler::OpenAiScheduler;
use session_kernel::{SessionConfig, ThreadManager};
use tools::{LocalToolExecutor, tool_definitions};
use ui_bridge::{CliEvent, event_to_cli, user_text_op};

pub(crate) const SYSTEM_PROMPT: &str = "\
You are a coding assistant operating inside the user's project directory. \
You have access to tools for running shell commands, reading and writing files, \
editing files, listing directories, and searching code. \
When the task depends on project contents, use read_file, list_directory, or search_files \
to inspect the workspace and base your answer on real results. \
Prefer edit_file for targeted changes instead of rewriting entire files with write_file. \
Use find_files to locate files by glob pattern (e.g. **/*.rs). \
Do not claim you cannot access files or run commands when these tools are available. \
Use tools to accomplish the user's request, work step by step, verify progress, then provide a brief summary.";

#[derive(Parser)]
#[command(name = "lite-code", about = "Minimal vibe coding agent")]
struct Cli {
    /// Launch the web UI instead of CLI mode
    #[arg(long)]
    web: bool,

    /// Model to use
    #[arg(short, long, default_value = "nvidia/nemotron-3-super-120b-a12b:free")]
    model: String,

    /// Max tokens for each API response
    #[arg(long, default_value = "4096")]
    max_tokens: u32,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let api_key = std::env::var("OPENROUTER_API_KEY")
        .context("OPENROUTER_API_KEY environment variable not set")?;

    let scheduler = Arc::new(OpenAiScheduler::openrouter(api_key)?);
    let tool_executor = Arc::new(LocalToolExecutor);
    let history_root = std::env::current_dir()?.join(".lite-code");
    let manager = Arc::new(ThreadManager::new(
        scheduler,
        tool_executor,
        history_root.clone(),
    ));

    if cli.web {
        let state = Arc::new(web::AppState {
            manager,
            history_root,
            model: cli.model,
            max_tokens: cli.max_tokens,
        });
        return web::serve(state).await;
    }

    // --- CLI mode ---
    let tools = tool_definitions();
    let thread = manager
        .start_thread_with_tools(runtime_config(&cli, history_root), tools, true)
        .await?
        .thread;
    let _ = thread.next_event().await?;
    let mut user_stdin_input: String = String::new();

    loop {
        user_stdin_input.clear();
        io::stdin()
            .read_line(&mut user_stdin_input)
            .expect("Failed to read from stdin.");
        user_stdin_input = String::from(user_stdin_input.trim());

        if user_stdin_input == "exit" {
            break;
        }

        thread.submit(user_text_op(&user_stdin_input)).await?;

        loop {
            match event_to_cli(&thread.next_event().await?) {
                CliEvent::Print(text) => {
                    print!("{text}");
                    io::stdout().flush().ok();
                }
                CliEvent::ToolStart { name, arguments } => {
                    eprintln!("\n[tool] {name} {arguments}");
                }
                CliEvent::ToolEnd { name, output } => {
                    eprintln!("[tool] {name} done\n{output}");
                }
                CliEvent::Error(error) => {
                    eprintln!("\n{error}");
                    break;
                }
                CliEvent::Done => break,
                CliEvent::Ignore => {}
            }
        }
        println!();
    }

    Ok(())
}

fn runtime_config(cli: &Cli, history_root: std::path::PathBuf) -> SessionConfig {
    let mut config = SessionConfig::new(
        cli.model.clone(),
        std::env::current_dir().unwrap_or_else(|_| ".".into()),
    );
    config.system_prompt = SYSTEM_PROMPT.to_string();
    config.max_tokens = cli.max_tokens;
    config.history_root = history_root;
    config
}
