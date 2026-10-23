//! Moteur central : registre de commandes, jobs et orchestration.
//!
//! Toute action de plstudio passe par [`Engine::execute`] : la CLI, l'UI,
//! l'agent et MCP partagent le même registre. Les paramètres sont validés
//! (objet JSON) avant exécution, chaque invocation est enregistrée dans la
//! table des jobs, et l'annulation reste coopérative via `CommandCtx`.

#![forbid(unsafe_code)]

use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use pls_base::{
    Command, CommandCtx, CommandId, CommandMeta, CommandOutput, ErrorCode, JobId, PlsError, Result,
};
use serde_json::Value;

pub mod commands;
pub mod editor;
pub mod jobs;
pub mod toolchain;

pub use editor::{BufferSummary, EditorStore, MatchSpan};
pub use jobs::{JobError, JobRecord, JobState, JobTable};

/// Générateur monotone de jobs.
#[derive(Debug, Default)]
pub struct JobRegistry {
    next: AtomicU64,
}

impl JobRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn next_job(&self) -> JobId {
        let raw = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        JobId::new(raw)
    }
}

/// Registre unique des commandes de plstudio.
///
/// `Default` ne connaît aucune commande ; `Engine::new` est la porte d'entrée
/// et enregistre les commandes système `engine.*`.
#[derive(Default)]
pub struct Engine {
    /// Le registre des commandes est immuable après le bootstrap : les
    /// mutations passent par `register`, réservé à la construction.
    commands: BTreeMap<CommandId, Box<dyn Command>>,
    job_ids: JobRegistry,
    pub(crate) job_table: Arc<JobTable>,
}

