//! Cycle de vie d'une session `flutter run --machine`.
//!
//! Phase 1 : modélisation (fixtures de `pls-testkit`) + pilotage réel
//! ([`live::LiveRun`]) — stdout drainable, demandes stdin, kill. Le
//! protocole de contrôle exposera ces sessions à la UI ; ici le moteur
//! les tient déjà, headless.

#![forbid(unsafe_code)]

use pls_base::{ErrorCode, PlsError, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub mod live;

/// Ligne NDJSON du daemon flutter.
///
/// `params` reste brut et est typé à la demande (`params_as`) : flutter
/// ajoutera des champs demain — les structs typés ne couvrent que les
/// clés documentées du protocole daemon.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DaemonEvent {
    pub event: String,
    #[serde(default)]
    pub params: Value,
}

impl DaemonEvent {
    /// Désérialise `params` vers le modèle typé correspondant.
    pub fn params_as<T: for<'de> Deserialize<'de>>(&self) -> Result<T> {
        serde_json::from_value(self.params.clone()).map_err(|error| {
            PlsError::app(
                ErrorCode::INVALID_PARAMS,
                format!("malformed params for {}: {error}", self.event),
            )
        })
    }
}

/// Classification structurelle d'une ligne de sortie.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineClass {
    /// `{"event": ...}` du protocole daemon.
    Event,
    /// Texte hors protocole (verrou de build, anomalies stdout).
    Text,
    /// Vide : ignorée.
    Blank,
}

pub fn classify_line(line: &str) -> LineClass {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return LineClass::Blank;
    }

    if trimmed.starts_with('{') && serde_json::from_str::<DaemonEvent>(trimmed).is_ok() {
        LineClass::Event
    } else {
        LineClass::Text
    }
}

/// Parse une ligne en événement daemon ; aucune pour texte hors protocole.
pub fn parse_line(line: &str) -> Option<DaemonEvent> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    serde_json::from_str::<DaemonEvent>(trimmed).ok()
}

/// Lancement d'une app (`app.start`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppStart {
    #[serde(rename = "appId")]
    pub app_id: String,
    #[serde(rename = "deviceId")]
    pub device_id: Option<String>,
    #[serde(default)]
    pub directory: Option<String>,
}

/// Sortie d'une app (`app.stop`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppStop {
    #[serde(rename = "appId")]
    pub app_id: String,
}

/// Port du débogueur Dart (`app.debugPort`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DebugPort {
    #[serde(rename = "wsUri")]
    pub ws_uri: String,
    #[serde(default)]
    pub port: Option<u16>,
}

/// Progression (`app.progress`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Progress {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub finished: bool,
}

/// Message daemon (`daemon.logMessage`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogMessage {
    #[serde(default)]
    pub level: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
}

/// Fin du processus daemon (`daemon.exit`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DaemonExit {
    #[serde(default)]
    pub code: Option<i32>,
}

/// Attribution (log) d'un événement bien typé, dans l'ordre du flux.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunEvent {
    Started,
    AppStarted {
        app_id: String,
        device_id: Option<String>,
    },
    Progress {
        message: Option<String>,
        finished: bool,
    },
    Log {
        line: String,
    },
    DebugPort {
        ws_uri: String,
    },
    AppStopped {
        app_id: String,
    },
    DaemonExit {
        code: Option<i32>,
    },
    /// Événement daemon non reconnu, conservé (pas perdu).
    Raw {
        event: String,
    },
}

/// États observés d'une session de run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    #[default]
    Stopped,
    Starting,
    Building,
    Running,
    Stopping,
    Exited,
}

/// Suivi interprété du flux `--machine`.
///
/// Muté exclusivement via `observe` ; sérialisable pour la CLI, la UI
/// future et le protocole de contrôle rendront l'état visible.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RunObserver {
    pub state: RunState,
    pub app_id: Option<String>,
    pub device_id: Option<String>,
    pub debug_port: Option<DebugPort>,
    pub progress: Option<String>,
    pub exit_code: Option<i32>,
}

