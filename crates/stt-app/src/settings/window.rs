use std::io::Write;
use std::sync::Mutex;
use std::time::Duration;

use gpui::{
    px, size, App, AppContext, Bounds, ScrollHandle, TitlebarOptions, WindowBounds, WindowHandle,
    WindowKind, WindowOptions,
};
use stt_engine::LanguageTarget;
use stt_session::{EngineTarget, HoldTarget, InsertionTarget};

use crate::catalog::ModelId;
use crate::download::{DownloadRequest, DownloadRequestTracker};
use crate::hold::HoldCapture;
use crate::phase::{AppPhase, Progress};
use crate::prefs::Prefs;

use super::{SettingsSection, SettingsView, SETTINGS_TITLE};

static SETTINGS: Mutex<Option<WindowHandle<SettingsView>>> = Mutex::new(None);

pub fn open_settings(cx: &mut App, phase: AppPhase, downloads: DownloadRequestTracker) {
    let bounds = Bounds::centered(None, size(px(720.), px(480.)), cx);
    let history_path = crate::dirs::AppDirs::resolve().history_path();
    let (history, history_error) = match crate::history::HistoryStore::load(history_path.clone()) {
        Ok(history) => (history, None),
        Err(error) => (
            crate::history::HistoryStore::empty(history_path),
            Some(error),
        ),
    };
    let handle = cx
        .open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some(SETTINGS_TITLE.into()),
                    ..Default::default()
                }),
                app_id: Some("stt-app".into()),
                kind: WindowKind::Normal,
                focus: true,
                is_resizable: false,
                ..Default::default()
            },
            move |window, cx| {
                window.on_window_should_close(cx, |_, cx| {
                    cx.spawn(async move |cx| {
                        cx.background_executor()
                            .timer(Duration::from_millis(1))
                            .await;
                        let _ = cx.update(|cx| cx.quit());
                    })
                    .detach();
                    false
                });
                cx.new(move |cx| {
                    let _appearance = cx.observe_window_appearance(window, |_, _, cx| {
                        cx.notify();
                    });
                    SettingsView {
                        phase,
                        section: SettingsSection::Model,
                        language_open: false,
                        insertion_open: false,
                        save_error: None,
                        capture: HoldCapture::idle(),
                        hold_focus: cx.focus_handle().tab_stop(true),
                        content_scroll: ScrollHandle::new(),
                        hold_target: None,
                        engine_target: None,
                        language_target: None,
                        insertion_target: None,
                        downloads,
                        history,
                        history_error,
                        _appearance,
                    }
                })
            },
        )
        .expect("open settings window");
    *SETTINGS.lock().expect("settings handle") = Some(handle);
    print_opened_line();
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

pub fn settings_window_record_result(cx: &mut App, result: stt_session::DictationResult) {
    let handle = *SETTINGS.lock().expect("settings handle");
    if let Some(handle) = handle {
        let _ = handle.update(cx, |view, _window, cx| view.record_result(result, cx));
    }
}

fn print_opened_line() {
    eprintln!(
        "stt-app: window opened title={SETTINGS_TITLE} display={}",
        std::env::var("DISPLAY").unwrap_or_else(|_| "<unset>".into())
    );
    let _ = std::io::stderr().flush();
}
