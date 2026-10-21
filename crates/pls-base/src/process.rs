use std::{path::PathBuf, process::Command as StdCommand};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::Result;

/// Sortie normalisée d'un processus.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessOutput {
    pub status: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl ProcessOutput {
    pub fn success(&self) -> bool {
        self.status == Some(0)
    }
}

/// Description d'un processus à exécuter.
#[derive(Debug, Clone)]
pub struct ProcessSpec {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub env: Vec<(String, String)>,
}

impl ProcessSpec {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            cwd: None,
            env: Vec::new(),
        }
    }

    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    pub fn cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }
}

/// Abstraction d'exécution de processus, remplaçable dans les tests.
#[async_trait]
pub trait ProcessRunner: Send + Sync {
    async fn run(&self, spec: &ProcessSpec) -> Result<ProcessOutput>;
}

/// Implémentation système minimale.
///
/// Elle est volontairement bloquante en Phase 0 ; l'exécution tokio et
/// l'annulation arrivent avec la Phase 1.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemProcessRunner;

#[async_trait]
impl ProcessRunner for SystemProcessRunner {
    async fn run(&self, spec: &ProcessSpec) -> Result<ProcessOutput> {
        let mut command = StdCommand::new(&spec.program);
        command.args(&spec.args);

        if let Some(cwd) = &spec.cwd {
            command.current_dir(cwd);
        }

        for (key, value) in &spec.env {
            command.env(key, value);
        }

        let output = command.output()?;

        Ok(ProcessOutput {
            status: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}
