use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Enveloppe commune des événements du bus.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEnvelope<T> {
    pub seq: u64,
    pub ts_ms: u64,
    pub topic: String,
    pub payload: T,
}

impl<T> EventEnvelope<T> {
    pub fn new(seq: u64, topic: impl Into<String>, payload: T) -> Self {
        Self {
            seq,
            ts_ms: now_ms(),
            topic: topic.into(),
            payload,
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}
