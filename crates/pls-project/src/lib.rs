//! Détection et modèle de projet Flutter.

#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlutterProject {
    pub root: PathBuf,
    pub pubspec: PathBuf,
}

impl FlutterProject {
    /// Détecte un projet Flutter à partir de son dossier racine.
    pub fn detect(root: impl AsRef<Path>) -> Option<Self> {
        let root = root.as_ref();
        let pubspec = root.join("pubspec.yaml");

        if pubspec.is_file() {
            Some(Self {
                root: root.to_path_buf(),
                pubspec,
            })
        } else {
            None
        }
    }

    pub fn has_dart_entrypoint(&self) -> bool {
        self.root.join("lib").join("main.dart").is_file()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_a_pubspec() {
        let directory = tempfile::tempdir().expect("temp dir");
        std::fs::write(directory.path().join("pubspec.yaml"), "name: demo\n").expect("pubspec");
        let project = FlutterProject::detect(directory.path()).expect("flutter project");
        assert_eq!(project.root, directory.path());
        assert!(!project.has_dart_entrypoint());
    }

    #[test]
    fn ignores_a_non_flutter_directory() {
        let directory = tempfile::tempdir().expect("temp dir");
        assert!(FlutterProject::detect(directory.path()).is_none());
    }
}
