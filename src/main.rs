mod autonomy;
mod skill_mcp;
mod tools;
mod vscode;
mod web;

use anyhow::{Context, Result};
use clap::Parser;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::Arc;

use scheduler::{OpenAiCompatibleConfig, OpenAiScheduler};
use session_kernel::{SessionConfig, ThreadManager};
use tools::{LocalToolExecutor, ToolPolicy, tool_definitions_for_policy};
use ui_bridge::{CliEvent, event_to_cli, user_text_op};

pub(crate) const DEFAULT_MODEL: &str = "gpt-4.1-mini";
pub(crate) const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";

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

    /// Run the VSCode extension stdio bridge
    #[arg(long)]
    vscode_stdio: bool,

    /// Print a replayable trace summary from a rollout JSONL file
    #[arg(long)]
    print_trace: Option<PathBuf>,

    /// Model to use
    #[arg(short, long, default_value = DEFAULT_MODEL)]
    model: String,

    /// OpenAI-compatible base URL. Defaults to MARVIS_BASE_URL or https://api.openai.com/v1
    #[arg(long)]
    base_url: Option<String>,

    /// Max tokens for each API response
    #[arg(long, default_value = "4096")]
    max_tokens: u32,

    /// Allow model-requested file writes in CLI/web harnesses
    #[arg(long)]
    allow_workspace_write: bool,

    /// Allow risky shell/git/network-like commands in CLI/web harnesses
    #[arg(long)]
    allow_risky_shell: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    if let Some(path) = &cli.print_trace {
        return print_trace(path).await;
    }

    if cli.vscode_stdio {
        return vscode::serve_stdio(cli.model, cli.base_url, cli.max_tokens).await;
    }

    let model_config = load_model_config(cli.base_url.as_deref())?;

    let scheduler = Arc::new(OpenAiScheduler::openai_compatible(model_config)?);
    let tool_policy = cli_tool_policy(&cli)?;
    let tool_executor = Arc::new(LocalToolExecutor::new(tool_policy.clone()));
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
            tool_policy,
        });
        return web::serve(state).await;
    }

    // --- CLI mode ---
    let tools = tool_definitions_for_policy(&tool_policy);
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
            .context("failed to read from stdin")?;
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

pub(crate) fn load_model_config(base_url_override: Option<&str>) -> Result<OpenAiCompatibleConfig> {
    let api_key = std::env::var("MARVIS_API_KEY")
        .context("MARVIS_API_KEY environment variable not set. Set it to the key for your OpenAI-compatible provider.")?;
    let base_url = base_url_override
        .map(str::to_string)
        .or_else(|| std::env::var("MARVIS_BASE_URL").ok())
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
    Ok(OpenAiCompatibleConfig::new(api_key, base_url))
}

fn cli_tool_policy(cli: &Cli) -> Result<ToolPolicy> {
    Ok(ToolPolicy {
        cwd: std::env::current_dir()?,
        allow_workspace_write: cli.allow_workspace_write,
        allow_shell: true,
        allow_risky_shell: cli.allow_risky_shell,
        allow_git_write: cli.allow_risky_shell,
        allow_network: cli.allow_risky_shell,
        command_timeout_secs: 120,
    })
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

async fn print_trace(path: &PathBuf) -> Result<()> {
    let items = rollout::read_rollout_items(path).await?;
    for (index, item) in items.iter().enumerate() {
        match item {
            protocol::RolloutItem::SessionMeta(meta) => {
                println!(
                    "{index:04} session id={} source={:?} cwd={}",
                    meta.meta.id,
                    meta.meta.source,
                    meta.meta.cwd.display()
                );
            }
            protocol::RolloutItem::ResponseItem(item) => {
                let role = item.role().unwrap_or("tool");
                let text = item.text().unwrap_or_default();
                println!("{index:04} message role={role} text={}", one_line(&text));
            }
            protocol::RolloutItem::EventMsg(msg) => {
                println!("{index:04} event {}", event_label(msg));
            }
        }
    }
    Ok(())
}

fn event_label(msg: &protocol::EventMsg) -> String {
    match msg {
        protocol::EventMsg::Error(event) => format!("error {}", one_line(&event.message)),
        protocol::EventMsg::Warning(event) => format!("warning {}", one_line(&event.message)),
        protocol::EventMsg::SessionConfigured(event) => {
            format!("session_configured model={}", event.model)
        }
        protocol::EventMsg::ThreadNameUpdated(event) => {
            format!("thread_name_updated {}", event.name)
        }
        protocol::EventMsg::TurnStarted(event) => format!("turn_started {}", event.turn_id),
        protocol::EventMsg::TurnComplete(event) => format!(
            "turn_complete {} summary={}",
            event.turn_id,
            one_line(event.last_agent_message.as_deref().unwrap_or(""))
        ),
        protocol::EventMsg::TurnAborted(event) => {
            format!("turn_aborted {}", one_line(&event.reason))
        }
        protocol::EventMsg::UserMessage(event) => {
            format!("user_message {}", one_line(&event.message))
        }
        protocol::EventMsg::AgentMessage(event) => {
            format!("agent_message {}", one_line(&event.message))
        }
        protocol::EventMsg::AgentMessageDelta(event) => {
            format!("agent_delta {}", one_line(&event.delta))
        }
        protocol::EventMsg::ToolCallBegin(event) => {
            format!("tool_begin {} {}", event.name, one_line(&event.arguments))
        }
        protocol::EventMsg::ToolCallEnd(event) => {
            format!("tool_end {} {}", event.name, one_line(&event.output))
        }
        protocol::EventMsg::TokenCount(_) => "token_count".to_string(),
        protocol::EventMsg::ShutdownComplete => "shutdown_complete".to_string(),
    }
}

fn one_line(text: &str) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.len() > 160 {
        format!("{}...", &collapsed[..160])
    } else {
        collapsed
    }
}
