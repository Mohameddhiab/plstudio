use std::{
    fs,
    path::{Path, PathBuf},
};

use directories::ProjectDirs;

use crate::error::{PlsError, Result};

/// Chemins standard de plstudio.
#[derive(Debug, Clone)]
pub struct PlsPaths {
    config: PathBuf,
    data: PathBuf,
    logs: PathBuf,
}

impl PlsPaths {
    /// Détecte les chemins selon l'OS.
    ///
    /// Windows : `%APPDATA%\plstudio`, `%LOCALAPPDATA%\plstudio`.
    /// Linux : `~/.config/plstudio`, `~/.local/share/plstudio`.
    pub fn discover() -> Result<Self> {
        let dirs = ProjectDirs::from("dev", "", "plstudio")
            .ok_or_else(|| PlsError::config("cannot determine the user configuration directory"))?;

        Ok(Self {
            config: dirs.config_dir().to_path_buf(),
            data: dirs.data_dir().to_path_buf(),
            logs: dirs.data_local_dir().join("logs"),
        })
    }

    pub fn from_roots(config: impl Into<PathBuf>, data: impl Into<PathBuf>) -> Self {
        let data = data.into();
        Self {
            config: config.into(),
            logs: data.join("logs"),
            data,
        }
    }

    pub fn config_dir(&self) -> &Path {
        &self.config
    }

    pub fn data_dir(&self) -> &Path {
        &self.data
    }

    pub fn log_dir(&self) -> &Path {
        &self.logs
    }

    pub fn ensure(&self) -> Result<()> {
        for directory in [&self.config, &self.data, &self.logs] {
            fs::create_dir_all(directory)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_all_directories() {
        let root = tempfile::tempdir().expect("temp dir");
        let paths = PlsPaths::from_roots(root.path().join("config"), root.path().join("data"));
        paths.ensure().expect("ensure paths");
        assert!(paths.config_dir().is_dir());
        assert!(paths.data_dir().is_dir());
        assert!(paths.log_dir().is_dir());
    }
}
