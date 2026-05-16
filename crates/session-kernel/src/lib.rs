use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use protocol::protocol::{
    AgentMessageEvent, ErrorEvent, Event, EventMsg, RolloutItem, SessionConfiguredEvent,
    SessionSource, Submission, TokenUsageInfo, TurnAbortedEvent, TurnCompleteEvent,
    TurnStartedEvent, UserMessageEvent, W3cTraceContext,
};
use protocol::user_input::UserInput;
use protocol::{
    AskForApproval, DynamicToolSpec, Op, ResponseItem, SandboxPolicy, ThreadId, ToolCallBeginEvent,
    ToolCallEndEvent,
};
use rollout::{RolloutConfig, RolloutRecorder, RolloutRecorderParams};
use tokio::sync::{Mutex, RwLock, mpsc};

static SUBMISSION_COUNTER: AtomicU64 = AtomicU64::new(1);

pub type Result<T> = std::result::Result<T, KernelError>;

#[derive(Debug, thiserror::Error)]
pub enum KernelError {
    #[error("thread runtime is closed")]
    Closed,
    #[error("thread not found: {0}")]
    ThreadNotFound(ThreadId),
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Rollout(#[from] anyhow::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum SteerInputError {
    #[error("expected active turn {expected}, but active turn is {actual:?}")]
    WrongTurn {
        expected: String,
        actual: Option<String>,
    },
    #[error(transparent)]
    Kernel(#[from] KernelError),
}

#[derive(Debug, Clone)]
pub struct SessionConfig {
    pub model: String,
    pub model_provider_id: String,
    pub cwd: PathBuf,
    pub system_prompt: String,
    pub max_tokens: u32,
    pub approval_policy: AskForApproval,
    pub sandbox_policy: SandboxPolicy,
    pub dynamic_tools: Vec<DynamicToolSpec>,
    pub persist_history: bool,
    pub history_root: PathBuf,
    pub session_source: SessionSource,
}

pub type Config = SessionConfig;

impl SessionConfig {
    pub fn new(model: impl Into<String>, cwd: impl Into<PathBuf>) -> Self {
        let cwd = cwd.into();
        Self {
            model: model.into(),
            model_provider_id: "openrouter".to_string(),
            cwd,
            system_prompt: String::new(),
            max_tokens: 4096,
            approval_policy: AskForApproval::default(),
            sandbox_policy: SandboxPolicy::default(),
            dynamic_tools: Vec::new(),
            persist_history: true,
            history_root: default_history_root(),
            session_source: SessionSource::Cli,
        }
    }
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self::new(
            "synthetic",
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        )
    }
}

#[derive(Debug, Clone)]
pub struct ThreadConfigSnapshot {
    pub model: String,
    pub model_provider_id: String,
    pub approval_policy: AskForApproval,
    pub sandbox_policy: SandboxPolicy,
    pub cwd: PathBuf,
    pub ephemeral: bool,
    pub session_source: SessionSource,
}

#[derive(Clone)]
pub struct StartThreadOk {
    pub thread_id: ThreadId,
    pub thread: ThreadHandle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForkSnapshot {
    TruncateBeforeNthUserMessage(usize),
    Interrupted,
}

impl From<usize> for ForkSnapshot {
    fn from(value: usize) -> Self {
        Self::TruncateBeforeNthUserMessage(value)
    }
}

#[derive(Debug, Clone)]
pub struct ToolExecutionResult {
    pub output: String,
}

#[async_trait]
pub trait ToolExecutor: Send + Sync {
    async fn execute_tool(&self, name: &str, input: &serde_json::Value) -> ToolExecutionResult;
}

#[async_trait]
pub trait Scheduler: Send + Sync {
    async fn run_turn(&self, request: TurnRequest, events: EventEmitter)
    -> Result<SchedulerOutput>;
}

#[async_trait]
pub trait HistoryStore: Send + Sync {
    async fn create_thread_record(
        &self,
        thread_id: ThreadId,
        forked_from_id: Option<ThreadId>,
        config: &SessionConfig,
    ) -> Result<Option<PathBuf>>;

    async fn append_items(&self, thread_id: &ThreadId, items: Vec<RolloutItem>) -> Result<()>;

    async fn read_rollout(&self, path: PathBuf) -> Result<Vec<RolloutItem>>;

