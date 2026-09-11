use std::io::Write;
use std::sync::Mutex;
use std::time::Duration;

use gpui::{
    div, prelude::*, px, rgb, size, App, Bounds, Context, SharedString, TitlebarOptions, Window,
    WindowBounds, WindowHandle, WindowKind, WindowOptions,
};

use crate::phase::{AppPhase, OnboardStatus};
use crate::prefs::PackId;

pub const SETTINGS_TITLE: &str = "stt";

static SETTINGS: Mutex<Option<WindowHandle<SettingsView>>> = Mutex::new(None);

pub struct SettingsView {
    phase: AppPhase,
}

impl SettingsView {
    pub fn set_phase(&mut self, phase: AppPhase, cx: &mut Context<Self>) {
        self.phase = phase;
        cx.notify();
    }
}

pub fn open_settings(cx: &mut App, phase: AppPhase) {
    let bounds = Bounds::centered(None, size(px(520.), px(520.)), cx);
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
                cx.new(|_cx| SettingsView { phase })
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

fn print_opened_line() {
    eprintln!(
        "stt-app: window opened title={SETTINGS_TITLE} display={}",
        std::env::var("DISPLAY").unwrap_or_else(|_| "<unset>".into())
    );
    let _ = std::io::stderr().flush();
}

impl Render for SettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let prefs = match &self.phase {
            AppPhase::Onboarding { prefs, .. }
            | AppPhase::Live { prefs }
            | AppPhase::Refused { prefs, .. } => prefs.clone(),
        };
        let warning = match &self.phase {
            AppPhase::Onboarding { warning, .. } => warning.clone(),
            AppPhase::Refused { reason, .. } => Some(reason.clone()),
            AppPhase::Live { .. } => None,
        };
        let status_line = match &self.phase {
            AppPhase::Onboarding { status, .. } => match status {
                OnboardStatus::Idle => SharedString::from("Pick Light to download the pack."),
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
            .bg(rgb(0x141414))
            .text_color(rgb(0xe8e4dc))
            .px_6()
            .py_5()
            .gap_4()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().text_sm().text_color(rgb(0x8a8378)).child("stt"))
                    .child(div().text_xl().child("Dictation on this machine"))
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(0x8a8378))
                            .child("Audio never leaves the computer."),
                    ),
            )
            .children(warning.map(|text| {
                div()
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .bg(rgb(0x3a2418))
                    .text_color(rgb(0xf0c8a0))
                    .text_sm()
                    .child(text)
            }))
            .child(div().text_sm().text_color(rgb(0xc8c2b6)).child(status_line))
            .child(pack_row(
                PackId::Light,
                "Nemotron 0.6B INT4",
                "Pinned. About 800 MB.",
                true,
                prefs.pack == PackId::Light,
                cx,
            ))
            .child(pack_row(
                PackId::Medium,
                "Medium",
                "Visible until a snapshot survives Engine::open.",
                false,
                false,
                cx,
            ))
            .child(pack_row(
                PackId::Large,
                "Large",
                "Visible until a snapshot survives Engine::open.",
                false,
                false,
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
                            .text_color(rgb(0x8a8378))
                            .child("Chord and pack edits apply the next time you open the app."),
                    ),
            )
            .when(cfg!(windows), |el| {
                el.child(
                    div()
                        .text_xs()
                        .text_color(rgb(0xc4a070))
                        .child("Esc cancel does not fire on Windows. One hook is used for hold."),
                )
            })
            .children(matches!(self.phase, AppPhase::Refused { .. }).then(|| {
                div()
                    .id("retry")
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .bg(rgb(0x3a3832))
                    .cursor_pointer()
                    .child("Retry")
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let AppPhase::Refused { prefs, .. } = &this.phase {
                            crate::begin_pack(cx, prefs.clone());
                        }
                    }))
            }))
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(0x6e685e))
                    .child("Closing this window quits."),
            )
    }
}

fn pack_row(
    id: PackId,
    title: &'static str,
    detail: &'static str,
    enabled: bool,
    selected: bool,
    cx: &mut Context<SettingsView>,
) -> impl IntoElement {
    let dest = id.data_dir();
    let border = if selected {
        rgb(0xd4a017)
    } else if enabled {
        rgb(0x3a3832)
    } else {
        rgb(0x2a2824)
    };
    div()
        .id(id.as_str())
        .w_full()
        .px_3()
        .py_3()
        .rounded_md()
        .border_1()
        .border_color(border)
        .bg(if enabled {
            rgb(0x1c1b18)
        } else {
            rgb(0x181714)
        })
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
                AppPhase::Onboarding { prefs, .. } | AppPhase::Refused { prefs, .. } => {
                    let mut prefs = prefs.clone();
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
                .child(
                    div()
                        .text_xs()
                        .text_color(if enabled {
                            rgb(0xd4a017)
                        } else {
                            rgb(0x6e685e)
                        })
                        .child(if enabled { "available" } else { "not pinned" }),
                ),
        )
        .child(div().text_xs().text_color(rgb(0x8a8378)).child(detail))
        .child(
            div()
                .text_xs()
                .text_color(rgb(0x6e685e))
                .child(dest.display().to_string()),
        )
}
