//! Détection de l'environnement Android/Flutter.

#![forbid(unsafe_code)]

use std::{
    env,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

/// Résultat de `sdk.detect`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvironmentReport {
    pub java_home: Option<PathBuf>,
    pub java: Option<PathBuf>,
    pub flutter: Option<PathBuf>,
    pub adb: Option<PathBuf>,
    pub node: Option<PathBuf>,
    pub git: Option<PathBuf>,
}

impl EnvironmentReport {
    pub fn missing(&self) -> Vec<&'static str> {
        let mut missing = Vec::new();
        if self.java_home.is_none() && self.java.is_none() {
            missing.push("jdk");
        }
        if self.flutter.is_none() {
            missing.push("flutter");
        }
        if self.adb.is_none() {
            missing.push("adb");
        }
        if self.node.is_none() {
            missing.push("node");
        }
        if self.git.is_none() {
            missing.push("git");
        }
        missing
    }

    pub fn is_complete(&self) -> bool {
        self.missing().is_empty()
    }
}

/// Détecte les outils disponibles, sans lancer de processus.
pub fn detect() -> EnvironmentReport {
    EnvironmentReport {
        java_home: env::var_os("JAVA_HOME").map(PathBuf::from),
        java: find_in_path("java"),
        flutter: find_in_path("flutter"),
        adb: find_in_path("adb"),
        node: find_in_path("node"),
        git: find_in_path("git"),
    }
}

fn find_in_path(program: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;

    for directory in env::split_paths(&path) {
        if let Some(found) = find_in_directory(&directory, program) {
            return Some(found);
        }
    }

    None
}

fn find_in_directory(directory: &Path, program: &str) -> Option<PathBuf> {
    let direct = directory.join(program);
    if direct.is_file() {
        return Some(direct);
    }

    #[cfg(windows)]
    {
        for extension in ["exe", "cmd", "bat"] {
            let candidate = directory.join(format!("{program}.{extension}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_lists_expected_tools() {
        let report = EnvironmentReport::default();
        assert_eq!(report.missing(), ["jdk", "flutter", "adb", "node", "git"]);
        assert!(!report.is_complete());
    }
}
