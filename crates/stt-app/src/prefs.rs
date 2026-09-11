use std::path::{Path, PathBuf};

use serde::{Deserialize, Deserializer, Serialize};

/// Light is pinned. Medium and Large stay visible until a snapshot opens.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackId {
    Light,
    Medium,
    Large,
}

impl PackId {
    pub fn as_str(self) -> &'static str {
        match self {
            PackId::Light => "light",
            PackId::Medium => "medium",
            PackId::Large => "large",
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "light" => Some(PackId::Light),
            "medium" => Some(PackId::Medium),
            "large" => Some(PackId::Large),
            _ => None,
        }
    }

    pub fn data_dir(self) -> PathBuf {
        crate::dirs::AppDirs::resolve().pack_dir(self)
    }
}

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

/// Domain prefs. Wire TOML stays private.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prefs {
    hold: String,
    cancel: String,
    pub pack: PackId,
    appearance: AppearancePref,
}

impl Prefs {
    pub fn default_fresh() -> Self {
        Prefs {
            hold: "Ctrl+Space".to_string(),
            cancel: "Esc".to_string(),
            pack: PackId::Light,
            appearance: AppearancePref::Auto,
        }
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
}

/// Loading is total. Corrupt files never read as valid prefs.
pub enum PrefsLoad {
    Fresh(Prefs),
    Loaded(Prefs),
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

struct AppearanceWire(AppearancePref);

impl Default for AppearanceWire {
    fn default() -> Self {
        Self(AppearancePref::Auto)
    }
}

impl<'de> Deserialize<'de> for AppearanceWire {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = toml::Value::deserialize(deserializer)?;
        Ok(Self(match value {
            toml::Value::String(raw) => match raw.as_str() {
                "light" => AppearancePref::Fixed(Scheme::Light),
                "dark" => AppearancePref::Fixed(Scheme::Dark),
                _ => AppearancePref::Auto,
            },
            _ => AppearancePref::Auto,
        }))
    }
}

#[derive(Deserialize)]
struct WireIn {
    hold: String,
    cancel: String,
    pack: String,
    #[serde(default)]
    appearance: AppearanceWire,
}

#[derive(Serialize)]
struct WireOut {
    hold: String,
    cancel: String,
    pack: String,
    appearance: &'static str,
}

pub fn prefs_path() -> PathBuf {
    crate::dirs::AppDirs::resolve().prefs_path()
}

pub fn load() -> PrefsLoad {
    load_at(&prefs_path())
}

pub fn save(prefs: &Prefs) -> Result<(), PrefsError> {
    save_at(&prefs_path(), prefs)
}

pub(crate) fn load_at(path: &Path) -> PrefsLoad {
    match std::fs::read_to_string(path) {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            PrefsLoad::Fresh(Prefs::default_fresh())
        }
        Err(err) => quarantine(path, format!("prefs unreadable: {err}")),
        Ok(raw) => match parse_wire(&raw) {
            Ok(prefs) => PrefsLoad::Loaded(prefs),
            Err(warning) => quarantine(path, warning),
        },
    }
}

pub(crate) fn save_at(path: &Path, prefs: &Prefs) -> Result<(), PrefsError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(PrefsError::Io)?;
    }
    let wire = WireOut {
        hold: prefs.hold.clone(),
        cancel: prefs.cancel.clone(),
        pack: prefs.pack.as_str().to_string(),
        appearance: prefs.appearance().as_str(),
    };
    let body = toml::to_string_pretty(&wire).map_err(|err| PrefsError::Encode(err.to_string()))?;
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, body).map_err(PrefsError::Io)?;
    std::fs::rename(&tmp, path).map_err(PrefsError::Io)?;
    Ok(())
}

fn parse_wire(raw: &str) -> Result<Prefs, String> {
    let wire: WireIn = toml::from_str(raw).map_err(|err| format!("prefs corrupt: {err}"))?;
    let pack = PackId::parse(&wire.pack)
        .ok_or_else(|| format!("prefs pack {} is not light, medium, or large", wire.pack))?;
    stt_session::Config::from_prefs(&wire.hold, &wire.cancel, PathBuf::from("/"))
        .map_err(|err| format!("prefs chords rejected: {err}"))?;
    Ok(Prefs {
        hold: wire.hold,
        cancel: wire.cancel,
        pack,
        appearance: wire.appearance.0,
    })
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
                assert_eq!(prefs.pack, PackId::Light);
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
                assert_eq!(prefs.pack, PackId::Light);
                assert_eq!(prefs.hold(), "Ctrl+Space");
            }
            _ => panic!("file without appearance must be Loaded"),
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
                assert_eq!(prefs.pack, PackId::Light);
                assert_ne!(prefs.appearance().element_id(), prefs.pack.as_str());
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
}
