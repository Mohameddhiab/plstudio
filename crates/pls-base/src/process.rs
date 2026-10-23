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

    /// Lance un processus indépendant du cycle de vie de plstudio
    /// (émulateur, scrcpy). Le pid seul est retourné : le handle est
    /// volontairement abandonné, tuer l'UI ne doit pas tuer l'émulateur.
    async fn spawn_detached(&self, spec: &ProcessSpec) -> Result<DetachedProcess>;
}

/// Processus détaché, identifié par son pid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetachedProcess {
    pub pid: u32,
}

/// Implémentation système minimale.
///
/// `run` est bloquant (processus courts) ; les flux longs (flutter run)
/// passeront par le protocole de contrôle.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemProcessRunner;

#[async_trait]
impl ProcessRunner for SystemProcessRunner {
    async fn run(&self, spec: &ProcessSpec) -> Result<ProcessOutput> {
        let output = self.system_command(spec)?.output()?;

        Ok(ProcessOutput {
            status: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }

    async fn spawn_detached(&self, spec: &ProcessSpec) -> Result<DetachedProcess> {
        let child = self.system_command(spec)?.spawn()?;

        Ok(DetachedProcess { pid: child.id() })
    }
}

impl SystemProcessRunner {
    fn system_command(&self, spec: &ProcessSpec) -> Result<StdCommand> {
        let mut command = StdCommand::new(&spec.program);
        command.args(&spec.args);

        if let Some(cwd) = &spec.cwd {
            command.current_dir(cwd);
        }

        for (key, value) in &spec.env {
            command.env(key, value);
        }

        Ok(command)
    }
}
