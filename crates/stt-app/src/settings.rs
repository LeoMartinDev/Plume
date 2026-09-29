mod dictation;
mod history_view;
mod model;
mod view;
mod window;

use gpui::{
    Context, FocusHandle, KeyDownEvent, KeyUpEvent, ModifiersChangedEvent, ScrollHandle,
    Subscription, Window,
};
use stt_engine::{Language, LanguageTarget};
use stt_session::{EngineTarget, HoldTarget, InsertionConfig, InsertionMode, InsertionTarget};

use crate::catalog::{self, ModelId};
use crate::download::{DownloadRequest, DownloadRequestTracker};
use crate::hold::{classify_keydown, CaptureEffect, ChordText, HoldCapture, ModBits};
use crate::phase::{AppPhase, OnboardStatus, Progress};
use crate::prefs::{AppearancePref, LanguagePref, Prefs, DEFAULT_HOLD};

pub use window::{
    open_settings, settings_window_accepts_download, settings_window_engine_target,
    settings_window_prefs, settings_window_record_result, settings_window_register_download,
    settings_window_set_phase, settings_window_show_fetch_failed, settings_window_show_live,
    settings_window_show_progress, settings_window_show_refused, settings_window_show_swapped,
};

pub const SETTINGS_TITLE: &str = "stt";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SettingsSection {
    Dictation,
    Model,
    Appearance,
    History,
}

impl SettingsSection {
    fn title(self) -> &'static str {
        match self {
            Self::Dictation => "Dictation",
            Self::Model => "Model",
            Self::Appearance => "Appearance",
            Self::History => "History",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::Dictation => "fluent/mic.svg",
            Self::Model => "fluent/cube.svg",
            Self::Appearance => "fluent/theme.svg",
            Self::History => "fluent/history.svg",
        }
    }
}

pub struct SettingsView {
    phase: AppPhase,
    section: SettingsSection,
    language_open: bool,
    insertion_open: bool,
    save_error: Option<String>,
    capture: HoldCapture,
    hold_focus: FocusHandle,
    content_scroll: ScrollHandle,
    hold_target: Option<HoldTarget>,
    engine_target: Option<EngineTarget>,
    language_target: Option<LanguageTarget>,
    insertion_target: Option<InsertionTarget>,
    downloads: DownloadRequestTracker,
    history: crate::history::HistoryStore,
    history_error: Option<String>,
    _appearance: Subscription,
}

impl SettingsView {
    fn reset_for_phase(&mut self) {
        self.capture.cancel();
        self.language_open = false;
        self.insertion_open = false;
    }

    pub fn set_phase(&mut self, phase: AppPhase, cx: &mut Context<Self>) {
        self.reset_for_phase();
        self.phase = phase;
        cx.notify();
    }

    fn register_download(&mut self, model: ModelId) -> DownloadRequest {
        self.downloads.register(model)
    }

    fn accepts_download(&self, request: DownloadRequest) -> bool {
        self.downloads.accepts(self.phase.prefs().model, request)
    }

    fn begin_download(&mut self, prefs: Prefs, cx: &mut Context<Self>) -> DownloadRequest {
        let request = self.register_download(prefs.model);
        let installed = catalog::is_complete(prefs.model, &prefs.model.data_dir());
        let status = if installed {
            OnboardStatus::Activating
        } else {
            OnboardStatus::Fetching {
                last: Progress {
                    file: "starting".into(),
                    bytes: 0,
                    total: None,
                    bytes_per_second: None,
                },
            }
        };
        self.set_phase(
            AppPhase::Onboarding {
                prefs,
                status,
                warning: None,
            },
            cx,
        );
        request
    }

    pub fn toggle_hold_capture(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.capture.is_listening() {
            self.capture.cancel();
            window.blur();
            cx.notify();
            return;
        }
        self.capture.begin();
        window.focus(&self.hold_focus);
        cx.notify();
    }

