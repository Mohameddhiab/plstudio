//! Cycle de vie de `flutter run` et de logcat.
//!
//! Le parsing NDJSON et adb arrivent en Phase 1.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    Stopped,
    Starting,
    Running,
    Reloading,
    Stopping,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunEvent {
    Log { line: String },
    ReloadStarted,
    ReloadFinished { duration_ms: u64 },
    RestartStarted,
    RestartFinished,
    Exited { code: Option<i32> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunConfig {
    pub device_id: Option<String>,
    pub debug: bool,
    pub track_widget_creation: bool,
}
