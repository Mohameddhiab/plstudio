//! Modèle et contrôle des AVD Android.
//!
//! Toute exécution passe par `ProcessRunner` (remplaçable par
//! `pls-testkit` dans les tests). Les fonctions analytiques
//! (`parse_avd_list`, `parse_adb_devices`) sont pures et testées sur les
//! fixtures de `pls-testkit`.

#![forbid(unsafe_code)]

use std::{path::Path, time::Duration};

use pls_base::{
    Cancellation, DetachedProcess, ErrorCode, PlsError, ProcessOutput, ProcessRunner, ProcessSpec,
    Result,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AvdStatus {
    Stopped,
    Starting,
    Running,
    Stopping,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AvdSummary {
    pub name: String,
    pub device: String,
    pub api: u32,
    pub target: String,
    pub status: AvdStatus,
}

impl AvdSummary {
    pub fn is_running(&self) -> bool {
        self.status == AvdStatus::Running
    }
}

/// AVD photographe demandé : les champs sont validés avant exécution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvdCreateRequest {
    pub name: String,
    pub device: Option<String>,
    pub system_image: String,
}

/// Appareil vu par `adb devices`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceRecord {
    pub serial: String,
    pub state: String,
}

/// Liste les AVD déclarés par `avdmanager list avd`.
pub async fn list_avds(
    runner: &dyn ProcessRunner,
    avdmanager: &Path,
    cancellation: &Cancellation,
) -> Result<Vec<AvdSummary>> {
    if cancellation.is_cancelled() {
        return Err(cancelled());
    }

    let spec = ProcessSpec::new(avdmanager.to_string_lossy())
        .arg("list")
        .arg("avd");
    let output = runner.run(&spec).await?;
    ensure_success(avdmanager, &output)?;

    Ok(parse_avd_list(&output.stdout))
}

/// Crée un AVD (`avdmanager create avd`), paramètres validés.
pub async fn create_avd(
    runner: &dyn ProcessRunner,
    avdmanager: &Path,
    request: &AvdCreateRequest,
    cancellation: &Cancellation,
) -> Result<()> {
    if cancellation.is_cancelled() {
        return Err(cancelled());
    }

    request.validate()?;

    let mut spec = ProcessSpec::new(avdmanager.to_string_lossy())
        .arg("create")
        .arg("avd")
        .arg("-n")
        .arg(&request.name);
    if let Some(device) = &request.device {
        spec = spec.arg("-d").arg(device);
    }
    spec = spec.arg("-k").arg(&request.system_image);

    let output = runner.run(&spec).await?;
    ensure_success(avdmanager, &output)
}

/// Lance l'émulateur, détaché de plstudio.
pub async fn start_emulator(
    runner: &dyn ProcessRunner,
    emulator: &Path,
    name: &str,
    cancellation: &Cancellation,
) -> Result<DetachedProcess> {
    if cancellation.is_cancelled() {
        return Err(cancelled());
    }

    validate_identifier(name, "avd name")?;

    let spec = ProcessSpec::new(emulator.to_string_lossy())
        .arg("-avd")
        .arg(name);
    runner.spawn_detached(&spec).await
}

/// Appareils visibles par `adb devices`.
pub async fn list_devices(
    runner: &dyn ProcessRunner,
    adb: &Path,
    cancellation: &Cancellation,
) -> Result<Vec<DeviceRecord>> {
    if cancellation.is_cancelled() {
        return Err(cancelled());
    }

    let spec = ProcessSpec::new(adb.to_string_lossy()).arg("devices");
    let output = runner.run(&spec).await?;
    ensure_success(adb, &output)?;

    Ok(parse_adb_devices(&output.stdout))
}

