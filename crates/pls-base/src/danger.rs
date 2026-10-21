use serde::{Deserialize, Serialize};

/// Niveau de danger d'une commande.
///
/// L'UI et MCP s'appuient sur ce niveau pour demander une confirmation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DangerLevel {
    Read,
    Write,
    Destructive,
}

impl DangerLevel {
    pub const fn requires_confirmation(self) -> bool {
        matches!(self, Self::Write | Self::Destructive)
    }
}
