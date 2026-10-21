use serde::{Deserialize, Serialize};

/// Niveau de confiance accordé au workspace courant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustLevel {
    /// Dépôt inconnu : pas de build, pas de Gradle/npm, pas d'agent automatique.
    Restricted,
    /// Dépôt approuvé par l'utilisateur.
    Trusted,
}

impl TrustLevel {
    pub const fn is_trusted(self) -> bool {
        matches!(self, Self::Trusted)
    }
}