/// Attend que l'appareil soit démarré (`sys.boot_completed == 1`).
///
/// Sonde toutes les secondes, borne par `max_attempts` ; respecte le
/// drapeau d'annulation entre chaque tentative.
pub async fn wait_for_boot(
    runner: &dyn ProcessRunner,
    adb: &Path,
    serial: Option<&str>,
    max_attempts: u32,
    cancellation: &Cancellation,
) -> Result<()> {
    if let Some(serial) = serial {
        validate_identifier(serial, "device serial")?;
    }

    for attempt in 0..max_attempts {
        if cancellation.is_cancelled() {
            return Err(cancelled());
        }

        let mut spec = ProcessSpec::new(adb.to_string_lossy());
        if let Some(serial) = serial {
            spec = spec.arg("-s").arg(serial);
        }
        spec = spec.arg("shell").arg("getprop").arg("sys.boot_completed");

        let output = runner.run(&spec).await?;
        if output.status == Some(0) && output.stdout.trim() == "1" {
            return Ok(());
        }

        if attempt + 1 < max_attempts {
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }

    Err(PlsError::app(
        ErrorCode::IO,
        format!(
            "device not booted after {max_attempts} attempts ({})",
            serial.unwrap_or("all devices")
        ),
    )
    .with_remediation("check the emulator state with `avd.devices`, or retry"))
}

/// Capture `logcat -d -v threadtime` (instantané, pas de flux).
pub async fn logcat_dump(
    runner: &dyn ProcessRunner,
    adb: &Path,
    serial: Option<&str>,
    cancellation: &Cancellation,
) -> Result<String> {
    if cancellation.is_cancelled() {
        return Err(cancelled());
    }

    if let Some(serial) = serial {
        validate_identifier(serial, "device serial")?;
    }

    let mut spec = ProcessSpec::new(adb.to_string_lossy());
    if let Some(serial) = serial {
        spec = spec.arg("-s").arg(serial);
    }
    spec = spec.arg("logcat").arg("-d").arg("-v").arg("threadtime");

    let output = runner.run(&spec).await?;
    ensure_success(adb, &output)?;

    Ok(output.stdout)
}

impl AvdCreateRequest {
    fn validate(&self) -> Result<()> {
        validate_identifier(&self.name, "avd name")?;
        if let Some(device) = &self.device {
            validate_identifier(device, "device profile")?;
        }
        validate_image(&self.system_image)?;

        Ok(())
    }
}

/// Identifiants passés en argument : bornés et sans caractère suspect.
///
/// Les arguments sont poussés sans shell, mais des identifiants bornés
/// évitent déjà les incohérences côté SDK et rendent le diagnostic limpide.
fn validate_identifier(value: &str, what: &str) -> Result<()> {
    if value.is_empty() {
        return Err(PlsError::app(
            ErrorCode::INVALID_PARAMS,
            format!("empty {what}"),
        ));
    }

    if value.len() > 128 {
        return Err(PlsError::app(
            ErrorCode::INVALID_PARAMS,
            format!(
                "{what} is too long ({len} chars, max 128)",
                len = value.len()
            ),
        ));
    }

    let ok = value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | ':'));
    if !ok {
        return Err(PlsError::app(
            ErrorCode::INVALID_PARAMS,
            format!("invalid character in {what}: allowed are letters, digits, . _ - :"),
        )
        .with_remediation("rename the target (spaces, `;`, `/` are not valid identifiers)"));
    }

    Ok(())
}

/// Image système : séparateurs `;` et `/` autorisés en plus des identifiants.
fn validate_image(value: &str) -> Result<()> {
    if value.is_empty() {
        return Err(PlsError::app(
            ErrorCode::INVALID_PARAMS,
            "empty system image",
        ));
    }

    if value.len() > 256 {
        return Err(PlsError::app(
            ErrorCode::INVALID_PARAMS,
            format!(
                "system image is too long ({len} chars, max 256)",
                len = value.len()
            ),
        ));
    }

    let ok = value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | ':' | ';' | '/'));
    if !ok {
        return Err(PlsError::app(
            ErrorCode::INVALID_PARAMS,
            "invalid character in system image: allowed are letters, digits, . _ - : ; /",
        )
        .with_remediation(
            "use the sdkmanager notation, e.g. system-images;android-34;google_apis;x86_64",
        ));
    }

    Ok(())
}

/// Parse `avdmanager list avd` : blocs séparés par `-----`.
pub fn parse_avd_list(stdout: &str) -> Vec<AvdSummary> {
    let mut avds = Vec::new();
    let mut current: PartialAvd = PartialAvd::default();

    let flush = |current: &mut PartialAvd, avds: &mut Vec<AvdSummary>| {
        if !current.name.is_empty() {
            avds.push(AvdSummary {
                name: std::mem::take(&mut current.name),
                device: std::mem::take(&mut current.device),
                api: current.api.unwrap_or_default(),
                target: std::mem::take(&mut current.target),
                status: AvdStatus::Stopped,
            });
        }
    };

    for line in stdout.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("---------") {
            flush(&mut current, &mut avds);
            continue;
        }

        if let Some((key, value)) = trimmed.split_once(':') {
            let value = value.trim();
            match key.trim() {
                "Name" => {
                    flush(&mut current, &mut avds);
                    current.name = value.to_string();
                }
                "Device" => current.device = value.to_string(),
                "Target" => {
                    current.target = value.to_string();
                    current.api = target_api(value);
                }
                _ => {}
            }
        }
    }

    flush(&mut current, &mut avds);
    avds
}