impl RunObserver {
    pub fn observe(&mut self, event: &DaemonEvent) -> Result<RunEvent> {
        let out = match event.event.as_str() {
            "daemon.initialized" => {
                if self.state == RunState::Stopped {
                    self.state = RunState::Starting;
                }
                RunEvent::Started
            }
            "app.start" => {
                let start: AppStart = event.params_as()?;
                self.app_id = Some(start.app_id.clone());
                self.device_id = start.device_id.clone();
                RunEvent::AppStarted {
                    app_id: start.app_id,
                    device_id: start.device_id,
                }
            }
            "app.progress" => {
                let progress: Progress = event.params_as()?;
                self.progress = progress.message.clone();
                self.transition_on_progress(&progress);
                RunEvent::Progress {
                    message: progress.message,
                    finished: progress.finished,
                }
            }
            "app.debugPort" => {
                let port: DebugPort = event.params_as()?;
                self.debug_port = Some(port.clone());
                RunEvent::DebugPort {
                    ws_uri: port.ws_uri,
                }
            }
            "app.log" => {
                let log: LogMessage = event.params_as()?;
                RunEvent::Log {
                    line: log.message.unwrap_or_default(),
                }
            }
            "daemon.logMessage" => {
                let log: LogMessage = event.params_as()?;
                RunEvent::Log {
                    line: log.message.unwrap_or_default(),
                }
            }
            "app.stop" => {
                let stop: AppStop = event.params_as()?;
                self.app_id = Some(stop.app_id.clone());
                if self.state != RunState::Exited {
                    self.state = RunState::Stopping;
                }
                RunEvent::AppStopped {
                    app_id: stop.app_id,
                }
            }
            "daemon.exit" => {
                let exit: DaemonExit = event.params_as()?;
                self.exit_code = exit.code;
                self.state = RunState::Exited;
                RunEvent::DaemonExit { code: exit.code }
            }
            unknown => RunEvent::Raw {
                event: unknown.to_string(),
            },
        };

        Ok(out)
    }

    fn transition_on_progress(&mut self, progress: &Progress) {
        if self.state == RunState::Stopping || self.state == RunState::Exited {
            return;
        }

        if progress.finished {
            self.state = RunState::Running;
        } else {
            self.state = RunState::Building;
        }
    }
}

/// Traite en ordre une sortie complète, renvoie les événements vus.
pub fn feed_lines(observer: &mut RunObserver, stdout: &str) -> Result<Vec<RunEvent>> {
    let mut events = Vec::new();

    for line in stdout.lines() {
        if let Some(parsed) = parse_line(line) {
            events.push(observer.observe(&parsed)?);
        } else if !line.trim().is_empty() {
            // Hors protocole : conservé en log (verrou de build, anomalies).
            events.push(RunEvent::Log {
                line: line.to_string(),
            });
        }
    }

    Ok(events)
}

/// Raccourci : sortie brute → observateur finalisé + événements.
pub fn observe_output(stdout: &str) -> Result<(RunObserver, Vec<RunEvent>)> {
    let mut observer = RunObserver::default();
    let events = feed_lines(&mut observer, stdout)?;
    Ok((observer, events))
}

