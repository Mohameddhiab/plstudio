//! Commandes `run.*` : sessions de `flutter run --machine` vivantes.
//!
//! Une session est créée par `run.launch` (bloque jusqu'à app prête),
//! inspectée par `run.log`/`run.status`, pilotée par `run.reload`/
//! `run.stop`, et libérée par `run.detach`. Toutes partagent le même
//! `ToolchainState` que les commandes plateforme (un seul résolveur).

use std::{
    collections::BTreeMap,
    sync::{
        Arc, PoisonError, RwLock,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use async_trait::async_trait;
use pls_base::{
    Command, CommandCtx, CommandMeta, CommandOutput, DangerLevel, ErrorCode, PlsError, Result,
    parse_params,
};
use pls_run::{LiveRun, RunConfig, RunState};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::time::sleep;

use crate::toolchain::ToolchainState;

/// Registre des sessions de run, numérotées à partir de 1.
#[derive(Default, Debug)]
pub struct RunRegistry {
    next_id: AtomicUsize,
    sessions: RwLock<BTreeMap<usize, Arc<LiveRun>>>,
}

impl RunRegistry {
    pub fn insert(&self, live: Arc<LiveRun>) -> usize {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        self.lock_sessions().insert(id, live);
        id
    }

    pub fn get(&self, id: usize) -> Option<Arc<LiveRun>> {
        self.lock_sessions().get(&id).cloned()
    }

    pub fn remove(&self, id: usize) -> Option<Arc<LiveRun>> {
        self.lock_sessions().remove(&id)
    }

    pub fn count(&self) -> usize {
        self.lock_sessions().len()
    }

    fn lock_sessions(&self) -> std::sync::RwLockWriteGuard<'_, BTreeMap<usize, Arc<LiveRun>>> {
        self.sessions
            .write()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

fn unknown_session(run_id: usize) -> PlsError {
    PlsError::app(
        ErrorCode::COMMAND,
        format!("no active run session {run_id}"),
    )
    .with_remediation("run.launch returns ids; sessions are removed by run.detach and kept after exit for inspection")
}

/// Facade des commandes `run.*` (registre + résolveur partagés).
pub struct RunCommands {
    toolchain: Arc<ToolchainState>,
}

impl RunCommands {
    pub fn new(toolchain: Arc<ToolchainState>) -> Self {
        Self { toolchain }
    }

    fn live(&self, run_id: usize) -> Result<Arc<LiveRun>> {
        self.toolchain
            .runs
            .get(run_id)
            .ok_or_else(|| unknown_session(run_id))
    }
}

pub fn register_with(engine: &mut crate::Engine, state: Arc<ToolchainState>) {
    let commands = Arc::new(RunCommands::new(state.clone()));
    engine.register(LaunchCommand::new(commands.clone()));
    engine.register(LogCommand::new(commands.clone()));
    engine.register(ReloadCommand::new(commands.clone()));
    engine.register(StopCommand::new(commands.clone()));
    engine.register(StatusCommand::new(commands.clone()));
    engine.register(DetachCommand::new(commands));
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LaunchParams {
    device_id: Option<String>,
    debug: Option<bool>,
    wait_seconds: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LogParams {
    run_id: usize,
    from: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReloadParams {
    run_id: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StopParams {
    run_id: usize,
    force: Option<bool>,
    stop_seconds: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StatusParams {
    run_id: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DetachParams {
    run_id: usize,
}

/// Baseline de session : insertion + wait jusqu'à Running/Exited.
/// Retourne un snapshot ; l'erreur tranchée entre "exited early" / timeout.
async fn wait_until_ready(
    live: &Arc<LiveRun>,
    cancellation: &pls_base::Cancellation,
    wait_seconds: u32,
) -> Result<pls_run::RunObserver> {
    let polls = (wait_seconds as u64) * 4; // 250ms par sonde

    for _ in 0..polls {
        if cancellation.is_cancelled() {
            live.kill();
            return Err(PlsError::app(
                ErrorCode::CANCELLED,
                "cancelled while launching the run session",
            )
            .with_remediation("the process was killed; run.status shows the final state"));
        }

        if let Err(cause) = live.pump() {
            let snapshot = live.snapshot();
            return Err(PlsError::app(
                ErrorCode::COMMAND,
                format!(
                    "run session aborted: {cause} (session state: {:?})",
                    snapshot.state
                ),
            ));
        }

        let snapshot = live.snapshot();
        match snapshot.state {
            RunState::Running => return Ok(snapshot),
            RunState::Exited => {
                return Err(PlsError::app(
                    ErrorCode::COMMAND,
                    format!(
                        "flutter run exited early (code {}); session inspected via journal",
                        snapshot.exit_code.unwrap_or_default(),
                    ),
                )
                .with_remediation("run.log shows the events leading to the exit"));
            }
            _ => sleep(Duration::from_millis(250)).await,
        }
    }

    Ok(live.snapshot())
}

/// `run.launch` — démarre une session et attend l'app prête.
pub struct LaunchCommand {
    commands: Arc<RunCommands>,
}

impl LaunchCommand {
    pub fn new(commands: Arc<RunCommands>) -> Self {
        Self { commands }
    }
}

#[async_trait]
impl Command for LaunchCommand {
    fn meta(&self) -> CommandMeta {
        CommandMeta::new("run.launch", "Launch an app", "run", DangerLevel::Write).with_description(
            "Lance `flutter run --machine` dans le workspace courant et \
                 attend que l'app soit prête (défaut 300s, borné 1..3600). \
                 La session reste inspectable via run.log/run.status.",
        )
    }

    async fn run(&self, ctx: &CommandCtx, params: Value) -> Result<CommandOutput> {
        let params: LaunchParams = parse_params(&params)?;
        let flutter = self.commands.toolchain.flutter()?;
        let wait_seconds = params.wait_seconds.unwrap_or(300).clamp(1, 3600);

        let config = RunConfig {
            device_id: params.device_id,
            debug: params.debug.unwrap_or(true),
        };
        let live = Arc::new(
            LiveRun::start(
                self.commands.toolchain.runner.as_ref(),
                &config,
                &flutter,
                &ctx.workspace,
            )
            .await?,
        );
        let run_id = self.commands.toolchain.runs.insert(live.clone());

        let snapshot = wait_until_ready(&live, &ctx.cancellation, wait_seconds).await?;
        match snapshot.state {
            RunState::Running => Ok(CommandOutput::json(json!({
                "run_id": run_id,
                "state": "running",
                "app_id": snapshot.app_id,
                "device_id": snapshot.device_id,
                "debug_port": snapshot.debug_port,
            }))),
            RunState::Exited => Err(PlsError::app(
                ErrorCode::COMMAND,
                format!(
                    "flutter run exited before being ready (code {}; session {run_id} kept)",
                    snapshot.exit_code.unwrap_or_default(),
                ),
            )
            .with_remediation(format!("inspect `run.log` with run_id {run_id}"))),
            _ => Err(PlsError::app(
                ErrorCode::COMMAND,
                format!("session {run_id} not running yet after {wait_seconds}s"),
            )
            .with_remediation(format!(
                "raise wait_seconds, or inspect `run.status` with run_id {run_id}"
            ))),
        }
    }
}

/// `run.log` — événements journalisés depuis un offset.
pub struct LogCommand {
    commands: Arc<RunCommands>,
}

impl LogCommand {
    pub fn new(commands: Arc<RunCommands>) -> Self {
        Self { commands }
    }
}

#[async_trait]
impl Command for LogCommand {
    fn meta(&self) -> CommandMeta {
        CommandMeta::new("run.log", "Session journal", "run", DangerLevel::Read).with_description(
            "Événements d'une session depuis l'offset `from` (défaut 0), \
                 avec l'état courant. Le flux live passera par le protocole de \
                 contrôle.",
        )
    }

    async fn run(&self, _ctx: &CommandCtx, params: Value) -> Result<CommandOutput> {
        let params: LogParams = parse_params(&params)?;
        let live = self.commands.live(params.run_id)?;

        let from = params.from.unwrap_or(0);
        let (events, next) = live.events_from(from);
        let snapshot = live.snapshot();

        Ok(CommandOutput::json(json!({
            "run_id": params.run_id,
            "state": snapshot.state,
            "events": events,
            "next": next,
        })))
    }
}

/// `run.reload` — hot reload de la session.
pub struct ReloadCommand {
    commands: Arc<RunCommands>,
}

impl ReloadCommand {
    pub fn new(commands: Arc<RunCommands>) -> Self {
        Self { commands }
    }
}

#[async_trait]
impl Command for ReloadCommand {
    fn meta(&self) -> CommandMeta {
        CommandMeta::new("run.reload", "Hot reload", "run", DangerLevel::Write)
            .with_description("Demande `app.reload` sur stdin de la session.")
    }

    async fn run(&self, _ctx: &CommandCtx, params: Value) -> Result<CommandOutput> {
        let params: ReloadParams = parse_params(&params)?;
        let live = self.commands.live(params.run_id)?;

        live.reload()?;

        Ok(CommandOutput::json(json!({
            "run_id": params.run_id,
            "sent": true,
        })))
    }
}

/// `run.stop` — arrêt gracieux ou kill forcé.
pub struct StopCommand {
    commands: Arc<RunCommands>,
}

impl StopCommand {
    pub fn new(commands: Arc<RunCommands>) -> Self {
        Self { commands }
    }
}

#[async_trait]
impl Command for StopCommand {
    fn meta(&self) -> CommandMeta {
        CommandMeta::new("run.stop", "Stop a session", "run", DangerLevel::Write).with_description(
            "Demande `app.stop` (gracieux, `force` tue). Surveille la sortie \
                 jusqu'à `stop_seconds` (défaut 10, borné 1..60).",
        )
    }

    async fn run(&self, ctx: &CommandCtx, params: Value) -> Result<CommandOutput> {
        let params: StopParams = parse_params(&params)?;
        let live = self.commands.live(params.run_id)?;

        if params.force.unwrap_or(false) {
            live.kill();
            let snapshot = live.snapshot();
            return Ok(CommandOutput::json(json!({
                "run_id": params.run_id,
                "killed": true,
                "exit_code": snapshot.exit_code,
            })));
        }

        live.stop()?;
        let stop_seconds = params.stop_seconds.unwrap_or(10).clamp(1, 60);
        let polls = (stop_seconds as u64) * 4;

        let mut exited: Option<i32> = None;
        for _ in 0..polls {
            if ctx.cancellation.is_cancelled() {
                return Err(PlsError::app(
                    ErrorCode::CANCELLED,
                    "cancelled while stopping the run session",
                ));
            }

            if let Err(cause) = live.pump() {
                // La session est morte : le statut de sortie tranche.
                if !live.is_finished() {
                    return Err(cause);
                }
            }

            let snapshot = live.snapshot();
            if snapshot.state == RunState::Exited {
                exited = snapshot.exit_code;
                break;
            }
            sleep(Duration::from_millis(250)).await;
        }

        let snapshot = live.snapshot();
        Ok(CommandOutput::json(json!({
            "run_id": params.run_id,
            "stopped": exited.is_some(),
            "exit_code": exited.or(snapshot.exit_code),
            "state": snapshot.state,
        })))
    }
}

/// `run.status` — instantané de l'observateur.
pub struct StatusCommand {
    commands: Arc<RunCommands>,
}

impl StatusCommand {
    pub fn new(commands: Arc<RunCommands>) -> Self {
        Self { commands }
    }
}

#[async_trait]
impl Command for StatusCommand {
    fn meta(&self) -> CommandMeta {
        CommandMeta::new("run.status", "Session status", "run", DangerLevel::Read).with_description(
            "État, identités (app/appareil), port debug, pid et taille du journal.",
        )
    }

    async fn run(&self, _ctx: &CommandCtx, params: Value) -> Result<CommandOutput> {
        let params: StatusParams = parse_params(&params)?;
        let live = self.commands.live(params.run_id)?;
        let snapshot = live.snapshot();

        Ok(CommandOutput::json(json!({
            "run_id": params.run_id,
            "pid": live.pid(),
            "alive": !live.is_finished(),
            "state": snapshot.state,
            "app_id": snapshot.app_id,
            "device_id": snapshot.device_id,
            "debug_port": snapshot.debug_port,
            "progress": snapshot.progress,
            "exit_code": snapshot.exit_code,
            "events": live.event_count(),
        })))
    }
}

/// `run.detach` — libère la session (le processus vit sans plstudio).
pub struct DetachCommand {
    commands: Arc<RunCommands>,
}

impl DetachCommand {
    pub fn new(commands: Arc<RunCommands>) -> Self {
        Self { commands }
    }
}

#[async_trait]
impl Command for DetachCommand {
    fn meta(&self) -> CommandMeta {
        CommandMeta::new("run.detach", "Forget a session", "run", DangerLevel::Write)
            .with_description(
                "Retire la session du registre : plstudio n'expose plus de \
                 contrôle (le processus continue). run_id perd sa validité.",
            )
    }

    async fn run(&self, _ctx: &CommandCtx, params: Value) -> Result<CommandOutput> {
        let params: DetachParams = parse_params(&params)?;
        let live = self
            .commands
            .toolchain
            .runs
            .remove(params.run_id)
            .ok_or_else(|| unknown_session(params.run_id))?;

        Ok(CommandOutput::json(json!({
            "run_id": params.run_id,
            "detached": true,
            "pid": live.pid(),
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Engine, toolchain::ToolchainState};
    use pls_base::{CommandCtx, CommandId, ProcessSession, TrustLevel};
    use pls_sdk::EnvironmentReport;
    use pls_testkit::ScriptedProcessRunner;
    use serde_json::json;
    use std::{path::PathBuf, sync::Arc};

    async fn run(engine: &Engine, id: &str, params: Value) -> Value {
        let ctx = CommandCtx::new(engine.next_job(), ".", TrustLevel::Trusted);
        engine
            .execute(&CommandId::from(id), &ctx, params)
            .await
            .expect("command succeeds")
            .value
    }

    async fn execute_raw(engine: &Engine, id: &str, params: Value) -> Result<Value> {
        let ctx = CommandCtx::new(engine.next_job(), ".", TrustLevel::Trusted);
        engine
            .execute(&CommandId::from(id), &ctx, params)
            .await
            .map(|output| output.value)
    }

    fn state_with_sessions(
        scripts: Vec<Vec<String>>,
    ) -> (Arc<ToolchainState>, ScriptedProcessRunner) {
        let runner = ScriptedProcessRunner::new(Vec::new()).with_session_scripts(scripts);
        let state = ToolchainState {
            runner: Arc::new(runner.clone()),
            report: EnvironmentReport::default(),
            avdmanager: None,
            adb: None,
            emulator: None,
            flutter: Some(PathBuf::from("flutter")),
            scrcpy: None,
            runs: RunRegistry::default(),
        };
        (Arc::new(state), runner)
    }

    fn engine_with(state: Arc<ToolchainState>) -> Engine {
        let mut engine = Engine::new();
        crate::toolchain::register_with(&mut engine, state.clone());
        register_with(&mut engine, state);
        engine
    }

    fn ready_session_lines() -> Vec<Vec<String>> {
        vec![vec![
            "{\"event\":\"daemon.initialized\",\"params\":{\"version\":\"0.0.0\"}}".to_string(),
            "{\"event\":\"app.start\",\"params\":{\"appId\":\"com.example.demo\",\"deviceId\":\"emulator-5554\"}}".to_string(),
            "{\"event\":\"app.debugPort\",\"params\":{\"wsUri\":\"ws://127.0.0.1:51234/ws\",\"port\":51234}}".to_string(),
            "{\"event\":\"app.progress\",\"params\":{\"finished\":true}}".to_string(),
        ]]
    }

    #[tokio::test]
    async fn launch_reaches_running_and_exposes_the_session() {
        let (state, runner) = state_with_sessions(ready_session_lines());
        let engine = engine_with(state);

        let value = run(
            &engine,
            "run.launch",
            json!({ "device_id": "emulator-5554" }),
        )
        .await;
        assert_eq!(value.get("run_id").and_then(Value::as_u64), Some(1));
        assert_eq!(value.get("state").and_then(Value::as_str), Some("running"));
        assert_eq!(
            value.get("app_id").and_then(Value::as_str),
            Some("com.example.demo")
        );
        assert!(value.get("debug_port").is_some());

        // Le pid du mote de session est lisible via run.status.
        let status = run(&engine, "run.status", json!({ "run_id": 1 })).await;
        assert_eq!(status.get("pid").and_then(Value::as_u64), Some(4242));
        assert_eq!(status.get("alive").and_then(Value::as_bool), Some(true));

        let calls = runner.calls();
        assert_eq!(
            calls[0].args,
            vec!["run", "--machine", "-d", "emulator-5554"]
        );
    }

    #[tokio::test]
    async fn launch_early_exit_keeps_the_session_for_inspection() {
        let scripts = vec![vec![
            "{\"event\":\"daemon.initialized\"}".to_string(),
            "{\"event\":\"daemon.exit\",\"params\":{\"code\":1}}".to_string(),
            "{\"event\":\"app.log\",\"params\":{\"log\":\"launcher failure\"}}".to_string(),
        ]];
        let (state, _) = state_with_sessions(scripts);
        let engine = engine_with(state);

        let error = execute_raw(&engine, "run.launch", json!({}))
            .await
            .expect_err("exited early");
        assert_eq!(error.code(), ErrorCode::COMMAND);

        let log = run(&engine, "run.log", json!({ "run_id": 1 })).await;
        assert_eq!(log.get("next").and_then(Value::as_u64), Some(3));

        let status = run(&engine, "run.status", json!({ "run_id": 1 })).await;
        assert_eq!(status.get("state").and_then(Value::as_str), Some("exited"));
        assert_eq!(status.get("exit_code").and_then(Value::as_i64), Some(1));
    }

    #[tokio::test]
    async fn launch_times_out_when_never_ready() {
        let scripts = vec![vec!["{\"event\":\"daemon.initialized\"}".to_string()]];
        let (state, _) = state_with_sessions(scripts);
        let engine = engine_with(state);

        let error = execute_raw(&engine, "run.launch", json!({ "wait_seconds": 1 }))
            .await
            .expect_err("timeout");
        assert_eq!(error.code(), ErrorCode::COMMAND);
    }

    #[tokio::test]
    async fn reload_then_stop_reach_the_scripted_stdin() {
        let (state, runner) = state_with_sessions(ready_session_lines());
        let engine = engine_with(state);

        run(&engine, "run.launch", json!({})).await;
        run(&engine, "run.reload", json!({ "run_id": 1 })).await;

        // Session factice reste vivante : stop borne court, 1 seconde.
        let stopped = run(
            &engine,
            "run.stop",
            json!({ "run_id": 1, "stop_seconds": 1 }),
        )
        .await;
        assert_eq!(stopped.get("stopped").and_then(Value::as_bool), Some(false));
        assert_eq!(
            stopped.get("state").and_then(Value::as_str),
            Some("running")
        );

        let scripted = runner.last_session().expect("session");
        let requests = scripted.written();
        assert_eq!(requests.len(), 2);
        assert!(requests[0].contains("app.reload"));
        assert!(requests[1].contains("app.stop"));
        assert!(requests[0].contains("\"id\":1"));
    }

    #[tokio::test]
    async fn force_stop_kills_and_pins_the_exit() {
        let (state, runner) = state_with_sessions(ready_session_lines());
        let engine = engine_with(state);

        run(&engine, "run.launch", json!({})).await;
        let killed = run(&engine, "run.stop", json!({ "run_id": 1, "force": true })).await;
        assert_eq!(killed.get("killed").and_then(Value::as_bool), Some(true));

        let scripted = runner.last_session().expect("session");
        assert!(scripted.is_killed());
        assert_eq!(scripted.exit_status(), Some(1));
    }

    #[tokio::test]
    async fn log_serves_the_journal_by_cursor() {
        let (state, _) = state_with_sessions(ready_session_lines());
        let engine = engine_with(state);

        run(&engine, "run.launch", json!({})).await;

        let first = run(&engine, "run.log", json!({ "run_id": 1, "from": 0 })).await;
        assert_eq!(first.get("next").and_then(Value::as_u64), Some(4));

        let tail = run(&engine, "run.log", json!({ "run_id": 1, "from": 2 })).await;
        assert_eq!(tail.get("next").and_then(Value::as_u64), Some(4));
        assert_eq!(
            tail.get("events").and_then(Value::as_array).map(Vec::len),
            Some(2)
        );
    }

    #[tokio::test]
    async fn unknown_session_is_a_clean_not_found() {
        let (state, _) = state_with_sessions(vec![]);
        let engine = engine_with(state);

        let error = execute_raw(&engine, "run.status", json!({ "run_id": 999 }))
            .await
            .expect_err("unknown session");
        assert_eq!(error.code(), ErrorCode::COMMAND);
    }

    #[tokio::test]
    async fn detaching_frees_the_run_id() {
        let (state, _) = state_with_sessions(ready_session_lines());
        let engine = engine_with(state);

        let launched = run(&engine, "run.launch", json!({})).await;
        assert_eq!(launched.get("run_id").and_then(Value::as_u64), Some(1));

        let detached = run(&engine, "run.detach", json!({ "run_id": 1 })).await;
        assert_eq!(detached.get("pid").and_then(Value::as_u64), Some(4242));

        let error = execute_raw(&engine, "run.status", json!({ "run_id": 1 }))
            .await
            .expect_err("forgotten");
        assert_eq!(error.code(), ErrorCode::COMMAND);
    }

    #[tokio::test]
    async fn cancelled_launch_kills_the_session() {
        let (state, runner) =
            state_with_sessions(vec![vec!["{\"event\":\"daemon.initialized\"}".to_string()]]);
        let engine = engine_with(state);

        let ctx = CommandCtx::new(engine.next_job(), ".", TrustLevel::Trusted);
        ctx.cancellation.cancel();

        let error = engine
            .execute(
                &CommandId::from("run.launch"),
                &ctx,
                json!({ "wait_seconds": 5 }),
            )
            .await
            .expect_err("cancelled");
        assert_eq!(error.code(), ErrorCode::CANCELLED);
        assert!(runner.last_session().expect("session").is_killed());
    }
}
