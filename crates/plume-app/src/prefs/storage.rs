use super::{wire, Prefs, PrefsError, PrefsLoad};
use std::path::{Path, PathBuf};

pub fn prefs_path() -> PathBuf {
    crate::dirs::AppDirs::resolve().prefs_path()
}

pub fn load() -> PrefsLoad {
    load_at(&prefs_path())
}

pub fn save(prefs: &Prefs) -> Result<(), PrefsError> {
    save_at(&prefs_path(), prefs)
}

/// Load preferences from an explicit file, using the same validation as startup.
pub fn load_at(path: &Path) -> PrefsLoad {
    match std::fs::read_to_string(path) {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            PrefsLoad::Fresh(Prefs::default_fresh())
        }
        Err(err) => quarantine(path, format!("prefs unreadable: {err}")),
        Ok(raw) => match wire::parse_wire(&raw) {
            Ok((prefs, warnings)) if warnings.is_empty() => PrefsLoad::Loaded(prefs),
            Ok((prefs, warnings)) => PrefsLoad::LoadedWithWarnings { prefs, warnings },
            Err(warning) => quarantine(path, warning),
        },
    }
}

pub(crate) fn save_at(path: &Path, prefs: &Prefs) -> Result<(), PrefsError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(PrefsError::Io)?;
    }
    let body = wire::encode(prefs)?;
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, body).map_err(PrefsError::Io)?;
    crate::file_store::replace_file(&tmp, path).map_err(PrefsError::Io)?;
    Ok(())
}

fn quarantine(path: &Path, warning: String) -> PrefsLoad {
    let bad = unique_bad_path(path);
    if path.exists() {
        let _ = std::fs::rename(path, &bad);
    }
    PrefsLoad::Quarantined {
        prefs: Prefs::default_fresh(),
        warning,
    }
}

fn unique_bad_path(path: &Path) -> PathBuf {
    let primary = path.with_extension("toml.bad");
    if !primary.exists() {
        return primary;
    }
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    path.with_extension(format!("toml.bad.{secs}"))
}
