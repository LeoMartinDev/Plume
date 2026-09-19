use std::io::Write;
use std::sync::Mutex;
use std::time::Duration;

use gpui::{
    div, prelude::*, px, rgb, size, App, Bounds, Context, FocusHandle, FontWeight, KeyDownEvent,
    SharedString, Subscription, TitlebarOptions, Window, WindowBounds, WindowHandle, WindowKind,
    WindowOptions,
};
use stt_ui::{AccentButton, InsetRow, ListGroup, Palette, Tokens};

use crate::hold::{
    classify_keydown, pill_hint, pill_label, CaptureEffect, ChordText, HoldCapture, ModBits,
};
use crate::phase::{AppPhase, OnboardStatus, Progress};
use crate::prefs::{AppearancePref, PackId, Prefs, Scheme, DEFAULT_HOLD};

pub const SETTINGS_TITLE: &str = "stt";
const HOLD_CAPTURE_ID: &str = "hold-capture";

static SETTINGS: Mutex<Option<WindowHandle<SettingsView>>> = Mutex::new(None);

const THEME_CARDS: [(AppearancePref, &str); 3] = [
    (AppearancePref::Auto, "Auto"),
    (AppearancePref::Fixed(Scheme::Light), "Light"),
    (AppearancePref::Fixed(Scheme::Dark), "Dark"),
];

pub struct SettingsView {
    phase: AppPhase,
    save_error: Option<String>,
    capture: HoldCapture,
    hold_focus: FocusHandle,
    _appearance: Subscription,
}

impl SettingsView {
    fn reset_for_phase(&mut self) {
        self.capture.cancel();
    }

    pub fn set_phase(&mut self, phase: AppPhase, cx: &mut Context<Self>) {
        self.reset_for_phase();
        self.phase = phase;
        cx.notify();
    }

    pub fn toggle_hold_capture(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.capture.is_listening() {
            self.capture.cancel();
            window.blur();
            cx.notify();
            return;
        }
        self.capture.begin();
        window.focus(&self.hold_focus);
        cx.notify();
    }

    pub fn on_hold_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        window.prevent_default();
        let stroke = classify_keydown(
            event.is_held,
            event.keystroke.key.as_str(),
            ModBits::from_gpui(event.keystroke.modifiers),
        );
        match self.capture.apply(stroke) {
            CaptureEffect::None | CaptureEffect::StayListening => {}
            CaptureEffect::Cancelled | CaptureEffect::Rejected(_) => {
                window.blur();
                cx.notify();
            }
            CaptureEffect::Offer(chord) => self.commit_hold(chord, window, cx),
        }
    }

    fn reset_hold(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.capture.cancel();
        let previous = self.phase.prefs().hold().to_string();
        match self.phase.prefs_mut().try_set_hold(DEFAULT_HOLD) {
            Err(err) => {
                self.capture.set_reject(err.to_string());
                window.blur();
                cx.notify();
            }
            Ok(()) if previous == DEFAULT_HOLD => {
                window.blur();
                cx.notify();
            }
            Ok(()) => {
                match crate::prefs::save(self.phase.prefs()) {
                    Ok(()) => self.save_error = None,
                    Err(err) => self.save_error = Some(err.to_string()),
                }
                window.blur();
                cx.notify();
            }
        }
    }

    fn commit_hold(&mut self, chord: ChordText, window: &mut Window, cx: &mut Context<Self>) {
        let previous = self.phase.prefs().hold().to_string();
        match self.phase.prefs_mut().try_set_hold(chord.as_str()) {
            Err(err) => {
                self.capture.set_reject(err.to_string());
                window.blur();
                cx.notify();
            }
            Ok(()) if previous == chord.as_str() => {
                window.blur();
                cx.notify();
            }
            Ok(()) => {
                match crate::prefs::save(self.phase.prefs()) {
                    Ok(()) => self.save_error = None,
                    Err(err) => self.save_error = Some(err.to_string()),
                }
                window.blur();
                cx.notify();
            }
        }
    }

    pub fn commit_appearance(&mut self, next: AppearancePref, cx: &mut Context<Self>) {
        let was_listening = self.capture.is_listening();
        self.capture.cancel();
        if self.phase.prefs().appearance() == next {
            if was_listening {
                cx.notify();
            }
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
        self.reset_for_phase();
        if let AppPhase::Onboarding { status, .. } = &mut self.phase {
            *status = OnboardStatus::Fetching { last };
        }
        cx.notify();
    }

    pub fn show_fetch_failed(&mut self, reason: String, cx: &mut Context<Self>) {
        self.reset_for_phase();
        if let AppPhase::Onboarding { status, .. } = &mut self.phase {
            *status = OnboardStatus::Failed { reason };
        }
        cx.notify();
    }

    pub fn show_live(&mut self, cx: &mut Context<Self>) {
        self.reset_for_phase();
        self.phase = AppPhase::Live {
            prefs: self.phase.prefs().clone(),
        };
        cx.notify();
    }

    pub fn show_refused(&mut self, reason: String, cx: &mut Context<Self>) {
        self.reset_for_phase();
        self.phase = AppPhase::Refused {
            prefs: self.phase.prefs().clone(),
            reason,
        };
        cx.notify();
    }
}