    pub fn on_hold_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        window.prevent_default();
        let stroke = classify_keydown(
            event.is_held,
            event.keystroke.key.as_str(),
            ModBits::from_gpui(event.keystroke.modifiers),
        );
        let effect = self.capture.apply(stroke);
        self.handle_hold_capture(effect, window, cx);
    }

    pub fn on_hold_modifiers(
        &mut self,
        event: &ModifiersChangedEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        window.prevent_default();
        let effect = self
            .capture
            .apply_modifiers(ModBits::from_gpui(event.modifiers));
        self.handle_hold_capture(effect, window, cx);
    }

    pub fn on_hold_key_up(
        &mut self,
        _event: &KeyUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        window.prevent_default();
        let effect = self.capture.apply_key_up();
        self.handle_hold_capture(effect, window, cx);
    }

    fn handle_hold_capture(
        &mut self,
        effect: CaptureEffect,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match effect {
            CaptureEffect::None => {}
            CaptureEffect::StayListening => cx.notify(),
            CaptureEffect::Cancelled | CaptureEffect::Rejected(_) => {
                window.blur();
                cx.notify();
            }
            CaptureEffect::Offer(chord) => self.commit_hold(chord, window, cx),
        }
    }

    fn reset_hold(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.capture.cancel();
        let previous = self.phase.prefs().hold().to_string();
        match self.phase.prefs_mut().try_set_hold(DEFAULT_HOLD) {
            Err(err) => {
                self.capture.set_reject(err.to_string());
                window.blur();
                cx.notify();
            }
            Ok(()) if previous == DEFAULT_HOLD => {
                window.blur();
                cx.notify();
            }
            Ok(()) => {
                self.save_prefs();
                self.retarget_hold(DEFAULT_HOLD);
                window.blur();
                cx.notify();
            }
        }
    }

    fn commit_hold(&mut self, chord: ChordText, window: &mut Window, cx: &mut Context<Self>) {
        let previous = self.phase.prefs().hold().to_string();
        match self.phase.prefs_mut().try_set_hold(chord.as_str()) {
            Err(err) => {
                self.capture.set_reject(err.to_string());
                window.blur();
                cx.notify();
            }
            Ok(()) if previous == chord.as_str() => {
                window.blur();
                cx.notify();
            }
            Ok(()) => {
                self.save_prefs();
                self.retarget_hold(chord.as_str());
                window.blur();
                cx.notify();
            }
        }
    }

    fn save_prefs(&mut self) {
        self.save_error = crate::prefs::save(self.phase.prefs())
            .err()
            .map(|err| err.to_string());
    }

    fn retarget_hold(&self, hold: &str) {
        if let Some(target) = &self.hold_target {
            target.set(hold);
        }
    }

    fn commit_language(&mut self, next: LanguagePref, cx: &mut Context<Self>) {
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

    fn commit_insertion_mode(&mut self, next: InsertionMode, cx: &mut Context<Self>) {
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

    fn toggle_copy_on_failure(&mut self, cx: &mut Context<Self>) {
        let enabled = !self.phase.prefs().copy_on_failure();
        self.phase.prefs_mut().set_copy_on_failure(enabled);
        self.save_prefs();
        self.retarget_insertion();
        cx.notify();
    }

    fn retarget_insertion(&self) {
        if let Some(target) = &self.insertion_target {
            target.set(insertion_config(self.phase.prefs()));
        }
    }

    fn record_result(&mut self, result: stt_session::DictationResult, cx: &mut Context<Self>) {
        self.history_error = self.history.push(result).err();
        cx.notify();
    }

    fn copy_history(&mut self, text: String, cx: &mut Context<Self>) {
        self.history_error = crate::history::copy_text(&text).err();
        cx.notify();
    }

    fn delete_history(&mut self, id: u64, cx: &mut Context<Self>) {
        self.history_error = self.history.delete(id).err();
        cx.notify();
    }

    fn clear_history(&mut self, cx: &mut Context<Self>) {
        self.history_error = self.history.clear().err();
        cx.notify();
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

    pub fn show_progress(&mut self, last: Progress, cx: &mut Context<Self>) {
        self.reset_for_phase();
        self.section = SettingsSection::Model;
        if let AppPhase::Onboarding { status, .. } = &mut self.phase {
            *status = OnboardStatus::Fetching { last };
        }
        cx.notify();
    }

    pub fn show_fetch_failed(&mut self, reason: String, cx: &mut Context<Self>) {
        self.reset_for_phase();
        self.section = SettingsSection::Model;
        self.downloads.clear();
        if let AppPhase::Onboarding { status, .. } = &mut self.phase {
            *status = OnboardStatus::Failed { reason };
        }
        cx.notify();
    }

    pub fn show_live(
        &mut self,
        hold_target: HoldTarget,
        engine_target: EngineTarget,
        language_target: LanguageTarget,
        insertion_target: InsertionTarget,
        cx: &mut Context<Self>,
    ) {
        self.reset_for_phase();
        self.downloads.clear();
        self.hold_target = Some(hold_target);
        self.engine_target = Some(engine_target);
        self.language_target = Some(language_target);
        self.insertion_target = Some(insertion_target);
        self.phase = AppPhase::Live {
            prefs: self.phase.prefs().clone(),
        };
        cx.notify();
    }

    fn engine_target(&self) -> Option<EngineTarget> {
        self.engine_target.clone()
    }

    fn show_swapped(&mut self, language_target: LanguageTarget, cx: &mut Context<Self>) {
        self.language_target = Some(language_target);
        self.downloads.clear();
        self.phase = AppPhase::Live {
            prefs: self.phase.prefs().clone(),
        };
        cx.notify();
    }

    fn prefs_snapshot(&self) -> Prefs {
        self.phase.prefs().clone()
    }

    pub fn show_refused(&mut self, reason: String, cx: &mut Context<Self>) {
        self.reset_for_phase();
        self.section = SettingsSection::Model;
        self.downloads.clear();
        self.phase = AppPhase::Refused {
            prefs: self.phase.prefs().clone(),
            reason,
        };
        cx.notify();
    }

    fn delete_model(&mut self, id: ModelId, cx: &mut Context<Self>) {
        let selected = self.phase.prefs().model == id;
        if self.downloads.is_active_for(id) {
            self.save_error =
                Some("Wait for the current download to finish before deleting it.".into());
            cx.notify();
            return;
        }

        match crate::download::remove(id.offer(), &id.data_dir()) {
            Ok(()) => {
                self.save_error = None;
                if selected {
                    self.downloads.clear();
                    self.phase = AppPhase::Onboarding {
                        prefs: self.phase.prefs().clone(),
                        status: OnboardStatus::Idle,
                        warning: None,
                    };
                }
            }
            Err(err) => self.save_error = Some(format!("Could not delete model: {err}")),
        }
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
