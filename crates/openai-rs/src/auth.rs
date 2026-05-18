use crate::request::Request;
use http::HeaderValue;

/// Provides bearer token for API authentication.
pub trait AuthProvider: Send + Sync {
    fn bearer_token(&self) -> Option<String>;
}

/// Simple bearer token authentication.
#[derive(Debug, Clone)]
pub struct BearerAuth {
    token: String,
}

impl BearerAuth {
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
        }
    }
}

impl AuthProvider for BearerAuth {
    fn bearer_token(&self) -> Option<String> {
        Some(self.token.clone())
    }
}

pub(crate) fn add_auth_headers(auth: &dyn AuthProvider, mut req: Request) -> Request {
    if let Some(token) = auth.bearer_token()
        && let Ok(header) = HeaderValue::from_str(&format!("Bearer {token}"))
    {
        req.headers.insert(http::header::AUTHORIZATION, header);
    }
    req
}
