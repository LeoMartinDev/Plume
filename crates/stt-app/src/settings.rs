use std::io::Write;
use std::sync::Mutex;
use std::time::Duration;

use gpui::{
    div, prelude::*, px, rgb, size, App, Bounds, Context, Rgba, SharedString, Subscription,
    TitlebarOptions, Window, WindowAppearance, WindowBounds, WindowHandle, WindowKind,
    WindowOptions,
};

use crate::phase::{AppPhase, OnboardStatus, Progress};
use crate::prefs::{AppearancePref, PackId, Prefs, Scheme};

pub const SETTINGS_TITLE: &str = "stt";

static SETTINGS: Mutex<Option<WindowHandle<SettingsView>>> = Mutex::new(None);

const APPEARANCE_SEGMENTS: [(AppearancePref, &'static str); 3] = [
    (AppearancePref::Fixed(Scheme::Light), "Light"),
    (AppearancePref::Fixed(Scheme::Dark), "Dark"),
    (AppearancePref::Auto, "Auto"),
];

#[derive(Clone, Copy)]
struct Theme {
    canvas: Rgba,
    text: Rgba,
    muted: Rgba,
    hairline: Rgba,
    fill: Rgba,
    accent: Rgba,
}

impl Theme {
    fn resolve(pref: AppearancePref, os: WindowAppearance) -> Self {
        Self::for_scheme(pref.resolve(scheme_from_window(os)))
    }

    fn for_scheme(scheme: Scheme) -> Self {
        match scheme {
            Scheme::Light => Theme {
                canvas: rgb(0xffffff),
                text: rgb(0x141414),
                muted: rgb(0x6b6b6b),
                hairline: rgb(0xe5e5e5),
                fill: rgb(0xf0f0f0),
                accent: rgb(0x3b6dff),
            },
            Scheme::Dark => Theme {
                canvas: rgb(0x141414),
                text: rgb(0xededed),
                muted: rgb(0x8a8a8a),
                hairline: rgb(0x2a2a2a),
                fill: rgb(0x1e1e1e),
                accent: rgb(0x3b6dff),
            },
        }
    }
}

fn scheme_from_window(os: WindowAppearance) -> Scheme {
    match os {
        WindowAppearance::Light | WindowAppearance::VibrantLight => Scheme::Light,
        WindowAppearance::Dark | WindowAppearance::VibrantDark => Scheme::Dark,
    }
}

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
    let bounds = Bounds::centered(None, size(px(520.), px(560.)), cx);
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
        let theme = Theme::resolve(prefs.appearance(), window.appearance());
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

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.canvas)
            .text_color(theme.text)
            .px_6()
            .py_5()
            .gap_4()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().text_sm().text_color(theme.muted).child("stt"))
                    .child(div().text_xl().child("Dictation on this machine"))
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.muted)
                            .child("Audio never leaves the computer."),
                    ),
            )
            .children(warning.map(|text| {
                div()
                    .px_3()
                    .py_2()
                    .border_1()
                    .border_color(theme.hairline)
                    .text_color(theme.muted)
                    .text_sm()
                    .child(text)
            }))
            .child(div().text_sm().text_color(theme.muted).child(status_line))
            .children(
                self.save_error
                    .clone()
                    .map(|text| div().text_sm().text_color(theme.muted).child(text)),
            )
            .child(appearance_group(prefs.appearance(), theme, cx))
            .child(pack_row(
                PackId::Light,
                "Nemotron 0.6B INT4",
                "About 800 MB.",
                true,
                prefs.pack == PackId::Light,
                theme,
                cx,
            ))
            .child(pack_row(
                PackId::Medium,
                "Medium",
                "Visible until a snapshot survives Engine::open.",
                false,
                false,
                theme,
                cx,
            ))
            .child(pack_row(
                PackId::Large,
                "Large",
                "Visible until a snapshot survives Engine::open.",
                false,
                false,
                theme,
                cx,
            ))
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
                            .text_color(theme.muted)
                            .child("Pack and chords apply the next time you open the app."),
                    ),
            )
            .when(cfg!(windows), |el| {
                el.child(
                    div()
                        .text_xs()
                        .text_color(theme.muted)
                        .child("Esc cancel does not fire on Windows. One hook is used for hold."),
                )
            })
            .children(matches!(self.phase, AppPhase::Refused { .. }).then(|| {
                div()
                    .id("retry")
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .bg(theme.accent)
                    .text_color(theme.text)
                    .cursor_pointer()
                    .child("Retry")
                    .on_click(cx.listener(|this, _, _, cx| {
                        if matches!(this.phase, AppPhase::Refused { .. }) {
                            crate::begin_pack(cx, this.phase.prefs().clone());
                        }
                    }))
            }))
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted)
                    .child("Closing this window quits."),
            )
    }
}

fn appearance_group(
    selected: AppearancePref,
    theme: Theme,
    cx: &mut Context<SettingsView>,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(div().text_sm().text_color(theme.muted).child("Appearance"))
        .child(
            div()
                .flex()
                .flex_row()
                .border_1()
                .border_color(theme.hairline)
                .rounded_md()
                .children(APPEARANCE_SEGMENTS.into_iter().map(|(pref, label)| {
                    appearance_segment(pref, label, selected == pref, theme, cx)
                })),
        )
        .child(
            div()
                .text_xs()
                .text_color(theme.muted)
                .child("Auto follows the system appearance."),
        )
}

fn appearance_segment(
    pref: AppearancePref,
    label: &'static str,
    selected: bool,
    theme: Theme,
    cx: &mut Context<SettingsView>,
) -> impl IntoElement {
    div()
        .id(pref.element_id())
        .flex_1()
        .px_3()
        .py_2()
        .cursor_pointer()
        .bg(if selected { theme.fill } else { theme.canvas })
        .text_color(if selected { theme.accent } else { theme.text })
        .child(label)
        .on_click(cx.listener(move |this, _, _, cx| {
            this.commit_appearance(pref, cx);
        }))
}

fn pack_row(
    id: PackId,
    title: &'static str,
    detail: &'static str,
    enabled: bool,
    selected: bool,
    theme: Theme,
    cx: &mut Context<SettingsView>,
) -> impl IntoElement {
    div()
        .id(id.as_str())
        .w_full()
        .px_3()
        .py_3()
        .border_1()
        .border_color(theme.hairline)
        .bg(if selected { theme.fill } else { theme.canvas })
        .flex()
        .flex_col()
        .gap_1()
        .when(enabled, |el| el.cursor_pointer())
        .on_click(cx.listener(move |this, _, _, cx| {
            if !enabled {
                return;
            }
            match &this.phase {
                AppPhase::Live { .. }
                | AppPhase::Onboarding {
                    status: OnboardStatus::Fetching { .. },
                    ..
                } => {}
                AppPhase::Onboarding { .. } | AppPhase::Refused { .. } => {
                    let mut prefs = this.phase.prefs().clone();
                    prefs.pack = id;
                    crate::begin_pack(cx, prefs);
                }
            }
        }))
        .child(
            div()
                .flex()
                .flex_row()
                .justify_between()
                .child(div().child(title))
                .child(div().text_xs().text_color(theme.muted).child(if enabled {
                    "pinned"
                } else {
                    "not pinned"
                })),
        )
        .child(div().text_xs().text_color(theme.muted).child(detail))
}
