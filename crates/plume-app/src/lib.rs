mod app_icon;
pub mod assets;
pub mod catalog;
pub mod dirs;
pub mod download;
mod file_store;
pub mod history;
pub mod history_policy;
mod localization;
pub mod lock;
mod permissions;
pub mod phase;
pub mod prefs;
pub mod settings;
pub mod shortcut_capture;
mod tray;

use std::sync::mpsc;

use gpui::{App, Application};
use plume_engine::{Engine, Language};

use crate::assets::Assets;

use crate::download::{DownloadEvent, DownloadRequest, PackStatus};
use crate::lock::{AlreadyRunning, AppLock};
use crate::phase::{AppPhase, OnboardStatus};
use crate::prefs::{Prefs, PrefsLoad};
use crate::settings::{
    open_settings, settings_window_accepts_download, settings_window_engine_target,
    settings_window_prefs, settings_window_record_result, settings_window_show_fetch_failed,
    settings_window_show_live, settings_window_show_progress, settings_window_show_refused,
    settings_window_show_swapped,
};

/// Product entry. Model loading starts immediately; the compositor follows once proven.
pub fn product_main() {
    plume_logging::init();
    let _lock = match AppLock::acquire() {
        Ok(lock) => lock,
        Err(AlreadyRunning(pid)) => {
            tracing::warn!("plume-app: already running pid={pid}");
            std::process::exit(0);
        }
    };
    plume_overlay::prepare_display();
    let loaded = prefs::load();
    let (prefs, warning) = match loaded {
        PrefsLoad::Fresh(prefs) | PrefsLoad::Loaded(prefs) => (prefs, None),
        PrefsLoad::LoadedWithWarnings { prefs, warnings } => (prefs, Some(warnings.join("; "))),
        PrefsLoad::Quarantined { prefs, warning } => (prefs, Some(warning)),
    };
    let status = if catalog::is_complete(prefs.model, &prefs.model.data_dir()) {
        OnboardStatus::Activating
    } else {
        OnboardStatus::Idle
    };
    let mut downloads = crate::download::DownloadRequestTracker::default();
    let request = downloads.register(prefs.model);
    let startup_events = start_reconcile(prefs.clone(), request, false);

    let application = Application::new().with_assets(Assets);
    application.on_reopen(settings::show_settings);
    application.run(move |cx| {
        open_settings(
            cx,
            AppPhase::Onboarding {
                prefs: prefs.clone(),
                status,
                warning,
            },
            downloads,
        );
        tray::install(cx);
        // Installing the macOS tray changes the activation policy. Reveal the
        // actual window afterward, rather than only activating the application.
        cx.defer(settings::show_settings);
        drain_download(cx, startup_events);
    });
}

pub fn begin_pack(cx: &mut App, prefs: Prefs, request: DownloadRequest) {
    spawn_reconcile(cx, prefs, request, true);
}

fn spawn_reconcile(
    cx: &mut App,
    prefs: Prefs,
    request: DownloadRequest,
    fetch_if_incomplete: bool,
) {
    let rx = start_reconcile(prefs, request, fetch_if_incomplete);
    drain_download(cx, rx);
}

fn start_reconcile(
    prefs: Prefs,
    request: DownloadRequest,
    fetch_if_incomplete: bool,
) -> mpsc::Receiver<DownloadEvent> {
    let dest = prefs.model.data_dir();
    let offer = prefs.model.offer();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        tracing::debug!("plume-app: reconcile dest={}", dest.display());
        match download::reconcile(offer, &dest) {
            Ok(PackStatus::Proven(engine)) => {
                log_line("plume-app: pack proven");
                match tx.send(DownloadEvent::Proven { request, engine }) {
                    Ok(()) => log_line("plume-app: proven sent"),
                    Err(_) => log_line("plume-app: proven send failed"),
                }
            }
            Ok(PackStatus::Incomplete(mut staging)) => {
                tracing::debug!("plume-app: pack incomplete started={}", staging.started());
                if fetch_if_incomplete || staging.started() {
                    staging.resume(request, tx);
                }
            }
            Err(err) => {
                tracing::error!("plume-app: reconcile failed: {err}");
                let _ = tx.send(DownloadEvent::Failed {
                    request,
                    error: err,
                });
            }
        }
    });
    rx
}