    async fn list_thread_ids(&self) -> Result<Vec<ThreadId>>;
}

#[async_trait]
pub trait EventSink: Send + Sync {
    async fn emit(&self, event: Event) -> Result<()>;
}

#[derive(Clone)]
pub struct TurnRequest {
    pub thread_id: ThreadId,
    pub submission_id: String,
    pub turn_id: String,
    pub model: String,
    pub max_tokens: u32,
    pub system_prompt: String,
    pub history: Vec<ResponseItem>,
    pub input: Vec<UserInput>,
    pub dynamic_tools: Vec<DynamicToolSpec>,
    pub final_output_json_schema: Option<serde_json::Value>,
    pub tool_executor: Arc<dyn ToolExecutor>,
    pub cancellation: CancellationFlag,
}

#[derive(Debug, Clone, Default)]
pub struct SchedulerOutput {
    pub response_items: Vec<ResponseItem>,
    pub final_message: Option<String>,
    pub token_usage: Option<TokenUsageInfo>,
}

#[derive(Clone)]
pub struct EventEmitter {
    submission_id: String,
    tx: mpsc::Sender<Event>,
}

impl EventEmitter {
    pub async fn emit(&self, msg: EventMsg) -> Result<()> {
        self.tx
            .send(Event {
                id: self.submission_id.clone(),
                msg,
            })
            .await
            .map_err(|_| KernelError::Closed)
    }

    pub async fn tool_begin(
        &self,
        call_id: impl Into<String>,
        name: impl Into<String>,
        arguments: impl Into<String>,
    ) -> Result<()> {
        self.emit(EventMsg::ToolCallBegin(ToolCallBeginEvent {
            call_id: call_id.into(),
            name: name.into(),
            arguments: arguments.into(),
        }))
        .await
    }

    pub async fn tool_end(
        &self,
        call_id: impl Into<String>,
        name: impl Into<String>,
        output: impl Into<String>,
    ) -> Result<()> {
        self.emit(EventMsg::ToolCallEnd(ToolCallEndEvent {
            call_id: call_id.into(),
            name: name.into(),
            output: output.into(),
        }))
        .await
    }
}

#[derive(Debug, Clone)]
pub struct CancellationFlag(Arc<AtomicBool>);

impl CancellationFlag {
    pub fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

impl Default for CancellationFlag {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
pub struct LocalHistoryStore {
    root: PathBuf,
}

impl LocalHistoryStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

#[async_trait]
impl HistoryStore for LocalHistoryStore {
    async fn create_thread_record(
        &self,
        thread_id: ThreadId,
        forked_from_id: Option<ThreadId>,
        config: &SessionConfig,
    ) -> Result<Option<PathBuf>> {
        if !config.persist_history {
            return Ok(None);
        }

        let recorder = RolloutRecorder::new(RolloutRecorderParams {
            config: RolloutConfig::new(&self.root),
            thread_id,
            forked_from_id,
            cwd: config.cwd.clone(),
            originator: "lite-code".to_string(),
            cli_version: env!("CARGO_PKG_VERSION").to_string(),
            source: config.session_source.clone(),
            model_provider: Some(config.model_provider_id.clone()),
            dynamic_tools: Some(config.dynamic_tools.clone()),
        })
        .await?;
        Ok(Some(recorder.path().to_path_buf()))
    }

    async fn append_items(&self, thread_id: &ThreadId, items: Vec<RolloutItem>) -> Result<()> {
        let path = rollout::rollout_path_for_thread(&self.root, thread_id);
        for item in items {
            rollout::append_rollout_item_to_path(&path, &item).await?;
        }
        Ok(())
    }

    async fn read_rollout(&self, path: PathBuf) -> Result<Vec<RolloutItem>> {
        Ok(rollout::read_rollout_items(path).await?)
    }

    async fn list_thread_ids(&self) -> Result<Vec<ThreadId>> {
        Ok(
            rollout::get_threads_in_root(&self.root, rollout::ThreadListConfig::default())
                .await?
                .items
                .into_iter()
                .map(|item| item.thread_id)
                .collect(),
        )
    }
}

pub struct ThreadManager {
    state: Arc<ThreadManagerState>,
}

struct ThreadManagerState {
    threads: RwLock<HashMap<ThreadId, ThreadHandle>>,
    scheduler: Arc<dyn Scheduler>,
    tool_executor: Arc<dyn ToolExecutor>,
    history_store: Arc<dyn HistoryStore>,
}

impl ThreadManager {
    pub fn new(
        scheduler: Arc<dyn Scheduler>,
        tool_executor: Arc<dyn ToolExecutor>,
        history_root: impl Into<PathBuf>,
    ) -> Self {
        Self {
            state: Arc::new(ThreadManagerState {
                threads: RwLock::new(HashMap::new()),
                scheduler,
                tool_executor,
                history_store: Arc::new(LocalHistoryStore::new(history_root)),
            }),
        }
    }

