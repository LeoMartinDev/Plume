use std::sync::mpsc;
use std::time::{Duration, Instant};

use gpui::{div, prelude::*, px, relative, AnyElement, Context, ElementId, FontWeight};
use plume_ui::Tokens;
use plume_updater::{InstallPlan, Release};

use super::SettingsView;

pub(super) enum UpdateState {
    Idle,
    Checking(Box<UpdateState>),
    Current,
    Available(Release),
    Downloading {
        release: Release,
        received: u64,
        total: u64,
    },
    Ready {
        release: Release,
        plan: InstallPlan,
    },
    Error(String),
}

enum Event {
    Checked(plume_updater::Result<Option<Release>>),
    Progress {
        release: Release,
        received: u64,
        total: u64,
    },
    Prepared(plume_updater::Result<(Release, InstallPlan)>),
}

impl SettingsView {
    pub(super) fn check_updates(&mut self, cx: &mut Context<Self>) {
        if matches!(
            self.update,
            UpdateState::Checking(_) | UpdateState::Downloading { .. } | UpdateState::Ready { .. }
        ) {
            return;
        }
        let previous = std::mem::replace(&mut self.update, UpdateState::Idle);
        self.update = UpdateState::Checking(Box::new(previous));
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
        self.update = UpdateState::Downloading {
            release: release.clone(),
            received: 0,
            total: release.size,
        };
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let progress_tx = tx.clone();
            let progress_release = release.clone();
            // The callback only runs on this worker; a cell avoids thousands
            // of queued progress messages on fast downloads.
            let last_report = std::cell::Cell::new(Instant::now() - Duration::from_secs(1));
            let result = plume_updater::prepare(&release, &installation, |received, total| {
                if last_report.get().elapsed() >= Duration::from_millis(100) || received == total {
                    let _ = progress_tx.send(Event::Progress {
                        release: progress_release.clone(),
                        received,
                        total,
                    });
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
                                Event::Progress {
                                    release,
                                    received,
                                    total,
                                } => UpdateState::Downloading {
                                    release,
                                    received,
                                    total,
                                },
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

impl UpdateState {
    fn status(&self) -> String {
        match self {
            UpdateState::Idle => "Check for a new version".into(),
            UpdateState::Checking(previous) => previous.status(),
            UpdateState::Current => "Plume is up to date".into(),
            UpdateState::Available(release) => format!("Version {} available", release.version),
            UpdateState::Downloading {
                received, total, ..
            } => {
                if *total > 0 && received >= total {
                    "Preparing update…".into()
                } else if *total == 0 {
                    "Downloading…".into()
                } else {
                    format!("Downloading… {}%", download_percent(*received, *total))
                }
            }
            UpdateState::Ready { release, .. } => {
                format!("Version {} ready to install", release.version)
            }
            UpdateState::Error(_) => "Update failed".into(),
        }
    }

    fn notes_release(&self) -> Option<&Release> {
        match self {
            Self::Available(release)
            | Self::Downloading { release, .. }
            | Self::Ready { release, .. } => Some(release),
            Self::Checking(previous) => previous.notes_release(),
            _ => None,
        }
    }

    #[cfg(test)]
    fn notes_label(&self) -> String {
        match self.notes_release() {
            Some(release) => super::release_notes::NotesContent::link_label(&release.version, true),
            None => {
                super::release_notes::NotesContent::link_label(env!("CARGO_PKG_VERSION"), false)
            }
        }
    }

    pub(super) fn notes_content(&self) -> super::release_notes::NotesContent {
        match self.notes_release() {
            Some(release) => {
                super::release_notes::NotesContent::update(&release.version, &release.notes)
            }
            None => super::release_notes::NotesContent::installed(),
        }
    }

    pub(super) fn needs_attention(&self) -> bool {
        matches!(self, Self::Available(_) | Self::Ready { .. })
    }
}

/// Visual fixtures used only by the isolated settings preview.
pub(super) fn preview_state() -> Option<UpdateState> {
    let state = std::env::args()
        .find_map(|arg| arg.strip_prefix("--preview-update=").map(str::to_owned))?;
    Some(preview_state_for(&state))
}

pub(super) fn preview_state_for(state: &str) -> UpdateState {
    let release = Release {
        version: "0.2.0".into(),
        notes: String::new(),
        url: plume_updater::RELEASES_URL.into(),
        installer_name: String::new(),
        installer_url: String::new(),
        checksum_url: String::new(),
        size: 100,
    };
    match state {
        "checking" => UpdateState::Checking(Box::new(UpdateState::Current)),
        "available" => UpdateState::Available(release),
        "downloading" => UpdateState::Downloading {
            release,
            received: 42,
            total: 100,
        },
        "preparing" => UpdateState::Downloading {
            release,
            received: 100,
            total: 100,
        },
        "ready" => UpdateState::Ready {
            release,
            plan: InstallPlan {
                installer: Default::default(),
                version: "0.2.0".into(),
                target: Default::default(),
                executable: Default::default(),
                parent_pid: std::process::id(),
                work: Default::default(),
            },
        },
        "error" => UpdateState::Error(
            "Could not reach the update server. Check your connection and try again.".into(),
        ),
        "idle" => UpdateState::Idle,
        _ => UpdateState::Current,
    }
}

pub(super) fn about_page(
    view: &SettingsView,
    tokens: &Tokens,
    cx: &mut Context<SettingsView>,
) -> AnyElement {
    let busy = matches!(
        view.update,
        UpdateState::Checking(_) | UpdateState::Downloading { .. }
    );
    let ready = matches!(view.update, UpdateState::Ready { .. });
    let available = matches!(view.update, UpdateState::Available(_));
    let downloading = matches!(view.update, UpdateState::Downloading { .. });
    let status = view.update.status();
    let software_update = div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(16.))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(16.))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_sm()
                        .text_color(tokens.muted)
                        .child(status),
                )
                .child(
                    div()
                        .flex()
                        .flex_shrink_0()
                        .gap(px(8.))
                        .items_center()
                        .child(button(
                            tokens,
                            "update-check",
                            "Check for updates",
                            !busy && !ready,
                            cx,
                            |view, cx| view.check_updates(cx),
                        ))
                        .when(available || downloading, |row| {
                            row.child(button(
                                tokens,
                                "update-download",
                                "Download update",
                                available && !view.settings_preview,
                                cx,
                                |view, cx| view.download_update(cx),
                            ))
                        })
                        .when(ready, |row| {
                            row.child(button(
                                tokens,
                                "update-install",
                                "Restart Plume",
                                !view.settings_preview,
                                cx,
                                |view, cx| view.install_update(cx),
                            ))
                        }),
                ),
        )
        .when(downloading, |section| {
            let UpdateState::Downloading {
                received, total, ..
            } = &view.update
            else {
                unreachable!()
            };
            section.child(
                div()
                    .w_full()
                    .h(px(3.))
                    .rounded_full()
                    .bg(tokens.fill)
                    .overflow_hidden()
                    .child(
                        div()
                            .h_full()
                            .w(relative(download_percent(*received, *total) as f32 / 100.))
                            .bg(tokens.accent),
                    ),
            )
        })
        .when(matches!(view.update, UpdateState::Error(_)), |section| {
            let UpdateState::Error(error) = &view.update else {
                unreachable!()
            };
            section.child(
                div()
                    .id("update-error-detail")
                    .max_h(px(64.))
                    .overflow_y_scroll()
                    .text_xs()
                    .text_color(tokens.muted)
                    .child(error.clone()),
            )
        });
    div()
        .w_full()
        .max_w(px(480.))
        .flex()
        .flex_col()
        .gap(px(20.))
        .child(
            div()
                .flex()
                .justify_between()
                .items_center()
                .child(
                    div()
                        .text_lg()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("About"),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(tokens.muted)
                        .child(format!("Version {}", env!("CARGO_PKG_VERSION"))),
                ),
        )
        .child(software_update)
        .child(super::release_notes::inline_notes(
            &view.update.notes_content(),
            tokens,
        ))
        .into_any_element()
}

fn download_percent(received: u64, total: u64) -> u64 {
    received
        .saturating_mul(100)
        .checked_div(total)
        .unwrap_or(0)
        .min(100)
}

fn button(
    tokens: &Tokens,
    id: impl Into<ElementId>,
    label: &'static str,
    enabled: bool,
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
        .whitespace_nowrap()
        .flex_shrink_0()
        .when(!enabled, |el| el.opacity(0.45))
        .when(enabled, |el| {
            el.cursor_pointer()
                .hover(|style| style.bg(tokens.fill_hover))
                .on_click(cx.listener(move |view, _, _, cx| action(view, cx)))
        })
        .child(label)
}

#[cfg(test)]
mod notes_tests {
    use super::*;

    #[test]
    fn checking_preserves_status_and_notes_until_the_result_arrives() {
        for state in ["idle", "current", "available"] {
            let previous = preview_state_for(state);
            let status = previous.status();
            let notes = previous.notes_label();
            let checking = UpdateState::Checking(Box::new(previous));
            assert_eq!(checking.status(), status);
            assert_eq!(checking.notes_label(), notes);
        }
    }

    #[test]
    fn pending_update_notes_keep_the_target_version_through_download_and_preparation() {
        for state in ["available", "downloading", "preparing", "ready"] {
            let update = preview_state_for(state);
            assert_eq!(update.notes_label(), "What's new in version 0.2.0");
            assert_eq!(
                update.notes_label(),
                super::super::release_notes::NotesContent::link_label("0.2.0", true)
            );
        }
        for state in [
            UpdateState::Idle,
            UpdateState::Checking(Box::new(UpdateState::Idle)),
            UpdateState::Current,
            UpdateState::Error("offline".into()),
        ] {
            assert_eq!(
                state.notes_label(),
                format!("Installed release notes — {}", env!("CARGO_PKG_VERSION"))
            );
        }
    }
}
