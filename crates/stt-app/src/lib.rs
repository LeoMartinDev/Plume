pub mod assets;
pub mod catalog;
pub mod dirs;
pub mod download;
pub mod hold;
pub mod lock;
pub mod phase;
pub mod prefs;
pub mod settings;

use std::sync::mpsc;
use std::time::Duration;

use gpui::{App, Application};
use stt_engine::{Engine, Language};

use crate::assets::Assets;

use crate::download::{DownloadEvent, DownloadRequest, PackStatus};
use crate::lock::{AlreadyRunning, AppLock};
use crate::phase::{AppPhase, OnboardStatus};
use crate::prefs::{Prefs, PrefsLoad};
use crate::settings::{
    open_settings, settings_window_accepts_download, settings_window_engine_target,
    settings_window_prefs, settings_window_register_download, settings_window_show_fetch_failed,
    settings_window_show_live, settings_window_show_progress, settings_window_show_refused,
    settings_window_show_swapped,
};

/// Product entry. Settings window first. Compositor starts after a proven pack.
pub fn product_main() {
    let _lock = match AppLock::acquire() {
        Ok(lock) => lock,
        Err(AlreadyRunning(pid)) => {
            eprintln!("stt-app: already running pid={pid}");
            std::process::exit(0);
        }
    };
    stt_overlay::prepare_display();
    Application::new().with_assets(Assets).run(|cx| {
        let loaded = prefs::load();
        let (prefs, warning) = match loaded {
            PrefsLoad::Fresh(prefs) | PrefsLoad::Loaded(prefs) => (prefs, None),
            PrefsLoad::Quarantined { prefs, warning } => (prefs, Some(warning)),
        };
        let status = if catalog::is_complete(prefs.model, &prefs.model.data_dir()) {
            OnboardStatus::Activating
        } else {
            OnboardStatus::Idle
        };
        open_settings(
            cx,
            AppPhase::Onboarding {
                prefs: prefs.clone(),
                status,
                warning,
            },
        );
        cx.activate(true);
        boot_after_window_opens(cx, prefs);
    });
}

pub fn begin_pack(cx: &mut App, prefs: Prefs, request: DownloadRequest) {
    spawn_reconcile(cx, prefs, request, true);
}

fn boot_after_window_opens(cx: &mut App, prefs: Prefs) {
    let Some(request) = settings_window_register_download(cx, prefs.model) else {
        log_line("stt-app: initial reconcile request not registered");
        return;
    };
    spawn_reconcile(cx, prefs, request, false);
}

fn spawn_reconcile(
    cx: &mut App,
    prefs: Prefs,
    request: DownloadRequest,
    fetch_if_incomplete: bool,
) {
    let dest = prefs.model.data_dir();
    let offer = prefs.model.offer();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        eprintln!("stt-app: reconcile dest={}", dest.display());
        match download::reconcile(offer, &dest) {
            Ok(PackStatus::Proven(engine)) => {
                log_line("stt-app: pack proven");
                match tx.send(DownloadEvent::Proven { request, engine }) {
                    Ok(()) => log_line("stt-app: proven sent"),
                    Err(_) => log_line("stt-app: proven send failed"),
                }
            }
            Ok(PackStatus::Incomplete(mut staging)) => {
                eprintln!("stt-app: pack incomplete started={}", staging.started());
                if fetch_if_incomplete || staging.started() {
                    staging.resume(request, tx);
                }
            }
            Err(err) => {
                eprintln!("stt-app: reconcile failed: {err}");
                let _ = tx.send(DownloadEvent::Failed {
                    request,
                    error: err,
                });
            }
        }
    });
    drain_download(cx, rx);
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
        log_line("stt-app: drain started");
        loop {
            cx.background_executor()
                .timer(Duration::from_millis(16))
                .await;
            let (batch, closed) = match poll_download(&rx) {
                DrainPoll::Live(batch) => (batch, false),
                DrainPoll::Closed(batch) => (batch, true),
            };
            if !batch.is_empty() {
                log_line(format!("stt-app: drain events={}", batch.len()));
            }
            for event in batch {
                if cx.update(|cx| handle_download_event(cx, event)).is_err() {
                    log_line("stt-app: drain update failed");
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
    eprintln!("{msg}");
    let _ = std::io::Write::flush(&mut std::io::stderr());
}

fn handle_download_event(cx: &mut App, event: DownloadEvent) {
    let request = event.request();
    if !settings_window_accepts_download(cx, request) {
        log_line(format!(
            "stt-app: ignored stale download event model={} generation={}",
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
    log_line("stt-app: go_live");
    if !settings_window_accepts_download(cx, request) {
        log_line("stt-app: ignored stale proven download");
        return;
    }
    let Some(prefs) = settings_window_prefs(cx) else {
        return;
    };
    if prefs.model != request.model {
        log_line("stt-app: ignored proven download for a different model");
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
    if let Some(engine_target) = settings_window_engine_target(cx) {
        engine_target.set(engine);
        settings_window_show_swapped(cx, language_target);
        log_line("stt-app: engine swapped");
        return;
    }
    let config =
        match stt_session::Config::from_prefs(prefs.hold(), prefs.cancel(), prefs.model.data_dir())
        {
            Ok(config) => config,
            Err(err) => {
                settings_window_show_fetch_failed(cx, err);
                return;
            }
        };
    let ready = stt_session::Ready::from_open(config, engine);
    match stt_session::start(ready) {
        Ok(live) => {
            log_line("stt-app: compositor started");
            stt_overlay::attach(cx, live.bubbles, live.levels);
            settings_window_show_live(cx, live.hold, live.engine, language_target);
        }
        Err(err) => {
            log_line(format!("stt-app: compositor refused: {err}"));
            settings_window_show_refused(cx, err.to_string());
        }
    }
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