enum DrainPoll {
    Live(Vec<DownloadEvent>),
    Closed(Vec<DownloadEvent>),
}

fn poll_download(rx: &mpsc::Receiver<DownloadEvent>) -> DrainPoll {
    let mut batch = Vec::new();
    loop {
        match rx.try_recv() {
            Ok(event) => batch.push(event),
            Err(mpsc::TryRecvError::Empty) => return DrainPoll::Live(batch),
            Err(mpsc::TryRecvError::Disconnected) => return DrainPoll::Closed(batch),
        }
    }
}

fn drain_download(cx: &mut App, rx: mpsc::Receiver<DownloadEvent>) {
    cx.spawn(async move |cx| {
        log_line("plume-app: drain started");
        loop {
            cx.background_executor()
                .timer(plume_ui::UI_REFRESH_INTERVAL)
                .await;
            let (batch, closed) = match poll_download(&rx) {
                DrainPoll::Live(batch) => (batch, false),
                DrainPoll::Closed(batch) => (batch, true),
            };
            if !batch.is_empty() {
                log_line(format!("plume-app: drain events={}", batch.len()));
            }
            for event in batch {
                if cx.update(|cx| handle_download_event(cx, event)).is_err() {
                    log_line("plume-app: drain update failed");
                }
            }
            if closed {
                return;
            }
        }
    })
    .detach();
}

fn log_line(msg: impl std::fmt::Display) {
    tracing::debug!("{msg}");
}

fn handle_download_event(cx: &mut App, event: DownloadEvent) {
    let request = event.request();
    if !settings_window_accepts_download(cx, request) {
        log_line(format!(
            "plume-app: ignored stale download event model={} generation={}",
            request.model.as_str(),
            request.generation
        ));
        return;
    }
    match event {
        DownloadEvent::Progress { last, .. } => settings_window_show_progress(cx, last),
        DownloadEvent::Proven { engine, .. } => go_live(cx, request, engine),
        DownloadEvent::Failed { error, .. } => settings_window_show_fetch_failed(cx, error),
    }
}

fn go_live(cx: &mut App, request: DownloadRequest, engine: Engine) {
    log_line("plume-app: go_live");
    if !settings_window_accepts_download(cx, request) {
        log_line("plume-app: ignored stale proven download");
        return;
    }
    let Some(prefs) = settings_window_prefs(cx) else {
        return;
    };
    if prefs.model != request.model {
        log_line("plume-app: ignored proven download for a different model");
        return;
    }
    if !prefs.model.supports(prefs.language()) {
        settings_window_show_refused(cx, "This model does not support your dictation language. Choose a compatible model to continue.".into());
        return;
    }
    let language = match prefs.language() {
        crate::prefs::LanguagePref::Auto => Language::Auto,
        crate::prefs::LanguagePref::French => Language::French,
        crate::prefs::LanguagePref::English => Language::English,
    };
    engine.set_language(language);
    let language_target = engine.language_target();
    if let Err(err) = prefs::save(&prefs) {
        settings_window_show_fetch_failed(cx, err);
        return;
    }
    if settings::onboarding_model_loaded(cx, engine.clone()) {
        return;
    }
    if let Some(engine_target) = settings_window_engine_target(cx) {
        engine_target.set(engine);
        settings_window_show_swapped(cx, language_target);
        log_line("plume-app: engine swapped");
        return;
    }
    start_session(cx, prefs, engine, plume_session::SessionMode::System);
}

