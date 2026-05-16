use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

static THREAD_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ThreadId(String);

impl ThreadId {
    pub fn new() -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let counter = THREAD_COUNTER.fetch_add(1, Ordering::Relaxed);
        Self(format!(
            "{:08x}-{:04x}-7{:03x}-{:04x}-{:012x}",
            (now.as_secs() & 0xffff_ffff) as u32,
            ((now.subsec_nanos() >> 16) & 0xffff) as u16,
            (counter & 0x0fff) as u16,
            ((now.subsec_nanos() as u64 ^ counter) & 0xffff) as u16,
            ((now.as_nanos() as u64) ^ counter) & 0xffff_ffff_ffff
        ))
    }

    pub fn from_string(value: &str) -> Result<Self, crate::ProtocolError> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(crate::ProtocolError::InvalidThreadId(value.to_string()));
        }
        Ok(Self(trimmed.to_string()))
    }
}

impl Default for ThreadId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ThreadId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<ThreadId> for String {
    fn from(value: ThreadId) -> Self {
        value.0
    }
}

impl TryFrom<&str> for ThreadId {
    type Error = crate::ProtocolError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::from_string(value)
    }
}

impl Serialize for ThreadId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ThreadId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::from_string(&value).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::ThreadId;

    #[test]
    fn generated_ids_are_not_empty_and_differ() {
        let first = ThreadId::new();
        let second = ThreadId::new();

        assert!(!first.to_string().is_empty());
        assert_ne!(first, second);
    }
}