    pub fn with_history_store(
        scheduler: Arc<dyn Scheduler>,
        tool_executor: Arc<dyn ToolExecutor>,
        history_store: Arc<dyn HistoryStore>,
    ) -> Self {
        Self {
            state: Arc::new(ThreadManagerState {
                threads: RwLock::new(HashMap::new()),
                scheduler,
                tool_executor,
                history_store,
            }),
        }
    }

    pub async fn start_thread(&self, config: Config) -> Result<StartThreadOk> {
        self.create_thread(config, None, Vec::new()).await
    }

    pub async fn start_thread_with_tools(
        &self,
        mut config: Config,
        dynamic_tools: Vec<DynamicToolSpec>,
        persist_extended_history: bool,
    ) -> Result<StartThreadOk> {
        config.dynamic_tools = dynamic_tools;
        config.persist_history = persist_extended_history;
        self.create_thread(config, None, Vec::new()).await
    }

    pub async fn resume_thread_from_rollout(
        &self,
        config: Config,
        rollout_path: PathBuf,
    ) -> Result<StartThreadOk> {
        let items = self
            .state
            .history_store
            .read_rollout(rollout_path.clone())
            .await?;
        let mut thread_id = ThreadId::new();
        let mut history = Vec::new();

        for item in items {
            match item {
                RolloutItem::SessionMeta(line) => thread_id = line.meta.id,
                RolloutItem::ResponseItem(item) => history.push(item),
                RolloutItem::EventMsg(_) => {}
            }
        }

        self.create_thread_with_id(config, thread_id, None, Some(rollout_path), history)
            .await
    }

    pub async fn fork_thread<S>(
        &self,
        source_thread_id: ThreadId,
        snapshot: S,
        config: Config,
    ) -> Result<StartThreadOk>
    where
        S: Into<ForkSnapshot>,
    {
        let source = self
            .get_thread(&source_thread_id)
            .await
            .ok_or_else(|| KernelError::ThreadNotFound(source_thread_id.clone()))?;
        let history = source.history_snapshot().await;
        let fork_history = apply_fork_snapshot(history, snapshot.into());
        self.create_thread(config, Some(source_thread_id), fork_history)
            .await
    }

    pub async fn list_thread_ids(&self) -> Result<Vec<ThreadId>> {
        let mut ids = self.state.history_store.list_thread_ids().await?;
        ids.extend(self.state.threads.read().await.keys().cloned());
        ids.sort();
        ids.dedup();
        Ok(ids)
    }

    pub async fn get_thread(&self, thread_id: &ThreadId) -> Option<ThreadHandle> {
        self.state.threads.read().await.get(thread_id).cloned()
    }

    async fn create_thread(
        &self,
        config: Config,
        forked_from_id: Option<ThreadId>,
        history: Vec<ResponseItem>,
    ) -> Result<StartThreadOk> {
        let thread_id = ThreadId::new();
        self.create_thread_with_id(config, thread_id, forked_from_id, None, history)
            .await
    }

