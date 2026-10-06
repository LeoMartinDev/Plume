use std::sync::Mutex;
use std::time::Duration;

use gpui::{
    px, size, App, AppContext, Bounds, ScrollHandle, TitlebarOptions, WindowBounds, WindowHandle,
    WindowKind, WindowOptions,
};
use plume_engine::LanguageTarget;
use plume_session::{EngineTarget, HoldTarget, InsertionTarget};

use crate::catalog::ModelId;
use crate::download::{DownloadRequest, DownloadRequestTracker};
use crate::phase::{AppPhase, Progress};
use crate::prefs::Prefs;
use crate::shortcut_capture::ShortcutCapture;

use super::{SettingsSection, SettingsView, SETTINGS_TITLE};

static SETTINGS: Mutex<Option<WindowHandle<SettingsView>>> = Mutex::new(None);
const CLOSE_DISPATCH_DELAY: Duration = Duration::from_millis(1);
const SETTINGS_CONTENT_HEIGHT: f32 = 410.;

pub fn open_settings(cx: &mut App, phase: AppPhase, downloads: DownloadRequestTracker) {
    open_settings_in(cx, phase, downloads, crate::dirs::AppDirs::resolve());
}

fn open_settings_in(
    cx: &mut App,
    phase: AppPhase,
    downloads: DownloadRequestTracker,
    dirs: crate::dirs::AppDirs,
) {
    open_settings_window(cx, phase, downloads, dirs, false)
}

/// Settings preview with isolated preferences/history and disabled model actions.
pub fn open_settings_preview(cx: &mut App, prefs: Prefs, dirs: crate::dirs::AppDirs) {
    open_settings_window(
        cx,
        AppPhase::Onboarding {
            prefs,
            status: crate::phase::OnboardStatus::Idle,
            warning: None,
        },
        DownloadRequestTracker::default(),
        dirs,
        true,
    );
}

fn open_settings_window(
    cx: &mut App,
    phase: AppPhase,
    downloads: DownloadRequestTracker,
    dirs: crate::dirs::AppDirs,
    settings_preview: bool,
) {
    let mut arguments = std::env::args();
    let update_error = arguments
        .find(|argument| argument == "--update-error")
        .and_then(|_| arguments.next())
        .map(|error| error.chars().take(4096).collect::<String>());
    let height = SETTINGS_CONTENT_HEIGHT + if cfg!(target_os = "macos") { 32. } else { 0. };
    let bounds = Bounds::centered(None, size(px(720.), px(height)), cx);
    let history_path = dirs.history_path();
    let prefs_path = dirs.prefs_path();
    let (history, history_error) =
        match crate::history::HistoryStore::load(history_path.clone(), phase.prefs().history()) {
            Ok(mut history) => {
                let warning = history.take_load_warning();
                (history, warning)
            }
            Err(error) => (
                crate::history::HistoryStore::empty(history_path, phase.prefs().history()),
                Some(error),
            ),
        };
    let handle = cx
        .open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some(if settings_preview {
                        "Plume — Preview".into()
                    } else {
                        SETTINGS_TITLE.into()
                    }),
                    appears_transparent: cfg!(target_os = "macos"),
                    ..Default::default()
                }),
                app_id: Some("plume".into()),
                kind: WindowKind::Normal,
                focus: true,
                is_resizable: false,
                ..Default::default()
            },
            move |window, cx| {
                crate::app_icon::install(window);
                window.on_window_should_close(cx, move |_, cx| {
                    cx.spawn(async move |cx| {
                        cx.background_executor().timer(CLOSE_DISPATCH_DELAY).await;
                        let _ = cx.update(|cx| {
                            if crate::tray::is_available() && !settings_preview {
                                hide_settings(cx);
                            } else {
                                cx.quit();
                            }
                        });
                    })
                    .detach();
                    false
                });
                cx.new(move |cx| {
                    let _appearance = cx.observe_window_appearance(window, |_, _, cx| {
                        cx.notify();
                    });
                    let mut view = SettingsView {
                        phase,
                        prefs_path,
                        settings_preview,
                        section: if update_error.is_some() {
                            SettingsSection::Updates
                        } else if settings_preview {
                            SettingsSection::History
                        } else {
                            SettingsSection::Model
                        },
                        language_open: false,
                        insertion_open: false,
                        save_error: None,
                        capture: ShortcutCapture::idle(),
                        hold_focus: cx.focus_handle().tab_stop(true),
                        content_scroll: ScrollHandle::new(),
                        hold_target: None,
                        engine_target: None,
                        language_target: None,
                        insertion_target: None,
                        downloads,
                        history,
                        history_error,
                        history_menu: None,
                        copied_history_id: None,
                        copy_feedback_serial: 0,
                        update: update_error
                            .map(super::updates::UpdateState::Error)
                            .unwrap_or(super::updates::UpdateState::Idle),
                        _appearance,
                    };
                    if !settings_preview
                        && !matches!(view.update, super::updates::UpdateState::Error(_))
                    {
                        view.check_updates(cx);
                    }
                    view
                })
            },
        )
        .expect("open settings window");
    *SETTINGS.lock().expect("settings handle") = Some(handle);
    print_opened_line();
}

