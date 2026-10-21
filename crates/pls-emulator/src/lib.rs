//! Intégration scrcpy et contrôle de l'émulateur.
//!
//! v0.1 : scrcpy dans une fenêtre séparée. Le flux intégré est v0.2.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScrcpyConfig {
    pub serial: Option<String>,
    pub max_fps: u32,
    pub window_title: Option<String>,
    pub no_audio: bool,
}

impl Default for ScrcpyConfig {
    fn default() -> Self {
        Self {
            serial: None,
            max_fps: 30,
            window_title: Some("plstudio — Android".to_owned()),
            no_audio: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmulatorStatus {
    Stopped,
    Starting,
    Running,
    Stopping,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmulatorSummary {
    pub avd_name: String,
    pub serial: Option<String>,
    pub status: EmulatorStatus,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_scrcpy_is_quiet() {
        let config = ScrcpyConfig::default();
        assert_eq!(config.max_fps, 30);
        assert!(config.no_audio);
    }
}