    async fn create_thread_with_id(
        &self,
        config: Config,
        thread_id: ThreadId,
        forked_from_id: Option<ThreadId>,
        rollout_path_override: Option<PathBuf>,
        history: Vec<ResponseItem>,
    ) -> Result<StartThreadOk> {
        let rollout_path = match rollout_path_override {
            Some(path) => Some(path),
            None => {
                self.state
                    .history_store
                    .create_thread_record(thread_id.clone(), forked_from_id.clone(), &config)
                    .await?
            }
        };

        if !history.is_empty() && config.persist_history {
            self.state
                .history_store
                .append_items(
                    &thread_id,
                    history
                        .iter()
                        .cloned()
                        .map(RolloutItem::ResponseItem)
                        .collect(),
                )
                .await?;
        }

        let (submission_tx, submission_rx) = mpsc::channel(128);
        let (event_tx, event_rx) = mpsc::channel(512);
        let inner = Arc::new(ThreadRuntime {
            thread_id: thread_id.clone(),
            submission_tx,
            event_tx: event_tx.clone(),
            event_rx: Mutex::new(event_rx),
            config: RwLock::new(config.clone()),
            history: Mutex::new(history),
            active_turn: Mutex::new(None),
            pending_input: Mutex::new(Vec::new()),
            rollout_path,
            scheduler: Arc::clone(&self.state.scheduler),
            tool_executor: Arc::clone(&self.state.tool_executor),
            history_store: Arc::clone(&self.state.history_store),
        });

        tokio::spawn(submission_loop(Arc::clone(&inner), submission_rx));

        let handle = ThreadHandle { inner };
        self.state
            .threads
            .write()
            .await
            .insert(thread_id.clone(), handle.clone());

        handle
            .emit(
                "session_configured".to_string(),
                EventMsg::SessionConfigured(SessionConfiguredEvent {
                    session_id: thread_id.clone(),
                    forked_from_id,
                    thread_name: None,
                    model: config.model,
                    model_provider_id: config.model_provider_id,
                    approval_policy: config.approval_policy,
                    sandbox_policy: config.sandbox_policy,
                    cwd: config.cwd,
                    rollout_path: handle.inner.rollout_path.clone(),
                    initial_messages: None,
                }),
            )
            .await?;

        Ok(StartThreadOk {
            thread_id,
            thread: handle,
        })
    }
}

#[derive(Clone)]
pub struct ThreadHandle {
    inner: Arc<ThreadRuntime>,
}

struct ThreadRuntime {
    thread_id: ThreadId,
    submission_tx: mpsc::Sender<Submission>,
    event_tx: mpsc::Sender<Event>,
    event_rx: Mutex<mpsc::Receiver<Event>>,
    config: RwLock<SessionConfig>,
    history: Mutex<Vec<ResponseItem>>,
    active_turn: Mutex<Option<ActiveTurn>>,
    pending_input: Mutex<Vec<UserInput>>,
    rollout_path: Option<PathBuf>,
    scheduler: Arc<dyn Scheduler>,
    tool_executor: Arc<dyn ToolExecutor>,
    history_store: Arc<dyn HistoryStore>,
}

#[derive(Clone)]
struct ActiveTurn {
    turn_id: String,
    cancellation: CancellationFlag,
}

impl ThreadHandle {
    pub async fn submit(&self, op: Op) -> Result<String> {
        let id = next_submission_id();
        self.submit_with_id(Submission {
            id: id.clone(),
            op,
            trace: None,
        })
        .await?;
        Ok(id)
    }

    pub async fn submit_with_id(&self, sub: Submission) -> Result<()> {
        if matches!(sub.op, Op::Interrupt) {
            self.interrupt(sub.id).await?;
            return Ok(());
        }

        self.inner
            .submission_tx
            .send(sub)
            .await
            .map_err(|_| KernelError::Closed)
    }

    pub async fn submit_with_trace(
        &self,
        op: Op,
        trace: Option<W3cTraceContext>,
    ) -> Result<String> {
        let id = next_submission_id();
        self.submit_with_id(Submission {
            id: id.clone(),
            op,
            trace,
        })
        .await?;
        Ok(id)
    }

    pub async fn next_event(&self) -> Result<Event> {
        self.inner
            .event_rx
            .lock()
            .await
            .recv()
            .await
            .ok_or(KernelError::Closed)
    }

    pub async fn steer_input(
        &self,
        input: Vec<UserInput>,
        expected_turn_id: Option<&str>,
        _client_metadata: Option<HashMap<String, String>>,
    ) -> std::result::Result<String, SteerInputError> {
        let active = self.inner.active_turn.lock().await.clone();

        if let Some(expected) = expected_turn_id {
            let actual = active.as_ref().map(|turn| turn.turn_id.clone());
            if actual.as_deref() != Some(expected) {
                return Err(SteerInputError::WrongTurn {
                    expected: expected.to_string(),
                    actual,
                });
            }
        }

        if active.is_some() {
            self.inner.pending_input.lock().await.extend(input);
            Ok(next_submission_id())
        } else {
            Ok(self
                .submit(Op::UserInput {
                    items: input,
                    final_output_json_schema: None,
                    responsesapi_client_metadata: None,
                })
                .await?)
        }
    }

