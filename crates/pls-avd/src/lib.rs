//! Modèle des AVD Android.
//!
//! L'appel réel à `avdmanager` arrive en Phase 1.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AvdStatus {
    Stopped,
    Starting,
    Running,
    Stopping,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AvdSummary {
    pub name: String,
    pub device: String,
    pub api: u32,
    pub target: String,
    pub status: AvdStatus,
}

impl AvdSummary {
    pub fn is_running(&self) -> bool {
        self.status == AvdStatus::Running
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvdCreateRequest {
    pub name: String,
    pub device: String,
    pub api: u32,
    pub image: String,
    pub architecture: String,
}
