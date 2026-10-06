use super::{AppearancePref, LanguagePref, Prefs, PrefsError, Scheme};
use crate::catalog::ModelId;
use crate::history_policy::HistoryPolicy;
use plume_session::InsertionMode;
use serde::{Deserialize, Deserializer, Serialize};
use std::path::PathBuf;

struct AppearanceWire(AppearancePref);

impl Default for AppearanceWire {
    fn default() -> Self {
        Self(AppearancePref::Auto)
    }
}

struct LanguageWire(LanguagePref);

impl Default for LanguageWire {
    fn default() -> Self {
        Self(LanguagePref::Auto)
    }
}

impl<'de> Deserialize<'de> for LanguageWire {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = toml::Value::deserialize(deserializer)?;
        Ok(Self(match value {
            toml::Value::String(raw) => match raw.as_str() {
                "fr" => LanguagePref::French,
                "en" => LanguagePref::English,
                _ => LanguagePref::Auto,
            },
            _ => LanguagePref::Auto,
        }))
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
    #[serde(default)]
    history: Option<toml::Value>,
    hold: String,
    cancel: String,
    #[serde(alias = "pack")]
    model: String,
    #[serde(default)]
    appearance: AppearanceWire,
    #[serde(default)]
    language: LanguageWire,
    #[serde(default = "default_insertion_mode")]
    insertion_mode: String,
    #[serde(default = "default_copy_on_failure")]
    copy_on_failure: bool,
}

#[derive(Serialize)]
struct WireOut {
    history: HistoryWireOut,
    hold: String,
    cancel: String,
    model: String,
    appearance: &'static str,
    language: &'static str,
    insertion_mode: &'static str,
    copy_on_failure: bool,
}

fn default_insertion_mode() -> String {
    "auto".to_string()
}

fn default_copy_on_failure() -> bool {
    true
}

pub(super) fn parse_wire(raw: &str) -> Result<(Prefs, Vec<String>), String> {
    let wire: WireIn = toml::from_str(raw).map_err(|err| format!("prefs corrupt: {err}"))?;
    let model = ModelId::parse(&wire.model)
        .ok_or_else(|| format!("prefs model {} is unknown", wire.model))?;
    plume_session::Config::from_prefs(&wire.hold, &wire.cancel, PathBuf::from("/"))
        .map_err(|err| format!("prefs chords rejected: {err}"))?;
    let (history, warnings) = parse_history(wire.history);
    Ok((
        Prefs {
            hold: wire.hold,
            cancel: wire.cancel,
            model,
            appearance: wire.appearance.0,
            language: wire.language.0,
            insertion_mode: match wire.insertion_mode.as_str() {
                "clipboard" => InsertionMode::Clipboard,
                "typing" => InsertionMode::Typing,
                _ => InsertionMode::Auto,
            },
            copy_on_failure: wire.copy_on_failure,
            history,
        },
        warnings,
    ))
}