pub fn open_settings(cx: &mut App, phase: AppPhase) {
    let bounds = Bounds::centered(None, size(px(520.), px(680.)), cx);
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
                        capture: HoldCapture::idle(),
                        hold_focus: cx.focus_handle().tab_stop(true),
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
        let listening = self.capture.is_listening();
        let reject = self.capture.reject().map(str::to_string);
        let phase = self.capture.phase();
        let pill = if listening {
            SharedString::from(pill_label(phase, prefs.hold()).to_string())
        } else {
            SharedString::from(pretty_hold(prefs.hold()))
        };

        tokens
            .page()
            .child(header(&tokens))
            .children(warning.map(|text| warning_line(&tokens, text)))
            .child(div().text_sm().text_color(tokens.muted).child(status_line))
            .child(hold_hero(
                &tokens,
                pill,
                SharedString::from(pill_hint(phase)),
                &self.hold_focus,
                listening,
                cx,
            ))
            .children(reject.map(|text| muted_error_line(&tokens, text)))
            .children(
                self.save_error
                    .clone()
                    .map(|text| muted_error_line(&tokens, text)),
            )
            .child(pack_group(&tokens, &prefs, cx))
            .child(theme_cards(prefs.appearance(), &tokens, cx))
            .child(cancel_line(&tokens, prefs.cancel()))
            .when(cfg!(windows), |el| el.child(windows_esc_note(&tokens)))
            .children(matches!(self.phase, AppPhase::Refused { .. }).then(|| {
                AccentButton::new(tokens, "retry", "Retry", cx, |this, cx| {
                    if matches!(this.phase, AppPhase::Refused { .. }) {
                        crate::begin_pack(cx, this.phase.prefs().clone());
                    }
                })
            }))
            .child(footer(&tokens))
    }
}

fn header(tokens: &Tokens) -> impl IntoElement {
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
        )
}

fn warning_line(tokens: &Tokens, text: String) -> impl IntoElement {
    div()
        .px_3()
        .py_2()
        .border_1()
        .border_color(tokens.hairline)
        .text_color(tokens.muted)
        .text_sm()
        .child(text)
}

fn muted_error_line(tokens: &Tokens, text: String) -> impl IntoElement {
    div().text_sm().text_color(tokens.muted).child(text)
}

fn pretty_hold(hold: &str) -> String {
    hold.split('+').collect::<Vec<_>>().join(" + ")
}

fn hold_hero(
    tokens: &Tokens,
    pill: SharedString,
    hint: SharedString,
    hold_focus: &FocusHandle,
    listening: bool,
    cx: &mut Context<SettingsView>,
) -> impl IntoElement {
    div()
        .id(HOLD_CAPTURE_ID)
        .track_focus(hold_focus)
        .flex()
        .flex_col()
        .gap_2()
        .px_3()
        .py_3()
        .rounded_md()
        .border_1()
        .border_color(if listening {
            tokens.accent
        } else {
            tokens.hairline
        })
        .bg(tokens.elevated)
        .cursor_pointer()
        .hover(|style| style.border_color(tokens.accent))
        .on_click(cx.listener(|this, _ev, window, cx| {
            this.toggle_hold_capture(window, cx);
        }))
        .when(listening, |el| {
            el.on_key_down(cx.listener(SettingsView::on_hold_key))
        })
        .child(
            div()
                .text_xs()
                .text_color(tokens.muted)
                .child("Push to talk"),
        )
        .child(div().text_lg().child("Your shortcut"))
        .child(div().text_sm().text_color(tokens.muted).child(hint))
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .px_3()
                        .py_2()
                        .rounded_md()
                        .bg(tokens.fill)
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .child(pill),
                )
                .child(
                    div()
                        .id("hold-reset")
                        .px_3()
                        .py_2()
                        .rounded_md()
                        .border_1()
                        .border_color(tokens.hairline)
                        .text_sm()
                        .cursor_pointer()
                        .hover(|style| style.bg(tokens.fill_hover))
                        .on_click(cx.listener(|this, _ev, window, cx| {
                            // Hero root toggles listen. Stop Reset from bubbling into begin.
                            cx.stop_propagation();
                            this.reset_hold(window, cx);
                        }))
                        .child("Reset"),
                ),
        )
}

