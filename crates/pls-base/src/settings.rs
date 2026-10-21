use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use serde::{Serialize, de::DeserializeOwned};

use crate::error::{ErrorCode, PlsError, Result};

/// Stockage JSON atomique.
#[derive(Debug, Clone)]
pub struct SettingsStore {
    path: PathBuf,
}

impl SettingsStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load<T: DeserializeOwned>(&self) -> Result<Option<T>> {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };

        serde_json::from_slice(&bytes).map(Some).map_err(|error| {
            PlsError::config(format!(
                "invalid settings file {}: {error}",
                self.path.display()
            ))
        })
    }

    pub fn save<T: Serialize>(&self, value: &T) -> Result<()> {
        let parent = self
            .path
            .parent()
            .ok_or_else(|| PlsError::config("settings path has no parent directory"))?;
        fs::create_dir_all(parent)?;

        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        serde_json::to_writer_pretty(&mut temporary, value)
            .map_err(|error| PlsError::config(format!("cannot serialize settings: {error}")))?;
        temporary.write_all(b"\n")?;
        temporary.as_file().sync_all()?;
        temporary
            .persist(&self.path)
            .map_err(|error| PlsError::Io {
                code: ErrorCode::IO,
                source: error.error,
            })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
    struct TestSettings {
        enabled: bool,
    }

    #[test]
    fn round_trips_settings() {
        let directory = tempfile::tempdir().expect("temp dir");
        let store = SettingsStore::new(directory.path().join("settings.json"));

        assert_eq!(store.load::<TestSettings>().expect("load missing"), None);

        store
            .save(&TestSettings { enabled: true })
            .expect("save settings");

        assert_eq!(
            store.load::<TestSettings>().expect("load settings"),
            Some(TestSettings { enabled: true })
        );
    }
}