#[derive(Debug, Default)]
struct PartialAvd {
    name: String,
    device: String,
    target: String,
    api: Option<u32>,
}

fn target_api(target: &str) -> Option<u32> {
    target.strip_prefix("android-")?.parse().ok()
}

/// Parse `adb devices` : séries séparées par tabulation après l'en-tête.
pub fn parse_adb_devices(stdout: &str) -> Vec<DeviceRecord> {
    let mut devices = Vec::new();
    let mut past_header = false;

    for line in stdout.lines() {
        if line.contains("List of devices attached") {
            past_header = true;
            continue;
        }
        if !past_header {
            continue;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('*') {
            continue;
        }

        if let Some((serial, state)) = trimmed.split_once('\t') {
            devices.push(DeviceRecord {
                serial: serial.trim().to_string(),
                state: state.trim().to_string(),
            });
        } else {
            devices.push(DeviceRecord {
                serial: trimmed.to_string(),
                state: String::new(),
            });
        }
    }

    devices
}

fn ensure_success(program: &Path, output: &ProcessOutput) -> Result<()> {
    if output.success() {
        return Ok(());
    }

    let stderr_tail = output
        .stderr
        .lines()
        .filter(|line| !line.trim().is_empty())
        .rev()
        .take(3)
        .collect::<Vec<_>>()
        .join(" | ");

    let suffix = if stderr_tail.is_empty() {
        String::new()
    } else {
        format!(": {stderr_tail}")
    };

    Err(PlsError::app(
        ErrorCode::IO,
        format!(
            "{} exited with status {}{}",
            program.display(),
            output
                .status
                .map(|code| code.to_string())
                .unwrap_or_else(|| "signal".to_string()),
            suffix
        ),
    )
    .with_remediation("rerun the command manually to inspect the toolchain output"))
}

fn cancelled() -> PlsError {
    PlsError::app(ErrorCode::CANCELLED, "cancelled while working on the avd")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use pls_base::{Cancellation, ErrorCode, ProcessOutput};
    use pls_testkit::{ADB_DEVICES, AVDMANAGER_LIST_AVD, FakeProcessRunner, ScriptedProcessRunner};

    use super::*;

    fn tools() -> (PathBuf, PathBuf, PathBuf) {
        (
            PathBuf::from("adb"),
            PathBuf::from("avdmanager"),
            PathBuf::from("emulator"),
        )
    }

    fn cancelled_flag(cancelled: bool) -> Cancellation {
        let cancellation = Cancellation::new();
        if cancelled {
            cancellation.cancel();
        }
        cancellation
    }

    #[test]
    fn parses_the_avdmanager_fixture() {
        let avds = parse_avd_list(AVDMANAGER_LIST_AVD);

        assert_eq!(avds.len(), 2);
        assert_eq!(avds[0].name, "Pixel_7_API_34");
        assert_eq!(avds[0].device, "pixel_7");
        assert_eq!(avds[0].target, "android-34");
        assert_eq!(avds[0].api, 34);
        assert_eq!(avds[0].status, AvdStatus::Stopped);
        assert_eq!(avds[1].name, "Small_Empty");
        assert_eq!(avds[1].api, 33);
    }

    #[test]
    fn parsing_yields_nothing_on_malformed_output() {
        assert!(parse_avd_list("").is_empty());
        assert!(parse_avd_list("No Android Virtual Devices found\n").is_empty());
    }

    #[test]
    fn parses_the_adb_devices_fixture() {
        let devices = parse_adb_devices(ADB_DEVICES);

        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].serial, "emulator-5554");
        assert_eq!(devices[0].state, "device");
        assert_eq!(devices[1].state, "offline");
    }

    #[tokio::test]
    async fn lists_avds_through_the_runner() {
        let runner = FakeProcessRunner::succeeding(AVDMANAGER_LIST_AVD);
        let (_, avdmanager, _) = tools();

        let avds = list_avds(&runner, &avdmanager, &cancelled_flag(false))
            .await
            .expect("list");
        assert_eq!(avds.len(), 2);

        let calls = runner.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].args, vec!["list", "avd"]);
    }

    #[tokio::test]
    async fn list_avds_surfaces_the_tool_stderr() {
        let runner = FakeProcessRunner::new(ProcessOutput {
            status: Some(2),
            stdout: String::new(),
            stderr: "PANIC: subtype error\n".to_string(),
        });
        let (_, avdmanager, _) = tools();

        let error = list_avds(&runner, &avdmanager, &cancelled_flag(false))
            .await
            .unwrap_err();
        assert_eq!(error.code(), ErrorCode::IO);
        assert!(error.to_string().contains("subtype error"));
    }

    #[tokio::test]
    async fn create_avd_passes_validated_args() {
        let runner = FakeProcessRunner::succeeding("");
        let (_, avdmanager, _) = tools();
        let request = AvdCreateRequest {
            name: "pixel_7_test".to_string(),
            device: Some("pixel_7".to_string()),
            system_image: "system-images;android-34;google_apis;x86_64".to_string(),
        };

        create_avd(&runner, &avdmanager, &request, &cancelled_flag(false))
            .await
            .expect("create");

        let calls = runner.calls();
        assert_eq!(
            calls[0].args,
            vec![
                "create",
                "avd",
                "-n",
                "pixel_7_test",
                "-d",
                "pixel_7",
                "-k",
                "system-images;android-34;google_apis;x86_64",
            ]
        );
    }

    #[tokio::test]
    async fn create_avd_rejects_dirty_identifiers() {
        let runner = FakeProcessRunner::succeeding("");
        let (_, avdmanager, _) = tools();
        let request = AvdCreateRequest {
            name: "bad name; rm".to_string(),
            device: None,
            system_image: "system-images;android-34;google_apis;x86_64".to_string(),
        };

        let error = create_avd(&runner, &avdmanager, &request, &cancelled_flag(false))
            .await
            .unwrap_err();
        assert_eq!(error.code(), ErrorCode::INVALID_PARAMS);
        // Rien n'a tourné : la validation est avant exécution.
        assert!(runner.calls().is_empty());
    }

    #[tokio::test]
    async fn start_emulator_spawns_detached() {
        let runner = FakeProcessRunner::default();
        let (_, _, emulator) = tools();

        let process = start_emulator(&runner, &emulator, "Pixel_7_API_34", &cancelled_flag(false))
            .await
            .expect("spawn");
        assert_eq!(process.pid, 4242);

        let calls = runner.calls();
        assert_eq!(calls[0].program, "emulator");
        assert_eq!(calls[0].args, vec!["-avd", "Pixel_7_API_34"]);
    }

    #[tokio::test]
    async fn list_devices_through_the_runner() {
        let runner = FakeProcessRunner::succeeding(ADB_DEVICES);
        let (adb, _, _) = tools();

        let devices = list_devices(&runner, &adb, &cancelled_flag(false))
            .await
            .expect("devices");
        assert_eq!(devices.len(), 2);
    }

    #[tokio::test]
    async fn wait_for_boot_succeeds_on_second_probe() {
        let runner = ScriptedProcessRunner::stdout_lines(vec!["0\n", "1\n"]);
        let (adb, _, _) = tools();

        wait_for_boot(
            &runner,
            &adb,
            Some("emulator-5554"),
            3,
            &cancelled_flag(false),
        )
        .await
        .expect("booted");
        assert_eq!(runner.calls().len(), 2);
    }

    #[tokio::test]
    async fn wait_for_boot_times_out_then_reports() {
        let runner = ScriptedProcessRunner::stdout_lines(vec!["0\n"]);
        let (adb, _, _) = tools();

        let error = wait_for_boot(&runner, &adb, None, 2, &cancelled_flag(false))
            .await
            .unwrap_err();
        assert_eq!(error.code(), ErrorCode::IO);
        assert!(error.to_string().contains("after 2 attempts"));
    }

    #[tokio::test]
    async fn wait_for_boot_honours_cancellation() {
        let runner = ScriptedProcessRunner::stdout_lines(vec!["0\n"]);
        let (adb, _, _) = tools();

        let error = wait_for_boot(&runner, &adb, None, 100, &cancelled_flag(true))
            .await
            .unwrap_err();
        assert_eq!(error.code(), ErrorCode::CANCELLED);
        assert!(runner.calls().is_empty());
    }

    #[tokio::test]
    async fn logcat_dump_returns_the_capture() {
        let runner = FakeProcessRunner::succeeding("10-23 10:00:00.000 1 1 I tag : boot ok\n");
        let (adb, _, _) = tools();

        let dump = logcat_dump(&runner, &adb, Some("emulator-5554"), &cancelled_flag(false))
            .await
            .expect("dump");
        assert!(dump.contains("boot ok"));

        let spec = &runner.calls()[0];
        assert_eq!(spec.args[0], "-s");
        assert_eq!(
            &spec.args[spec.args.len() - 4..],
            ["logcat", "-d", "-v", "threadtime"]
        );
    }
}
