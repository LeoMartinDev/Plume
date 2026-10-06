mod actions;
mod dictation;
mod history_view;
mod model;
mod updates;
mod view;
mod window;

use crate::download::DownloadRequestTracker;
use crate::phase::AppPhase;
use crate::prefs::Prefs;
use crate::shortcut_capture::ShortcutCapture;
use gpui::{Context, FocusHandle, ScrollHandle, Subscription};
use plume_engine::LanguageTarget;
use plume_session::{EngineTarget, HoldTarget, InsertionTarget};

pub use window::{
    open_settings, open_settings_preview, settings_window_accepts_download,
    settings_window_engine_target, settings_window_prefs, settings_window_record_result,
    settings_window_register_download, settings_window_set_phase,
    settings_window_show_fetch_failed, settings_window_show_live, settings_window_show_progress,
    settings_window_show_refused, settings_window_show_swapped, show_settings,
};

pub const SETTINGS_TITLE: &str = "Plume";
#[cfg(target_os = "macos")]
const MACOS_TITLEBAR_HEIGHT: f32 = 32.;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SettingsSection {
    Dictation,
    Model,
    Appearance,
    History,
    Updates,
}

impl SettingsSection {
    fn title(self) -> &'static str {
        match self {
            Self::Dictation => "Dictation",
            Self::Model => "Model",
            Self::Appearance => "Appearance",
            Self::History => "History",
            Self::Updates => "Updates",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::Dictation => "fluent/mic.svg",
            Self::Model => "fluent/cube.svg",
            Self::Appearance => "fluent/theme.svg",
            Self::History => "fluent/history.svg",
            Self::Updates => "fluent/cube.svg",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HistoryLimitMenu {
    Retention,
    Entries,
}

pub struct SettingsView {
    phase: AppPhase,
    prefs_path: std::path::PathBuf,
    settings_preview: bool,
    section: SettingsSection,
    language_open: bool,
    insertion_open: bool,
    save_error: Option<String>,
    capture: ShortcutCapture,
    hold_focus: FocusHandle,
    content_scroll: ScrollHandle,
    hold_target: Option<HoldTarget>,
    engine_target: Option<EngineTarget>,
    language_target: Option<LanguageTarget>,
    insertion_target: Option<InsertionTarget>,
    downloads: DownloadRequestTracker,
    history: crate::history::HistoryStore,
    history_error: Option<String>,
    history_menu: Option<HistoryLimitMenu>,
    copied_history_id: Option<u64>,
    copy_feedback_serial: u64,
    update: updates::UpdateState,
    _appearance: Subscription,
}

impl SettingsView {
    fn reset_for_phase(&mut self) {
        self.capture.cancel();
        self.language_open = false;
        self.insertion_open = false;
        self.history_menu = None;
    }

    pub fn set_phase(&mut self, phase: AppPhase, cx: &mut Context<Self>) {
        self.reset_for_phase();
        self.phase = phase;
        cx.notify();
    }

    fn prefs_snapshot(&self) -> Prefs {
        self.phase.prefs().clone()
    }
}
