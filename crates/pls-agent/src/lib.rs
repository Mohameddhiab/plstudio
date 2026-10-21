//! Agent IA : contexte, outils, diffs et budgets.
//!
//! `pls-agent` définit `ToolHost` ; `pls-engine` l'implémente.

#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use pls_base::Result;
use serde::{Deserialize, Serialize};

/// Outils que l'agent peut demander au moteur.
#[async_trait]
pub trait ToolHost: Send + Sync {
    async fn read_file(&self, path: &Path) -> Result<String>;
    async fn write_file(&self, path: &Path, contents: &str) -> Result<()>;
    async fn build(&self) -> Result<String>;
    async fn logcat(&self, max_lines: usize) -> Result<String>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentRequest {
    pub prompt: String,
    pub context_files: Vec<PathBuf>,
    pub include_build_errors: bool,
}

impl AgentRequest {
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            context_files: Vec::new(),
            include_build_errors: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentDiff {
    pub path: PathBuf,
    pub before: String,
    pub after: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentResponse {
    pub message: String,
    pub diffs: Vec<AgentDiff>,
}

/// Fournisseur de modèle (Claude en v0.1).
#[async_trait]
pub trait AgentProvider: Send + Sync {
    async fn complete(&self, request: &AgentRequest) -> Result<AgentResponse>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_request_includes_build_errors() {
        let request = AgentRequest::new("fix this");
        assert!(request.include_build_errors);
        assert!(request.context_files.is_empty());
    }
}
