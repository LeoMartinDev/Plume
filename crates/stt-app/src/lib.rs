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
use stt_engine::Engine;

use crate::download::{DownloadEvent, PackStatus};
use crate::lock::{AlreadyRunning, AppLock};
use crate::phase::{AppPhase, OnboardStatus};
use crate::prefs::{Prefs, PrefsLoad};
use crate::settings::{
    open_settings, settings_window_prefs, settings_window_set_phase,
    settings_window_show_fetch_failed, settings_window_show_live, settings_window_show_progress,
    settings_window_show_refused,
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
    Application::new().run(|cx| {
        let loaded = prefs::load();
        let (prefs, warning) = match loaded {
            PrefsLoad::Fresh(prefs) | PrefsLoad::Loaded(prefs) => (prefs, None),
            PrefsLoad::Quarantined { prefs, warning } => (prefs, Some(warning)),
        };
        open_settings(
            cx,
            AppPhase::Onboarding {
                prefs: prefs.clone(),
                status: OnboardStatus::Idle,
                warning,
            },
        );
        cx.activate(true);
        boot_after_window_opens(cx, prefs);
    });
}

pub fn begin_pack(cx: &mut App, prefs: Prefs) {
    settings_window_set_phase(
        cx,
        AppPhase::Onboarding {
            prefs: prefs.clone(),
            status: OnboardStatus::Fetching {
                last: crate::phase::Progress {
                    file: "starting".into(),
                    bytes: 0,
                    total: None,
                },
            },
            warning: None,
        },
    );
    spawn_reconcile(cx, prefs, true);
}

fn boot_after_window_opens(cx: &mut App, prefs: Prefs) {
    spawn_reconcile(cx, prefs, false);
}

fn spawn_reconcile(cx: &mut App, prefs: Prefs, fetch_if_incomplete: bool) {
    let dest = prefs.pack.data_dir();
    let offer = prefs.pack.offer();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        eprintln!("stt-app: reconcile dest={}", dest.display());
        match download::reconcile(offer, &dest) {
            Ok(PackStatus::Proven(engine)) => {
                log_line("stt-app: pack proven");
                match tx.send(DownloadEvent::Proven(engine)) {
                    Ok(()) => log_line("stt-app: proven sent"),
                    Err(_) => log_line("stt-app: proven send failed"),
                }
            }
            Ok(PackStatus::Incomplete(mut staging)) => {
                eprintln!("stt-app: pack incomplete started={}", staging.started());
                if fetch_if_incomplete || staging.started() {
                    staging.resume(tx);
                }
            }
            Err(err) => {
                eprintln!("stt-app: reconcile failed: {err}");
                let _ = tx.send(DownloadEvent::Failed(err));
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
                if cx
                    .update(|cx| match event {
                        DownloadEvent::Progress(last) => settings_window_show_progress(cx, last),
                        DownloadEvent::Proven(engine) => go_live(cx, engine),
                        DownloadEvent::Failed(err) => settings_window_show_fetch_failed(cx, err),
                    })
                    .is_err()
                {
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

fn go_live(cx: &mut App, engine: Engine) {
    log_line("stt-app: go_live");
    let Some(prefs) = settings_window_prefs(cx) else {
        return;
    };
    if let Err(err) = prefs::save(&prefs) {
        settings_window_show_fetch_failed(cx, err);
        return;
    }
    let config = match stt_session::Config::from_prefs(
        prefs.hold(),
        prefs.cancel(),
        prefs.pack.data_dir(),
    ) {
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
            settings_window_show_live(cx);
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
        DownloadEvent::Failed(DownloadError::Fetch(msg.into()))
    }

    fn failed_reason(event: &DownloadEvent) -> String {
        match event {
            DownloadEvent::Failed(err) => err.to_string(),
            DownloadEvent::Progress(_) => panic!("expected Failed, got Progress"),
            DownloadEvent::Proven(_) => panic!("expected Failed, got Proven"),
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
