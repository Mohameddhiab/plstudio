//! Coquille UI de plstudio.
//!
//! Le choix egui / GPUI / webview est tranché par l'ADR 0001 à la fin de S3.
//! Ce crate isole l'UI du moteur.

#![forbid(unsafe_code)]

use pls_base::CommandId;
use pls_engine::Engine;

#[derive(Debug, Clone, Default)]
pub struct UiState {
    pub selected_command: Option<CommandId>,
    pub status: String,
}

impl UiState {
    pub fn new() -> Self {
        Self {
            selected_command: None,
            status: boot_message().to_owned(),
        }
    }

    pub fn command_count(&self, engine: &Engine) -> usize {
        engine.command_count()
    }
}

pub const fn boot_message() -> &'static str {
    "plstudio pre-alpha — UI decision pending ADR 0001"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boots_with_a_status() {
        let state = UiState::new();
        assert!(state.status.contains("pre-alpha"));
        assert!(state.selected_command.is_none());
    }
}
