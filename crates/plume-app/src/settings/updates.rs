use std::sync::mpsc;
use std::time::{Duration, Instant};

use gpui::{div, prelude::*, px, AnyElement, Context, ElementId};
use plume_ui::Tokens;
use plume_updater::{InstallPlan, Release};

use super::view::{page, settings_group};
use super::{SettingsSection, SettingsView};

pub(super) enum UpdateState {
    Idle,
    Checking,
    Current,
    Available(Release),
    Downloading(u64, u64),
    Ready { release: Release, plan: InstallPlan },
    Error(String),
}

enum Event {
    Checked(plume_updater::Result<Option<Release>>),
    Progress(u64, u64),
    Prepared(plume_updater::Result<(Release, InstallPlan)>),
}

impl SettingsView {
    pub(super) fn check_updates(&mut self, cx: &mut Context<Self>) {
        if matches!(
            self.update,
            UpdateState::Checking | UpdateState::Downloading(..) | UpdateState::Ready { .. }
        ) {
            return;
        }
        self.update = UpdateState::Checking;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(Event::Checked(plume_updater::check()));
        });
        self.drain_updates(rx, cx);
        cx.notify();
    }

    fn download_update(&mut self, cx: &mut Context<Self>) {
        let UpdateState::Available(release) = &self.update else {
            return;
        };
        let release = release.clone();
        let installation = match plume_updater::installation() {
            Ok(installation) => installation,
            Err(error) => {
                self.update = UpdateState::Error(error);
                cx.notify();
                return;
            }
        };
        self.update = UpdateState::Downloading(0, release.size);
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let progress_tx = tx.clone();
            // The callback only runs on this worker; a cell avoids thousands
            // of queued progress messages on fast downloads.
            let last_report = std::cell::Cell::new(Instant::now() - Duration::from_secs(1));
            let result = plume_updater::prepare(&release, &installation, |received, total| {
                if last_report.get().elapsed() >= Duration::from_millis(100) || received == total {
                    let _ = progress_tx.send(Event::Progress(received, total));
                    last_report.set(Instant::now());
                }
            })
            .map(|plan| (release, plan));
            let _ = tx.send(Event::Prepared(result));
        });
        self.drain_updates(rx, cx);
        cx.notify();
    }

    fn install_update(&mut self, cx: &mut Context<Self>) {
        let UpdateState::Ready { plan, .. } = &self.update else {
            return;
        };
        match plume_updater::start_helper(plan) {
            Ok(()) => cx.quit(),
            Err(error) => {
                self.update = UpdateState::Error(error);
                cx.notify();
            }
        }
    }

    fn drain_updates(&mut self, rx: mpsc::Receiver<Event>, cx: &mut Context<Self>) {
        cx.spawn(async move |view, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(100))
                .await;
            let mut events = Vec::new();
            let mut closed = false;
            loop {
                match rx.try_recv() {
                    Ok(event) => events.push(event),
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        closed = true;
                        break;
                    }
                }
            }
            if !events.is_empty()
                && view
                    .update(cx, |view, cx| {
                        for event in events {
                            view.update = match event {
                                Event::Checked(Ok(Some(release))) => {
                                    UpdateState::Available(release)
                                }
                                Event::Checked(Ok(None)) => UpdateState::Current,
                                Event::Checked(Err(error)) | Event::Prepared(Err(error)) => {
                                    UpdateState::Error(error)
                                }
                                Event::Progress(received, total) => {
                                    UpdateState::Downloading(received, total)
                                }
                                Event::Prepared(Ok((release, plan))) => {
                                    UpdateState::Ready { release, plan }
                                }
                            };
                        }
                        cx.notify();
                    })
                    .is_err()
            {
                return;
            }
            if closed {
                return;
            }
        })
        .detach();
    }
}

