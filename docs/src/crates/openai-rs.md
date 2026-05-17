# OpenAI Client

`crates/openai-rs` is a reusable OpenAI-compatible client. The scheduler uses it for streaming chat completions, but the crate itself has no Marvis-specific concepts.

## Module Map

| Module | Purpose |
| --- | --- |
| `lib.rs` | Public module declarations and top-level re-exports. |
| `auth.rs` | Auth provider trait and bearer-token auth. |
| `client.rs` | Top-level `Client` and `ClientBuilder`. |
| `provider.rs` | Provider URL, headers, query params, retry config, and stream timeout. |
| `request.rs` | Transport-neutral request and response structs. |
| `transport.rs` | HTTP transport trait and reqwest implementation. |
| `retry.rs` | Retry policy, retry predicates, exponential backoff with jitter. |
| `sse.rs` | SSE byte stream parser task. |
| `error.rs` | Transport, stream, and API error enums. |
| `chat/mod.rs` | Chat namespace. |
| `chat/completions.rs` | Chat completion create and create_stream APIs. |
| `types/chat.rs` | Chat request, message, role, tool call, and response types. |
| `types/common.rs` | Tool definition, function definition, usage, finish reason. |
| `types/stream.rs` | Streaming chunk and delta types. |

## Public Re-exports

The crate root re-exports:

- `AuthProvider`, `BearerAuth`
- `ChatStream`, `ChatStreamEvent`
- `Client`, `ClientBuilder`
- `ApiError`, `StreamError`, `TransportError`
- `Provider`, `RetryConfig`
- chat request/response/message/tool types
- common usage and finish-reason types
- streaming delta types

This lets consumers use `openai_rs::Client` and related types without deep module paths.

## Client And Provider

`Client::builder()` creates a `ClientBuilder` with defaults:

- base URL: `https://api.openai.com/v1`
- max retries: `3`
- no global request timeout
- stream idle timeout: `30s`

Builder methods:

- `api_key(key)`
- `base_url(url)`
- `max_retries(n)`
- `timeout(duration)`
- `stream_idle_timeout(duration)`
- `build()`

`Provider` is the runtime endpoint config:

- `base_url`
- optional query params
- headers
- retry config
- stream idle timeout

`Provider::url_for_path` joins base URL, path, and query params. `Provider::build_request` creates a `Request` with method, URL, cloned headers, no body, and no timeout.

## Auth

`AuthProvider` returns an optional bearer token. `BearerAuth` stores one token and returns it for each request. Internal `add_auth_headers` attaches an `Authorization: Bearer ...` header when a token is available.

## Transport

`HttpTransport` defines:

- `execute(req) -> Response`
- `stream(req) -> StreamResponse`

`ReqwestTransport` implements it with `reqwest::Client`.

Transport behavior:

- JSON bodies are sent with `.json(&body)`.
- Non-success responses become `TransportError::Http`.
- Timeouts become `TransportError::Timeout`.
- Other reqwest errors become `TransportError::Network`.
- Streaming responses expose a boxed byte stream.

## Retry

`RetryConfig` converts to `RetryPolicy`.

Retry controls:

- maximum attempts
- base delay
- retry 429
- retry 5xx
- retry transport errors

`run_with_retry` regenerates the request for every attempt. `backoff` uses exponential delay with 0.9 to 1.1 jitter.

## SSE Streaming

`sse_stream` takes a byte stream and forwards raw SSE `data:` payload strings over a channel. It applies an idle timeout per stream item. Timeouts and parsing failures are sent as `StreamError` values.

## Chat Completions

`Chat::completions()` returns a `Completions` namespace.

`Completions::create`:

- forces `stream = false`
- serializes `ChatCompletionRequest`
- runs a POST to `chat/completions` with retry
- parses `ChatCompletionResponse`

`Completions::create_stream`:

- forces `stream = true`
- sets `Accept: text/event-stream`
- starts SSE parsing
- converts chunks into `ChatStreamEvent::Delta`
- emits `ChatStreamEvent::Done` when `[DONE]` appears

`ChatStreamEvent::Delta` contains optional content, optional tool-call deltas, and optional finish reason.

## Type Design

The type set intentionally mirrors OpenAI-compatible chat APIs:

- `ChatCompletionRequest`
- `Message`
- `Role`
- `ToolCall`
- `ToolCallFunction`
- `ChatCompletionResponse`
- `Choice`
- `ToolDefinition`
- `FunctionDef`
- `Usage`
- `FinishReason`
- `StreamChunk`, `StreamChoice`, `Delta`, `ToolCallDelta`, `FunctionDelta`

This makes the scheduler mapping straightforward and keeps provider-specific JSON concerns inside one crate.
