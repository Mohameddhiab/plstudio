//! Binaire desktop plstudio.
//!
//! L'UI réelle est choisie par l'ADR 0001. En Phase 0, le binaire vérifie
//! que le moteur, la couche UI et le logging démarrent correctement.

#![forbid(unsafe_code)]

use std::process::ExitCode;

use pls_engine::Engine;
use pls_ui::UiState;

fn main() -> ExitCode {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let engine = Engine::new();
    let state = UiState::new();

    println!("plstudio {}", env!("CARGO_PKG_VERSION"));
    println!("{}", state.status);
    println!("commands registered: {}", state.command_count(&engine));

    ExitCode::SUCCESS
}
