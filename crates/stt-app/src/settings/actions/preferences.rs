use super::super::SettingsView;
use crate::prefs::{AppearancePref, LanguagePref, Prefs};
use gpui::Context;
use stt_engine::Language;
use stt_session::{InsertionConfig, InsertionMode};

impl SettingsView {
    pub(in crate::settings) fn save_prefs(&mut self) {
        self.save_error = crate::prefs::save_at(&self.prefs_path, self.phase.prefs())
            .err()
            .map(|err| err.to_string());
    }

    pub(in crate::settings) fn commit_language(
        &mut self,
        next: LanguagePref,
        cx: &mut Context<Self>,
    ) {
        self.language_open = false;
        if self.phase.prefs().language() == next {
            cx.notify();
            return;
        }
        self.phase.prefs_mut().set_language(next);
        self.save_prefs();
        if let Some(target) = &self.language_target {
            target.set(engine_language(next));
        }
        cx.notify();
    }

    pub(in crate::settings) fn commit_insertion_mode(
        &mut self,
        next: InsertionMode,
        cx: &mut Context<Self>,
    ) {
        self.insertion_open = false;
        if self.phase.prefs().insertion_mode() == next {
            cx.notify();
            return;
        }
        self.phase.prefs_mut().set_insertion_mode(next);
        self.save_prefs();
        self.retarget_insertion();
        cx.notify();
    }

    pub(in crate::settings) fn toggle_copy_on_failure(&mut self, cx: &mut Context<Self>) {
        let enabled = !self.phase.prefs().copy_on_failure();
        self.phase.prefs_mut().set_copy_on_failure(enabled);
        self.save_prefs();
        self.retarget_insertion();
        cx.notify();
    }

    pub(in crate::settings) fn retarget_insertion(&self) {
        if let Some(target) = &self.insertion_target {
            target.set(insertion_config(self.phase.prefs()));
        }
    }

    pub fn commit_appearance(&mut self, next: AppearancePref, cx: &mut Context<Self>) {
        let was_listening = self.capture.is_listening();
        self.capture.cancel();
        if self.phase.prefs().appearance() == next {
            if was_listening {
                cx.notify();
            }
            return;
        }
        self.phase.prefs_mut().set_appearance(next);
        self.save_prefs();
        cx.notify();
    }
}

fn engine_language(language: LanguagePref) -> Language {
    match language {
        LanguagePref::Auto => Language::Auto,
        LanguagePref::French => Language::French,
        LanguagePref::English => Language::English,
    }
}

fn insertion_config(prefs: &Prefs) -> InsertionConfig {
    InsertionConfig {
        mode: prefs.insertion_mode(),
        copy_on_failure: prefs.copy_on_failure(),
    }
}