#[derive(Serialize)]
#[serde(untagged)]
enum HistoryLimitWire {
    Limited(u64),
    Unlimited(&'static str),
}
impl HistoryLimitWire {
    fn from_limit(limit: Option<u64>) -> Self {
        match limit {
            Some(value) => Self::Limited(value),
            None => Self::Unlimited("unlimited"),
        }
    }
}

#[derive(Serialize)]
struct HistoryWireOut {
    retention_days: HistoryLimitWire,
    max_entries: HistoryLimitWire,
}

pub(super) fn encode(prefs: &Prefs) -> Result<String, PrefsError> {
    let wire = WireOut {
        hold: prefs.hold.clone(),
        cancel: prefs.cancel.clone(),
        model: prefs.model.as_str().to_string(),
        appearance: prefs.appearance().as_str(),
        language: prefs.language().as_str(),
        insertion_mode: prefs.insertion_mode().as_str(),
        copy_on_failure: prefs.copy_on_failure(),
        history: HistoryWireOut {
            retention_days: HistoryLimitWire::from_limit(
                prefs.history().retention_days().map(u64::from),
            ),
            max_entries: HistoryLimitWire::from_limit(
                prefs.history().max_entries().map(|entries| entries as u64),
            ),
        },
    };
    toml::to_string_pretty(&wire).map_err(|err| PrefsError::Encode(err.to_string()))
}

fn parse_history(value: Option<toml::Value>) -> (HistoryPolicy, Vec<String>) {
    let mut warnings = Vec::new();
    let table = match value {
        None => return (HistoryPolicy::default(), warnings),
        Some(toml::Value::Table(table)) => table,
        Some(_) => {
            warnings.push("history must be a table; using default limits".into());
            return (HistoryPolicy::default(), warnings);
        }
    };
    let mut read_limit = |key: &str, default: u64, maximum: u64| -> Option<u64> {
        match table.get(key) {
            None => Some(default),
            Some(toml::Value::String(value)) if value == "unlimited" => None,
            Some(value) => match value
                .as_integer()
                .and_then(|number| u64::try_from(number).ok())
            {
                Some(number) if (1..=maximum).contains(&number) => Some(number),
                _ => {
                    warnings.push(format!(
                        "history.{key} must be between 1 and {maximum}, or unlimited; using {default}"
                    ));
                    Some(default)
                }
            },
        }
    };
    let days = read_limit(
        "retention_days",
        u64::from(HistoryPolicy::DEFAULT_RETENTION_DAYS),
        u64::from(HistoryPolicy::MAX_RETENTION_DAYS),
    );
    let entries = read_limit(
        "max_entries",
        HistoryPolicy::DEFAULT_MAX_ENTRIES as u64,
        HistoryPolicy::MAX_ENTRIES as u64,
    );
    (
        HistoryPolicy::new(
            days.map(|days| days as u32),
            entries.map(|entries| entries as usize),
        )
        .expect("validated history limits"),
        warnings,
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::prefs::DEFAULT_HOLD;
    fn old_prefs() -> String {
        let mut value: toml::Value = encode(&Prefs::default_fresh()).unwrap().parse().unwrap();
        value.as_table_mut().unwrap().remove("history");
        toml::to_string(&value).unwrap()
    }
    #[test]
    fn old_preferences_keep_default_history_without_warnings() {
        let (prefs, warnings) = parse_wire(&old_prefs()).unwrap();
        assert_eq!(prefs.history(), HistoryPolicy::default());
        assert!(warnings.is_empty());
    }
    #[test]
    fn history_round_trips_custom_limits() {
        let mut prefs = Prefs::default_fresh();
        prefs.set_history(HistoryPolicy::new(Some(42), Some(1234)).unwrap());
        let (loaded, warnings) = parse_wire(&encode(&prefs).unwrap()).unwrap();
        assert_eq!(loaded, prefs);
        assert!(warnings.is_empty());
    }
    #[test]
    fn invalid_fields_default_independently_and_keep_other_preferences() {
        for invalid in [
            "0",
            "-1",
            "3651",
            "99999999999",
            "\"bad\"",
            "true",
            "1.5",
            "[]",
        ] {
            let raw = format!(
                "{}\n[history]\nretention_days = {invalid}\nmax_entries = 1234\n",
                old_prefs()
            );
            let (prefs, warnings) = parse_wire(&raw).unwrap();
            assert_eq!(
                prefs.history(),
                HistoryPolicy::new(Some(30), Some(1234)).unwrap()
            );
            assert_eq!(prefs.hold(), DEFAULT_HOLD);
            assert_eq!(warnings.len(), 1);
        }
        for invalid in ["0", "-1", "100001", "\"bad\"", "false", "1.5", "{}"] {
            let raw = format!(
                "{}\n[history]\nretention_days = 42\nmax_entries = {invalid}\n",
                old_prefs()
            );
            let (prefs, warnings) = parse_wire(&raw).unwrap();
            assert_eq!(
                prefs.history(),
                HistoryPolicy::new(Some(42), Some(500)).unwrap()
            );
            assert_eq!(warnings.len(), 1);
        }
    }
    #[test]
    fn malformed_history_table_warns_and_missing_fields_use_defaults() {
        let (prefs, warnings) = parse_wire(&format!("{}\nhistory = false", old_prefs())).unwrap();
        assert_eq!(prefs.history(), HistoryPolicy::default());
        assert_eq!(warnings.len(), 1);
        let (prefs, warnings) =
            parse_wire(&format!("{}\n[history]\nretention_days = 7", old_prefs())).unwrap();
        assert_eq!(
            prefs.history(),
            HistoryPolicy::new(Some(7), Some(500)).unwrap()
        );
        assert!(warnings.is_empty());
    }
    #[test]
    fn unlimited_limits_round_trip_independently() {
        for (days, entries) in [(None, None), (None, Some(500)), (Some(30), None)] {
            let mut prefs = Prefs::default_fresh();
            prefs.set_history(HistoryPolicy::new(days, entries).unwrap());
            let encoded = encode(&prefs).unwrap();
            assert!(encoded.contains("unlimited"));
            let (loaded, warnings) = parse_wire(&encoded).unwrap();
            assert_eq!(loaded, prefs);
            assert!(warnings.is_empty());
        }
    }
}