/// Reveal the existing window, preserving the session and in-flight downloads.
pub fn show_settings(cx: &mut App) {
    let handle = *SETTINGS.lock().expect("settings handle");
    if let Some(handle) = handle {
        cx.activate(true);
        let _ = handle.update(cx, |_, window, _| {
            crate::tray::show_window(window);
            window.activate_window();
        });
    }
}

fn hide_settings(cx: &mut App) {
    let handle = *SETTINGS.lock().expect("settings handle");
    if let Some(handle) = handle {
        let _ = handle.update(cx, |view, window, cx| {
            view.reset_for_phase();
            window.blur();
            crate::tray::hide_window(window);
            cx.notify();
        });
    }
}

pub fn settings_window_set_phase(cx: &mut App, phase: AppPhase) {
    let handle = *SETTINGS.lock().expect("settings handle");
    if let Some(handle) = handle {
        let _ = handle.update(cx, |view, _window, cx| view.set_phase(phase, cx));
    }
}

/// Registers a background reconciliation without changing the visible phase.
pub fn settings_window_register_download(cx: &mut App, model: ModelId) -> Option<DownloadRequest> {
    let handle = *SETTINGS.lock().expect("settings handle");
    handle.and_then(|handle| {
        handle
            .update(cx, |view, _window, _cx| view.register_download(model))
            .ok()
    })
}

/// Returns whether an event still belongs to the selected, latest request.
pub fn settings_window_accepts_download(cx: &mut App, request: DownloadRequest) -> bool {
    let handle = *SETTINGS.lock().expect("settings handle");
    handle
        .and_then(|handle| {
            handle
                .update(cx, |view, _window, _cx| view.accepts_download(request))
                .ok()
        })
        .unwrap_or(false)
}

pub fn settings_window_show_progress(cx: &mut App, last: Progress) {
    let handle = *SETTINGS.lock().expect("settings handle");
    if let Some(handle) = handle {
        let _ = handle.update(cx, |view, _window, cx| view.show_progress(last, cx));
    }
}

pub fn settings_window_show_fetch_failed(cx: &mut App, err: impl std::fmt::Display) {
    let reason = err.to_string();
    let handle = *SETTINGS.lock().expect("settings handle");
    if let Some(handle) = handle {
        let _ = handle.update(cx, |view, _window, cx| view.show_fetch_failed(reason, cx));
    }
}

pub fn settings_window_show_live(
    cx: &mut App,
    hold_target: HoldTarget,
    engine_target: EngineTarget,
    language_target: LanguageTarget,
    insertion_target: InsertionTarget,
) {
    let handle = *SETTINGS.lock().expect("settings handle");
    if let Some(handle) = handle {
        let _ = handle.update(cx, |view, _window, cx| {
            view.show_live(
                hold_target,
                engine_target,
                language_target,
                insertion_target,
                cx,
            )
        });
    }
}

pub fn settings_window_engine_target(cx: &mut App) -> Option<EngineTarget> {
    let handle = *SETTINGS.lock().expect("settings handle");
    handle.and_then(|handle| {
        handle
            .update(cx, |view, _window, _cx| view.engine_target())
            .ok()
            .flatten()
    })
}

pub fn settings_window_show_swapped(cx: &mut App, language_target: LanguageTarget) {
    let handle = *SETTINGS.lock().expect("settings handle");
    if let Some(handle) = handle {
        let _ = handle.update(cx, |view, _window, cx| {
            view.show_swapped(language_target, cx)
        });
    }
}

pub fn settings_window_show_refused(cx: &mut App, reason: String) {
    let handle = *SETTINGS.lock().expect("settings handle");
    if let Some(handle) = handle {
        let _ = handle.update(cx, |view, _window, cx| view.show_refused(reason, cx));
    }
}

pub fn settings_window_prefs(cx: &mut App) -> Option<Prefs> {
    let handle = *SETTINGS.lock().expect("settings handle");
    handle.and_then(|handle| {
        handle
            .update(cx, |view, _window, _cx| view.prefs_snapshot())
            .ok()
    })
}

pub fn settings_window_record_result(cx: &mut App, result: plume_session::DictationResult) {
    let handle = *SETTINGS.lock().expect("settings handle");
    if let Some(handle) = handle {
        let _ = handle.update(cx, |view, _window, cx| view.record_result(result, cx));
    }
}

fn print_opened_line() {
    tracing::debug!(
        "plume-app: window opened title={SETTINGS_TITLE} display={}",
        std::env::var("DISPLAY").unwrap_or_else(|_| "<unset>".into())
    );
}
