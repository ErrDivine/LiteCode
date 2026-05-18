use axum::extract::State;
use axum::response::sse::{Event, Sse};
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use axum::{Json, Router};
use protocol::ResponseItem;
use serde::Deserialize;
use session_kernel::{SessionConfig, ThreadHandle, ThreadManager};
use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::Arc;
use tokio_stream::wrappers::ReceiverStream;

use crate::tools::{ToolPolicy, tool_definitions_for_policy};

const INDEX_HTML: &str = include_str!("../static/index.html");

pub struct AppState {
    pub manager: Arc<ThreadManager>,
    pub history_root: PathBuf,
    pub model: String,
    pub max_tokens: u32,
    pub tool_policy: ToolPolicy,
}

#[derive(Deserialize)]
struct ChatRequest {
    messages: Vec<WebMessage>,
}

#[derive(Deserialize)]
struct WebMessage {
    role: String,
    content: Option<String>,
}

pub async fn serve(state: Arc<AppState>) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/", get(index))
        .route("/api/chat", post(chat))
        .with_state(state);

    let addr = "127.0.0.1:3000";
    println!("lite-code running at http://localhost:3000");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn index() -> impl IntoResponse {
    Html(INDEX_HTML)
}

async fn chat(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ChatRequest>,
) -> Sse<impl futures::Stream<Item = Result<Event, Infallible>>> {
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(256);

    let state_clone = Arc::clone(&state);
    tokio::spawn(async move {
        let _ = run_thread_loop(state_clone, payload.messages, tx).await;
    });

    Sse::new(ReceiverStream::new(rx))
}

async fn run_thread_loop(
    state: Arc<AppState>,
    messages: Vec<WebMessage>,
    tx: tokio::sync::mpsc::Sender<Result<Event, Infallible>>,
) -> anyhow::Result<()> {
    let Some((last, prior)) = messages.split_last() else {
        send_event(
            &tx,
            "error",
            &serde_json::json!({"error": "missing message"}),
        )
        .await;
        return Ok(());
    };
    let Some(user_text) = last.content.clone() else {
        send_event(
            &tx,
            "error",
            &serde_json::json!({"error": "missing user content"}),
        )
        .await;
        return Ok(());
    };

    let thread = start_web_thread(&state).await?;
    let _ = thread.next_event().await?;

    let history = prior
        .iter()
        .filter_map(|message| {
            let content = message.content.clone()?;
            match message.role.as_str() {
                "user" => Some(ResponseItem::message("user", content)),
                "assistant" => Some(ResponseItem::message("assistant", content)),
                _ => None,
            }
        })
        .collect::<Vec<_>>();
    if !history.is_empty() {
        thread.inject_response_items(history).await?;
    }

    thread.submit(ui_bridge::user_text_op(user_text)).await?;

    while let Ok(event) = thread.next_event().await {
        if let Some(web_event) = ui_bridge::event_to_web(&event) {
            send_event(&tx, web_event.event, &web_event.data).await;
            if web_event.event == "done" || web_event.event == "error" {
                break;
            }
        }
    }

    let _ = tx.send(Ok(Event::default().data("[DONE]"))).await;
    Ok(())
}

async fn start_web_thread(state: &AppState) -> anyhow::Result<ThreadHandle> {
    let mut config = SessionConfig::new(
        state.model.clone(),
        std::env::current_dir().unwrap_or_else(|_| ".".into()),
    );
    config.max_tokens = state.max_tokens;
    config.history_root = state.history_root.clone();
    config.session_source = protocol::SessionSource::Web;
    config.system_prompt = crate::SYSTEM_PROMPT.to_string();

    Ok(state
        .manager
        .start_thread_with_tools(
            config,
            tool_definitions_for_policy(&state.tool_policy),
            true,
        )
        .await?
        .thread)
}

async fn send_event(
    tx: &tokio::sync::mpsc::Sender<Result<Event, Infallible>>,
    event_type: &str,
    data: &serde_json::Value,
) {
    let _ = tx
        .send(Ok(Event::default()
            .event(event_type)
            .data(data.to_string())))
        .await;
}