/// Demande de lancement conservée pour `run.start` (profils, config).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunConfig {
    #[serde(default)]
    pub device_id: Option<String>,
    #[serde(default)]
    pub debug: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use pls_testkit::FLUTTER_RUN_MACHINE;
    use serde_json::json;

    fn event(name: &str, params: Value) -> DaemonEvent {
        DaemonEvent {
            event: name.to_string(),
            params,
        }
    }

    #[test]
    fn classify_the_fixture_lines() {
        let lines: Vec<LineClass> = FLUTTER_RUN_MACHINE.lines().map(classify_line).collect();

        // La première ligne de fixture est vide (boot du daemon).
        assert_eq!(lines[0], LineClass::Blank);
        assert_eq!(lines[1], LineClass::Event);

        // "Waiting for another flutter command to release the startup lock."
        let text_count = lines
            .iter()
            .filter(|class| **class == LineClass::Text)
            .count();
        assert_eq!(text_count, 1);
        let event_count = lines
            .iter()
            .filter(|class| **class == LineClass::Event)
            .count();
        assert_eq!(event_count, 11);
    }

    #[test]
    fn json_without_event_is_text() {
        assert_eq!(classify_line("{}"), LineClass::Text);
        assert_eq!(parse_line("{}"), None);
    }

    #[test]
    fn full_fixture_walks_the_whole_lifecycle() {
        let (observer, events) = observe_output(FLUTTER_RUN_MACHINE).expect("fixture parses");

        assert_eq!(events.len(), 12);

        assert_eq!(events[0], RunEvent::Started);
        assert_eq!(
            events[3],
            RunEvent::AppStarted {
                app_id: "com.example.demo".to_string(),
                device_id: Some("emulator-5554".to_string()),
            }
        );
        assert_eq!(
            events[5],
            RunEvent::DebugPort {
                ws_uri: "ws://127.0.0.1:51234/DSfY=".to_string(),
            }
        );
        assert_eq!(
            events[8],
            RunEvent::Log {
                line: "Waiting for another flutter command to release the startup lock."
                    .to_string()
            }
        );
        assert_eq!(
            events[10],
            RunEvent::AppStopped {
                app_id: "com.example.demo".to_string(),
            }
        );
        assert_eq!(events[11], RunEvent::DaemonExit { code: Some(0) });

        // Gradle en cours au milieu, app prête à la fin.
        assert_eq!(observer.app_id.as_deref(), Some("com.example.demo"));
        assert_eq!(observer.device_id.as_deref(), Some("emulator-5554"));
        assert_eq!(observer.exit_code, Some(0));
        assert_eq!(observer.state, RunState::Exited);
    }

    #[test]
    fn building_then_running_via_progress() {
        let mut observer = RunObserver::default();

        observer
            .observe(&event("daemon.initialized", json!({"version":"0.0.0"})))
            .expect("init");
        assert_eq!(observer.state, RunState::Starting);

        observer
            .observe(&event("app.start", json!({"appId":"com.example.demo"})))
            .expect("start");
        assert_eq!(observer.state, RunState::Starting);

        observer
            .observe(&event(
                "app.progress",
                json!({"message":"Gradle tasks","finished":false}),
            ))
            .expect("progress");
        assert_eq!(observer.state, RunState::Building);

        observer
            .observe(&event("app.progress", json!({"finished":true})))
            .expect("progress done");
        assert_eq!(observer.state, RunState::Running);
    }

    #[test]
    fn app_stop_then_exit_moves_to_stopping_and_exited() {
        let mut observer = RunObserver::default();

        observer
            .observe(&event("app.start", json!({"appId":"com.example.demo"})))
            .expect("start");
        observer
            .observe(&event("app.progress", json!({"finished":true})))
            .expect("running");
        assert_eq!(observer.state, RunState::Running);

        observer
            .observe(&event("app.stop", json!({"appId":"com.example.demo"})))
            .expect("stop");
        assert_eq!(observer.state, RunState::Stopping);

        observer
            .observe(&event("daemon.exit", json!({"code":0})))
            .expect("exit");
        assert_eq!(observer.state, RunState::Exited);
        assert_eq!(observer.exit_code, Some(0));
    }

    #[test]
    fn malformed_known_params_surface_the_event_name() {
        let mut observer = RunObserver::default();

        let error = observer
            .observe(&event("app.start", json!({})))
            .expect_err("missing appId");
        assert_eq!(error.code(), ErrorCode::INVALID_PARAMS);
        assert!(error.to_string().contains("app.start"));
    }

    #[test]
    fn unknown_events_are_kept_as_raw() {
        let mut observer = RunObserver::default();

        let out = observer
            .observe(&event("app.exoticFutureThing", json!({"appId":"x"})))
            .expect("raw tolerated");
        assert_eq!(
            out,
            RunEvent::Raw {
                event: "app.exoticFutureThing".to_string()
            }
        );
        assert_eq!(observer.state, RunState::Stopped);
    }

    #[test]
    fn typed_params_match_the_wire_names() {
        let line = r#"{"event":"app.debugPort","params":{"wsUri":"ws://127.0.0.1:9/ws","port":9}}"#;
        let parsed = parse_line(line).expect("json line");
        let port: DebugPort = parsed.params_as().expect("typed");
        assert_eq!(
            port,
            DebugPort {
                ws_uri: "ws://127.0.0.1:9/ws".to_string(),
                port: Some(9),
            }
        );
    }

    #[test]
    fn run_config_defaults_offline() {
        let config: RunConfig = serde_json::from_value(json!({})).expect("defaults");
        assert_eq!(
            config,
            RunConfig {
                device_id: None,
                debug: false
            }
        );
    }
}
