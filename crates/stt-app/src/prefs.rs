mod storage;
mod wire;
pub(crate) use storage::save_at;
pub use storage::{load, load_at, prefs_path, save};

use std::path::PathBuf;

use crate::catalog::ModelId;
use crate::history_policy::HistoryPolicy;
use stt_session::InsertionMode;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Scheme {
    Light,
    Dark,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppearancePref {
    Fixed(Scheme),
    Auto,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LanguagePref {
    Auto,
    French,
    English,
}

impl LanguagePref {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::French => "fr",
            Self::English => "en",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "Automatic",
            Self::French => "Français",
            Self::English => "English",
        }
    }
}

impl AppearancePref {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fixed(Scheme::Light) => "light",
            Self::Fixed(Scheme::Dark) => "dark",
            Self::Auto => "auto",
        }
    }

    pub fn element_id(self) -> &'static str {
        match self {
            Self::Fixed(Scheme::Light) => "appearance-light",
            Self::Fixed(Scheme::Dark) => "appearance-dark",
            Self::Auto => "appearance-auto",
        }
    }

    pub fn resolve(self, os: Scheme) -> Scheme {
        match self {
            Self::Fixed(scheme) => scheme,
            Self::Auto => os,
        }
    }
}

pub const DEFAULT_HOLD: &str = stt_session::Config::DEFAULT_HOLD;

/// Domain prefs. Wire TOML stays private.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prefs {
    hold: String,
    cancel: String,
    pub model: ModelId,
    appearance: AppearancePref,
    language: LanguagePref,
    insertion_mode: InsertionMode,
    copy_on_failure: bool,
    history: HistoryPolicy,
}

impl Prefs {
    pub fn default_fresh() -> Self {
        Prefs {
            hold: DEFAULT_HOLD.to_string(),
            cancel: stt_session::Config::DEFAULT_CANCEL.to_string(),
            model: ModelId::default(),
            appearance: AppearancePref::Auto,
            language: LanguagePref::Auto,
            insertion_mode: InsertionMode::Auto,
            copy_on_failure: true,
            history: HistoryPolicy::default(),
        }
    }

    pub fn history(&self) -> HistoryPolicy {
        self.history
    }

    pub fn set_history(&mut self, policy: HistoryPolicy) {
        self.history = policy;
    }

    pub fn hold(&self) -> &str {
        &self.hold
    }

    pub fn cancel(&self) -> &str {
        &self.cancel
    }

    pub fn appearance(&self) -> AppearancePref {
        self.appearance
    }

    pub fn set_appearance(&mut self, pref: AppearancePref) {
        self.appearance = pref;
    }

    pub fn language(&self) -> LanguagePref {
        self.language
    }

    pub fn set_language(&mut self, language: LanguagePref) {
        self.language = language;
    }

    pub fn insertion_mode(&self) -> InsertionMode {
        self.insertion_mode
    }

    pub fn set_insertion_mode(&mut self, mode: InsertionMode) {
        self.insertion_mode = mode;
    }

    pub fn copy_on_failure(&self) -> bool {
        self.copy_on_failure
    }

    pub fn set_copy_on_failure(&mut self, enabled: bool) {
        self.copy_on_failure = enabled;
    }

    pub fn try_set_hold(&mut self, hold: &str) -> Result<(), stt_session::ConfigError> {
        stt_session::Config::from_prefs(hold, self.cancel(), PathBuf::from("/"))?;
        if self.hold != hold {
            self.hold = hold.to_string();
        }
        Ok(())
    }
}

/// Loading is total. Corrupt files never read as valid prefs.
pub enum PrefsLoad {
    Fresh(Prefs),
    Loaded(Prefs),
    LoadedWithWarnings { prefs: Prefs, warnings: Vec<String> },
    Quarantined { prefs: Prefs, warning: String },
}

#[derive(Debug)]
pub enum PrefsError {
    Io(std::io::Error),
    Encode(String),
}

impl std::fmt::Display for PrefsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PrefsError::Io(err) => write!(f, "prefs io: {err}"),
            PrefsError::Encode(err) => write!(f, "prefs encode: {err}"),
        }
    }
}

