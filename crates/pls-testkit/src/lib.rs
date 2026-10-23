//! Fakes, fixtures et harnais de test partagés.
//!
//! Ce crate est une aide de test ; il est exclu du contrôle de couches.

#![forbid(unsafe_code)]

use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
};

use async_trait::async_trait;
use pls_base::{
    DetachedProcess, ProcessOutput, ProcessRunner, ProcessSession, ProcessSpec, Result,
};

/// Session scriptée : lignes à drainer, demandes enregistrées, fin commandée.
#[derive(Debug)]
pub struct ScriptedSession {
    pid: u32,
    stdout: Mutex<VecDeque<String>>,
    stderr: Mutex<VecDeque<String>>,
    written: Mutex<Vec<String>>,
    status: Mutex<Option<i32>>,
    killed: AtomicBool,
}

impl ScriptedSession {
    pub fn new(stdout_lines: Vec<String>, stderr_lines: Vec<String>) -> Self {
        Self {
            pid: 4242,
            stdout: Mutex::new(stdout_lines.into()),
            stderr: Mutex::new(stderr_lines.into()),
            written: Mutex::new(Vec::new()),
            status: Mutex::new(None),
            killed: AtomicBool::new(false),
        }
    }

    pub fn stdout_lines(lines: Vec<&str>) -> Self {
        Self::new(lines.into_iter().map(String::from).collect(), Vec::new())
    }

    /// Lignes écrites sur stdin, dans l'ordre.
    pub fn written(&self) -> Vec<String> {
        self.lock(&self.written).clone()
    }

    /// Force le statut de sortie (processus "terminé").
    pub fn set_exit(&self, status: i32) {
        *self.lock(&self.status) = Some(status);
    }

    pub fn is_killed(&self) -> bool {
        self.killed.load(Ordering::Relaxed)
    }

    fn lock<'a, T>(&self, mutex: &'a Mutex<T>) -> MutexGuard<'a, T> {
        mutex.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn drain(&self, queue: &Mutex<VecDeque<String>>) -> Vec<String> {
        self.lock(queue).drain(..).collect()
    }
}

impl ProcessSession for ScriptedSession {
    fn pid(&self) -> u32 {
        self.pid
    }

    fn take_lines(&self) -> Vec<String> {
        self.drain(&self.stdout)
    }

    fn take_stderr_lines(&self) -> Vec<String> {
        self.drain(&self.stderr)
    }

    fn write_request(&self, line: &str) -> Result<()> {
        self.lock(&self.written).push(line.to_string());
        Ok(())
    }

    fn exit_status(&self) -> Option<i32> {
        let status = *self.lock(&self.status);
        // Un kill épinglé vaut code 1, comme la session système.
        if status.is_none() && self.killed.load(Ordering::Relaxed) {
            Some(1)
        } else {
            status
        }
    }

    fn kill(&self) {
        self.killed.store(true, Ordering::Relaxed);
    }
}

/// Runner de processus déterministe.
#[derive(Debug, Clone)]
pub struct FakeProcessRunner {
    calls: Arc<Mutex<Vec<ProcessSpec>>>,
    output: ProcessOutput,
}

impl FakeProcessRunner {
    pub fn new(output: ProcessOutput) -> Self {
        Self {
            calls: Arc::new(Mutex::new(Vec::new())),
            output,
        }
    }

    pub fn succeeding(stdout: impl Into<String>) -> Self {
        Self::new(ProcessOutput {
            status: Some(0),
            stdout: stdout.into(),
            stderr: String::new(),
        })
    }

    pub fn calls(&self) -> Vec<ProcessSpec> {
        match self.calls.lock() {
            Ok(calls) => calls.clone(),
            Err(_) => Vec::new(),
        }
    }
}

impl Default for FakeProcessRunner {
    fn default() -> Self {
        Self::succeeding("")
    }
}

#[async_trait]
impl ProcessRunner for FakeProcessRunner {
    async fn run(&self, spec: &ProcessSpec) -> Result<ProcessOutput> {
        if let Ok(mut calls) = self.calls.lock() {
            calls.push(spec.clone());
        }

        Ok(self.output.clone())
    }

