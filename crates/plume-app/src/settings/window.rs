use std::sync::Mutex;
use std::time::Duration;

use gpui::{
    px, size, App, AppContext, Bounds, Entity, Global, ScrollHandle, TitlebarOptions, WindowBounds,
    WindowHandle, WindowKind, WindowOptions,
};
use plume_engine::LanguageTarget;
use plume_session::{EngineTarget, HoldTarget, InsertionTarget};

use crate::catalog::ModelId;
use crate::download::{DownloadRequest, DownloadRequestTracker};
use crate::phase::{AppPhase, Progress};
use crate::prefs::Prefs;
use crate::shortcut_capture::ShortcutCapture;

use super::{SettingsSection, SettingsView, SETTINGS_TITLE};

#[cfg(windows)]
mod windows;

static SETTINGS: Mutex<Option<WindowHandle<SettingsView>>> = Mutex::new(None);
/// Keeps the controller alive independently of either native window.
struct AppController(Entity<SettingsView>);
impl Global for AppController {}

fn controller(cx: &App) -> Option<Entity<SettingsView>> {
    cx.try_global::<AppController>()
        .map(|state| state.0.clone())
}

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
    open_settings_window(cx, phase, downloads, dirs, false, false)
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
        false,
    );
}

/// Isolated visual preview; no downloads, permissions, or native dictation.
pub fn open_onboarding_preview(cx: &mut App, prefs: Prefs, dirs: crate::dirs::AppDirs) {
    let status = if std::env::args().any(|arg| arg == "--preview-download") {
        crate::phase::OnboardStatus::Fetching {
            last: Progress {
                file: "model.bin".into(),
                bytes: 312 * 1_048_576,
                total: Some(793 * 1_048_576),
                bytes_per_second: Some(12 * 1_048_576),
            },
        }
    } else if std::env::args().any(|arg| arg == "--preview-connecting") {
        crate::phase::OnboardStatus::Fetching {
            last: Progress {
                file: "model.bin".into(),
                bytes: 0,
                total: None,
                bytes_per_second: None,
            },
        }
    } else if std::env::args().any(|arg| arg == "--preview-loading") {
        crate::phase::OnboardStatus::Activating
    } else {
        crate::phase::OnboardStatus::Idle
    };
    open_settings_window(
        cx,
        AppPhase::Onboarding {
            prefs,
            status,
            warning: None,
        },
        DownloadRequestTracker::default(),
        dirs,
        true,
        true,
    );
}

