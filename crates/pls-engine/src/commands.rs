//! Commandes système du moteur : observation et contrôle de lui-même.
//!
//! Ces commandes sont enregistrées automatiquement par `Engine::new` ; elles
//! opèrent sur la table des jobs partagée avec le moteur.

use std::sync::Arc;

use async_trait::async_trait;
use pls_base::parse_params;
use pls_base::{Command, CommandCtx, CommandMeta, CommandOutput, DangerLevel, JobId, Result};
use serde::Deserialize;
use serde_json::Value;

use crate::jobs::JobTable;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct JobsParams {}

/// `engine.jobs` — liste les jobs du moteur avec leur état.
#[derive(Debug)]
pub struct JobsCommand {
    jobs: Arc<JobTable>,
}

impl JobsCommand {
    pub fn new(jobs: Arc<JobTable>) -> Self {
        Self { jobs }
    }
}

#[async_trait]
impl Command for JobsCommand {
    fn meta(&self) -> CommandMeta {
        CommandMeta::new(
            "engine.jobs",
            "List engine jobs",
            "engine",
            DangerLevel::Read,
        )
        .with_description(
            "Liste les jobs exécutés par le moteur depuis son démarrage : identifiant, \
                 commande, état, horodatages et erreur éventuelle.",
        )
    }

    async fn run(&self, _ctx: &CommandCtx, params: Value) -> Result<CommandOutput> {
        let _params: JobsParams = parse_params(&params)?;

        Ok(CommandOutput::json(serde_json::json!({
            "jobs": self.jobs.snapshot(),
        })))
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CancelParams {
    job_id: JobId,
}

/// `engine.cancel` — demande l'annulation coopérative d'un job en cours.
#[derive(Debug)]
pub struct CancelCommand {
    jobs: Arc<JobTable>,
}

impl CancelCommand {
    pub fn new(jobs: Arc<JobTable>) -> Self {
        Self { jobs }
    }
}

#[async_trait]
impl Command for CancelCommand {
    fn meta(&self) -> CommandMeta {
        CommandMeta::new(
            "engine.cancel",
            "Cancel a running job",
            "engine",
            DangerLevel::Write,
        )
        .with_description(
            "Pose le drapeau d'annulation d'un job en cours. L'annulation est \
                 coopérative : la commande s'arrête à sa prochaine vérification.",
        )
    }

    async fn run(&self, _ctx: &CommandCtx, params: Value) -> Result<CommandOutput> {
        let params: CancelParams = parse_params(&params)?;
        self.jobs.cancel(params.job_id)?;

        Ok(CommandOutput::json(
            serde_json::json!({ "cancelled": true, "job_id": params.job_id.get() }),
        ))
    }
}