    async fn spawn_detached(&self, spec: &ProcessSpec) -> Result<DetachedProcess> {
        if let Ok(mut calls) = self.calls.lock() {
            calls.push(spec.clone());
        }

        Ok(DetachedProcess { pid: 4242 })
    }

    async fn spawn_session(&self, spec: &ProcessSpec) -> Result<Arc<dyn ProcessSession>> {
        if let Ok(mut calls) = self.calls.lock() {
            calls.push(spec.clone());
        }

        Ok(Arc::new(ScriptedSession::new(Vec::new(), Vec::new())))
    }
}

/// Runner scripté : sorties de `run` en ordre (puis répétition de la
/// dernière), sessions d'un script de lignes par `spawn_session`.
#[derive(Debug, Clone)]
pub struct ScriptedProcessRunner {
    calls: Arc<Mutex<Vec<ProcessSpec>>>,
    outputs: Arc<Mutex<Vec<ProcessOutput>>>,
    scripts: Arc<Mutex<VecDeque<Vec<String>>>>,
    spawned: Arc<Mutex<Vec<Arc<ScriptedSession>>>>,
}

impl ScriptedProcessRunner {
    pub fn new(outputs: Vec<ProcessOutput>) -> Self {
        Self {
            calls: Arc::new(Mutex::new(Vec::new())),
            outputs: Arc::new(Mutex::new(outputs)),
            scripts: Arc::new(Mutex::new(VecDeque::new())),
            spawned: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Sorties texte stdout, chacune avec un code 0.
    pub fn stdout_lines(lines: Vec<&str>) -> Self {
        let outputs = lines
            .into_iter()
            .map(|stdout| ProcessOutput {
                status: Some(0),
                stdout: stdout.to_string(),
                stderr: String::new(),
            })
            .collect();
        Self::new(outputs)
    }

    /// Session scriptée pour chaque `spawn_session` dans l'ordre.
    pub fn with_session_scripts(self, scripts: Vec<Vec<String>>) -> Self {
        if let Ok(mut queued) = self.scripts.lock() {
            *queued = scripts.into_iter().collect();
        }
        self
    }

    /// Variante texte : scripts de session par lignes littérales.
    pub fn with_session_text(scripts: Vec<Vec<&str>>) -> Self {
        let owned = scripts
            .into_iter()
            .map(|lines| lines.into_iter().map(String::from).collect())
            .collect();
        Self::default().with_session_scripts(owned)
    }

    pub fn calls(&self) -> Vec<ProcessSpec> {
        match self.calls.lock() {
            Ok(calls) => calls.clone(),
            Err(_) => Vec::new(),
        }
    }

    /// Session du dernier `spawn_session`, pour les assertions.
    pub fn last_session(&self) -> Option<Arc<ScriptedSession>> {
        self.lock(&self.spawned).last().cloned()
    }

    fn lock<'a, T>(&self, mutex: &'a Mutex<T>) -> MutexGuard<'a, T> {
        mutex.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Default for ScriptedProcessRunner {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

#[async_trait]
impl ProcessRunner for ScriptedProcessRunner {
    async fn run(&self, spec: &ProcessSpec) -> Result<ProcessOutput> {
        if let Ok(mut calls) = self.calls.lock() {
            calls.push(spec.clone());
        }

        let mut outputs = self.lock(&self.outputs);
        match outputs.len() {
            0 => Ok(ProcessOutput {
                status: None,
                stdout: String::new(),
                stderr: String::new(),
            }),
            _ => {
                let output = if outputs.len() == 1 {
                    outputs[0].clone()
                } else {
                    outputs.remove(0)
                };
                Ok(output)
            }
        }
    }

    async fn spawn_detached(&self, spec: &ProcessSpec) -> Result<DetachedProcess> {
        if let Ok(mut calls) = self.calls.lock() {
            calls.push(spec.clone());
        }

        Ok(DetachedProcess { pid: 4242 })
    }

    async fn spawn_session(&self, spec: &ProcessSpec) -> Result<Arc<dyn ProcessSession>> {
        if let Ok(mut calls) = self.calls.lock() {
            calls.push(spec.clone());
        }

        let lines = self.lock(&self.scripts).pop_front().unwrap_or_default();
        let session = Arc::new(ScriptedSession::new(lines, Vec::new()));
        self.lock(&self.spawned).push(session.clone());

        Ok(session)
    }
}

pub const FLUTTER_RUN_MACHINE: &str = r#"
{"event":"daemon.initialized","params":{"version":"0.0.0"}}
{"event":"daemon.logMessage","params":{"level":"info","message":"flutter run --machine sur pixel_7_api_34"}}
{"event":"app.progress","params":{"id":"1","message":"Launching lib/main.dart on emulator-5554","finished":false}}
{"event":"app.start","params":{"appId":"com.example.demo","deviceId":"emulator-5554","directory":"/repo/demo"}}
{"event":"app.progress","params":{"id":"1","message":"Running Gradle task 'assembleDebug'","finished":false}}
{"event":"app.debugPort","params":{"wsUri":"ws://127.0.0.1:51234/DSfY=","port":51234}}
{"event":"app.log","params":{"log":"A Dart VM Service on Pixel 7 API 34 is available at: http://127.0.0.1:45671/xyz=/"}}
{"event":"app.progress","params":{"id":"1","finished":true}}
Waiting for another flutter command to release the startup lock.
{"event":"daemon.logMessage","params":{"level":"warning","message":"memory pressure on the emulator"}}
{"event":"app.stop","params":{"appId":"com.example.demo"}}
{"event":"daemon.exit","params":{"code":0}}
"#;

pub const LOGCAT_THREADTIME: &str = "10-21 12:01:00.123  1234  5678 I flutter : Hello\n";

pub const AVDMANAGER_LIST_DEVICE: &str =
    "id: 1 or \"pixel_7\"\n    Name: Pixel 7\n    OEM : Google\n";

pub const AVDMANAGER_LIST_AVD: &str = "Available Android Virtual Devices:\n\
    Name: Pixel_7_API_34\n\
  Device: pixel_7\n\
    Path: C:\\Users\\test\\.android\\avd\\Pixel_7_API_34.avd\n\
  Target: android-34\n\
    Skin: pixel_7\n\
---------\n\
    Name: Small_Empty\n\
  Device: small_phone\n\
    Path: C:\\Users\\test\\.android\\avd\\Small_Empty.avd\n\
  Target: android-33\n\
---------\n";

pub const ADB_DEVICES: &str = "List of devices attached\n\
emulator-5554\tdevice\n\
emulator-5556\toffline\n\
\n";

#[cfg(test)]
mod tests {
    use super::*;
    use futures_lite::future::block_on;

    #[test]
    fn fake_records_calls_and_returns_output() {
        let runner = FakeProcessRunner::succeeding("ok");
        let spec = ProcessSpec::new("adb").arg("devices");

        let output = block_on(runner.run(&spec)).expect("run");
        assert!(output.success());
        assert_eq!(runner.calls().len(), 1);
    }

    #[test]
    fn scripted_sessions_drain_then_record_requests() {
        let runner = ScriptedProcessRunner::with_session_text(vec![vec![
            "{\"event\":\"daemon.initialized\"}",
            "{\"event\":\"app.start\",\"params\":{\"appId\":\"demo\"}}",
        ]]);

        let session = block_on(runner.spawn_session(&ProcessSpec::new("flutter"))).expect("spawn");
        assert_eq!(session.take_lines().len(), 2);
        assert!(session.take_lines().is_empty(), "drained once");

        session
            .write_request("{\"id\":1,\"method\":\"app.reload\"}")
            .expect("request");
        session.kill();

        let scripted = runner.last_session().expect("spawned");
        assert_eq!(
            scripted.written(),
            vec!["{\"id\":1,\"method\":\"app.reload\"}".to_string()]
        );
        assert!(scripted.is_killed());
        assert_eq!(scripted.exit_status(), Some(1));
        assert_eq!(session.pid(), 4242);
    }
}