pub(crate) fn start_session(
    cx: &mut App,
    prefs: Prefs,
    engine: Engine,
    mode: plume_session::SessionMode,
) {
    let language_target = engine.language_target();
    if !prefs.model.supports(prefs.language()) {
        settings_window_show_refused(cx, "This model does not support your dictation language. Choose a compatible model to continue.".into());
        return;
    }
    let config = match plume_session::Config::from_prefs(
        prefs.hold(),
        prefs.cancel(),
        prefs.model.data_dir(),
    )
    .and_then(|config| config.with_toggle(prefs.toggle()))
    {
        Ok(config) => config,
        Err(err) => {
            settings_window_show_fetch_failed(cx, err);
            return;
        }
    };
    let ready = plume_session::PreparedSession::from_open(config, engine).with_insertion(
        plume_session::InsertionConfig {
            mode: prefs.insertion_mode(),
            copy_on_failure: prefs.copy_on_failure(),
        },
    );
    let dirs = crate::dirs::AppDirs::resolve();
    let ready = ready
        .with_recordings(dirs.recordings_path(), dirs.vad_path())
        .with_mode(mode);
    match plume_session::start(ready) {
        Ok(live) => {
            log_line("plume-app: compositor started");
            plume_overlay::attach(cx, live.bubbles, live.levels);
            settings_window_show_live(
                cx,
                live.hold,
                live.engine,
                language_target,
                live.insertion,
                live.control,
                live.recordings,
            );
            drain_results(cx, live.results);
            drain_preview(cx, live.preview);
        }
        Err(err) => {
            tracing::error!("plume-app: compositor refused: {err}");
            settings_window_show_refused(cx, err.to_string());
        }
    }
}

fn drain_preview(cx: &mut App, rx: mpsc::Receiver<plume_session::PreviewEvent>) {
    cx.spawn(async move |cx| loop {
        cx.background_executor()
            .timer(plume_ui::UI_REFRESH_INTERVAL)
            .await;
        loop {
            match rx.try_recv() {
                Ok(event) => {
                    if cx
                        .update(|cx| settings::onboarding_preview_event(cx, event))
                        .is_err()
                    {
                        return;
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => return,
            }
        }
    })
    .detach();
}

fn drain_results(cx: &mut App, rx: mpsc::Receiver<plume_session::DictationResult>) {
    cx.spawn(async move |cx| loop {
        cx.background_executor()
            .timer(plume_ui::UI_REFRESH_INTERVAL)
            .await;
        loop {
            match rx.try_recv() {
                Ok(result) => {
                    if cx
                        .update(|cx| settings_window_record_result(cx, result))
                        .is_err()
                    {
                        return;
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => return,
            }
        }
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::download::DownloadError;

    fn failed(msg: &str) -> DownloadEvent {
        DownloadEvent::Failed {
            request: DownloadRequest {
                model: crate::catalog::ModelId::Nemotron35Compact,
                generation: 1,
            },
            error: DownloadError::Fetch(msg.into()),
        }
    }

    fn failed_reason(event: &DownloadEvent) -> String {
        match event {
            DownloadEvent::Failed { error, .. } => error.to_string(),
            DownloadEvent::Progress { .. } => panic!("expected Failed, got Progress"),
            DownloadEvent::Proven { .. } => panic!("expected Failed, got Proven"),
        }
    }

    #[test]
    fn poll_keeps_events_when_sender_drops() {
        let (tx, rx) = mpsc::channel();
        tx.send(failed("one")).unwrap();
        tx.send(failed("two")).unwrap();
        drop(tx);
        match poll_download(&rx) {
            DrainPoll::Closed(batch) => {
                assert_eq!(batch.len(), 2);
                assert_eq!(failed_reason(&batch[0]), "pack fetch: one");
                assert_eq!(failed_reason(&batch[1]), "pack fetch: two");
            }
            DrainPoll::Live(_) => panic!("sender already dropped"),
        }
    }

    #[test]
    fn poll_empty_while_sender_lives() {
        let (tx, rx) = mpsc::channel();
        match poll_download(&rx) {
            DrainPoll::Live(batch) => assert!(batch.is_empty()),
            DrainPoll::Closed(_) => panic!("sender still held"),
        }
        drop(tx);
    }
}
