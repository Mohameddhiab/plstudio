//! Exécution des processus extérieurs, contrôlée et remplaçable dans les
//! tests.
//!
//! Trois modes : `run` (processus courts, bloquant), `spawn_detached`
//! (processus qui survivent à plstudio), `spawn_session` (processus longs
//! pilotés : lignes stdout drainables, demandes stdin, kill possible).

use std::{
    collections::VecDeque,
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{Child, ChildStdin, Command as StdCommand, Stdio},
    sync::{
        Arc, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::{ErrorCode, PlsError, Result};

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

    pub fn args(mut self, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        for arg in args {
            self.args.push(arg.into());
        }
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

/// Processus détaché, identifié par son pid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetachedProcess {
    pub pid: u32,
}

/// Abstraction d'exécution de processus, remplaçable dans les tests.
#[async_trait]
pub trait ProcessRunner: Send + Sync {
    async fn run(&self, spec: &ProcessSpec) -> Result<ProcessOutput>;

    /// Lance un processus indépendant du cycle de vie de plstudio
    /// (émulateur, scrcpy). Le pid seul est retourné : le handle est
    /// volontairement abandonné, tuer l'UI ne doit pas tuer l'émulateur.
    async fn spawn_detached(&self, spec: &ProcessSpec) -> Result<DetachedProcess>;

    /// Lance un processus long piloté (ex. `flutter run --machine`) :
    /// stdout drainable ligne à ligne, demandes via stdin, kill possible.
    /// La vie du processus appartient à la session, pas à l'appelant.
    async fn spawn_session(&self, spec: &ProcessSpec) -> Result<Arc<dyn ProcessSession>>;
}

/// Session d'un processus long, contrôlée par son propriétaire.
///
/// Modèle de poll : `take_lines` draine, `exit_status` sonne la fin,
/// `write_request` envoie une demande. Pas de flux tirable : le pompage
/// est fait par la couche au-dessus (pls-run), un seul acteur à la fois.
pub trait ProcessSession: Send + Sync + std::fmt::Debug {
    fn pid(&self) -> u32;

    /// Drains les lignes stdout accumulées depuis le dernier appel.
    fn take_lines(&self) -> Vec<String>;

    /// Drains les lignes stderr (diagnostics hors protocole daemon).
    fn take_stderr_lines(&self) -> Vec<String>;

    /// Écrit une demande sur stdin (une ligne NDJSON terminée par \n).
    fn write_request(&self, line: &str) -> Result<()>;

    /// Code de sortie une fois le processus terminé ; `None` tant qu'il vit.
    ///
    /// Invariant : dès que le statut est `Some`, tout le flux (stdout et
    /// stderr) a été poussé et est récupérable par les drains.
    fn exit_status(&self) -> Option<i32>;

    /// Coupe le processus ; le statut devient non nul après coup.
    fn kill(&self);

    fn is_alive(&self) -> bool {
        self.exit_status().is_none()
    }
}

/// Implémentation système minimale.
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

    async fn spawn_session(&self, spec: &ProcessSpec) -> Result<Arc<dyn ProcessSession>> {
        let mut command = self.system_command(spec)?;
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = command.spawn()?;
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let stdin = child.stdin.take();
        let pid = child.id();

        let session = Arc::new(ChildSession {
            pid,
            stdin: Mutex::new(stdin),
            child: Mutex::new(Some(child)),
            stdout: Mutex::new(VecDeque::new()),
            stderr: Mutex::new(VecDeque::new()),
            status: Mutex::new(None),
            killed: AtomicBool::new(false),
        });

        // Lecteurs d'abord : le finaliseur les joindra avant de poser le
        // statut, pour garantir « statut posé ⇒ tout le flux est drainé ».
        let mut readers = Vec::new();
        if let Some(stdout_pipe) = stdout {
            let session = session.clone();
            readers.push(thread::spawn(move || {
                read_pipe(stdout_pipe, session.as_ref(), Queue::Stdout)
            }));
        }
        if let Some(stderr_pipe) = stderr {
            let session = session.clone();
            readers.push(thread::spawn(move || {
                read_pipe(stderr_pipe, session.as_ref(), Queue::Stderr)
            }));
        }

        let finalizer_session = session.clone();
        thread::spawn(move || wait_for_exit(finalizer_session, readers));

        Ok(session)
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Queue {
    Stdout,
    Stderr,
}

/// Session d'un enfant système : possession du `Child`, threads lecteurs.
struct ChildSession {
    pid: u32,
    stdin: Mutex<Option<ChildStdin>>,
    child: Mutex<Option<Child>>,
    stdout: Mutex<VecDeque<String>>,
    stderr: Mutex<VecDeque<String>>,
    status: Mutex<Option<i32>>,
    /// Unix : SIGKILL n'expose pas de code de sortie ; on l'épingle ici.
    killed: AtomicBool,
}

impl std::fmt::Debug for ChildSession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ChildSession")
            .field("pid", &self.pid)
            .field("alive", &self.is_alive())
            .finish()
    }
}

impl ChildSession {
    fn lock<'a, T>(&self, mutex: &'a Mutex<T>) -> MutexGuard<'a, T> {
        mutex.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn queue(&self, queue: Queue) -> &Mutex<VecDeque<String>> {
        match queue {
            Queue::Stdout => &self.stdout,
            Queue::Stderr => &self.stderr,
        }
    }

    fn drain(&self, queue: Queue) -> Vec<String> {
        self.lock(self.queue(queue)).drain(..).collect()
    }

    fn set_status(&self, status: Option<i32>) {
        *self.lock(&self.status) = status;
    }
}

