use bytes::Bytes;
use http::Method;
use reqwest::header::HeaderMap;
use serde_json::Value;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct Request {
    pub method: Method,
    pub url: String,
    pub headers: HeaderMap,
    pub body: Option<Value>,
    pub timeout: Option<Duration>,
}

impl Request {
    pub fn new(method: Method, url: String) -> Self {
        Self {
            method,
            url,
            headers: HeaderMap::new(),
            body: None,
            timeout: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Response {
    pub status: http::StatusCode,
    pub headers: HeaderMap,
    pub body: Bytes,
}
