use crate::prefs::InterfaceLanguage;
use std::cell::Cell;

thread_local! {
    static LANGUAGE: Cell<InterfaceLanguage> = const { Cell::new(InterfaceLanguage::ENGLISH) };
}

/// Scope translations to the view being rendered, including virtual list callbacks.
/// Restore the previous language so previews and tests cannot affect another view.
pub(super) struct LanguageScope(InterfaceLanguage);

impl LanguageScope {
    pub(super) fn new(language: InterfaceLanguage) -> Self {
        Self(LANGUAGE.with(|current| current.replace(language)))
    }
}

impl Drop for LanguageScope {
    fn drop(&mut self) {
        LANGUAGE.with(|current| current.set(self.0));
    }
}

pub(super) fn tr(key: &str) -> &str {
    crate::localization::translate(LANGUAGE.with(Cell::get).code(), key)
}

pub(super) fn age_label(created_at: u64) -> String {
    let label = crate::history::age_label(created_at);
    for (suffix, key) in [
        ("m ago", "history.age.minutes"),
        ("h ago", "history.age.hours"),
        ("d ago", "history.age.days"),
    ] {
        if let Some(count) = label.strip_suffix(suffix) {
            return tr(key).replace("{count}", count);
        }
    }
    tr(&label).to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scopes_restore_the_language_and_preserve_user_content() {
        assert_eq!(tr("common.next"), "Next");
        {
            let _scope = LanguageScope::new(InterfaceLanguage::FRENCH);
            assert_eq!(tr("common.next"), "Suivant");
            assert_eq!(tr("English"), "English");
            assert_eq!(
                tr("Bonjour, ceci est ma dictée."),
                "Bonjour, ceci est ma dictée."
            );
            {
                let _nested = LanguageScope::new(InterfaceLanguage::ENGLISH);
                assert_eq!(tr("common.next"), "Next");
            }
            assert_eq!(tr("common.next"), "Suivant");
        }
        assert_eq!(tr("common.next"), "Next");
    }
}