    pub async fn inject_response_items(&self, items: Vec<ResponseItem>) -> Result<()> {
        if items.is_empty() {
            return Err(KernelError::InvalidRequest(
                "items must not be empty".to_string(),
            ));
        }

        self.inner.history.lock().await.extend(items.clone());
        self.persist_items(items.into_iter().map(RolloutItem::ResponseItem).collect())
            .await
    }

    pub async fn flush_rollout(&self) -> std::io::Result<()> {
        Ok(())
    }

    pub async fn config_snapshot(&self) -> ThreadConfigSnapshot {
        let config = self.inner.config.read().await;
        ThreadConfigSnapshot {
            model: config.model.clone(),
            model_provider_id: config.model_provider_id.clone(),
            approval_policy: config.approval_policy.clone(),
            sandbox_policy: config.sandbox_policy.clone(),
            cwd: config.cwd.clone(),
            ephemeral: !config.persist_history,
            session_source: config.session_source.clone(),
        }
    }

    pub async fn token_usage_info(&self) -> Option<TokenUsageInfo> {
        None
    }

    pub fn rollout_path(&self) -> Option<PathBuf> {
        self.inner.rollout_path.clone()
    }

    async fn history_snapshot(&self) -> Vec<ResponseItem> {
        self.inner.history.lock().await.clone()
    }

    async fn interrupt(&self, submission_id: String) -> Result<()> {
        let active = self.inner.active_turn.lock().await.clone();
        if let Some(active) = active {
            active.cancellation.cancel();
            self.emit(
                submission_id,
                EventMsg::TurnAborted(TurnAbortedEvent {
                    reason: "interrupted".to_string(),
                }),
            )
            .await?;
        } else {
            self.emit(
                submission_id,
                EventMsg::TurnAborted(TurnAbortedEvent {
                    reason: "idle".to_string(),
                }),
            )
            .await?;
        }
        Ok(())
    }

    async fn emit(&self, id: String, msg: EventMsg) -> Result<()> {
        self.inner
            .event_tx
            .send(Event { id, msg })
            .await
            .map_err(|_| KernelError::Closed)
    }

    async fn persist_items(&self, items: Vec<RolloutItem>) -> Result<()> {
        if items.is_empty() || self.inner.rollout_path.is_none() {
            return Ok(());
        }
        self.inner
            .history_store
            .append_items(&self.inner.thread_id, items)
            .await
    }
}

async fn submission_loop(runtime: Arc<ThreadRuntime>, mut rx: mpsc::Receiver<Submission>) {
    while let Some(submission) = rx.recv().await {
        if let Err(err) = process_submission(Arc::clone(&runtime), submission.clone()).await {
            let _ = runtime
                .event_tx
                .send(Event {
                    id: submission.id,
                    msg: EventMsg::Error(ErrorEvent {
                        message: err.to_string(),
                    }),
                })
                .await;
        }
    }
}

async fn process_submission(runtime: Arc<ThreadRuntime>, submission: Submission) -> Result<()> {
    match submission.op {
        Op::Interrupt => Ok(()),
        Op::InjectResponseItems { items } => {
            runtime.history.lock().await.extend(items.clone());
            runtime
                .history_store
                .append_items(
                    &runtime.thread_id,
                    items.into_iter().map(RolloutItem::ResponseItem).collect(),
                )
                .await
        }
        Op::Synthetic { message } => {
            run_user_turn(
                runtime,
                submission.id,
                vec![UserInput::text(message)],
                None,
                None,
            )
            .await
        }
        Op::UserInput {
            items,
            final_output_json_schema,
            ..
        } => {
            run_user_turn(
                runtime,
                submission.id,
                items,
                final_output_json_schema,
                None,
            )
            .await
        }
        Op::UserTurn {
            items,
            cwd,
            approval_policy,
            sandbox_policy,
            model,
            final_output_json_schema,
        } => {
            {
                let mut config = runtime.config.write().await;
                config.cwd = cwd;
                config.approval_policy = approval_policy;
                config.sandbox_policy = sandbox_policy;
                config.model = model;
            }
            run_user_turn(
                runtime,
                submission.id,
                items,
                final_output_json_schema,
                None,
            )
            .await
        }
    }
}

async fn run_user_turn(
    runtime: Arc<ThreadRuntime>,
    submission_id: String,
    input: Vec<UserInput>,
    final_output_json_schema: Option<serde_json::Value>,
    forced_turn_id: Option<String>,
) -> Result<()> {
    let config = runtime.config.read().await.clone();
    let turn_id = forced_turn_id.unwrap_or_else(next_turn_id);
    let cancellation = CancellationFlag::new();
    let started_at = unix_seconds();
    let started = Instant::now();

    {
        let mut active = runtime.active_turn.lock().await;
        *active = Some(ActiveTurn {
            turn_id: turn_id.clone(),
            cancellation: cancellation.clone(),
        });
    }

    let emitter = EventEmitter {
        submission_id: submission_id.clone(),
        tx: runtime.event_tx.clone(),
    };
    emitter
        .emit(EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: turn_id.clone(),
            started_at: Some(started_at),
            model_context_window: None,
        }))
        .await?;