impl ProcessSession for ChildSession {
    fn pid(&self) -> u32 {
        self.pid
    }

    fn take_lines(&self) -> Vec<String> {
        self.drain(Queue::Stdout)
    }

    fn take_stderr_lines(&self) -> Vec<String> {
        self.drain(Queue::Stderr)
    }

    fn write_request(&self, line: &str) -> Result<()> {
        let mut slot = self.lock(&self.stdin);
        match slot.as_mut() {
            Some(stdin) => {
                writeln!(stdin, "{line}")?;
                stdin.flush()?;
                Ok(())
            }
            None => Err(PlsError::app(
                ErrorCode::IO,
                format!("stdin already closed for process {}", self.pid),
            )
            .with_remediation("the process exited; the request is ignored")),
        }
    }

    fn exit_status(&self) -> Option<i32> {
        let status = *self.lock(&self.status);
        // Un kill épinglé vaut code 1 quand le système n'expose pas de
        // code (SIGKILL sur Unix n'en produit pas).
        if status.is_none() && self.killed.load(Ordering::Relaxed) {
            Some(1)
        } else {
            status
        }
    }

    fn kill(&self) {
        self.killed.store(true, Ordering::Relaxed);
        if let Some(child) = self.lock(&self.child).as_mut() {
            let _ignored = child.kill();
        }
        // Le finaliseur capte alors le EOF et stocke le statut.
    }
}

fn read_pipe(pipe: impl std::io::Read + Send + 'static, session: &ChildSession, queue: Queue) {
    let mut reader = BufReader::new(pipe);

    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => break, // EOF : le processus a fermé sa sortie.
            Ok(_) => {
                let line = line.trim_end_matches(['\n', '\r']);
                session
                    .lock(session.queue(queue))
                    .push_back(line.to_string());
            }
            Err(_) => break, // Pipe rompu : le statut de sortie tranchera.
        }
    }
}

/// Finalise une session en trois temps :
///
/// 1. prend la possession du `Child` (plus de kill direct : le drapeau
///    `killed` est honoré par la boucle d'attente) ;
/// 2. joint les lecteurs, qui se terminent au EOF des pipes — toutes les
///    lignes sont donc déjà dans les tampons ;
/// 3. pose le statut.
///
/// Invariant offert aux consommateurs : dès que `exit_status()` répond
/// `Some`, `take_lines`/`take_stderr_lines` retournent le flux complet.
/// Avant ce correctif, le statut pouvait passer `Some` avant que les
/// lecteurs aient poussé les dernières lignes (course observée sur
/// Linux : l'écho perdu, la session « finie » avec un journal vide).
fn wait_for_exit(session: Arc<ChildSession>, readers: Vec<thread::JoinHandle<()>>) {
    let taken = session.lock(&session.child).take();
    let status = match taken {
        Some(mut child) => loop {
            if session.killed.load(Ordering::Relaxed) {
                let _ignored = child.kill();
            }

            match child.try_wait() {
                Ok(Some(status)) => break status.code(),
                Ok(None) => thread::sleep(Duration::from_millis(10)),
                Err(_) => break None,
            }
        },
        None => session.exit_status(), // déjà finalisée par un tiers.
    };

    for reader in readers {
        let _ignored = reader.join();
    }

    session.set_status(status);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn echo_spec() -> ProcessSpec {
        if cfg!(windows) {
            ProcessSpec::new("cmd").arg("/c").arg("echo session_hi")
        } else {
            ProcessSpec::new("sh").arg("-c").arg("echo session_hi")
        }
    }

    fn slow_spec() -> ProcessSpec {
        if cfg!(windows) {
            ProcessSpec::new("ping")
                .arg("-n")
                .arg("30")
                .arg("127.0.0.1")
        } else {
            ProcessSpec::new("sleep").arg("5")
        }
    }

    fn spin_until_exited(session: &dyn ProcessSession, max_polls: u32) -> Option<i32> {
        for _ in 0..max_polls {
            if let Some(status) = session.exit_status() {
                return Some(status);
            }
            thread::sleep(Duration::from_millis(20));
        }

        None
    }

    #[tokio::test]
    async fn session_collects_lines_then_exit_status() {
        let runner = SystemProcessRunner;
        let session = runner.spawn_session(&echo_spec()).await.expect("spawn");

        assert!(spin_until_exited(session.as_ref(), 200).is_some());
        assert!(
            session
                .take_lines()
                .iter()
                .any(|line| line.contains("session_hi"))
        );
        assert_eq!(session.exit_status(), Some(0));
    }

    #[tokio::test]
    async fn kill_ends_a_long_process() {
        let runner = SystemProcessRunner;
        let session = runner.spawn_session(&slow_spec()).await.expect("spawn");

        assert!(session.is_alive(), "the slow process just started");
        session.kill();

        assert!(
            spin_until_exited(session.as_ref(), 200).is_some(),
            "kill ends the process"
        );
        assert!(!session.is_alive());
    }

    #[tokio::test]
    async fn write_request_fails_once_stdin_is_closed() {
        let runner = SystemProcessRunner;
        let session = runner.spawn_session(&echo_spec()).await.expect("spawn");

        assert!(spin_until_exited(session.as_ref(), 200).is_some());
        let error = session
            .write_request("{\"id\":1}")
            .expect_err("stdin closed");
        assert_eq!(error.code(), ErrorCode::IO);
    }
}