impl Engine {
    /// Moteur prêt à l'emploi avec les commandes système `engine.*`,
    /// `editor.*` et les commandes plateforme `sdk.*`/`avd.*`.
    pub fn new() -> Self {
        let mut engine = Self::default();
        commands::register(&mut engine);
        editor::commands::register(&mut engine);
        toolchain::register(&mut engine);
        engine
    }
    pub fn register<C: Command + 'static>(&mut self, command: C) {
        let id = command.meta().id;
        self.commands.insert(id, Box::new(command));
    }

    pub fn commands(&self) -> Vec<CommandMeta> {
        self.commands
            .values()
            .map(|command| command.meta())
            .collect()
    }

    pub fn command_count(&self) -> usize {
        self.commands.len()
    }

    pub fn next_job(&self) -> JobId {
        self.job_ids.next_job()
    }

    /// Exécute une commande enregistrée, avec validation et suivi de job.
    ///
    /// Le `ctx.job_id` doit venir de [`Self::next_job`] : les appelants qui
    /// veulent suivre un job lui gardent une référence pour `engine.cancel`.
    pub async fn execute(
        &self,
        id: &CommandId,
        ctx: &CommandCtx,
        params: Value,
    ) -> Result<CommandOutput> {
        if !params.is_object() {
            return Err(PlsError::app(
                ErrorCode::INVALID_PARAMS,
                "command params must be a JSON object",
            )
            .with_remediation("pass an empty object `{}` or the documented param shape"));
        }

        let command = self
            .commands
            .get(id)
            .ok_or_else(|| PlsError::app(ErrorCode::COMMAND, format!("unknown command `{id}`")))?;

        let meta = command.meta();
        self.job_table.start(
            ctx.job_id,
            meta.id.clone(),
            meta.title,
            meta.danger,
            ctx.cancellation.clone(),
        );

        match command.run(ctx, params).await {
            Ok(output) => {
                self.job_table.finish_success(ctx.job_id);
                Ok(output)
            }
            Err(error) => {
                self.job_table.finish_error(ctx.job_id, &error);
                Err(error)
            }
        }
    }

    /// État courant de tous les jobs, par identifiant croissant.
    pub fn jobs_snapshot(&self) -> Vec<JobRecord> {
        self.job_table.snapshot()
    }

    /// Demande l'annulation coopérative d'un job en cours.
    pub fn cancel_job(&self, id: JobId) -> Result<()> {
        self.job_table.cancel(id)
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use pls_base::{DangerLevel, TrustLevel};
    use serde_json::json;
    use std::sync::Arc;
    use std::time::Duration;

    use super::*;

    struct EchoCommand {
        meta: CommandMeta,
    }

    #[async_trait]
    impl Command for EchoCommand {
        fn meta(&self) -> CommandMeta {
            self.meta.clone()
        }

        async fn run(&self, _ctx: &CommandCtx, params: Value) -> Result<CommandOutput> {
            Ok(CommandOutput::json(params))
        }
    }

    struct UnstoppableCommand {
        meta: CommandMeta,
    }

    struct FailingCommand {
        meta: CommandMeta,
    }

    #[async_trait]
    impl Command for FailingCommand {
        fn meta(&self) -> CommandMeta {
            self.meta.clone()
        }

        async fn run(&self, _ctx: &CommandCtx, _params: Value) -> Result<CommandOutput> {
            Err(PlsError::app(ErrorCode::COMMAND, "boom").with_remediation("nothing to do"))
        }
    }

    #[async_trait]
    impl Command for UnstoppableCommand {
        fn meta(&self) -> CommandMeta {
            self.meta.clone()
        }

        async fn run(&self, ctx: &CommandCtx, _params: Value) -> Result<CommandOutput> {
            // Coopération minimale : on s'arrête dès que le drapeau est posé.
            while !ctx.cancellation.is_cancelled() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }

            Err(PlsError::app(
                ErrorCode::CANCELLED,
                "cancelled while waiting",
            ))
        }
    }

    fn spawn_engine() -> Arc<Engine> {
        Arc::new(Engine::new())
    }

    #[test]
    fn registers_and_lists_commands() {
        let mut engine = Engine::new();
        engine.register(EchoCommand {
            meta: CommandMeta::new("test.echo", "Echo", "test", DangerLevel::Read),
        });

        assert_eq!(engine.command_count(), 20);
        let metas = engine.commands();
        let ids: Vec<&str> = metas.iter().map(|meta| meta.id.as_str()).collect();
        assert!(ids.contains(&"test.echo"));
        assert!(ids.contains(&"engine.jobs"));
        assert!(ids.contains(&"engine.cancel"));
        assert!(ids.contains(&"editor.open"));
        assert!(ids.contains(&"editor.edit"));
        assert!(ids.contains(&"editor.undo"));
        assert!(ids.contains(&"editor.redo"));
        assert!(ids.contains(&"editor.save"));
        assert!(ids.contains(&"editor.find"));
        assert!(ids.contains(&"editor.list"));
        assert!(ids.contains(&"editor.close"));
        assert!(ids.contains(&"sdk.detect"));
        assert!(ids.contains(&"avd.list"));
        assert!(ids.contains(&"avd.create"));
        assert!(ids.contains(&"avd.start"));
        assert!(ids.contains(&"avd.devices"));
        assert!(ids.contains(&"avd.wait"));
        assert!(ids.contains(&"avd.logcat"));
        assert!(ids.contains(&"emulator.scrcpy"));
    }

    #[test]
    fn generates_monotonic_jobs() {
        let engine = Engine::new();
        assert_eq!(engine.next_job().get(), 1);
        assert_eq!(engine.next_job().get(), 2);
    }

    #[tokio::test]
    async fn tracks_success_and_failure() {
        let mut engine = Engine::new();
        engine.register(EchoCommand {
            meta: CommandMeta::new("test.echo", "Echo", "test", DangerLevel::Read),
        });
        engine.register(FailingCommand {
            meta: CommandMeta::new("test.fail", "Fail", "test", DangerLevel::Read),
        });

        let ctx = CommandCtx::new(engine.next_job(), ".", TrustLevel::Trusted);
        engine
            .execute(&CommandId::from("test.echo"), &ctx, json!({ "x": 1 }))
            .await
            .expect("echo");

        let failing_ctx = CommandCtx::new(engine.next_job(), ".", TrustLevel::Trusted);
        engine
            .execute(&CommandId::from("test.fail"), &failing_ctx, json!({}))
            .await
            .unwrap_err();

        // Une commande inconnue ne crée pas de job.
        let unknown_ctx = CommandCtx::new(engine.next_job(), ".", TrustLevel::Trusted);
        let error = engine
            .execute(&CommandId::from("test.missing"), &unknown_ctx, json!({}))
            .await
            .unwrap_err();
        assert_eq!(error.code(), ErrorCode::COMMAND);

        let snapshot = engine.jobs_snapshot();
        assert_eq!(snapshot.len(), 2);
        assert_eq!(snapshot[0].state, JobState::Succeeded);
        assert_eq!(snapshot[0].command.as_str(), "test.echo");
        assert_eq!(snapshot[1].state, JobState::Failed);
        let record_error = snapshot[1].error.as_ref().expect("error present");
        assert_eq!(record_error.code, "PLS-E-1003");
        assert_eq!(record_error.remediation.as_deref(), Some("nothing to do"));
    }

    #[tokio::test]
    async fn rejects_non_object_params() {
        let engine = spawn_engine();

        let ctx = CommandCtx::new(engine.next_job(), ".", TrustLevel::Trusted);
        let error = engine
            .execute(&CommandId::from("engine.jobs"), &ctx, json!("boom"))
            .await
            .unwrap_err();
        assert_eq!(error.code(), ErrorCode::INVALID_PARAMS);
    }

    #[tokio::test]
    async fn engine_jobs_lists_itself() {
        let engine = spawn_engine();

        let ctx = CommandCtx::new(engine.next_job(), ".", TrustLevel::Trusted);
        let output = engine
            .execute(&CommandId::from("engine.jobs"), &ctx, json!({}))
            .await
            .expect("engine.jobs");

        let jobs = output
            .value
            .get("jobs")
            .and_then(|value| value.as_array())
            .expect("jobs array");
        assert!(
            jobs.iter()
                .any(|job| job.get("command").and_then(|c| c.as_str()) == Some("engine.jobs"))
        );
    }

    #[tokio::test]
    async fn cancels_a_running_job_cooperatively() {
        let mut engine = Engine::new();
        engine.register(UnstoppableCommand {
            meta: CommandMeta::new("test.unstoppable", "Unstoppable", "test", DangerLevel::Read),
        });
        let engine = Arc::new(engine);

        let ctx = CommandCtx::new(engine.next_job(), ".", TrustLevel::Trusted);
        let job_id = ctx.job_id;
        let runner = engine.clone();
        let handle = tokio::spawn(async move {
            runner
                .execute(&CommandId::from("test.unstoppable"), &ctx, json!({}))
                .await
        });

        // Laisse le job démarrer, puis annule.
        tokio::time::sleep(Duration::from_millis(50)).await;
        engine.cancel_job(job_id).expect("cancel");

        let result = handle.await.expect("task not panicking");
        assert_eq!(result.expect_err("cancelled").code(), ErrorCode::CANCELLED);

        let record = engine
            .jobs_snapshot()
            .into_iter()
            .find(|record| record.id == job_id)
            .expect("recorded");
        assert_eq!(record.state, JobState::Cancelled);
        assert!(record.is_terminal());
    }

    #[tokio::test]
    async fn cancel_command_refuses_a_finished_job() {
        let engine = spawn_engine();

        let ctx = CommandCtx::new(engine.next_job(), ".", TrustLevel::Trusted);
        engine
            .execute(&CommandId::from("engine.jobs"), &ctx, json!({}))
            .await
            .expect("done");

        let error = engine.cancel_job(ctx.job_id).unwrap_err();
        assert_eq!(error.code(), ErrorCode::COMMAND);
    }
}
