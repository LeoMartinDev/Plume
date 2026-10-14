use super::{
    AppearancePref, InterfaceLanguage, LanguagePref, Microphone, Prefs, PrefsError, Scheme,
};
use crate::catalog::ModelId;
use crate::history_policy::HistoryPolicy;
use plume_session::{Destination, InsertionMode};
use serde::{Deserialize, Deserializer, Serialize};
use std::path::PathBuf;

struct AppearanceWire(AppearancePref);

struct LanguageWire(LanguagePref);

struct InterfaceLanguageWire(InterfaceLanguage);

impl<'de> Deserialize<'de> for InterfaceLanguageWire {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = toml::Value::deserialize(deserializer)?;
        Ok(Self(
            value
                .as_str()
                .and_then(InterfaceLanguage::from_code)
                .unwrap_or_default(),
        ))
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
    onboarding_version: u32,
    onboarding_step: super::OnboardingStep,
    /// Empty means the hands-free shortcut is disabled.
    toggle: String,
    history: toml::Value,
    hold: String,
    cancel: String,
    model: String,
    appearance: AppearanceWire,
    language: LanguageWire,
    interface_language: InterfaceLanguageWire,
    /// Any unknown value or type means the default: insert into the field.
    destination: toml::Value,
    insertion_mode: String,
    copy_on_failure: bool,
    /// Absent in files written before microphone selection: the system default.
    #[serde(default)]
    microphone: Option<toml::Value>,
}

#[derive(Serialize)]
struct WireOut {
    onboarding_version: u32,
    onboarding_step: super::OnboardingStep,
    toggle: String,
    history: HistoryWireOut,
    hold: String,
    cancel: String,
    model: String,
    appearance: &'static str,
    language: &'static str,
    interface_language: InterfaceLanguage,
    destination: &'static str,
    insertion_mode: &'static str,
    copy_on_failure: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    microphone: Option<MicrophoneWire>,
}

#[derive(Serialize)]
struct MicrophoneWire {
    id: String,
    name: String,
}

