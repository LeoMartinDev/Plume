use std::path::{Path, PathBuf};

use crate::catalog::ModelId;

/// Product folder under the OS config and local-data roots.
/// Same leaf name Tauri would append as the bundle identifier.
const APP_ID: &str = "stt";

/// OS app directories. Matches Tauri 2 `app_config_dir` and `app_local_data_dir`.
/// Packs live in local data so Windows does not roam hundreds of megabytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppDirs {
    config: PathBuf,
    local_data: PathBuf,
}

impl AppDirs {
    pub fn resolve() -> Self {
        let config = dirs::config_dir()
            .unwrap_or_else(|| fallback_home().join(".config"))
            .join(APP_ID);
        let local_data = dirs::data_local_dir()
            .unwrap_or_else(|| fallback_home().join(".local/share"))
            .join(APP_ID);
        Self { config, local_data }
    }

    pub fn from_roots(config: impl Into<PathBuf>, local_data: impl Into<PathBuf>) -> Self {
        Self {
            config: config.into(),
            local_data: local_data.into(),
        }
    }

    pub fn prefs_path(&self) -> PathBuf {
        self.config.join("prefs.toml")
    }

    pub fn model_dir(&self, id: ModelId) -> PathBuf {
        let leaf = match id {
            // Keep existing installations; only the user-facing tier name disappears.
            ModelId::Nemotron35Compact => "light",
            _ => id.as_str(),
        };
        self.local_data.join("packs").join(leaf)
    }

    pub fn lock_path(&self) -> PathBuf {
        self.local_data.join("lock")
    }

    pub fn history_path(&self) -> PathBuf {
        self.local_data.join("history.json")
    }

    pub fn config(&self) -> &Path {
        &self.config
    }

    pub fn local_data(&self) -> &Path {
        &self.local_data
    }
}

fn fallback_home() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roots_join_prefs_pack_and_lock() {
        let dirs = AppDirs::from_roots("/cfg", "/data");
        assert_eq!(dirs.prefs_path(), PathBuf::from("/cfg/prefs.toml"));
        assert_eq!(
            dirs.model_dir(ModelId::Nemotron35Compact),
            PathBuf::from("/data/packs/light")
        );
        assert_eq!(dirs.lock_path(), PathBuf::from("/data/lock"));
        assert_eq!(dirs.history_path(), PathBuf::from("/data/history.json"));
    }

    #[test]
    fn resolve_matches_dirs_crate() {
        let got = AppDirs::resolve();
        let config = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from(".").join(".config"))
            .join("stt");
        let local_data = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from(".").join(".local/share"))
            .join("stt");
        assert_eq!(got.prefs_path(), config.join("prefs.toml"));
        assert_eq!(got.lock_path(), local_data.join("lock"));
        assert_eq!(
            got.model_dir(ModelId::Nemotron35Compact),
            local_data.join("packs/light")
        );
    }
}
