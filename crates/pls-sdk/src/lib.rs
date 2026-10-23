//! Détection de l'environnement Android/Flutter.
//!
//! Localisation des toolchains sans lancer de processus : variables
//! d'environnement (`ANDROID_HOME`, `ANDROID_SDK_ROOT`, `FLUTTER_ROOT`,
//! `JAVA_HOME`), `PATH`, puis défauts Windows connus. Le résultat est un
//! rapport sérialisable qui pilote les commandes `sdk.*` et l'onboarding.

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
    pub android_sdk_dir: Option<PathBuf>,
    pub adb: Option<PathBuf>,
    pub avdmanager: Option<PathBuf>,
    pub emulator: Option<PathBuf>,
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
        if self.android_sdk_dir.is_none() {
            missing.push("android-sdk");
        }
        if self.adb.is_none() {
            missing.push("adb");
        }
        if self.avdmanager.is_none() {
            missing.push("avdmanager");
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
    let flutter = env::var_os("FLUTTER_ROOT")
        .map(PathBuf::from)
        .and_then(|root| flutter_binary(&root))
        .or_else(|| find_in_path("flutter"));

    let android_sdk_dir = detect_android_sdk();
    let (adb, avdmanager, emulator) = match &android_sdk_dir {
        Some(sdk) => (
            find_tool_in_sdk(sdk, SdkTool::Adb).or_else(|| find_in_path("adb")),
            find_tool_in_sdk(sdk, SdkTool::Avdmanager),
            find_tool_in_sdk(sdk, SdkTool::Emulator),
        ),
        None => (find_in_path("adb"), None, None),
    };

    EnvironmentReport {
        java_home: env::var_os("JAVA_HOME").map(PathBuf::from),
        java: find_in_path("java"),
        flutter,
        android_sdk_dir,
        adb,
        avdmanager,
        emulator,
        node: find_in_path("node"),
        git: find_in_path("git"),
    }
}

/// Outils d'un SDK Android, avec leur chemin canonique dans l'arborescence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SdkTool {
    /// `platform-tools/adb`
    Adb,
    /// `cmdline-tools/latest/bin/avdmanager`
    Avdmanager,
    /// `cmdline-tools/latest/bin/sdkmanager`
    Sdkmanager,
    /// `emulator/emulator`
    Emulator,
}

/// Localise un outil d'un SDK Android connu.
pub fn find_tool_in_sdk(sdk: &Path, tool: SdkTool) -> Option<PathBuf> {
    let (relative, binary) = match tool {
        SdkTool::Adb => ("platform-tools", "adb"),
        SdkTool::Avdmanager => ("cmdline-tools", "avdmanager"),
        SdkTool::Sdkmanager => ("cmdline-tools", "sdkmanager"),
        SdkTool::Emulator => ("emulator", "emulator"),
    };

    // `cmdline-tools/latest/bin` est canonique ; quelques installs
    // placent les outils directement dans `cmdline-tools/bin`.
    let directories: Vec<PathBuf> = match tool {
        SdkTool::Avdmanager | SdkTool::Sdkmanager => vec![
            sdk.join(relative).join("latest").join("bin"),
            sdk.join(relative).join("bin"),
        ],
        _ => vec![sdk.join(relative)],
    };

    let names = executable_names(binary);
    directories.iter().find_map(|directory| {
        names
            .iter()
            .map(|name| directory.join(name))
            .find(|candidate| candidate.is_file())
    })
}

fn executable_names(binary: &str) -> Vec<String> {
    #[cfg(windows)]
    {
        vec![format!("{binary}.exe"), format!("{binary}.bat")]
    }

    #[cfg(not(windows))]
    {
        vec![binary.to_string()]
    }
}

/// Racine du SDK Android : env, puis défaut Windows, sinon rien.
pub fn detect_android_sdk() -> Option<PathBuf> {
    if let Some(candidate) = env_from_any(&["ANDROID_HOME", "ANDROID_SDK_ROOT"])
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
    {
        return Some(candidate);
    }

    #[cfg(windows)]
    {
        let local_appdata = env::var_os("LOCALAPPDATA").map(PathBuf::from);
        if let Some(default) = local_appdata.map(|base| base.join("Android").join("Sdk")) {
            if default.is_dir() {
                return Some(default);
            }
        }
        None
    }

    #[cfg(not(windows))]
    {
        None
    }
}

fn env_from_any(variables: &[&str]) -> Option<std::ffi::OsString> {
    variables
        .iter()
        .find_map(|name| env::var_os(name).filter(|value| !value.is_empty()))
}

/// Cherche un exécutable du `PATH`.
pub fn find_in_path(program: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;

    for directory in env::split_paths(&path) {
        if let Some(found) = find_in_directory(&directory, program) {
            return Some(found);
        }
    }

    None
}

fn flutter_binary(root: &Path) -> Option<PathBuf> {
    let directory = root.join("bin");
    let bat = directory.join("flutter.bat");
    let sh = directory.join("flutter");

    match (bat.is_file(), sh.is_file()) {
        (true, _) => Some(bat),
        (_, true) => Some(sh),
        (false, false) => None,
    }
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

    fn seed_fake_sdk(root: &Path) {
        std::fs::create_dir_all(root.join("platform-tools")).unwrap();
        std::fs::create_dir_all(root.join("cmdline-tools/latest/bin")).unwrap();
        std::fs::create_dir_all(root.join("emulator")).unwrap();

        #[cfg(windows)]
        {
            std::fs::write(root.join("platform-tools/adb.exe"), "").unwrap();
            std::fs::write(root.join("cmdline-tools/latest/bin/avdmanager.bat"), "").unwrap();
            std::fs::write(root.join("cmdline-tools/latest/bin/sdkmanager.bat"), "").unwrap();
            std::fs::write(root.join("emulator/emulator.exe"), "").unwrap();
        }

        #[cfg(not(windows))]
        {
            std::fs::write(root.join("platform-tools/adb"), "").unwrap();
            std::fs::write(root.join("cmdline-tools/latest/bin/avdmanager"), "").unwrap();
            std::fs::write(root.join("cmdline-tools/latest/bin/sdkmanager"), "").unwrap();
            std::fs::write(root.join("emulator/emulator"), "").unwrap();
        }
    }

    #[test]
    fn locates_all_tools_in_a_sdk_tree() {
        let root = tempfile::tempdir().unwrap();
        seed_fake_sdk(root.path());

        for tool in [
            SdkTool::Adb,
            SdkTool::Avdmanager,
            SdkTool::Sdkmanager,
            SdkTool::Emulator,
        ] {
            let found = find_tool_in_sdk(root.path(), tool).expect("tool located");
            assert!(found.is_file());
        }
    }

    #[test]
    fn missing_tool_is_none() {
        let root = tempfile::tempdir().unwrap();
        seed_fake_sdk(root.path());
        let _ = std::fs::remove_file(find_tool_in_sdk(root.path(), SdkTool::Emulator).unwrap());

        assert!(find_tool_in_sdk(root.path(), SdkTool::Emulator).is_none());
    }

    #[test]
    fn missing_lists_expected_tools() {
        let report = EnvironmentReport::default();
        assert_eq!(
            report.missing(),
            [
                "jdk",
                "flutter",
                "android-sdk",
                "adb",
                "avdmanager",
                "node",
                "git"
            ]
        );
        assert!(!report.is_complete());
    }
}