fn pack_group(tokens: &Tokens, prefs: &Prefs, cx: &mut Context<SettingsView>) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(div().text_sm().text_color(tokens.muted).child("Model"))
        .child(
            ListGroup::new(*tokens)
                .child(
                    InsetRow::new(*tokens, PackId::Light.as_str(), "Nemotron 0.6B INT4")
                        .meta(pack_meta(PackId::Light, prefs.pack))
                        .detail("About 800 MB.")
                        .selected(prefs.pack == PackId::Light)
                        .on_click(cx, |this, cx| {
                            this.capture.cancel();
                            match &this.phase {
                                AppPhase::Live { .. }
                                | AppPhase::Onboarding {
                                    status: OnboardStatus::Fetching { .. },
                                    ..
                                } => {
                                    cx.notify();
                                }
                                AppPhase::Onboarding { .. } | AppPhase::Refused { .. } => {
                                    let mut prefs = this.phase.prefs().clone();
                                    prefs.pack = PackId::Light;
                                    crate::begin_pack(cx, prefs);
                                }
                            }
                        }),
                )
                .child(
                    InsetRow::new(*tokens, PackId::Medium.as_str(), "Medium")
                        .meta(pack_meta(PackId::Medium, prefs.pack))
                        .detail("Visible until a snapshot survives Engine::open."),
                )
                .child(
                    InsetRow::new(*tokens, PackId::Large.as_str(), "Large")
                        .meta(pack_meta(PackId::Large, prefs.pack))
                        .detail("Visible until a snapshot survives Engine::open."),
                ),
        )
}

fn pack_meta(id: PackId, selected: PackId) -> &'static str {
    if id == selected {
        "active"
    } else if id == PackId::Light {
        "pinned"
    } else {
        "not pinned"
    }
}

fn theme_cards(
    selected: AppearancePref,
    tokens: &Tokens,
    cx: &mut Context<SettingsView>,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(div().text_sm().text_color(tokens.muted).child("Appearance"))
        .child(
            div().flex().flex_row().gap_2().children(
                THEME_CARDS
                    .into_iter()
                    .map(|(pref, label)| theme_card(pref, label, selected, tokens, cx)),
            ),
        )
        .child(
            div()
                .text_xs()
                .text_color(tokens.muted)
                .child("Auto follows the system appearance."),
        )
}

fn theme_card(
    pref: AppearancePref,
    label: &'static str,
    selected: AppearancePref,
    tokens: &Tokens,
    cx: &mut Context<SettingsView>,
) -> impl IntoElement {
    let is_selected = selected == pref;
    div()
        .id(pref.element_id())
        .flex_1()
        .flex()
        .flex_col()
        .gap_2()
        .px_2()
        .py_2()
        .rounded_md()
        .border_1()
        .border_color(if is_selected {
            tokens.accent
        } else {
            tokens.hairline
        })
        .bg(if is_selected {
            tokens.fill
        } else {
            tokens.canvas
        })
        .cursor_pointer()
        .hover(|style| style.bg(tokens.fill_hover))
        .on_click(cx.listener(move |this, _ev, _window, cx| {
            this.commit_appearance(pref, cx);
        }))
        .child(theme_swatch(pref))
        .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(label))
}

fn theme_swatch(pref: AppearancePref) -> impl IntoElement {
    match pref {
        AppearancePref::Fixed(Scheme::Light) => div()
            .h(px(34.))
            .rounded_md()
            .border_1()
            .border_color(rgb(0xe5e5e5))
            .bg(rgb(0xf7f7f7))
            .into_any_element(),
        AppearancePref::Fixed(Scheme::Dark) => div()
            .h(px(34.))
            .rounded_md()
            .border_1()
            .border_color(rgb(0x2b2b2b))
            .bg(rgb(0x181818))
            .into_any_element(),
        AppearancePref::Auto => div()
            .h(px(34.))
            .flex()
            .flex_row()
            .rounded_md()
            .overflow_hidden()
            .border_1()
            .border_color(rgb(0x2b2b2b))
            .child(div().flex_1().h_full().bg(rgb(0x181818)))
            .child(div().flex_1().h_full().bg(rgb(0xf7f7f7)))
            .into_any_element(),
    }
}

fn cancel_line(tokens: &Tokens, cancel: &str) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .text_sm()
                .text_color(tokens.muted)
                .child(format!("Cancel  {cancel}")),
        )
        .child(
            div()
                .text_xs()
                .text_color(tokens.muted)
                .child("Pack and chords apply the next time you open the app."),
        )
}

fn windows_esc_note(tokens: &Tokens) -> impl IntoElement {
    div()
        .text_xs()
        .text_color(tokens.muted)
        .child("Esc cancel does not fire on Windows. One hook is used for hold.")
}

fn footer(tokens: &Tokens) -> impl IntoElement {
    div()
        .text_xs()
        .text_color(tokens.muted)
        .child("Closing this window quits.")
}

#[cfg(test)]
mod tests {
    use super::pretty_hold;

    #[test]
    fn pretty_hold_spaces_tokens_and_leaves_a_bare_trigger() {
        assert_eq!(pretty_hold("Ctrl+Space"), "Ctrl + Space");
        assert_eq!(pretty_hold("Alt+a"), "Alt + a");
        assert_eq!(pretty_hold("F9"), "F9");
    }
}
