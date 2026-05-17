# Interface Reference

This page is a compact lookup for public interfaces across the workspace.

## Protocol Interfaces

| Interface | Kind | Purpose |
| --- | --- | --- |
| `ThreadId` | struct | Typed thread identifier with generation, parsing, display, and serde. |
| `W3cTraceContext` | struct | Optional traceparent/tracestate metadata. |
| `Submission` | struct | Submission id, operation, and optional trace. |
| `Op` | enum | Runtime operations: interrupt, user input, user turn, injection, synthetic. |
| `AskForApproval` | enum | Approval policy: never or on request. |
| `SandboxPolicy` | enum | Sandbox mode: workspace write, read only, danger full access. |
| `DynamicToolSpec` | struct | Tool name, description, and JSON schema parameters. |
| `UserInput` | enum | Text, image, local image, skill, or mention. |
| `TextElement` | struct | Structured byte range in a text input. |
| `ByteRange` | struct | Start/end byte offsets. |
| `ContentItem` | enum | Input or output text content. |
| `MessagePhase` | enum | Commentary or final answer marker. |
| `ResponseItem` | enum | Message, function call, or function call output. |
| `Event` | struct | Submission id plus `EventMsg`. |
| `EventMsg` | enum | Runtime event vocabulary. |
| Event payload structs | structs | Error, warning, session, turn, message, tool, token events. |
| `TokenUsage` | struct | Input/output/reasoning/total token counters. |
| `TokenUsageInfo` | struct | Last and total usage plus optional context window. |
| `SessionSource` | enum | CLI, web, custom, unknown. |
| `SessionMeta` | struct | Persistent thread metadata. |
| `SessionMetaLine` | struct | Rollout metadata wrapper. |
| `RolloutItem` | enum | JSONL line type. |
| `ForkedFrom` | enum | Fork source classification. |
| `ProtocolError` | enum | Invalid thread id. |

## Session Kernel Interfaces

| Interface | Kind | Purpose |
| --- | --- | --- |
| `KernelError` | enum | Runtime error type. |
| `SteerInputError` | enum | Steering error type. |
| `SessionConfig` | struct | Thread runtime configuration. |
| `ThreadConfigSnapshot` | struct | Read-only config snapshot. |
| `StartThreadOk` | struct | Thread id and handle returned by manager. |
| `ForkSnapshot` | enum | Fork history selection. |
| `ToolExecutionResult` | struct | Tool output wrapper. |
| `ToolExecutor` | trait | Async tool execution boundary. |
| `Scheduler` | trait | Async model turn boundary. |
| `HistoryStore` | trait | Thread history persistence boundary. |
| `EventSink` | trait | Async event sink boundary. |
| `TurnRequest` | struct | Complete scheduler request. |
| `SchedulerOutput` | struct | Scheduler result for history and final message. |
| `EventEmitter` | struct | Submission-scoped event sender. |
| `CancellationFlag` | struct | Shared cancellation flag. |
| `LocalHistoryStore` | struct | Rollout-backed history store. |
| `ThreadManager` | struct | Active thread manager and factory. |
| `ThreadHandle` | struct | Per-thread submission/event/history handle. |

## Scheduler Interfaces

| Interface | Kind | Purpose |
| --- | --- | --- |
| `OpenAiScheduler` | struct | Real OpenAI-compatible scheduler. |
| `SyntheticScheduler` | struct | Local deterministic scheduler. |

## OpenAI Client Interfaces

| Interface | Kind | Purpose |
| --- | --- | --- |
| `AuthProvider` | trait | Bearer-token source. |
| `BearerAuth` | struct | Static bearer-token auth. |
| `Client` | struct | Top-level API client. |
| `ClientBuilder` | struct | Client configuration builder. |
| `Provider` | struct | Endpoint, headers, query params, retry, timeout. |
| `RetryConfig` | struct | High-level retry settings. |
| `Request` | struct | Transport-neutral HTTP request. |
| `Response` | struct | Transport-neutral HTTP response. |
| `HttpTransport` | trait | Execute and stream HTTP requests. |
| `ReqwestTransport` | struct | Reqwest implementation. |
| `RetryPolicy` | struct | Retry execution policy. |
| `RetryOn` | struct | Retry predicate settings. |
| `Chat` | struct | Chat namespace. |
| `Completions` | struct | Chat completions API. |
| `ChatStream` | struct | Async stream of chat events. |
| `ChatStreamEvent` | enum | Delta or done. |
| `ChatCompletionRequest` | struct | Chat request payload. |
| `Message` | struct | Chat message. |
| `Role` | enum | System, user, assistant, tool. |
| `ToolCall` | struct | Assistant tool call. |
| `ToolCallFunction` | struct | Tool call function details. |
| `ChatCompletionResponse` | struct | Non-streaming response. |
| `Choice` | struct | Response choice. |
| `ToolDefinition` | struct | Tool definition payload. |
| `FunctionDef` | struct | Function tool details. |
| `Usage` | struct | Token usage. |
| `FinishReason` | enum | Stop, tool calls, length, content filter. |
| `StreamChunk` and delta types | structs | Streaming response shape. |
| `TransportError`, `StreamError`, `ApiError` | enums | Error hierarchy. |

