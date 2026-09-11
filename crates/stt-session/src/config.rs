use std::path::{Path, PathBuf};

use crate::chords::ChordSpec;

/// Hold chord, cancel chord, and model pack directory from the environment.
#[derive(Debug)]
pub struct Config {
    pub(crate) hold: ChordSpec,
    pub(crate) cancel: ChordSpec,
    pub(crate) model_dir: PathBuf,
}

#[derive(Debug)]
pub enum ConfigError {
    MissingModelDir,
    InvalidChord { var: &'static str, reason: String },
}

impl Config {
    /// Env boundary for the stt-session binary. Reads `STT_HOLD` (default
    /// `Ctrl+Space`), `STT_CANCEL` (default `Esc`), `STT_MODEL_DIR` (required).
    /// Rejects `Fn` on every OS: X11 refuses it, Windows swallows it.
    pub fn from_env() -> Result<Self, ConfigError> {
        let hold = chord_from_env("STT_HOLD", "Ctrl+Space")?;
        let cancel = chord_from_env("STT_CANCEL", "Esc")?;
        let model_dir = std::env::var_os("STT_MODEL_DIR")
            .map(PathBuf::from)
            .ok_or(ConfigError::MissingModelDir)?;
        Ok(Config {
            hold,
            cancel,
            model_dir,
        })
    }

    /// Boundary used by stt-app after it reads prefs.toml.
    /// `hold` and `cancel` go through ChordSpec::parse, which still rejects Fn.
    /// `model_dir` is a PackId dest, not a user-typed path.
    pub fn from_prefs(hold: &str, cancel: &str, model_dir: PathBuf) -> Result<Self, ConfigError> {
        let hold = ChordSpec::parse(hold).map_err(|reason| ConfigError::InvalidChord {
            var: "hold",
            reason,
        })?;
        let cancel = ChordSpec::parse(cancel).map_err(|reason| ConfigError::InvalidChord {
            var: "cancel",
            reason,
        })?;
        Ok(Config {
            hold,
            cancel,
            model_dir,
        })
    }

    pub fn model_dir(&self) -> &Path {
        &self.model_dir
    }
}

fn chord_from_env(var: &'static str, default: &str) -> Result<ChordSpec, ConfigError> {
    let raw = match std::env::var_os(var) {
        None => default.to_string(),
        Some(value) => value.into_string().map_err(|_| ConfigError::InvalidChord {
            var,
            reason: "value is not valid unicode".to_string(),
        })?,
    };
    ChordSpec::parse(&raw).map_err(|reason| ConfigError::InvalidChord { var, reason })
}

impl ConfigError {
    pub fn exit_code(&self) -> i32 {
        match self {
            ConfigError::MissingModelDir => 4,
            ConfigError::InvalidChord { .. } => 2,
        }
    }
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::MissingModelDir => write!(f, "model dir missing: set STT_MODEL_DIR"),
            ConfigError::InvalidChord { var, reason } => {
                write!(f, "{var} is not a usable chord: {reason}")
            }
        }
    }
}

impl std::error::Error for ConfigError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_prefs_accepts_defaults() {
        let config = Config::from_prefs("Ctrl+Space", "Esc", PathBuf::from("/tmp/pack")).unwrap();
        assert_eq!(config.model_dir(), Path::new("/tmp/pack"));
        assert_eq!(config.hold.as_str(), "Ctrl+Space");
        assert_eq!(config.cancel.as_str(), "Esc");
    }

    #[test]
    fn from_prefs_rejects_fn() {
        let err = Config::from_prefs("Fn+Space", "Esc", PathBuf::from("/tmp/pack")).unwrap_err();
        match err {
            ConfigError::InvalidChord { var, reason } => {
                assert_eq!(var, "hold");
                assert!(reason.contains("Fn"), "{reason}");
            }
            other => panic!("expected InvalidChord, got {other}"),
        }
    }
}
