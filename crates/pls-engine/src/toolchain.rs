//! Commandes plateforme : `sdk.detect`, `avd.*`, `emulator.scrcpy`.
//!
//! L'état (chemins d'outils résolus + runner) est construit au bootstrap du
//! moteur ; les commandes valident leurs paramètres puis délèguent à
//! `pls-avd` / `pls-sdk`. Les dépendances sont injectables : les tests
//! tournent sans SDK réel.

use std::{path::PathBuf, sync::Arc};

use async_trait::async_trait;
use pls_avd::{
    AvdCreateRequest, create_avd, list_avds, list_devices, logcat_dump, start_emulator,
    wait_for_boot,
};
use pls_base::{Command, CommandCtx, CommandMeta, CommandOutput, DangerLevel};
use pls_base::{
    ErrorCode, PlsError, ProcessRunner, ProcessSpec, Result, SystemProcessRunner, parse_params,
};
use pls_sdk::EnvironmentReport;
use serde::Deserialize;
use serde_json::{Value, json};

/// Chemins d'outils résolus + runner de processus du moteur.
pub struct ToolchainState {
    pub runner: Arc<dyn ProcessRunner>,
    pub report: EnvironmentReport,
    pub avdmanager: Option<PathBuf>,
    pub adb: Option<PathBuf>,
    pub emulator: Option<PathBuf>,
    /// Détecté via PATH seulement ; plstudio n'installe rien.
    pub scrcpy: Option<PathBuf>,
}

impl std::fmt::Debug for ToolchainState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ToolchainState")
            .field("report", &self.report)
            .field("avdmanager", &self.avdmanager)
            .field("adb", &self.adb)
            .field("emulator", &self.emulator)
            .field("scrcpy", &self.scrcpy)
            .finish()
    }
}

impl ToolchainState {
    /// Résolution réelle : PATH, variables d'environnement, défauts Windows.
    pub fn detect_system() -> Self {
        let report = pls_sdk::detect();
        let scrcpy = pls_sdk::find_in_path("scrcpy");

        Self {
            runner: Arc::new(SystemProcessRunner),
            avdmanager: report.avdmanager.clone(),
            adb: report.adb.clone(),
            emulator: report.emulator.clone(),
            scrcpy,
            report,
        }
    }

    pub fn avdmanager(&self) -> Result<PathBuf> {
        self.tool("avdmanager", self.avdmanager.as_ref())
    }

    pub fn adb(&self) -> Result<PathBuf> {
        self.tool("adb", self.adb.as_ref())
    }

    pub fn emulator(&self) -> Result<PathBuf> {
        self.tool("emulator", self.emulator.as_ref())
    }

    pub fn scrcpy(&self) -> Result<PathBuf> {
        self.tool("scrcpy", self.scrcpy.as_ref())
    }

    fn tool(&self, name: &str, found: Option<&PathBuf>) -> Result<PathBuf> {
        found.cloned().ok_or_else(|| {
            PlsError::config(format!("tool not found: {name}")).with_remediation(format!(
                "install {name}, extend PATH or set the relevant environment variable; \
                 run `sdk.detect` to see what is missing"
            ))
        })
    }
}

/// Enregistre les commandes plateforme avec la résolution système réelle.
pub fn register(engine: &mut crate::Engine) {
    register_with(engine, Arc::new(ToolchainState::detect_system()));
}

