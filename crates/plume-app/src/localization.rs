use std::collections::BTreeMap;
use std::sync::OnceLock;

include!(concat!(env!("OUT_DIR"), "/locales.rs"));

#[derive(serde::Deserialize)]
struct LocaleFile {
    name: String,
    messages: BTreeMap<String, String>,
}

pub(crate) struct Locale {
    pub code: &'static str,
    pub name: String,
    messages: BTreeMap<String, String>,
}

pub(crate) fn locales() -> &'static [Locale] {
    static LOCALES: OnceLock<Vec<Locale>> = OnceLock::new();
    LOCALES.get_or_init(|| {
        LOCALE_SOURCES
            .iter()
            .map(|(code, raw)| {
                let file: LocaleFile =
                    serde_json::from_str(raw).expect("locale validated during build");
                Locale {
                    code,
                    name: file.name,
                    messages: file.messages,
                }
            })
            .collect()
    })
}

fn english() -> &'static Locale {
    locales()
        .iter()
        .find(|locale| locale.code == "en")
        .expect("English catalog bundled during build")
}

fn message<'a>(locale: Option<&'a Locale>, english: &'a Locale, key: &str) -> Option<&'a str> {
    locale
        .and_then(|locale| locale.messages.get(key))
        .filter(|text| !text.trim().is_empty())
        .or_else(|| english.messages.get(key))
        .map(String::as_str)
}

/// English source text is accepted for dynamic labels and diagnostics from other crates.
/// UI call sites use stable keys; source-text lookup keeps those diagnostics readable.
pub(crate) fn translate<'a>(language: &str, key: &'a str) -> &'a str {
    let english = english();
    let canonical = if english.messages.contains_key(key) {
        key
    } else {
        static SOURCE_KEYS: OnceLock<BTreeMap<&'static str, &'static str>> = OnceLock::new();
        SOURCE_KEYS
            .get_or_init(|| {
                english
                    .messages
                    .iter()
                    .map(|(key, value)| (value.as_str(), key.as_str()))
                    .collect()
            })
            .get(key)
            .copied()
            .unwrap_or(key)
    };
    message(
        locales().iter().find(|locale| locale.code == language),
        english,
        canonical,
    )
    .unwrap_or(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn placeholders(text: &str) -> BTreeSet<&str> {
        text.split('{')
            .skip(1)
            .filter_map(|part| part.split_once('}').map(|(name, _)| name))
            .collect()
    }

    #[test]
    fn bundled_catalogs_have_known_keys_and_matching_placeholders() {
        let english = english();
        assert!(!english.messages.is_empty());
        for locale in locales() {
            for (key, text) in &locale.messages {
                let source = english
                    .messages
                    .get(key)
                    .unwrap_or_else(|| panic!("{}: unknown key {key}", locale.code));
                if !text.trim().is_empty() {
                    assert_eq!(
                        placeholders(text),
                        placeholders(source),
                        "{}: {key}",
                        locale.code
                    );
                }
            }
        }
    }

    #[test]
    fn missing_languages_and_unknown_content_fall_back_safely() {
        assert_eq!(translate("missing", "common.next"), "Next");
        assert_eq!(translate("fr", "common.next"), "Suivant");
        assert_eq!(
            translate("fr", "untranslated user dictation"),
            "untranslated user dictation"
        );
    }

    #[test]
    fn sparse_catalogs_use_english_for_missing_or_empty_messages() {
        let sparse = Locale {
            code: "test",
            name: "Test".into(),
            messages: BTreeMap::from([("common.back".into(), " ".into())]),
        };
        assert_eq!(
            message(Some(&sparse), english(), "common.next"),
            Some("Next")
        );
        assert_eq!(
            message(Some(&sparse), english(), "common.back"),
            Some("Back")
        );
    }

    #[test]
    fn every_discovered_locale_is_selectable_by_its_filename() {
        let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../locales");
        for entry in std::fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path
                .extension()
                .is_some_and(|extension| extension == "json")
            {
                let code = path.file_stem().unwrap().to_str().unwrap();
                assert!(
                    crate::prefs::InterfaceLanguage::from_code(code).is_some(),
                    "{code} was not bundled"
                );
            }
        }
        for locale in locales() {
            let language = crate::prefs::InterfaceLanguage::from_code(locale.code).unwrap();
            assert_eq!(language.code(), locale.code);
            assert_eq!(language.label(), locale.name);
            assert!(crate::prefs::InterfaceLanguage::available().any(|choice| choice == language));
        }
    }
}
