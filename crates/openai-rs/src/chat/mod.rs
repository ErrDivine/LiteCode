pub mod completions;

pub use completions::{ChatStream, ChatStreamEvent, Completions};

/// Chat API namespace — access via `client.chat()`.
pub struct Chat<'a> {
    pub(crate) transport: &'a dyn crate::transport::HttpTransport,
    pub(crate) provider: &'a crate::provider::Provider,
    pub(crate) auth: &'a dyn crate::auth::AuthProvider,
}

impl<'a> Chat<'a> {
    pub fn completions(&self) -> Completions<'a> {
        Completions {
            transport: self.transport,
            provider: self.provider,
            auth: self.auth,
        }
    }
}
