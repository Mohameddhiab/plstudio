//! Moteur central : registre de commandes, jobs et orchestration.

#![forbid(unsafe_code)]

use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicU64, Ordering},
};

use pls_base::{
    Command, CommandCtx, CommandId, CommandMeta, CommandOutput, ErrorCode, JobId, PlsError, Result,
};
use serde_json::Value;

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
#[derive(Default)]
pub struct Engine {
    commands: BTreeMap<CommandId, Box<dyn Command>>,
    jobs: JobRegistry,
}

impl Engine {
    pub fn new() -> Self {
        Self::default()
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
        self.jobs.next_job()
    }

    pub async fn execute(
        &self,
        id: &CommandId,
        ctx: &CommandCtx,
        params: Value,
    ) -> Result<CommandOutput> {
        let command = self
            .commands
            .get(id)
            .ok_or_else(|| PlsError::app(ErrorCode::COMMAND, format!("unknown command `{id}`")))?;

        command.run(ctx, params).await
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use pls_base::DangerLevel;

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

    #[test]
    fn registers_and_lists_commands() {
        let mut engine = Engine::new();
        engine.register(EchoCommand {
            meta: CommandMeta::new("test.echo", "Echo", "test", DangerLevel::Read),
        });

        assert_eq!(engine.command_count(), 1);
        assert_eq!(engine.commands()[0].id.as_str(), "test.echo");
    }

    #[test]
    fn generates_monotonic_jobs() {
        let engine = Engine::new();
        assert_eq!(engine.next_job().get(), 1);
        assert_eq!(engine.next_job().get(), 2);
    }
}
