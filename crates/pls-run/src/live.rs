//! Pilotage d'une session `flutter run --machine` vivante.
//!
//! Une [`LiveRun`] possède le processus contrôlé (`ProcessSession`),
//! l'observateur d'état et le journal des événements. Le pompage est
//! explicite (`pump`) : un seul acteur à la fois, comme le protocole de
//! contrôle l'exigera.

use std::{
    path::Path,
    sync::{
        Arc, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
};

use pls_base::{ProcessRunner, ProcessSession, ProcessSpec, Result};
use serde_json::json;

use crate::{RunConfig, RunEvent, RunObserver, parse_line};

/// Journal borné : au-delà, on perd les plus anciens évantements.
const LOG_LIMIT: usize = 10_000;

/// Une session de run vivante, côté plstudio.
#[derive(Debug)]
pub struct LiveRun {
    pub session: Arc<dyn ProcessSession>,
    observer: Mutex<RunObserver>,
    log: Mutex<Vec<RunEvent>>,
    next_request_id: AtomicU64,
}

impl LiveRun {
    /// Prépare la ligne de commande de la session (validée avant spawn).
    pub fn prepare_spec(
        config: &RunConfig,
        flutter: &Path,
        workspace: &Path,
    ) -> Result<ProcessSpec> {
        if let Some(device) = &config.device_id {
            pls_avd::validate_identifier(device, "device id")?;
        }

        let mut spec = ProcessSpec::new(flutter.to_string_lossy())
            .arg("run")
            .arg("--machine");
        if let Some(device) = &config.device_id {
            spec = spec.arg("-d").arg(device);
        }
        if !config.debug {
            spec = spec.arg("--release");
        }
        spec = spec.cwd(workspace);

        Ok(spec)
    }

    /// Lance `flutter run --machine` dans une session contrôlée.
    pub async fn start(
        runner: &dyn ProcessRunner,
        config: &RunConfig,
        flutter: &Path,
        workspace: &Path,
    ) -> Result<Self> {
        let spec = Self::prepare_spec(config, flutter, workspace)?;
        let session = runner.spawn_session(&spec).await?;

        Ok(Self {
            session,
            observer: Mutex::new(RunObserver::default()),
            log: Mutex::new(Vec::new()),
            next_request_id: AtomicU64::new(0),
        })
    }

    pub fn pid(&self) -> u32 {
        self.session.pid()
    }

    pub fn kill(&self) {
        self.session.kill();
    }

    pub fn is_finished(&self) -> bool {
        !self.session.is_alive()
    }

    /// Observateur : état interprété de la session.
    pub fn snapshot(&self) -> RunObserver {
        self.lock(&self.observer).clone()
    }

    /// Nombre d'événements du journal.
    pub fn event_count(&self) -> usize {
        self.lock(&self.log).len()
    }

    /// Événements à partir d'un offset, avec l'offset suivant.
    pub fn events_from(&self, from: usize) -> (Vec<RunEvent>, usize) {
        let log = self.lock(&self.log);
        let next = log.len().max(from);
        (log.iter().skip(from).cloned().collect(), next)
    }

    /// Drains stdout et stderr du processus,produit les événements neufs.
    ///
    /// Une erreur de params (protocole cassé) ignore la suite de la
    /// ligne ; le journal garde tout ce qui a été interprété avant.
    pub fn pump(&self) -> Result<Vec<RunEvent>> {
        let mut fresh = Vec::new();
        let mut error = None;

        for line in self.session.take_lines() {
            if let Some(event) = parse_line(&line) {
                let run_event = match self.lock(&self.observer).observe(&event) {
                    Ok(run_event) => run_event,
                    Err(cause) => {
                        error = Some(cause);
                        break;
                    }
                };
                self.push_to_log(run_event.clone());
                fresh.push(run_event);
            } else {
                // Hors protocole : verrou de build, anomalies stdout.
                let run_event = RunEvent::Log { line };
                self.push_to_log(run_event.clone());
                fresh.push(run_event);
            }
        }

        for line in self.session.take_stderr_lines() {
            let run_event = RunEvent::Log { line };
            self.push_to_log(run_event.clone());
            fresh.push(run_event);
        }

        match error {
            Some(cause) => Err(cause),
            None => Ok(fresh),
        }
    }

    /// Envoie une demande daemon sur stdin (`{"id":n,"method":...}`).
    pub fn request(&self, method: &str) -> Result<()> {
        let id = self.next_request_id.fetch_add(1, Ordering::Relaxed) + 1;
        self.session
            .write_request(&json!({ "method": method, "id": id }).to_string())
    }

    pub fn reload(&self) -> Result<()> {
        self.request("app.reload")
    }

    pub fn stop(&self) -> Result<()> {
        self.request("app.stop")
    }

    fn push_to_log(&self, event: RunEvent) {
        let mut log = self.lock(&self.log);

        if log.len() + 1 > LOG_LIMIT {
            let overflow = log.len() + 1 - LOG_LIMIT;
            log.drain(0..overflow);
        }
        log.push(event);
    }
}

impl LiveRun {
    fn lock<'a, T>(&self, mutex: &'a Mutex<T>) -> MutexGuard<'a, T> {
        mutex.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RunState;
    use pls_base::ErrorCode;
    use pls_testkit::FLUTTER_RUN_MACHINE;
    use std::path::PathBuf;

    fn fixture_lines() -> Vec<Vec<&'static str>> {
        vec![
            FLUTTER_RUN_MACHINE
                .lines()
                .filter(|line| !line.trim().is_empty())
                .collect(),
        ]
    }

    fn fixture_runner() -> pls_testkit::ScriptedProcessRunner {
        pls_testkit::ScriptedProcessRunner::with_session_text(fixture_lines())
    }

    #[tokio::test]
    async fn start_builds_a_validated_machine_spec() {
        let runner = fixture_runner();
        let config = RunConfig {
            device_id: Some("emulator-5554".to_string()),
            debug: true,
        };

        let live = LiveRun::start(
            &runner,
            &config,
            &PathBuf::from("flutter"),
            &PathBuf::from("."),
        )
        .await
        .expect("start");
        assert_eq!(live.pid(), 4242);

        let calls = runner.calls();
        assert_eq!(calls[0].program, "flutter");
        assert_eq!(
            calls[0].args,
            vec!["run", "--machine", "-d", "emulator-5554"]
        );
        assert!(calls[0].cwd.is_some());
        assert!(runner.last_session().is_some());
    }

    #[tokio::test]
    async fn release_flag_when_not_debugging() {
        let runner = pls_testkit::ScriptedProcessRunner::default();
        let config = RunConfig {
            device_id: None,
            debug: false,
        };

        LiveRun::start(
            &runner,
            &config,
            &PathBuf::from("flutter"),
            &PathBuf::from("."),
        )
        .await
        .expect("start");

        let calls = runner.calls();
        assert!(calls[0].args.contains(&"--release".to_string()));
    }

    #[tokio::test]
    async fn dirty_device_id_rejected_before_spawn() {
        let runner = fixture_runner();
        let config = RunConfig {
            device_id: Some("bad device; rm".to_string()),
            debug: true,
        };

        let error = LiveRun::start(
            &runner,
            &config,
            &PathBuf::from("flutter"),
            &PathBuf::from("."),
        )
        .await
        .expect_err("dirty device");
        assert_eq!(error.code(), ErrorCode::INVALID_PARAMS);
        assert!(runner.calls().is_empty(), "no process launched");
    }

    #[tokio::test]
    async fn pump_consumes_the_full_fixture_session() {
        let runner = fixture_runner();
        let live = start_fixture(&runner).await;

        let fresh = live.pump().expect("pump");
        // 11 events + 1 ligne hors protocole (verrou de build).
        assert_eq!(fresh.len(), 12);
        assert_eq!(
            fresh[3],
            RunEvent::AppStarted {
                app_id: "com.example.demo".to_string(),
                device_id: Some("emulator-5554".to_string()),
            }
        );
        assert_eq!(live.snapshot().state, RunState::Exited);

        // Second pompage : plus rien, journal intact.
        let again = live.pump().expect("pump");
        assert!(again.is_empty());
        assert_eq!(live.event_count(), 12);

        let (events, next) = live.events_from(0);
        assert_eq!(events.len(), 12);
        assert_eq!(next, 12);
    }

    #[tokio::test]
    async fn requests_increment_their_ids() {
        let runner = fixture_runner();
        let live = start_fixture(&runner).await;

        live.reload().expect("reload");
        live.request("app.restart").expect("restart");
        live.stop().expect("stop");

        let scripted = runner.last_session().expect("session");
        assert_eq!(
            scripted.written(),
            vec![
                json!({"method": "app.reload", "id": 1}).to_string(),
                json!({"method": "app.restart", "id": 2}).to_string(),
                json!({"method": "app.stop", "id": 3}).to_string(),
            ]
        );
    }

    #[tokio::test]
    async fn kill_flags_the_scripted_session() {
        let runner = fixture_runner();
        let live = start_fixture(&runner).await;

        assert!(!live.is_finished());
        live.kill();
        assert!(live.is_finished());

        let scripted = runner.last_session().expect("session");
        assert!(scripted.is_killed());
        assert_eq!(scripted.exit_status(), Some(1));
    }

    async fn start_fixture(runner: &pls_testkit::ScriptedProcessRunner) -> LiveRun {
        let config = RunConfig {
            device_id: None,
            debug: true,
        };
        LiveRun::start(
            runner,
            &config,
            &PathBuf::from("flutter"),
            &PathBuf::from("."),
        )
        .await
        .expect("start")
    }
}