## Status Interfaces

| Interface | Kind | Purpose |
| --- | --- | --- |
| `WorkspaceMeta` | struct | Workspace root/name/language. |
| `Freshness` | enum | Segment freshness. |
| `RiskLevel` | enum | Segment risk. |
| `SegmentKind` | enum | Segment category. |
| `StatusSegment` | struct | Deterministic status segment. |
| `Position`, `TextRange` | structs | Editor positions and ranges. |
| `EditorRef` | struct | File editor reference. |
| `VisibleRange` | struct | Visible editor range. |
| `SelectionState` | struct | Selection state. |
| `CursorContext` | struct | Cursor bubble and symbol hint. |
| `DiagnosticSeverity` | enum | VSCode diagnostic severity. |
| `DiagnosticEvent` | struct | Diagnostic record. |
| `TerminalSessionState` | struct | Terminal-like session state. |
| `VscodeTaskState` | struct | Task state. |
| `DebugSessionState` | struct | Debug state. |
| `ClipboardHint` | struct | Clipboard summary. |
| `VscodeStatus` | struct | Full IDE status payload. |
| `GitDiffSummary` | struct | Diff shortstat. |
| `GitState` | struct | Git working tree state. |
| `CommandResult` | struct | Command result record. |
| `CommandState` | struct | Recent command list. |
| `CodebaseStatus` | struct | Full status snapshot. |
| `ProactiveSuggestion` | struct | Suggested intervention. |
| `StucknessSignal` | struct | Friction detection result. |
| `StatusReport` | struct | UI-facing report. |
| `ContextCapsule` | struct | Prompt-facing context. |
| `StatusStore` | struct | Mutable status store. |
| `ParsedGitStatus` | struct | Parsed porcelain status. |

## Persistence Interfaces

| Interface | Kind | Purpose |
| --- | --- | --- |
| `RolloutConfig` | struct | Rollout root and derived dirs. |
| `RolloutRecorderParams` | struct | Metadata for recorder creation. |
| `RolloutRecorder` | struct | Append-only rollout writer. |
| `ThreadItem` | struct | Listed rollout thread. |
| `ThreadListConfig` | struct | Include archived flag. |
| `ThreadsPage` | struct | Rollout listing page. |
| `ThreadStoreError` | enum | Thread store errors. |
| `ThreadStore` | trait | Thread storage API. |
| `LocalThreadStore` | struct | Local rollout-backed store. |
| `RemoteThreadStore` | struct | Remote stub. |
| `ThreadRecorder` | struct | Append helper. |
| `StateRuntime` | struct | Metadata/log runtime. |
| `ThreadMetadata` | struct | Thread metadata row. |
| `ThreadMetadataBuilder` | struct | Optional metadata builder. |
| `LogEntry`, `LogQuery`, `LogRow` | structs | Log API shell. |

## UI Bridge Interfaces

| Interface | Kind | Purpose |
| --- | --- | --- |
| `VscodeRequestEnvelope` | struct | Stdio request envelope. |
| `VscodeRequest` | enum | Initialize, status update, command result, prompt, shutdown. |
| `VscodeResponseEnvelope` | struct | Stdio response envelope. |
| `VscodeResponse` | enum | Ready, status report, agent event, complete, error, shutdown complete. |
| `VscodeRuntimeEvent` | enum | Surface event for VSCode. |
| `WebEvent` | struct | SSE event name and JSON data. |
| `CliEvent` | enum | Terminal event rendering instruction. |

## Root Runtime Interfaces

| Interface | Kind | Purpose |
| --- | --- | --- |
| `Cli` | struct | CLI flag parser. |
| `AppState` | struct | Web harness state. |
| `ChatRequest`, `WebMessage` | structs | Web chat payload. |
| `LocalToolExecutor` | struct | Root tool executor implementation. |
| `ToolResult` | struct | Tool output wrapper. |
| `VscodeServer` | struct | Internal stdio server state. |

## VSCode Extension Interfaces

| Interface | Kind | Purpose |
| --- | --- | --- |
| `activate`, `deactivate` | functions | Extension lifecycle. |
| `MarvisController` | class | Main extension coordinator. |
| `RuntimeClient` | class | Child process JSON-RPC-like client. |
| `MarvisCodeActionProvider` | class | Diagnostic quick fixes. |
| helper functions | functions | Runtime resolution, status collection, rendering, truncation, path checks. |
