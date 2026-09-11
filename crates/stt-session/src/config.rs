use std::path::PathBuf;

use crate::chords::ChordSpec;

/// Hold chord, cancel chord, and model pack directory from the environment.
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
    /// Sole entry boundary. Reads `STT_HOLD` (default `Ctrl+Space`),
    /// `STT_CANCEL` (default `Esc`), `STT_MODEL_DIR` (required).
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