fn open_settings_window(
    cx: &mut App,
    mut phase: AppPhase,
    downloads: DownloadRequestTracker,
    dirs: crate::dirs::AppDirs,
    settings_preview: bool,
    onboarding_preview: bool,
) {
    let mut arguments = std::env::args();
    let show_about = std::env::args().any(|argument| argument == "--about");
    let update_error = arguments
        .find(|argument| argument == "--update-error")
        .and_then(|_| arguments.next())
        .map(|error| error.chars().take(4096).collect::<String>());
    let preview_update = settings_preview
        .then(super::updates::preview_state)
        .flatten();
    let onboarding = onboarding_preview || (!settings_preview && phase.prefs().needs_onboarding());
    if onboarding
        && !onboarding_preview
        && phase.prefs().onboarding_step != crate::prefs::OnboardingStep::Languages
        && !crate::catalog::is_complete(phase.prefs().model, &phase.prefs().model.data_dir())
    {
        phase.prefs_mut().onboarding_step = crate::prefs::OnboardingStep::Model;
    }
    let height = if onboarding {
        450.
    } else {
        SETTINGS_CONTENT_HEIGHT
    } + if cfg!(target_os = "macos") { 32. } else { 0. };
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
                    title: Some(if onboarding_preview {
                        "Plume — Onboarding Preview".into()
                    } else if settings_preview {
                        "Plume — Preview".into()
                    } else if onboarding {
                        "Welcome to Plume".into()
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
                #[cfg(windows)]
                windows::set_fixed_size(window, bounds.size, cx);
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
                    let setup =
                        onboarding.then(|| super::onboarding::OnboardingState::new(phase.prefs()));
                    let mut view = SettingsView {
                        onboarding: setup,
                        phase,
                        active_model: None,
                        prefs_path,
                        settings_preview,
                        section: if show_about || update_error.is_some() || preview_update.is_some()
                        {
                            SettingsSection::About
                        } else if settings_preview {
                            SettingsSection::History
                        } else {
                            SettingsSection::Model
                        },
                        language_menu: None,
                        language_menu_index: 0,
                        language_focus: [
                            cx.focus_handle().tab_stop(true),
                            cx.focus_handle().tab_stop(true),
                        ],
                        language_scroll: ScrollHandle::new(),
                        insertion_open: false,
                        save_error: None,
                        capture: ShortcutCapture::idle(),
                        capture_toggle: false,
                        shortcut_edit: None,
                        hold_focus: cx.focus_handle().tab_stop(true),
                        content_scroll: ScrollHandle::new(),
                        history_list: gpui::ListState::new(
                            0,
                            gpui::ListAlignment::Top,
                            gpui::px(200.),
                        )
                        .measure_all(),
                        history_list_width: None,
                        scrollbar_drag: Default::default(),
                        hold_target: None,
                        session_control: None,
                        recordings: None,
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
                            .or(preview_update)
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
    let entity = handle
        .update(cx, |_, _, cx| cx.entity())
        .expect("controller entity");
    cx.set_global(AppController(entity.clone()));
    if onboarding {
        entity.update(cx, |view, cx| view.poll_onboarding(cx));
    }
    print_opened_line();
}

/// Reveal the existing window, preserving the session and in-flight downloads.
pub fn show_settings(cx: &mut App) {
    let handle = *SETTINGS.lock().expect("settings handle");
    if let Some(handle) = handle {
        cx.activate(true);
        let _ = handle.update(cx, |view, window, cx| {
            view.onboarding_visibility(true, cx);
            crate::tray::show_window(window, cx);
            // Windows tray activation restores visibility and focus natively,
            // without GPUI reapplying the initial window placement.
            #[cfg(not(windows))]
            window.activate_window();
            window.on_next_frame(|window, _| {
                tracing::debug!(
                    active = window.is_window_active(),
                    "plume-app: settings revealed"
                );
            });
        });
    }
}

fn hide_settings(cx: &mut App) {
    let handle = *SETTINGS.lock().expect("settings handle");
    if let Some(handle) = handle {
        let _ = handle.update(cx, |view, window, cx| {
            view.reset_for_phase();
            view.onboarding_visibility(false, cx);
            window.blur();
            crate::tray::hide_window(window, cx);
            cx.notify();
        });
    }
}

pub fn settings_window_set_phase(cx: &mut App, phase: AppPhase) {
    if let Some(entity) = controller(cx) {
        entity.update(cx, |view, cx| view.set_phase(phase, cx));
    }
}

pub fn settings_window_register_download(cx: &mut App, model: ModelId) -> Option<DownloadRequest> {
    controller(cx).map(|entity| entity.update(cx, |view, _| view.register_download(model)))
}

pub fn settings_window_accepts_download(cx: &mut App, request: DownloadRequest) -> bool {
    controller(cx).is_some_and(|entity| entity.read(cx).accepts_download(request))
}

pub fn settings_window_show_progress(cx: &mut App, last: Progress) {
    if let Some(entity) = controller(cx) {
        entity.update(cx, |view, cx| view.show_progress(last, cx));
    }
}

pub fn settings_window_show_fetch_failed(cx: &mut App, err: impl std::fmt::Display) {
    if let Some(entity) = controller(cx) {
        entity.update(cx, |view, cx| {
            view.onboarding_model_failed();
            view.show_fetch_failed(err.to_string(), cx);
        });
    }
}

pub fn settings_window_show_live(
    cx: &mut App,
    hold_target: HoldTarget,
    engine_target: EngineTarget,
    language_target: LanguageTarget,
    insertion_target: InsertionTarget,
    control: plume_session::SessionControl,
    recordings: plume_session::SharedRecordings,
) {
    if let Some(entity) = controller(cx) {
        entity.update(cx, |view, cx| {
            view.session_control = Some(control);
            let records = recordings.lock().unwrap().records();
            view.history_error = view.history.import_recordings(&records).err();
            if view.history_error.is_none() {
                let mut store = recordings.lock().unwrap();
                for record in records {
                    if let Err(e) = store.acknowledge_history(record.id) {
                        view.history_error = Some(e.to_string());
                        break;
                    }
                }
                let ids: Vec<_> = view
                    .history
                    .entries()
                    .iter()
                    .filter_map(|e| e.record_id)
                    .collect();
                if let Err(e) = store.retain_history_text(&ids) {
                    view.history_error = Some(e.to_string());
                }
            }
            view.recordings = Some(recordings);
            if let Some(setup) = &mut view.onboarding {
                setup.session_starting = false;
                setup.session_error = None;
            }
            view.show_live(
                hold_target,
                engine_target,
                language_target,
                insertion_target,
                cx,
            );
        });
    }
}

pub fn settings_window_engine_target(cx: &mut App) -> Option<EngineTarget> {
    controller(cx).and_then(|entity| entity.read(cx).engine_target())
}

pub fn settings_window_show_swapped(cx: &mut App, language_target: LanguageTarget) {
    if let Some(entity) = controller(cx) {
        entity.update(cx, |view, cx| view.show_swapped(language_target, cx));
    }
}

pub fn settings_window_show_refused(cx: &mut App, reason: String) {
    if let Some(entity) = controller(cx) {
        entity.update(cx, |view, cx| {
            if let Some(setup) = &mut view.onboarding {
                setup.session_starting = false;
                setup.session_error = Some(reason);
                cx.notify();
            } else {
                view.show_refused(reason, cx);
            }
        });
    }
}

pub fn settings_window_prefs(cx: &mut App) -> Option<Prefs> {
    controller(cx).map(|entity| entity.read(cx).prefs_snapshot())
}

pub fn settings_window_record_result(cx: &mut App, result: plume_session::DictationResult) {
    if let Some(entity) = controller(cx) {
        entity.update(cx, |view, cx| view.record_result(result, cx));
    }
}

pub(crate) fn onboarding_model_loaded(cx: &mut App, engine: plume_engine::Engine) -> bool {
    controller(cx).is_some_and(|entity| {
        entity.update(cx, |view, cx| view.onboarding_model_loaded(engine, cx))
    })
}

pub(crate) fn onboarding_preview_event(cx: &mut App, event: plume_session::PreviewEvent) {
    if let Some(entity) = controller(cx) {
        entity.update(cx, |view, cx| view.onboarding_preview_event(event, cx));
    }
}

pub(super) fn finish_onboarding_window(cx: &mut App) {
    let Some(entity) = controller(cx) else {
        return;
    };
    let old = SETTINGS.lock().expect("settings handle").take();
    let bounds = Bounds::centered(
        None,
        size(
            px(720.),
            px(SETTINGS_CONTENT_HEIGHT + if cfg!(target_os = "macos") { 32. } else { 0. }),
        ),
        cx,
    );
    let handle = cx
        .open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some(SETTINGS_TITLE.into()),
                    appears_transparent: cfg!(target_os = "macos"),
                    ..Default::default()
                }),
                app_id: Some("plume".into()),
                kind: WindowKind::Normal,
                focus: !crate::tray::is_available(),
                show: !crate::tray::is_available(),
                is_resizable: false,
                ..Default::default()
            },
            move |window, cx| {
                crate::app_icon::install(window);
                #[cfg(windows)]
                windows::set_fixed_size(window, bounds.size, cx);
                window.on_window_should_close(cx, |_, cx| {
                    cx.defer(|cx| {
                        if crate::tray::is_available() {
                            hide_settings(cx);
                        } else {
                            cx.quit();
                        }
                    });
                    false
                });
                entity.update(cx, |view, cx| {
                    view.onboarding = None;
                    view._appearance = cx.observe_window_appearance(window, |_, _, cx| cx.notify());
                    cx.notify();
                });
                entity
            },
        )
        .expect("open settings after onboarding");
    *SETTINGS.lock().expect("settings handle") = Some(handle);
    if let Some(old) = old {
        let _ = old.update(cx, |_, window, _| window.remove_window());
    }
}

fn print_opened_line() {
    tracing::debug!(
        "plume-app: window opened title={SETTINGS_TITLE} display={}",
        std::env::var("DISPLAY").unwrap_or_else(|_| "<unset>".into())
    );
}
