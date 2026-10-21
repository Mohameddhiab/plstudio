use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::{danger::DangerLevel, error::Result, ids::JobId, trust::TrustLevel};

/// Identifiant stable d'une commande (`avd.start`, `engine.jobs`, ...).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CommandId(String);

impl CommandId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for CommandId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl From<&str> for CommandId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

/// Métadonnées exposées par l'UI, la palette, la CLI et MCP.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandMeta {
    pub id: CommandId,
    pub title: String,
    pub description: String,
    pub category: String,
    pub danger: DangerLevel,
    pub requires_confirmation: bool,
    pub undoable: bool,
    pub requires_ui: bool,
}

impl CommandMeta {
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        category: impl Into<String>,
        danger: DangerLevel,
    ) -> Self {
        Self {
            id: CommandId::new(id),
            title: title.into(),
            description: String::new(),
            category: category.into(),
            danger,
            requires_confirmation: danger.requires_confirmation(),
            undoable: false,
            requires_ui: false,
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    pub fn undoable(mut self, undoable: bool) -> Self {
        self.undoable = undoable;
        self
    }

    pub fn requires_ui(mut self, requires_ui: bool) -> Self {
        self.requires_ui = requires_ui;
        self
    }
}

/// Drapeau d'annulation coopératif, indépendant du runtime async.
#[derive(Debug, Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);

impl Cancellation {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// Contexte fourni à chaque commande.
#[derive(Debug, Clone)]
pub struct CommandCtx {
    pub job_id: JobId,
    pub workspace: PathBuf,
    pub trust: TrustLevel,
    pub cancellation: Cancellation,
}

impl CommandCtx {
    pub fn new(job_id: JobId, workspace: impl Into<PathBuf>, trust: TrustLevel) -> Self {
        Self {
            job_id,
            workspace: workspace.into(),
            trust,
            cancellation: Cancellation::new(),
        }
    }
}

/// Résultat d'une commande.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CommandOutput {
    pub job_id: Option<JobId>,
    pub value: serde_json::Value,
}

impl CommandOutput {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn json(value: serde_json::Value) -> Self {
        Self {
            job_id: None,
            value,
        }
    }

    pub fn with_job(mut self, job_id: JobId) -> Self {
        self.job_id = Some(job_id);
        self
    }
}

/// Contrat commun à toutes les actions de plstudio.
#[async_trait]
pub trait Command: Send + Sync {
    fn meta(&self) -> CommandMeta;

    async fn run(&self, ctx: &CommandCtx, params: serde_json::Value) -> Result<CommandOutput>;
}
