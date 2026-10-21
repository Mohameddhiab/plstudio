//! Fakes, fixtures et harnais de test partagés.
//!
//! Ce crate est une aide de test ; il est exclu du contrôle de couches.

#![forbid(unsafe_code)]

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use pls_base::{ProcessOutput, ProcessRunner, ProcessSpec, Result};

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
}

pub const FLUTTER_RUN_MACHINE: &str = r#"{"event":"app.start","params":{"appId":"demo","deviceId":"emulator-5554"}}
{"event":"app.log","params":{"log":"Hello plstudio"}}
"#;

pub const LOGCAT_THREADTIME: &str = "10-21 12:01:00.123  1234  5678 I flutter : Hello\n";

pub const AVDMANAGER_LIST_DEVICE: &str =
    "id: 1 or \"pixel_7\"\n    Name: Pixel 7\n    OEM : Google\n";

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