pub(super) fn parse_wire(raw: &str) -> Result<(Prefs, Vec<String>), String> {
    let wire: WireIn = toml::from_str(raw).map_err(|err| format!("prefs corrupt: {err}"))?;
    let model = ModelId::parse(&wire.model)
        .ok_or_else(|| format!("prefs model {} is unknown", wire.model))?;
    plume_session::Config::from_prefs(&wire.hold, &wire.cancel, PathBuf::from("/"))
        .map_err(|err| format!("prefs chords rejected: {err}"))?;
    let (history, mut warnings) = parse_history(wire.history);
    let microphone = wire.microphone.and_then(|value| {
        let microphone = parse_microphone(&value);
        if microphone.is_none() {
            warnings.push("microphone must have an id and a name; using the system default".into());
        }
        microphone
    });
    let toggle = if wire.toggle.is_empty() {
        None
    } else {
        plume_session::Config::from_prefs(&wire.hold, &wire.cancel, PathBuf::from("/"))
            .map_err(|e| e.to_string())?
            .with_toggle(Some(&wire.toggle))
            .map_err(|e| e.to_string())?;
        Some(wire.toggle)
    };
    Ok((
        Prefs {
            onboarding_version: wire.onboarding_version,
            onboarding_step: wire.onboarding_step,
            hold: wire.hold,
            cancel: wire.cancel,
            toggle,
            model,
            appearance: wire.appearance.0,
            language: wire.language.0,
            interface_language: wire.interface_language.0,
            destination: match wire.destination.as_str() {
                Some("copy") => Destination::Clipboard,
                _ => Destination::FocusedField,
            },
            insertion_mode: match wire.insertion_mode.as_str() {
                "clipboard" => InsertionMode::Clipboard,
                "typing" => InsertionMode::Typing,
                _ => InsertionMode::Auto,
            },
            copy_on_failure: wire.copy_on_failure,
            microphone,
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
        onboarding_version: prefs.onboarding_version,
        onboarding_step: prefs.onboarding_step,
        toggle: prefs.toggle.clone().unwrap_or_default(),
        hold: prefs.hold.clone(),
        cancel: prefs.cancel.clone(),
        model: prefs.model.as_str().to_string(),
        appearance: prefs.appearance().as_str(),
        language: prefs.language().as_str(),
        interface_language: prefs.interface_language(),
        destination: prefs.destination().as_str(),
        insertion_mode: prefs.insertion_mode().as_str(),
        copy_on_failure: prefs.copy_on_failure(),
        microphone: prefs.microphone().map(|microphone| MicrophoneWire {
            id: microphone.id.clone(),
            name: microphone.name.clone(),
        }),
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

fn parse_microphone(value: &toml::Value) -> Option<Microphone> {
    let field = |key| {
        value
            .get(key)
            .and_then(toml::Value::as_str)
            .filter(|text| !text.is_empty())
            .map(str::to_owned)
    };
    Some(Microphone {
        id: field("id")?,
        name: field("name")?,
    })
}

fn parse_history(value: toml::Value) -> (HistoryPolicy, Vec<String>) {
    let mut warnings = Vec::new();
    let table = match value {
        toml::Value::Table(table) => table,
        _ => {
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

    #[test]
    fn interface_and_dictation_languages_round_trip_independently() {
        for interface in InterfaceLanguage::available() {
            for dictation in [
                LanguagePref::Auto,
                LanguagePref::French,
                LanguagePref::English,
            ] {
                let mut prefs = Prefs::default_fresh();
                prefs.set_interface_language(interface);
                prefs.set_language(dictation);
                let (loaded, warnings) = parse_wire(&encode(&prefs).unwrap()).unwrap();
                assert_eq!(loaded, prefs);
                assert!(warnings.is_empty());
            }
        }
    }

    fn fields_with(edit: impl FnOnce(&mut toml::Table)) -> String {
        let mut value: toml::Value = encode(&Prefs::default_fresh()).unwrap().parse().unwrap();
        edit(value.as_table_mut().unwrap());
        toml::to_string(&value).unwrap()
    }
    fn prefs_without_history() -> String {
        fields_with(|table| {
            table.remove("history");
        })
    }
    #[test]
    fn invalid_interface_language_keeps_other_preferences() {
        let mut prefs = Prefs::default_fresh();
        prefs.set_language(LanguagePref::French);
        prefs.model = ModelId::WhisperSmall;
        let encoded = encode(&prefs).unwrap();
        for value in ["'zz-unavailable'", "42", "false"] {
            let raw = encoded
                .lines()
                .map(|line| {
                    if line.starts_with("interface_language") {
                        format!("interface_language = {value}")
                    } else {
                        line.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join(
                    "
",
                );
            let (loaded, _) = parse_wire(&raw).unwrap();
            assert_eq!(loaded.interface_language(), InterfaceLanguage::ENGLISH);
            assert_eq!(loaded.language(), LanguagePref::French);
            assert_eq!(loaded.model, ModelId::WhisperSmall);
        }
    }
    #[test]
    fn missing_fields_are_rejected() {
        for field in ["toggle", "history", "destination", "onboarding_version"] {
            let raw = fields_with(|table| {
                table.remove(field);
            });
            assert!(parse_wire(&raw).is_err(), "{field}");
        }
    }
    #[test]
    fn missing_microphone_follows_the_system_default() {
        let raw = fields_with(|table| {
            table.remove("microphone");
        });
        let (prefs, warnings) = parse_wire(&raw).unwrap();
        assert_eq!(prefs.microphone(), None);
        assert!(warnings.is_empty());
    }
    #[test]
    fn invalid_microphone_warns_and_keeps_other_preferences() {
        for invalid in [
            "\"usb\"",
            "42",
            "{ id = \"wasapi:x\" }",
            "{ id = \"\", name = \"Mic\" }",
            "{ id = 1, name = \"Mic\" }",
        ] {
            let raw = format!(
                "microphone = {invalid}
{}",
                prefs_without_history()
            ) + "
[history]
retention_days = 7
";
            let (prefs, warnings) = parse_wire(&raw).unwrap();
            assert_eq!(prefs.microphone(), None, "{invalid}");
            assert_eq!(prefs.history().retention_days(), Some(7), "{invalid}");
            assert_eq!(warnings.len(), 1, "{invalid}");
        }
    }
    #[test]
    fn disabled_toggle_round_trips() {
        let mut prefs = Prefs::default_fresh();
        prefs.try_set_toggle(None).unwrap();
        let (loaded, warnings) = parse_wire(&encode(&prefs).unwrap()).unwrap();
        assert_eq!(loaded.toggle(), None);
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
                prefs_without_history()
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
                prefs_without_history()
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
    fn malformed_history_table_warns_and_missing_limits_use_defaults() {
        let (prefs, warnings) =
            parse_wire(&format!("{}\nhistory = false", prefs_without_history())).unwrap();
        assert_eq!(prefs.history(), HistoryPolicy::default());
        assert_eq!(warnings.len(), 1);
        let (prefs, warnings) = parse_wire(&format!(
            "{}\n[history]\nretention_days = 7",
            prefs_without_history()
        ))
        .unwrap();
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
