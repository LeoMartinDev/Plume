use std::io::Write;
use std::sync::Mutex;
use std::time::Duration;

use gpui::{
    div, prelude::*, px, size, App, Bounds, Context, SharedString, Subscription, TitlebarOptions,
    Window, WindowBounds, WindowHandle, WindowKind, WindowOptions,
};
use stt_ui::{AccentButton, InsetRow, ListGroup, Palette, Segment, Segmented, Tokens};

use crate::phase::{AppPhase, OnboardStatus, Progress};
use crate::prefs::{AppearancePref, PackId, Prefs, Scheme};

pub const SETTINGS_TITLE: &str = "stt";

static SETTINGS: Mutex<Option<WindowHandle<SettingsView>>> = Mutex::new(None);

const APPEARANCE_SEGMENTS: [(AppearancePref, &str); 3] = [
    (AppearancePref::Fixed(Scheme::Light), "Light"),
    (AppearancePref::Fixed(Scheme::Dark), "Dark"),
    (AppearancePref::Auto, "Auto"),
];

pub struct SettingsView {
    phase: AppPhase,
    save_error: Option<String>,
    _appearance: Subscription,
}

impl SettingsView {
    pub fn set_phase(&mut self, phase: AppPhase, cx: &mut Context<Self>) {
        self.phase = phase;
        cx.notify();
    }

    pub fn commit_appearance(&mut self, next: AppearancePref, cx: &mut Context<Self>) {
        if self.phase.prefs().appearance() == next {
            return;
        }
        self.phase.prefs_mut().set_appearance(next);
        match crate::prefs::save(self.phase.prefs()) {
            Ok(()) => self.save_error = None,
            Err(err) => self.save_error = Some(err.to_string()),
        }
        cx.notify();
    }

    pub fn show_progress(&mut self, last: Progress, cx: &mut Context<Self>) {
        if let AppPhase::Onboarding { status, .. } = &mut self.phase {
            *status = OnboardStatus::Fetching { last };
            cx.notify();
        }
    }

    pub fn show_fetch_failed(&mut self, reason: String, cx: &mut Context<Self>) {
        if let AppPhase::Onboarding { status, .. } = &mut self.phase {
            *status = OnboardStatus::Failed { reason };
            cx.notify();
        }
    }

    pub fn show_live(&mut self, cx: &mut Context<Self>) {
        self.phase = AppPhase::Live {
            prefs: self.phase.prefs().clone(),
        };
        cx.notify();
    }

    pub fn show_refused(&mut self, reason: String, cx: &mut Context<Self>) {
        self.phase = AppPhase::Refused {
            prefs: self.phase.prefs().clone(),
            reason,
        };
        cx.notify();
    }
}

