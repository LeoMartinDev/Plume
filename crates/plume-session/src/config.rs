use std::path::{Path, PathBuf};

use crate::chords::ChordSpec;

/// Hold chord, cancel chord, and model pack directory from the environment.
#[derive(Debug, Clone)]
pub struct Config {
    pub(crate) hold: ChordSpec,
    pub(crate) cancel: ChordSpec,
    pub(crate) toggle: Option<ChordSpec>,
    pub(crate) model_dir: PathBuf,
}

#[derive(Debug)]
pub enum ConfigError {
    MissingModelDir,
    InvalidChord { var: &'static str, reason: String },
}

impl Config {
    pub const DEFAULT_HOLD: &'static str = "Ctrl+Space";
    pub const DEFAULT_TOGGLE: &'static str = "Ctrl+Shift+Space";
    pub const DEFAULT_CANCEL: &'static str = "Esc";
    /// Env boundary for the plume-session binary. Reads `PLUME_HOLD` (default
    /// `Ctrl+Space`), `PLUME_CANCEL` (default `Esc`), `PLUME_MODEL_DIR` (required).
    /// Rejects `Fn` on every OS: X11 refuses it, Windows swallows it.
    pub fn from_env() -> Result<Self, ConfigError> {
        let hold = chord_from_env("PLUME_HOLD", Self::DEFAULT_HOLD)?;
        let cancel = chord_from_env("PLUME_CANCEL", Self::DEFAULT_CANCEL)?;
        let model_dir = std::env::var_os("PLUME_MODEL_DIR")
            .map(PathBuf::from)
            .ok_or(ConfigError::MissingModelDir)?;
        let config = Config {
            hold,
            cancel,
            toggle: None,
            model_dir,
        };
        plume_hotkey::validate_bindings(&config.bindings()).map_err(|e| {
            ConfigError::InvalidChord {
                var: "shortcuts",
                reason: e.to_string(),
            }
        })?;
        let requested = std::env::var_os("PLUME_TOGGLE")
            .map(|raw| {
                raw.into_string().map_err(|_| ConfigError::InvalidChord {
                    var: "PLUME_TOGGLE",
                    reason: "value is not valid unicode".into(),
                })
            })
            .transpose()?;
        if let Some(raw) = requested {
            config.with_toggle(if raw.is_empty() { None } else { Some(&raw) })
        } else {
            match config.clone().with_toggle(Some(Self::DEFAULT_TOGGLE)) {
                Ok(config) => Ok(config),
                Err(_) => {
                    tracing::warn!(
                        "hands-free shortcut disabled: default overlaps an existing shortcut"
                    );
                    Ok(config)
                }
            }
        }
    }

    /// Boundary used by plume-app after it reads prefs.toml.
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
        let config = Config {
            hold,
            cancel,
            toggle: None,
            model_dir,
        };
        plume_hotkey::validate_bindings(&config.bindings()).map_err(|e| {
            ConfigError::InvalidChord {
                var: "shortcuts",
                reason: e.to_string(),
            }
        })?;
        Ok(config
            .clone()
            .with_toggle(Some(Self::DEFAULT_TOGGLE))
            .unwrap_or(config))
    }

    pub fn with_toggle(mut self, raw: Option<&str>) -> Result<Self, ConfigError> {
        self.toggle =
            raw.map(ChordSpec::parse)
                .transpose()
                .map_err(|reason| ConfigError::InvalidChord {
                    var: "toggle",
                    reason,
                })?;
        plume_hotkey::validate_bindings(&self.bindings()).map_err(|e| {
            ConfigError::InvalidChord {
                var: "shortcuts",
                reason: e.to_string(),
            }
        })?;
        Ok(self)
    }
    pub(crate) fn bindings(&self) -> Vec<plume_hotkey::HotkeyBinding> {
        use plume_hotkey::{HotkeyAction, HotkeyBinding};
        let mut bindings = vec![
            HotkeyBinding {
                action: HotkeyAction::Hold,
                shortcut: self.hold.as_str().into(),
            },
            HotkeyBinding {
                action: HotkeyAction::Cancel,
                shortcut: self.cancel.as_str().into(),
            },
        ];
        if let Some(toggle) = &self.toggle {
            bindings.push(HotkeyBinding {
                action: HotkeyAction::Toggle,
                shortcut: toggle.as_str().into(),
            });
        }
        bindings
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
            ConfigError::MissingModelDir => write!(f, "model dir missing: set PLUME_MODEL_DIR"),
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