impl std::error::Error for PrefsError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_prefs(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "stt-app-prefs-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("prefs.toml")
    }

    #[test]
    fn missing_file_is_fresh() {
        let path = temp_prefs("missing");
        match load_at(&path) {
            PrefsLoad::Fresh(prefs) => {
                assert_eq!(prefs.hold(), "Ctrl+Space");
                assert_eq!(prefs.model, ModelId::Nemotron35Compact);
                assert_eq!(prefs.appearance(), AppearancePref::Auto);
            }
            _ => panic!("missing file must be Fresh"),
        }
    }

    #[test]
    fn round_trip_save_load() {
        let path = temp_prefs("round");
        let prefs = Prefs::default_fresh();
        save_at(&path, &prefs).unwrap();
        match load_at(&path) {
            PrefsLoad::Loaded(loaded) => assert_eq!(loaded, prefs),
            _ => panic!("saved prefs must load"),
        }
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn named_model_round_trips() {
        let path = temp_prefs("named-model");
        let mut prefs = Prefs::default_fresh();
        prefs.model = ModelId::WhisperSmall;
        save_at(&path, &prefs).unwrap();
        match load_at(&path) {
            PrefsLoad::Loaded(loaded) => assert_eq!(loaded.model, ModelId::WhisperSmall),
            _ => panic!("saved named model must load"),
        }
        let body = std::fs::read_to_string(&path).unwrap();
        assert!(body.contains("model = \"whisper-small\""), "{body}");
        assert!(!body.contains("pack ="), "{body}");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn corrupt_file_is_quarantined() {
        let path = temp_prefs("bad");
        std::fs::write(&path, "this is not toml {{{").unwrap();
        match load_at(&path) {
            PrefsLoad::Quarantined { warning, .. } => {
                assert!(warning.contains("corrupt"), "{warning}");
                assert!(!path.exists());
                assert!(path.with_extension("toml.bad").exists());
            }
            _ => panic!("corrupt file must quarantine"),
        }
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn fn_chord_is_quarantined() {
        let path = temp_prefs("fn");
        std::fs::write(
            &path,
            "hold = \"Fn+Space\"\ncancel = \"Esc\"\npack = \"light\"\n",
        )
        .unwrap();
        match load_at(&path) {
            PrefsLoad::Quarantined { warning, .. } => {
                assert!(warning.contains("chords"), "{warning}");
            }
            _ => panic!("Fn hold must quarantine"),
        }
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn missing_appearance_key_is_loaded_auto() {
        let path = temp_prefs("no-appearance");
        std::fs::write(
            &path,
            "hold = \"Ctrl+Space\"\ncancel = \"Esc\"\npack = \"light\"\n",
        )
        .unwrap();
        match load_at(&path) {
            PrefsLoad::Loaded(prefs) => {
                assert_eq!(prefs.appearance(), AppearancePref::Auto);
                assert_eq!(prefs.language(), LanguagePref::Auto);
                assert_eq!(prefs.model, ModelId::Nemotron35Compact);
                assert_eq!(prefs.hold(), "Ctrl+Space");
            }
            _ => panic!("file without appearance must be Loaded"),
        }
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn language_round_trips_and_old_files_default_to_auto() {
        let path = temp_prefs("language-round");
        let mut prefs = Prefs::default_fresh();
        prefs.set_language(LanguagePref::French);
        save_at(&path, &prefs).unwrap();
        match load_at(&path) {
            PrefsLoad::Loaded(loaded) => {
                assert_eq!(loaded.language(), LanguagePref::French);
            }
            _ => panic!("saved language must load"),
        }
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn insertion_settings_round_trip() {
        let path = temp_prefs("insertion-round");
        let mut prefs = Prefs::default_fresh();
        prefs.set_insertion_mode(InsertionMode::Typing);
        prefs.set_copy_on_failure(false);
        save_at(&path, &prefs).unwrap();
        match load_at(&path) {
            PrefsLoad::Loaded(loaded) => {
                assert_eq!(loaded.insertion_mode(), InsertionMode::Typing);
                assert!(!loaded.copy_on_failure());
            }
            _ => panic!("saved insertion settings must load"),
        }
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn neon_appearance_is_loaded_auto_and_does_not_quarantine() {
        let path = temp_prefs("neon");
        std::fs::write(
            &path,
            "hold = \"Ctrl+Space\"\ncancel = \"Esc\"\npack = \"light\"\nappearance = \"neon\"\n",
        )
        .unwrap();
        match load_at(&path) {
            PrefsLoad::Loaded(prefs) => {
                assert_eq!(prefs.appearance(), AppearancePref::Auto);
                assert_eq!(prefs.hold(), "Ctrl+Space");
            }
            _ => panic!("unknown appearance must be Loaded"),
        }
        assert!(!path.with_extension("toml.bad").exists());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn integer_appearance_is_loaded_auto_and_does_not_quarantine() {
        let path = temp_prefs("int-appearance");
        std::fs::write(
            &path,
            "hold = \"Ctrl+Space\"\ncancel = \"Esc\"\npack = \"light\"\nappearance = 1\n",
        )
        .unwrap();
        match load_at(&path) {
            PrefsLoad::Loaded(prefs) => {
                assert_eq!(prefs.appearance(), AppearancePref::Auto);
                assert_eq!(prefs.hold(), "Ctrl+Space");
            }
            _ => panic!("integer appearance must be Loaded"),
        }
        assert!(!path.with_extension("toml.bad").exists());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn appearance_light_is_not_pack_light() {
        let path = temp_prefs("both-light");
        std::fs::write(
            &path,
            "hold = \"Ctrl+Space\"\ncancel = \"Esc\"\npack = \"light\"\nappearance = \"light\"\n",
        )
        .unwrap();
        match load_at(&path) {
            PrefsLoad::Loaded(prefs) => {
                assert_eq!(prefs.appearance(), AppearancePref::Fixed(Scheme::Light));
                assert_eq!(prefs.model, ModelId::Nemotron35Compact);
                assert_ne!(prefs.appearance().element_id(), prefs.model.as_str());
            }
            _ => panic!("appearance light must be Loaded"),
        }
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn save_dark_then_auto_round_trips() {
        let path = temp_prefs("appearance-round");
        let mut prefs = Prefs::default_fresh();
        prefs.set_appearance(AppearancePref::Fixed(Scheme::Dark));
        save_at(&path, &prefs).unwrap();
        match load_at(&path) {
            PrefsLoad::Loaded(loaded) => {
                assert_eq!(loaded.appearance(), AppearancePref::Fixed(Scheme::Dark));
            }
            _ => panic!("saved Dark must load"),
        }
        prefs.set_appearance(AppearancePref::Auto);
        save_at(&path, &prefs).unwrap();
        match load_at(&path) {
            PrefsLoad::Loaded(loaded) => {
                assert_eq!(loaded.appearance(), AppearancePref::Auto);
            }
            _ => panic!("saved Auto must load"),
        }
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn fixed_resolve_ignores_os_auto_takes_os() {
        assert_eq!(
            AppearancePref::Fixed(Scheme::Light).resolve(Scheme::Dark),
            Scheme::Light
        );
        assert_eq!(AppearancePref::Auto.resolve(Scheme::Dark), Scheme::Dark);
    }

    #[test]
    fn try_set_hold_accepts_a_valid_chord() {
        let mut prefs = Prefs::default_fresh();
        prefs.try_set_hold("Alt+a").unwrap();
        assert_eq!(prefs.hold(), "Alt+a");
    }

    #[test]
    fn try_set_hold_same_string_is_ok() {
        let mut prefs = Prefs::default_fresh();
        prefs.try_set_hold("Ctrl+Space").unwrap();
        assert_eq!(prefs.hold(), "Ctrl+Space");
    }

    #[test]
    fn try_set_hold_rejects_empty_and_keeps_the_old_hold() {
        let mut prefs = Prefs::default_fresh();
        let err = prefs.try_set_hold("").unwrap_err();
        assert!(
            err.to_string().contains("hold is not a usable chord"),
            "{err}"
        );
        assert_eq!(prefs.hold(), "Ctrl+Space");
    }

    #[test]
    fn try_set_hold_rejects_fn_and_keeps_the_old_hold() {
        let mut prefs = Prefs::default_fresh();
        let err = prefs.try_set_hold("Fn+Space").unwrap_err();
        assert!(
            err.to_string().contains("hold is not a usable chord"),
            "{err}"
        );
        assert!(err.to_string().contains("Fn"), "{err}");
        assert_eq!(prefs.hold(), "Ctrl+Space");
    }
}
