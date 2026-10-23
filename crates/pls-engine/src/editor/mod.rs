//! Édition headless : état des buffers ouverts et commandes `editor.*`.

pub mod commands;
pub mod store;

pub use store::{BufferSummary, EditorStore, MatchSpan};
