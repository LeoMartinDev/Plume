use std::path::PathBuf;

use super::chords::ChordSpec;

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
            ConfigError::MissingModelDir => f.write_str("model dir missing: set STT_MODEL_DIR"),
            ConfigError::InvalidChord { var, reason } => {
                write!(f, "{var}: invalid chord: {reason}")
            }
        }
    }
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_parts(
            std::env::var("STT_HOLD").ok(),
            std::env::var("STT_CANCEL").ok(),
            std::env::var_os("STT_MODEL_DIR").map(PathBuf::from),
        )
    }

    fn from_parts(
        hold: Option<String>,
        cancel: Option<String>,
        model_dir: Option<PathBuf>,
    ) -> Result<Self, ConfigError> {
        let model_dir = model_dir
            .filter(|path| !path.as_os_str().is_empty())
            .ok_or(ConfigError::MissingModelDir)?;
        let hold = chord("STT_HOLD", hold.as_deref().unwrap_or("Ctrl+Space"))?;
        let cancel = chord("STT_CANCEL", cancel.as_deref().unwrap_or("Esc"))?;
        Ok(Self {
            hold,
            cancel,
            model_dir,
        })
    }
}

fn chord(var: &'static str, raw: &str) -> Result<ChordSpec, ConfigError> {
    ChordSpec::parse(raw).map_err(|reason| ConfigError::InvalidChord { var, reason })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts(hold: &str, cancel: &str) -> Result<Config, ConfigError> {
        Config::from_parts(
            Some(hold.to_string()),
            Some(cancel.to_string()),
            Some(PathBuf::from("/models/nemotron")),
        )
    }

    #[test]
    fn unset_chords_fall_back_to_hold_and_cancel_defaults() {
        let config = Config::from_parts(None, None, Some(PathBuf::from("/models/nemotron")))
            .expect("defaults must parse");
        assert_eq!(config.hold.as_str(), "Ctrl+Space");
        assert_eq!(config.cancel.as_str(), "Esc");
        assert_eq!(config.model_dir, PathBuf::from("/models/nemotron"));
    }

    #[test]
    fn missing_model_dir_exits_4_like_transcribe() {
        let err = Config::from_parts(None, None, None).unwrap_err();
        assert!(matches!(err, ConfigError::MissingModelDir));
        assert_eq!(err.exit_code(), 4);
        assert_eq!(err.to_string(), "model dir missing: set STT_MODEL_DIR");
    }

    #[test]
    fn bad_chord_names_its_variable_and_exits_2() {
        let err = parts("Ctrl+", "Esc").unwrap_err();
        assert!(matches!(
            err,
            ConfigError::InvalidChord { var: "STT_HOLD", .. }
        ));
        assert_eq!(err.exit_code(), 2);
    }

    #[test]
    fn fn_hold_is_rejected_at_the_config_boundary() {
        let err = parts("Fn", "Esc").unwrap_err();
        assert!(matches!(
            err,
            ConfigError::InvalidChord { var: "STT_HOLD", .. }
        ));
        assert_eq!(err.exit_code(), 2);
    }
}