    let user_text = input
        .iter()
        .filter_map(UserInput::as_text)
        .collect::<Vec<_>>()
        .join("\n");
    if !user_text.is_empty() {
        emitter
            .emit(EventMsg::UserMessage(UserMessageEvent {
                message: user_text.clone(),
            }))
            .await?;
    }

    let user_item = ResponseItem::message("user", user_text);
    {
        runtime.history.lock().await.push(user_item.clone());
    }
    runtime
        .history_store
        .append_items(
            &runtime.thread_id,
            vec![RolloutItem::ResponseItem(user_item.clone())],
        )
        .await?;

    let history = runtime.history.lock().await.clone();
    let request = TurnRequest {
        thread_id: runtime.thread_id.clone(),
        submission_id: submission_id.clone(),
        turn_id: turn_id.clone(),
        model: config.model,
        max_tokens: config.max_tokens,
        system_prompt: config.system_prompt,
        history,
        input,
        dynamic_tools: config.dynamic_tools,
        final_output_json_schema,
        tool_executor: Arc::clone(&runtime.tool_executor),
        cancellation: cancellation.clone(),
    };

    let output = runtime.scheduler.run_turn(request, emitter.clone()).await?;

    if !cancellation.is_cancelled() {
        if let Some(message) = output.final_message.clone() {
            emitter
                .emit(EventMsg::AgentMessage(AgentMessageEvent { message }))
                .await?;
        }
        emitter
            .emit(EventMsg::TurnComplete(TurnCompleteEvent {
                turn_id: turn_id.clone(),
                last_agent_message: output.final_message.clone(),
                completed_at: Some(unix_seconds()),
                duration_ms: Some(started.elapsed().as_millis() as i64),
            }))
            .await?;
    }

    if !output.response_items.is_empty() {
        runtime
            .history
            .lock()
            .await
            .extend(output.response_items.clone());
        runtime
            .history_store
            .append_items(
                &runtime.thread_id,
                output
                    .response_items
                    .into_iter()
                    .map(RolloutItem::ResponseItem)
                    .collect(),
            )
            .await?;
    }

    {
        let mut active = runtime.active_turn.lock().await;
        *active = None;
    }

    let pending = {
        let mut pending = runtime.pending_input.lock().await;
        std::mem::take(&mut *pending)
    };
    if !pending.is_empty() {
        runtime
            .submission_tx
            .send(Submission {
                id: next_submission_id(),
                op: Op::UserInput {
                    items: pending,
                    final_output_json_schema: None,
                    responsesapi_client_metadata: None,
                },
                trace: None,
            })
            .await
            .map_err(|_| KernelError::Closed)?;
    }

    Ok(())
}

fn apply_fork_snapshot(history: Vec<ResponseItem>, snapshot: ForkSnapshot) -> Vec<ResponseItem> {
    match snapshot {
        ForkSnapshot::Interrupted => history,
        ForkSnapshot::TruncateBeforeNthUserMessage(n) => {
            let mut seen = 0usize;
            let mut kept = Vec::new();
            for item in history {
                if item.role() == Some("user") {
                    if seen == n {
                        break;
                    }
                    seen += 1;
                }
                kept.push(item);
            }
            kept
        }
    }
}

fn next_submission_id() -> String {
    format!("sub-{}", SUBMISSION_COUNTER.fetch_add(1, Ordering::Relaxed))
}

fn next_turn_id() -> String {
    format!(
        "turn-{}",
        SUBMISSION_COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

fn unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn default_history_root() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".lite-code")
}

#[cfg(test)]
mod tests {
    use protocol::AgentMessageDeltaEvent;

    use super::*;

    #[derive(Default)]
    struct EchoScheduler;

