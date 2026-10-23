//! Fakes, fixtures et harnais de test partagés.
//!
//! Ce crate est une aide de test ; il est exclu du contrôle de couches.

#![forbid(unsafe_code)]

use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use pls_base::{DetachedProcess, ProcessOutput, ProcessRunner, ProcessSpec, Result};

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
}

/// Runner scripté : renvoie les sorties dans l'ordre, puis répète la dernière.
///
/// Pratique pour les sondes répétées (attente de boot Android).
#[derive(Debug, Clone)]
pub struct ScriptedProcessRunner {
    calls: Arc<Mutex<Vec<ProcessSpec>>>,
    outputs: Arc<Mutex<Vec<ProcessOutput>>>,
}

impl ScriptedProcessRunner {
    pub fn new(outputs: Vec<ProcessOutput>) -> Self {
        Self {
            calls: Arc::new(Mutex::new(Vec::new())),
            outputs: Arc::new(Mutex::new(outputs)),
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

    pub fn calls(&self) -> Vec<ProcessSpec> {
        match self.calls.lock() {
            Ok(calls) => calls.clone(),
            Err(_) => Vec::new(),
        }
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

        let mut outputs = self.outputs.lock().unwrap_or_else(PoisonError::into_inner);
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
}

pub const FLUTTER_RUN_MACHINE: &str = r#"{"event":"app.start","params":{"appId":"demo","deviceId":"emulator-5554"}}
{"event":"app.log","params":{"log":"Hello plstudio"}}
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

    #[test]
    fn fake_records_calls_and_returns_output() {
        let runner = FakeProcessRunner::succeeding("ok");
        let spec = ProcessSpec::new("adb").arg("devices");

        let output = futures_lite::future::block_on(runner.run(&spec)).expect("run");
        assert!(output.success());
        assert_eq!(runner.calls().len(), 1);
    }
}