/// Enregistre les commandes plateforme avec une résolution injectée (tests).
///
/// Les identifiants écrasent toute résolution précédente : le dernier
/// enregistrement gagne, ce qui permet aux bins/UX d'imposer des outils.
pub fn register_with(engine: &mut crate::Engine, state: Arc<ToolchainState>) {
    engine.register(SdkDetectCommand);
    engine.register(AvdListCommand::new(state.clone()));
    engine.register(AvdCreateCommand::new(state.clone()));
    engine.register(AvdStartCommand::new(state.clone()));
    engine.register(AvdDevicesCommand::new(state.clone()));
    engine.register(AvdWaitCommand::new(state.clone()));
    engine.register(AvdLogcatCommand::new(state.clone()));
    engine.register(ScrcpyCommand::new(state));
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DetectParams {}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AvdListParams {}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AvdCreateParams {
    name: String,
    device: Option<String>,
    system_image: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AvdStartParams {
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DevicesParams {}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WaitParams {
    serial: Option<String>,
    max_attempts: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LogcatParams {
    serial: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ScrcpyParams {
    serial: Option<String>,
}

/// `sdk.detect` — rapport outilchain sans lancer de processus.
pub struct SdkDetectCommand;

#[async_trait]
impl Command for SdkDetectCommand {
    fn meta(&self) -> CommandMeta {
        CommandMeta::new("sdk.detect", "Detect toolchain", "sdk", DangerLevel::Read)
            .with_description(
                "Détecte le JDK, le SDK Android et ses outils, Flutter, node et git. \
                 Ne lance aucun processus.",
            )
    }

    async fn run(&self, _ctx: &CommandCtx, params: Value) -> Result<CommandOutput> {
        let _params: DetectParams = parse_params(&params)?;
        let report = pls_sdk::detect();

        Ok(CommandOutput::json(json!({
            "report": report,
            "missing": report.missing(),
            "complete": report.is_complete(),
        })))
    }
}

/// `avd.list` — AVD déclarés par `avdmanager`.
pub struct AvdListCommand {
    toolchain: Arc<ToolchainState>,
}

impl AvdListCommand {
    pub fn new(toolchain: Arc<ToolchainState>) -> Self {
        Self { toolchain }
    }
}

#[async_trait]
impl Command for AvdListCommand {
    fn meta(&self) -> CommandMeta {
        CommandMeta::new("avd.list", "List AVDs", "avd", DangerLevel::Read).with_description(
            "Liste les AVD via `avdmanager list avd`. L'état `running` n'est pas \
             croisé ici ; voir `avd.devices` pour les appareils actifs.",
        )
    }

    async fn run(&self, ctx: &CommandCtx, params: Value) -> Result<CommandOutput> {
        let _params: AvdListParams = parse_params(&params)?;
        let avdmanager = self.toolchain.avdmanager()?;
        let avds = list_avds(
            self.toolchain.runner.as_ref(),
            &avdmanager,
            &ctx.cancellation,
        )
        .await?;

        Ok(CommandOutput::json(json!({ "avds": avds })))
    }
}

/// `avd.create` — crée un AVD avec des identifiants validés.
pub struct AvdCreateCommand {
    toolchain: Arc<ToolchainState>,
}

impl AvdCreateCommand {
    pub fn new(toolchain: Arc<ToolchainState>) -> Self {
        Self { toolchain }
    }
}

#[async_trait]
impl Command for AvdCreateCommand {
    fn meta(&self) -> CommandMeta {
        CommandMeta::new("avd.create", "Create an AVD", "avd", DangerLevel::Write).with_description(
            "Crée un AVD : `name`, `device` (profil, optionnel) et `system_image` \
                 en notation sdkmanager (system-images;android-34;google_apis;x86_64).",
        )
    }

    async fn run(&self, ctx: &CommandCtx, params: Value) -> Result<CommandOutput> {
        let params: AvdCreateParams = parse_params(&params)?;
        let avdmanager = self.toolchain.avdmanager()?;
        let request = AvdCreateRequest {
            name: params.name.clone(),
            device: params.device.clone(),
            system_image: params.system_image,
        };

        create_avd(
            self.toolchain.runner.as_ref(),
            &avdmanager,
            &request,
            &ctx.cancellation,
        )
        .await?;

        Ok(CommandOutput::json(json!({
            "created": true,
            "name": params.name,
        })))
    }
}

/// `avd.start` — lance l'émulateur, détaché de plstudio.
pub struct AvdStartCommand {
    toolchain: Arc<ToolchainState>,
}

impl AvdStartCommand {
    pub fn new(toolchain: Arc<ToolchainState>) -> Self {
        Self { toolchain }
    }
}

#[async_trait]
impl Command for AvdStartCommand {
    fn meta(&self) -> CommandMeta {
        CommandMeta::new("avd.start", "Start an emulator", "avd", DangerLevel::Write)
            .with_description(
                "Lance `emulator -avd <name>` en processus détaché. Renvoie le pid ; \
                 l'attente du boot est une commande séparée (`avd.wait`).",
            )
    }

    async fn run(&self, ctx: &CommandCtx, params: Value) -> Result<CommandOutput> {
        let params: AvdStartParams = parse_params(&params)?;
        let emulator = self.toolchain.emulator()?;
        let process = start_emulator(
            self.toolchain.runner.as_ref(),
            &emulator,
            &params.name,
            &ctx.cancellation,
        )
        .await?;

        Ok(CommandOutput::json(json!({
            "started": true,
            "pid": process.pid,
        })))
    }
}

/// `avd.devices` — appareils visibles par adb.
pub struct AvdDevicesCommand {
    toolchain: Arc<ToolchainState>,
}

impl AvdDevicesCommand {
    pub fn new(toolchain: Arc<ToolchainState>) -> Self {
        Self { toolchain }
    }
}

#[async_trait]
impl Command for AvdDevicesCommand {
    fn meta(&self) -> CommandMeta {
        CommandMeta::new("avd.devices", "List devices", "avd", DangerLevel::Read)
            .with_description("Appareils connus d'adb, avec leur état (device, offline...).")
    }

    async fn run(&self, ctx: &CommandCtx, params: Value) -> Result<CommandOutput> {
        let _params: DevicesParams = parse_params(&params)?;
        let adb = self.toolchain.adb()?;
        let devices = list_devices(self.toolchain.runner.as_ref(), &adb, &ctx.cancellation).await?;

        Ok(CommandOutput::json(json!({ "devices": devices })))
    }
}

/// `avd.wait` — attend le boot d'un appareil (sonde toutes les secondes).
pub struct AvdWaitCommand {
    toolchain: Arc<ToolchainState>,
}

impl AvdWaitCommand {
    pub fn new(toolchain: Arc<ToolchainState>) -> Self {
        Self { toolchain }
    }
}

#[async_trait]
impl Command for AvdWaitCommand {
    fn meta(&self) -> CommandMeta {
        CommandMeta::new("avd.wait", "Wait for device boot", "avd", DangerLevel::Read)
            .with_description(
                "Sonde `sys.boot_completed` toutes les secondes, au plus \
                 `max_attempts` fois (défaut 120). Annulable via `engine.cancel`.",
            )
    }

    async fn run(&self, ctx: &CommandCtx, params: Value) -> Result<CommandOutput> {
        let params: WaitParams = parse_params(&params)?;
        let adb = self.toolchain.adb()?;
        let serial = params.serial.as_deref();
        let max_attempts = params.max_attempts.unwrap_or(120).clamp(1, 3600);

        wait_for_boot(
            self.toolchain.runner.as_ref(),
            &adb,
            serial,
            max_attempts,
            &ctx.cancellation,
        )
        .await?;

        Ok(CommandOutput::json(json!({
            "booted": true,
            "serial": params.serial,
        })))
    }
}

/// `avd.logcat` — instantané `logcat -d -v threadtime`.
pub struct AvdLogcatCommand {
    toolchain: Arc<ToolchainState>,
}

impl AvdLogcatCommand {
    pub fn new(toolchain: Arc<ToolchainState>) -> Self {
        Self { toolchain }
    }
}

#[async_trait]
impl Command for AvdLogcatCommand {
    fn meta(&self) -> CommandMeta {
        CommandMeta::new("avd.logcat", "Dump logcat", "avd", DangerLevel::Read).with_description(
            "Capture logcat (format threadtime) d'un appareil. Le flux continu \
                 arrive avec le protocole de contrôle.",
        )
    }

    async fn run(&self, ctx: &CommandCtx, params: Value) -> Result<CommandOutput> {
        let params: LogcatParams = parse_params(&params)?;
        let adb = self.toolchain.adb()?;
        let output = logcat_dump(
            self.toolchain.runner.as_ref(),
            &adb,
            params.serial.as_deref(),
            &ctx.cancellation,
        )
        .await?;

        Ok(CommandOutput::json(json!({
            "serial": params.serial,
            "output": output,
        })))
    }
}

/// `emulator.scrcpy` — miroir fenêtré (v0.1 : fenêtre séparée).
pub struct ScrcpyCommand {
    toolchain: Arc<ToolchainState>,
}

impl ScrcpyCommand {
    pub fn new(toolchain: Arc<ToolchainState>) -> Self {
        Self { toolchain }
    }
}

#[async_trait]
impl Command for ScrcpyCommand {
    fn meta(&self) -> CommandMeta {
        CommandMeta::new(
            "emulator.scrcpy",
            "Open scrcpy window",
            "avd",
            DangerLevel::Write,
        )
        .with_description(
            "Lance scrcpy (miroir de l'appareil) en fenêtre séparée, \
                 processus détaché. `serial` cible un appareil précis.",
        )
    }

    async fn run(&self, ctx: &CommandCtx, params: Value) -> Result<CommandOutput> {
        let params: ScrcpyParams = parse_params(&params)?;
        let scrcpy = self.toolchain.scrcpy()?;

        if ctx.cancellation.is_cancelled() {
            return Err(PlsError::app(
                ErrorCode::CANCELLED,
                "cancelled before launching scrcpy",
            ));
        }

        let mut spec = ProcessSpec::new(scrcpy.to_string_lossy());
        if let Some(serial) = &params.serial {
            spec = spec.arg("-s").arg(serial);
        }

        let process = self.toolchain.runner.spawn_detached(&spec).await?;

        Ok(CommandOutput::json(json!({
            "running": true,
            "pid": process.pid,
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Engine;
    use pls_base::{CommandId, TrustLevel};
    use pls_testkit::{AVDMANAGER_LIST_AVD, FakeProcessRunner, ScriptedProcessRunner};
    use serde_json::Value;

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

    fn engine_with(state: ToolchainState) -> Engine {
        let mut engine = Engine::new();
        register_with(&mut engine, Arc::new(state));
        engine
    }

    /// État injecté : tous les outils "trouvés", sorties factices.
    fn fake_state() -> (ToolchainState, FakeProcessRunner) {
        let runner = FakeProcessRunner::succeeding(AVDMANAGER_LIST_AVD);
        let state = ToolchainState {
            runner: Arc::new(runner.clone()),
            report: EnvironmentReport::default(),
            avdmanager: Some(PathBuf::from("avdmanager")),
            adb: Some(PathBuf::from("adb")),
            emulator: Some(PathBuf::from("emulator")),
            scrcpy: Some(PathBuf::from("scrcpy")),
        };
        (state, runner)
    }

    #[tokio::test]
    async fn sdk_detect_has_the_report_shape() {
        let engine = Engine::new();

        let value = run(&engine, "sdk.detect", serde_json::json!({})).await;
        assert!(value.get("report").is_some());
        assert!(value.get("missing").and_then(Value::as_array).is_some());
        assert!(value.get("complete").and_then(Value::as_bool).is_some());
    }

    #[tokio::test]
    async fn avd_list_parses_avdmanager_output() {
        let (state, runner) = fake_state();
        let engine = engine_with(state);

        let value = run(&engine, "avd.list", serde_json::json!({})).await;
        let avds = value.get("avds").and_then(Value::as_array).expect("avds");
        assert_eq!(avds.len(), 2);
        assert_eq!(
            avds[0].get("name").and_then(Value::as_str),
            Some("Pixel_7_API_34")
        );

        let calls = runner.calls();
        assert_eq!(calls[0].program, "avdmanager");
        assert_eq!(calls[0].args, vec!["list", "avd"]);
    }

    #[tokio::test]
    async fn avd_list_without_avdmanager_is_a_config_error() {
        let state = ToolchainState {
            runner: Arc::new(FakeProcessRunner::succeeding("")),
            report: EnvironmentReport::default(),
            avdmanager: None,
            adb: None,
            emulator: None,
            scrcpy: None,
        };
        let engine = engine_with(state);

        let error = execute_raw(&engine, "avd.list", serde_json::json!({}))
            .await
            .expect_err("tool missing");
        assert_eq!(error.code(), ErrorCode::CONFIG);
    }

    #[tokio::test]
    async fn avd_wait_reports_boot() {
        let runner = ScriptedProcessRunner::stdout_lines(vec!["1\n"]);
        let state = ToolchainState {
            runner: Arc::new(runner.clone()),
            report: EnvironmentReport::default(),
            avdmanager: None,
            adb: Some(PathBuf::from("adb")),
            emulator: None,
            scrcpy: None,
        };
        let engine = engine_with(state);

        let value = run(
            &engine,
            "avd.wait",
            serde_json::json!({ "serial": "emulator-5554", "max_attempts": 2 }),
        )
        .await;
        assert_eq!(value.get("booted").and_then(Value::as_bool), Some(true));
        assert_eq!(
            runner.calls()[0].args,
            vec![
                "-s",
                "emulator-5554",
                "shell",
                "getprop",
                "sys.boot_completed"
            ]
        );
    }

    #[tokio::test]
    async fn scrcpy_spawns_detached_with_serial() {
        let (state, runner) = fake_state();
        let engine = engine_with(state);

        let value = run(
            &engine,
            "emulator.scrcpy",
            serde_json::json!({ "serial": "emulator-5554" }),
        )
        .await;
        assert_eq!(value.get("pid").and_then(Value::as_u64), Some(4242));

        let calls = runner.calls();
        assert_eq!(calls[0].program, "scrcpy");
        assert_eq!(calls[0].args, vec!["-s", "emulator-5554"]);
    }

    #[tokio::test]
    async fn avd_create_rejects_dirty_name_before_execution() {
        let (state, runner) = fake_state();
        let engine = engine_with(state);

        let error = execute_raw(
            &engine,
            "avd.create",
            serde_json::json!({
                "name": "bad name; rm",
                "system_image": "system-images;android-34;google_apis;x86_64",
            }),
        )
        .await
        .expect_err("dirty name");
        assert_eq!(error.code(), ErrorCode::INVALID_PARAMS);
        // Aucun processus lancé : la validation précède l'exécution.
        assert!(runner.calls().is_empty());
    }
}