pub(super) fn update_notice(
    view: &SettingsView,
    tokens: &Tokens,
    cx: &mut Context<SettingsView>,
) -> Option<AnyElement> {
    let label = match &view.update {
        UpdateState::Available(release) => format!("Plume {} is available", release.version),
        UpdateState::Ready { .. } => "Update ready to install".into(),
        _ => return None,
    };
    Some(
        div()
            .px(px(16.))
            .py(px(8.))
            .bg(tokens.fill)
            .text_xs()
            .flex()
            .justify_between()
            .items_center()
            .child(label)
            .child(button(
                tokens,
                "update-notice",
                "View update",
                cx,
                |view, cx| {
                    view.section = SettingsSection::Updates;
                    cx.notify();
                },
            ))
            .into_any_element(),
    )
}

pub(super) fn updates_page(
    view: &SettingsView,
    tokens: &Tokens,
    cx: &mut Context<SettingsView>,
) -> AnyElement {
    let busy = matches!(
        view.update,
        UpdateState::Checking | UpdateState::Downloading(..)
    );
    let message = match &view.update {
        UpdateState::Idle => "Check for the latest version of Plume.".into(),
        UpdateState::Checking => "Checking for updates…".into(),
        UpdateState::Current => "You're using the latest available version.".into(),
        UpdateState::Available(release) => format!("Plume {} is available.", release.version),
        UpdateState::Downloading(received, total) => {
            let percent = received
                .saturating_mul(100)
                .checked_div(*total)
                .unwrap_or(0);
            if received == total {
                "Verifying and preparing the update…".into()
            } else {
                format!("Downloading update… {percent}%")
            }
        }
        UpdateState::Ready { release, .. } => {
            format!("Plume {} is ready. Restart to install it.", release.version)
        }
        UpdateState::Error(error) => error.clone(),
    };
    let release = match &view.update {
        UpdateState::Available(release) | UpdateState::Ready { release, .. } => Some(release),
        _ => None,
    };
    let body = div().flex().flex_col().gap(px(12.))
        .child(settings_group(tokens).p(px(14.)).flex().flex_col().gap(px(10.))
            .child(format!("Plume {}", env!("CARGO_PKG_VERSION")))
            .child(div().text_sm().text_color(tokens.muted).child(message))
            .child(div().flex().gap(px(8.))
                .when(!busy && !matches!(view.update, UpdateState::Ready { .. }), |row| row.child(button(tokens, "update-check", "Check for updates", cx, |view, cx| view.check_updates(cx))))
                .when(matches!(view.update, UpdateState::Available(_)) && !view.settings_preview, |row| row.child(button(tokens, "update-download", "Download update", cx, |view, cx| view.download_update(cx))))
                .when(matches!(view.update, UpdateState::Ready { .. }), |row| row.child(button(tokens, "update-install", "Restart and install", cx, |view, cx| view.install_update(cx))))
                .child(button(tokens, "update-releases", "GitHub Releases", cx, |_, cx| cx.open_url(plume_updater::RELEASES_URL)))))
        .child(div().text_xs().text_color(tokens.muted).child("Plume checks stable GitHub releases at startup. Updates are installed only when you choose to restart. Your models, settings and history are kept."))
        .children(release.map(|release| {
            let url = release.url.clone();
            div().flex().flex_col().gap(px(8.))
                .child(button(tokens, "update-notes", "Open release notes", cx, move |_, cx| cx.open_url(&url)))
        }));
    page("Updates", body)
}

fn button(
    tokens: &Tokens,
    id: impl Into<ElementId>,
    label: &'static str,
    cx: &mut Context<SettingsView>,
    action: impl Fn(&mut SettingsView, &mut Context<SettingsView>) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .px(px(10.))
        .py(px(6.))
        .rounded(px(6.))
        .bg(tokens.fill)
        .border_1()
        .border_color(tokens.hairline)
        .text_xs()
        .cursor_pointer()
        .hover(|style| style.bg(tokens.fill_hover))
        .on_click(cx.listener(move |view, _, _, cx| action(view, cx)))
        .child(label)
}