pub fn open_settings(cx: &mut App, phase: AppPhase) {
    let bounds = Bounds::centered(None, size(px(520.), px(640.)), cx);
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
            |window, cx| {
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
                cx.new(|cx| {
                    let _appearance = cx.observe_window_appearance(window, |_, _, cx| {
                        cx.notify();
                    });
                    SettingsView {
                        phase,
                        save_error: None,
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

pub fn settings_window_show_live(cx: &mut App) {
    let handle = *SETTINGS.lock().expect("settings handle");
    if let Some(handle) = handle {
        let _ = handle.update(cx, |view, _window, cx| view.show_live(cx));
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
            .update(cx, |view, _window, _cx| view.phase.prefs().clone())
            .ok()
    })
}

fn print_opened_line() {
    eprintln!(
        "stt-app: window opened title={SETTINGS_TITLE} display={}",
        std::env::var("DISPLAY").unwrap_or_else(|_| "<unset>".into())
    );
    let _ = std::io::stderr().flush();
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let prefs = self.phase.prefs().clone();
        let tokens = Tokens::new(match prefs.appearance() {
            AppearancePref::Fixed(Scheme::Light) => Palette::Light,
            AppearancePref::Fixed(Scheme::Dark) => Palette::Dark,
            AppearancePref::Auto => Palette::from_window(window),
        });
        let warning = match &self.phase {
            AppPhase::Onboarding { warning, .. } => warning.clone(),
            AppPhase::Refused { reason, .. } => Some(reason.clone()),
            AppPhase::Live { .. } => None,
        };
        let status_line = match &self.phase {
            AppPhase::Onboarding { status, .. } => match status {
                OnboardStatus::Idle => SharedString::from("Download the pinned pack to start."),
                OnboardStatus::Fetching { last } => SharedString::from(match last.total {
                    Some(total) => {
                        format!("Fetching {} ({}/{} bytes)", last.file, last.bytes, total)
                    }
                    None => format!("Fetching {} ({} bytes)", last.file, last.bytes),
                }),
                OnboardStatus::Failed { reason } => SharedString::from(reason.clone()),
            },
            AppPhase::Live { .. } => SharedString::from("Hold the chord and speak."),
            AppPhase::Refused { .. } => SharedString::from(
                "The injector or hold chord refused. Retry after switching to X11.",
            ),
        };

        tokens
            .page()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().text_sm().text_color(tokens.muted).child("stt"))
                    .child(div().text_xl().child("Dictation on this machine"))
                    .child(
                        div()
                            .text_sm()
                            .text_color(tokens.muted)
                            .child("Audio never leaves the computer."),
                    ),
            )
            .children(warning.map(|text| {
                div()
                    .px_3()
                    .py_2()
                    .border_1()
                    .border_color(tokens.hairline)
                    .text_color(tokens.muted)
                    .text_sm()
                    .child(text)
            }))
            .child(div().text_sm().text_color(tokens.muted).child(status_line))
            .children(
                self.save_error
                    .clone()
                    .map(|text| div().text_sm().text_color(tokens.muted).child(text)),
            )
            .child(appearance_group(prefs.appearance(), tokens, cx))
            .child(
                ListGroup::new(tokens)
                    .child(
                        InsetRow::new(tokens, PackId::Light.as_str(), "Nemotron 0.6B INT4")
                            .meta("pinned")
                            .detail("About 800 MB.")
                            .selected(prefs.pack == PackId::Light)
                            .on_click(cx, |this, cx| match &this.phase {
                                AppPhase::Live { .. }
                                | AppPhase::Onboarding {
                                    status: OnboardStatus::Fetching { .. },
                                    ..
                                } => {}
                                AppPhase::Onboarding { .. } | AppPhase::Refused { .. } => {
                                    let mut prefs = this.phase.prefs().clone();
                                    prefs.pack = PackId::Light;
                                    crate::begin_pack(cx, prefs);
                                }
                            }),
                    )
                    .child(
                        InsetRow::new(tokens, PackId::Medium.as_str(), "Medium")
                            .meta("not pinned")
                            .detail("Visible until a snapshot survives Engine::open."),
                    )
                    .child(
                        InsetRow::new(tokens, PackId::Large.as_str(), "Large")
                            .meta("not pinned")
                            .detail("Visible until a snapshot survives Engine::open."),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .mt_2()
                    .child(div().text_sm().child(format!("Hold  {}", prefs.hold())))
                    .child(div().text_sm().child(format!("Cancel  {}", prefs.cancel())))
                    .child(
                        div()
                            .text_xs()
                            .text_color(tokens.muted)
                            .child("Pack and chords apply the next time you open the app."),
                    ),
            )
            .when(cfg!(windows), |el| {
                el.child(
                    div()
                        .text_xs()
                        .text_color(tokens.muted)
                        .child("Esc cancel does not fire on Windows. One hook is used for hold."),
                )
            })
            .children(matches!(self.phase, AppPhase::Refused { .. }).then(|| {
                AccentButton::new(tokens, "retry", "Retry", cx, |this, cx| {
                    if matches!(this.phase, AppPhase::Refused { .. }) {
                        crate::begin_pack(cx, this.phase.prefs().clone());
                    }
                })
            }))
            .child(
                div()
                    .text_xs()
                    .text_color(tokens.muted)
                    .child("Closing this window quits."),
            )
    }
}

fn appearance_group(
    selected: AppearancePref,
    tokens: Tokens,
    cx: &mut Context<SettingsView>,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(div().text_sm().text_color(tokens.muted).child("Appearance"))
        .child(Segmented::new(
            tokens,
            selected,
            APPEARANCE_SEGMENTS.into_iter().map(|(pref, label)| {
                Segment::new(pref, pref.element_id(), label)
                    .on_click(cx, move |this, cx| this.commit_appearance(pref, cx))
            }),
        ))
        .child(
            div()
                .text_xs()
                .text_color(tokens.muted)
                .child("Auto follows the system appearance."),
        )
}
