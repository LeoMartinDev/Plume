use super::super::SettingsView;
use crate::prefs::{AppearancePref, InterfaceLanguage, LanguagePref, Prefs};
use gpui::Context;
use plume_engine::Language;
use plume_session::{InsertionConfig, InsertionMode};

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
        self.language_menu = None;
        if self
            .session_control
            .as_ref()
            .is_some_and(|control| control.is_busy())
            || matches!(
                self.phase,
                crate::phase::AppPhase::Onboarding {
                    status: crate::phase::OnboardStatus::Fetching { .. }
                        | crate::phase::OnboardStatus::Activating,
                    ..
                }
            )
        {
            cx.notify();
            return;
        }
        if self.phase.prefs().language() == next {
            cx.notify();
            return;
        }
        let previous = self.phase.prefs().language();
        let previous_step = self.phase.prefs().onboarding_step;
        let previous_mode = self.session_control.as_ref().map(|control| control.mode());
        let compatible = self
            .active_model
            .unwrap_or(self.phase.prefs().model)
            .supports(next);
        if let Some(control) = &self.session_control {
            if let Err(error) = control.set_mode(plume_session::SessionMode::Suspended) {
                self.save_error = Some(error);
                cx.notify();
                return;
            }
        }
        self.phase.prefs_mut().set_language(next);
        if !compatible && self.onboarding.is_some() {
            self.phase.prefs_mut().onboarding_step = crate::prefs::OnboardingStep::Model;
        }
        self.save_prefs();
        if self.save_error.is_some() {
            self.phase.prefs_mut().set_language(previous);
            self.phase.prefs_mut().onboarding_step = previous_step;
            if let (Some(control), Some(mode)) = (&self.session_control, previous_mode) {
                if let Err(error) = control.set_mode(mode) {
                    self.save_error = Some(error);
                }
            }
            cx.notify();
            return;
        }
        if !compatible {
            self.section = super::super::SettingsSection::Model;
            cx.notify();
            return;
        }
        if let Some(target) = &self.language_target {
            target.set(engine_language(next));
        }
        if let Some(engine) = self
            .onboarding
            .as_ref()
            .and_then(|setup| setup.engine.as_ref())
        {
            engine.set_language(engine_language(next));
        }
        if self.onboarding.is_none() {
            if let Some(control) = &self.session_control {
                if let Err(error) = control.set_mode(plume_session::SessionMode::System) {
                    self.save_error = Some(error);
                }
            }
        }
        cx.notify();
    }

    pub(in crate::settings) fn commit_interface_language(
        &mut self,
        next: InterfaceLanguage,
        cx: &mut Context<Self>,
    ) {
        self.language_menu = None;
        let previous = self.phase.prefs().interface_language();
        self.phase.prefs_mut().set_interface_language(next);
        self.save_prefs();
        if self.save_error.is_some() {
            self.phase.prefs_mut().set_interface_language(previous);
        }
        super::super::history_view::invalidate_list(
            &self.history_list,
            self.history.entries().len(),
        );
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