    #[async_trait]
    impl Scheduler for EchoScheduler {
        async fn run_turn(
            &self,
            request: TurnRequest,
            events: EventEmitter,
        ) -> Result<SchedulerOutput> {
            let text = request
                .input
                .iter()
                .filter_map(UserInput::as_text)
                .collect::<Vec<_>>()
                .join("\n");
            let output = format!("echo: {text}");
            events
                .emit(EventMsg::AgentMessageDelta(AgentMessageDeltaEvent {
                    delta: output.clone(),
                }))
                .await?;
            Ok(SchedulerOutput {
                response_items: vec![ResponseItem::message("assistant", output.clone())],
                final_message: Some(output),
                token_usage: None,
            })
        }
    }

    #[derive(Default)]
    struct NoopTools;

    #[async_trait]
    impl ToolExecutor for NoopTools {
        async fn execute_tool(
            &self,
            _name: &str,
            _input: &serde_json::Value,
        ) -> ToolExecutionResult {
            ToolExecutionResult {
                output: String::new(),
            }
        }
    }

    fn manager(temp: &tempfile::TempDir) -> ThreadManager {
        ThreadManager::new(
            Arc::new(EchoScheduler),
            Arc::new(NoopTools),
            temp.path().join("history"),
        )
    }

    #[tokio::test]
    async fn start_submit_and_consume_events() {
        let temp = tempfile::tempdir().unwrap();
        let manager = manager(&temp);
        let ok = manager
            .start_thread(SessionConfig::default())
            .await
            .unwrap();

        assert!(matches!(
            ok.thread.next_event().await.unwrap().msg,
            EventMsg::SessionConfigured(_)
        ));

        ok.thread.submit(Op::user_text("hello")).await.unwrap();
        assert!(matches!(
            ok.thread.next_event().await.unwrap().msg,
            EventMsg::TurnStarted(_)
        ));
        assert!(matches!(
            ok.thread.next_event().await.unwrap().msg,
            EventMsg::UserMessage(_)
        ));
        assert!(matches!(
            ok.thread.next_event().await.unwrap().msg,
            EventMsg::AgentMessageDelta(_)
        ));
        assert!(matches!(
            ok.thread.next_event().await.unwrap().msg,
            EventMsg::AgentMessage(_)
        ));
        assert!(matches!(
            ok.thread.next_event().await.unwrap().msg,
            EventMsg::TurnComplete(_)
        ));
    }

    #[tokio::test]
    async fn resume_and_fork_keep_history() {
        let temp = tempfile::tempdir().unwrap();
        let manager = manager(&temp);
        let ok = manager
            .start_thread(SessionConfig::default())
            .await
            .unwrap();
        let _ = ok.thread.next_event().await.unwrap();
        ok.thread
            .inject_response_items(vec![ResponseItem::message("user", "old")])
            .await
            .unwrap();
        let rollout_path = ok.thread.rollout_path().unwrap();

        let resumed = manager
            .resume_thread_from_rollout(SessionConfig::default(), rollout_path)
            .await
            .unwrap();
        assert_eq!(resumed.thread.history_snapshot().await.len(), 1);

        let forked = manager
            .fork_thread(
                ok.thread_id,
                ForkSnapshot::Interrupted,
                SessionConfig::default(),
            )
            .await
            .unwrap();
        assert_eq!(forked.thread.history_snapshot().await.len(), 1);
    }

    #[tokio::test]
    async fn idle_steer_input_starts_a_turn() {
        let temp = tempfile::tempdir().unwrap();
        let manager = manager(&temp);
        let ok = manager
            .start_thread(SessionConfig::default())
            .await
            .unwrap();
        let _ = ok.thread.next_event().await.unwrap();

        ok.thread
            .steer_input(vec![UserInput::text("queued")], None, None)
            .await
            .unwrap();

        assert!(matches!(
            ok.thread.next_event().await.unwrap().msg,
            EventMsg::TurnStarted(_)
        ));
    }

    #[tokio::test]
    async fn interrupt_while_idle_emits_abort_event() {
        let temp = tempfile::tempdir().unwrap();
        let manager = manager(&temp);
        let ok = manager
            .start_thread(SessionConfig::default())
            .await
            .unwrap();
        let _ = ok.thread.next_event().await.unwrap();

        ok.thread.submit(Op::Interrupt).await.unwrap();

        assert!(matches!(
            ok.thread.next_event().await.unwrap().msg,
            EventMsg::TurnAborted(_)
        ));
    }
}
