//! Serveur MCP.
//!
//! Le transport `rmcp` arrive en Phase 4. Ce crate expose déjà le modèle
//! d'outils et les niveaux de danger.

#![forbid(unsafe_code)]

use pls_base::{CommandMeta, DangerLevel};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    pub danger: DangerLevel,
}

impl McpTool {
    pub fn from_command(meta: &CommandMeta) -> Self {
        Self {
            name: meta.id.as_str().to_owned(),
            description: meta.description.clone(),
            danger: meta.danger,
        }
    }
}

/// Outils toujours disponibles, même en mode restreint.
pub fn read_only_tools() -> Vec<McpTool> {
    vec![
        McpTool {
            name: "plstudio.commands".to_owned(),
            description: "Liste les commandes et leurs niveaux de danger.".to_owned(),
            danger: DangerLevel::Read,
        },
        McpTool {
            name: "plstudio.jobs".to_owned(),
            description: "Liste les jobs et leur progression.".to_owned(),
            danger: DangerLevel::Read,
        },
        McpTool {
            name: "plstudio.inspect_project".to_owned(),
            description: "Détecte Flutter, le SDK, les AVD et les appareils.".to_owned(),
            danger: DangerLevel::Read,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_only_tools_are_read_only() {
        let tools = read_only_tools();
        assert!(!tools.is_empty());
        assert!(tools.iter().all(|tool| tool.danger == DangerLevel::Read));
    }
}
