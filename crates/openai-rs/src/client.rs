use crate::auth::{AuthProvider, BearerAuth};
use crate::chat::Chat;
use crate::error::ApiError;
use crate::provider::{Provider, RetryConfig};
use crate::transport::ReqwestTransport;
use std::time::Duration;

/// Top-level OpenAI-compatible API client.
///
/// ```rust,no_run
/// use openai_rs::Client;
///
/// let client = Client::builder()
///     .api_key("sk-...")
///     .base_url("https://api.openai.com/v1")
///     .build()
///     .unwrap();
/// ```
pub struct Client {
    transport: ReqwestTransport,
    provider: Provider,
    auth: Box<dyn AuthProvider>,
}

impl Client {
    pub fn builder() -> ClientBuilder {
        ClientBuilder::default()
    }

    /// Access the Chat API namespace.
    pub fn chat(&self) -> Chat<'_> {
        Chat {
            transport: &self.transport,
            provider: &self.provider,
            auth: self.auth.as_ref(),
        }
    }

    /// Access the underlying provider config.
    pub fn provider(&self) -> &Provider {
        &self.provider
    }
}

pub struct ClientBuilder {
    api_key: Option<String>,
    base_url: String,
    max_retries: u64,
    timeout: Option<Duration>,
    stream_idle_timeout: Duration,
}

impl Default for ClientBuilder {
    fn default() -> Self {
        Self {
            api_key: None,
            base_url: "https://api.openai.com/v1".to_string(),
            max_retries: 3,
            timeout: None,
            stream_idle_timeout: Duration::from_secs(30),
        }
    }
}

impl ClientBuilder {
    pub fn api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }

    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    pub fn max_retries(mut self, n: u64) -> Self {
        self.max_retries = n;
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn stream_idle_timeout(mut self, timeout: Duration) -> Self {
        self.stream_idle_timeout = timeout;
        self
    }

    pub fn build(self) -> Result<Client, ApiError> {
        let api_key = self.api_key.ok_or_else(|| ApiError::InvalidRequest {
            message: "api_key is required".to_string(),
        })?;

        let mut reqwest_builder = reqwest::Client::builder();
        if let Some(timeout) = self.timeout {
            reqwest_builder = reqwest_builder.timeout(timeout);
        }
        let reqwest_client = reqwest_builder
            .build()
            .map_err(|e| ApiError::InvalidRequest {
                message: format!("failed to build HTTP client: {e}"),
            })?;

        let provider = Provider {
            base_url: self.base_url,
            query_params: None,
            headers: http::HeaderMap::new(),
            retry: RetryConfig {
                max_attempts: self.max_retries,
                ..RetryConfig::default()
            },
            stream_idle_timeout: self.stream_idle_timeout,
        };

        Ok(Client {
            transport: ReqwestTransport::new(reqwest_client),
            provider,
            auth: Box::new(BearerAuth::new(api_key)),
        })
    }
}
