use std::io::Write;
use std::sync::Mutex;
use std::time::Duration;

use gpui::{
    div, prelude::*, px, size, svg, AnyElement, App, Bounds, Context, Div, FocusHandle, FontWeight,
    KeyDownEvent, SharedString, Subscription, TitlebarOptions, Window, WindowBounds, WindowHandle,
    WindowKind, WindowOptions,
};
use stt_engine::{Language, LanguageTarget};
use stt_session::HoldTarget;
use stt_ui::{Palette, Segment, Segmented, Tokens};

use crate::hold::{classify_keydown, pill_label, CaptureEffect, ChordText, HoldCapture, ModBits};
use crate::phase::{AppPhase, OnboardStatus, Progress};
use crate::prefs::{AppearancePref, LanguagePref, PackId, Prefs, Scheme, DEFAULT_HOLD};

pub const SETTINGS_TITLE: &str = "stt";
const HOLD_CAPTURE_ID: &str = "hold-capture";

static SETTINGS: Mutex<Option<WindowHandle<SettingsView>>> = Mutex::new(None);

const THEMES: [(AppearancePref, &str); 3] = [
    (AppearancePref::Auto, "System"),
    (AppearancePref::Fixed(Scheme::Light), "Light"),
    (AppearancePref::Fixed(Scheme::Dark), "Dark"),
];

const LANGUAGES: [LanguagePref; 3] = [
    LanguagePref::Auto,
    LanguagePref::French,
    LanguagePref::English,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SettingsSection {
    Dictation,
    Model,
    Appearance,
}

impl SettingsSection {
    fn title(self) -> &'static str {
        match self {
            Self::Dictation => "Dictation",
            Self::Model => "Model",
            Self::Appearance => "Appearance",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::Dictation => "fluent/mic.svg",
            Self::Model => "fluent/cube.svg",
            Self::Appearance => "fluent/theme.svg",
        }
    }
}

pub struct SettingsView {
    phase: AppPhase,
    section: SettingsSection,
    language_open: bool,
    save_error: Option<String>,
    capture: HoldCapture,
    hold_focus: FocusHandle,
    hold_target: Option<HoldTarget>,
    language_target: Option<LanguageTarget>,
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
                self.save_prefs();
                self.retarget_hold(DEFAULT_HOLD);
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
                self.save_prefs();
                self.retarget_hold(chord.as_str());
                window.blur();
                cx.notify();
            }
        }
    }

    fn save_prefs(&mut self) {
        self.save_error = crate::prefs::save(self.phase.prefs())
            .err()
            .map(|err| err.to_string());
    }

    fn retarget_hold(&self, hold: &str) {
        if let Some(target) = &self.hold_target {
            target.set(hold);
        }
    }

    fn commit_language(&mut self, next: LanguagePref, cx: &mut Context<Self>) {
        self.language_open = false;
        if self.phase.prefs().language() == next {
            cx.notify();
            return;
        }
        self.phase.prefs_mut().set_language(next);
        self.save_prefs();
        if let Some(target) = &self.language_target {
            target.set(engine_language(next));
        }
        cx.notify();
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
        self.save_prefs();
        cx.notify();
    }

    pub fn show_progress(&mut self, last: Progress, cx: &mut Context<Self>) {
        self.reset_for_phase();
        self.section = SettingsSection::Model;
        if let AppPhase::Onboarding { status, .. } = &mut self.phase {
            *status = OnboardStatus::Fetching { last };
        }
        cx.notify();
    }

    pub fn show_fetch_failed(&mut self, reason: String, cx: &mut Context<Self>) {
        self.reset_for_phase();
        self.section = SettingsSection::Model;
        if let AppPhase::Onboarding { status, .. } = &mut self.phase {
            *status = OnboardStatus::Failed { reason };
        }
        cx.notify();
    }

    pub fn show_live(
        &mut self,
        hold_target: HoldTarget,
        language_target: LanguageTarget,
        cx: &mut Context<Self>,
    ) {
        self.reset_for_phase();
        self.hold_target = Some(hold_target);
        self.language_target = Some(language_target);
        self.phase = AppPhase::Live {
            prefs: self.phase.prefs().clone(),
        };
        cx.notify();
    }

    pub fn show_refused(&mut self, reason: String, cx: &mut Context<Self>) {
        self.reset_for_phase();
        self.section = SettingsSection::Model;
        self.phase = AppPhase::Refused {
            prefs: self.phase.prefs().clone(),
            reason,
        };
        cx.notify();
    }
}

fn engine_language(language: LanguagePref) -> Language {
    match language {
        LanguagePref::Auto => Language::Auto,
        LanguagePref::French => Language::French,
        LanguagePref::English => Language::English,
    }
}

pub fn open_settings(cx: &mut App, phase: AppPhase) {
    let bounds = Bounds::centered(None, size(px(720.), px(280.)), cx);
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
                        section: SettingsSection::Model,
                        language_open: false,
                        save_error: None,
                        capture: HoldCapture::idle(),
                        hold_focus: cx.focus_handle().tab_stop(true),
                        hold_target: None,
                        language_target: None,
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

pub fn settings_window_show_live(
    cx: &mut App,
    hold_target: HoldTarget,
    language_target: LanguageTarget,
) {
    let handle = *SETTINGS.lock().expect("settings handle");
    if let Some(handle) = handle {
        let _ = handle.update(cx, |view, _window, cx| {
            view.show_live(hold_target, language_target, cx)
        });
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

#[cfg(windows)]
fn sync_native_titlebar(window: &Window, palette: Palette) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR,
        DWMWA_USE_IMMERSIVE_DARK_MODE,
    };

    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return;
    };
    let RawWindowHandle::Win32(win) = handle.as_raw() else {
        return;
    };
    let hwnd = win.hwnd.get() as HWND;
    let (dark, chrome, border, text) = match palette {
        Palette::Light => (0_i32, 0x00f3f3f3_u32, 0x00e5e5e5_u32, 0x00141414_u32),
        Palette::Dark => (1_i32, 0x00202020_u32, 0x002b2b2b_u32, 0x00e8e8e8_u32),
    };

    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE as u32,
            (&dark as *const i32).cast(),
            std::mem::size_of_val(&dark) as u32,
        );
        for (attribute, color) in [
            (DWMWA_CAPTION_COLOR, chrome),
            (DWMWA_BORDER_COLOR, border),
            (DWMWA_TEXT_COLOR, text),
        ] {
            let _ = DwmSetWindowAttribute(
                hwnd,
                attribute as u32,
                (&color as *const u32).cast(),
                std::mem::size_of_val(&color) as u32,
            );
        }
    }
}

#[cfg(not(windows))]
fn sync_native_titlebar(_window: &Window, _palette: Palette) {}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let prefs = self.phase.prefs().clone();
        let palette = match prefs.appearance() {
            AppearancePref::Fixed(Scheme::Light) => Palette::Light,
            AppearancePref::Fixed(Scheme::Dark) => Palette::Dark,
            AppearancePref::Auto => Palette::from_window(window),
        };
        sync_native_titlebar(window, palette);
        let tokens = Tokens::new(palette);
        let content = match self.section {
            SettingsSection::Dictation => dictation_page(self, &tokens, &prefs, cx),
            SettingsSection::Model => model_page(self, &tokens, &prefs, cx),
            SettingsSection::Appearance => appearance_page(&tokens, prefs.appearance(), cx),
        };

        div()
            .id("settings-shell")
            .size_full()
            .flex()
            .flex_row()
            .overflow_hidden()
            .bg(tokens.chrome)
            .text_color(tokens.text)
            .child(sidebar(self.section, &tokens, cx))
            .child(
                div().flex_1().h_full().p(px(8.)).child(
                    div()
                        .id("settings-content")
                        .size_full()
                        .overflow_y_scroll()
                        .rounded(px(12.))
                        .border_1()
                        .border_color(tokens.hairline)
                        .bg(tokens.content)
                        .px(px(28.))
                        .py(px(24.))
                        .child(content),
                ),
            )
    }
}

fn sidebar(
    selected: SettingsSection,
    tokens: &Tokens,
    cx: &mut Context<SettingsView>,
) -> impl IntoElement {
    div()
        .w(px(170.))
        .h_full()
        .flex_shrink_0()
        .flex()
        .flex_col()
        .px(px(12.))
        .py(px(18.))
        .bg(tokens.chrome)
        .child(
            div()
                .px(px(10.))
                .pb(px(16.))
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .child("stt"),
        )
        .child(
            div().flex().flex_col().gap(px(4.)).children(
                [
                    SettingsSection::Dictation,
                    SettingsSection::Model,
                    SettingsSection::Appearance,
                ]
                .into_iter()
                .map(|section| nav_item(section, selected, tokens, cx)),
            ),
        )
}

fn nav_item(
    section: SettingsSection,
    selected: SettingsSection,
    tokens: &Tokens,
    cx: &mut Context<SettingsView>,
) -> impl IntoElement {
    let is_selected = section == selected;
    div()
        .id(SharedString::from(format!("nav-{:?}", section)))
        .h(px(34.))
        .px(px(10.))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(9.))
        .rounded(px(7.))
        .text_sm()
        .text_color(if is_selected {
            tokens.text
        } else {
            tokens.muted
        })
        .when(is_selected, |el| {
            el.bg(tokens.fill).font_weight(FontWeight::MEDIUM)
        })
        .when(!is_selected, |el| {
            el.cursor_pointer()
                .hover(|style| style.bg(tokens.fill_hover).text_color(tokens.text))
                .on_click(cx.listener(move |this, _event, window, cx| {
                    this.capture.cancel();
                    this.language_open = false;
                    this.section = section;
                    window.blur();
                    cx.notify();
                }))
        })
        .child(
            svg()
                .path(section.icon())
                .size(px(16.))
                .text_color(if is_selected {
                    tokens.text
                } else {
                    tokens.muted
                }),
        )
        .child(section.title())
}

fn page(title: &'static str, body: impl IntoElement) -> AnyElement {
    div()
        .w_full()
        .max_w(px(420.))
        .flex()
        .flex_col()
        .gap(px(16.))
        .child(
            div()
                .text_lg()
                .font_weight(FontWeight::SEMIBOLD)
                .child(title),
        )
        .child(body)
        .into_any_element()
}

fn settings_group(tokens: &Tokens) -> Div {
    div()
        .w_full()
        .rounded(px(10.))
        .border_1()
        .border_color(tokens.hairline)
        .bg(tokens.group)
}

fn dictation_page(
    view: &SettingsView,
    tokens: &Tokens,
    prefs: &Prefs,
    cx: &mut Context<SettingsView>,
) -> AnyElement {
    let listening = view.capture.is_listening();
    let label = if listening {
        SharedString::from(pill_label(view.capture.phase(), prefs.hold()).to_string())
    } else {
        SharedString::from(pretty_hold(prefs.hold()))
    };
    let error = view
        .capture
        .reject()
        .map(str::to_string)
        .or_else(|| view.save_error.clone());

    page(
        "Dictation",
        div()
            .flex()
            .flex_col()
            .child(
                settings_group(tokens)
                    .id(HOLD_CAPTURE_ID)
                    .track_focus(&view.hold_focus)
                    .min_h(px(58.))
                    .px(px(14.))
                    .py(px(11.))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .gap(px(16.))
                    .when(listening, |el| el.bg(tokens.fill))
                    .when(listening, |el| {
                        el.on_key_down(cx.listener(SettingsView::on_hold_key))
                    })
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(div().text_sm().child("Push to talk"))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(tokens.muted)
                                    .child(if listening {
                                        "Press the new shortcut"
                                    } else {
                                        "Hold while speaking"
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(6.))
                            .child(
                                div()
                                    .id("hold-change")
                                    .h(px(32.))
                                    .px(px(11.))
                                    .flex()
                                    .items_center()
                                    .rounded(px(6.))
                                    .bg(tokens.fill)
                                    .border_1()
                                    .border_color(tokens.hairline)
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .cursor_pointer()
                                    .hover(|style| style.bg(tokens.fill_hover))
                                    .on_click(cx.listener(|this, _event, window, cx| {
                                        this.toggle_hold_capture(window, cx);
                                    }))
                                    .child(label),
                            )
                            .child(
                                div()
                                    .id("hold-reset")
                                    .h(px(32.))
                                    .px(px(10.))
                                    .flex()
                                    .items_center()
                                    .rounded(px(6.))
                                    .text_xs()
                                    .text_color(tokens.muted)
                                    .cursor_pointer()
                                    .hover(|style| {
                                        style.bg(tokens.fill_hover).text_color(tokens.text)
                                    })
                                    .on_click(cx.listener(|this, _event, window, cx| {
                                        this.reset_hold(window, cx);
                                    }))
                                    .child("Reset"),
                            ),
                    ),
            )
            .children(error.map(|text| error_text(tokens, text))),
    )
}

fn model_page(
    view: &SettingsView,
    tokens: &Tokens,
    prefs: &Prefs,
    cx: &mut Context<SettingsView>,
) -> AnyElement {
    let (action, enabled, error) = model_state(&view.phase);
    page(
        "Model",
        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(
                settings_group(tokens)
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .w_full()
                            .min_h(px(64.))
                            .px(px(14.))
                            .py(px(10.))
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .gap(px(16.))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(3.))
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(FontWeight::MEDIUM)
                                            .child("Nemotron 0.6B INT4"),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(tokens.muted)
                                            .child("~800 MB · French & English"),
                                    ),
                            )
                            .child(model_button(tokens, action, enabled, cx)),
                    )
                    .child(div().h(px(1.)).mx(px(14.)).bg(tokens.hairline))
                    .child(language_row(
                        tokens,
                        prefs.language(),
                        view.language_open,
                        cx,
                    )),
            )
            .children(error.map(|text| error_text(tokens, text)))
            .children(view.save_error.clone().map(|text| error_text(tokens, text))),
    )
}

fn model_state(phase: &AppPhase) -> (SharedString, bool, Option<String>) {
    match phase {
        AppPhase::Live { .. } => ("Installed".into(), false, None),
        AppPhase::Onboarding {
            status, warning, ..
        } => match status {
            OnboardStatus::Idle => ("Download".into(), true, warning.clone()),
            OnboardStatus::Fetching { last } => {
                (download_progress(last).into(), false, warning.clone())
            }
            OnboardStatus::Failed { reason } => ("Retry".into(), true, Some(reason.clone())),
        },
        AppPhase::Refused { reason, .. } => ("Retry".into(), true, Some(reason.clone())),
    }
}

fn download_progress(progress: &Progress) -> String {
    match progress.total {
        Some(total) if total > 0 => {
            let percent = (progress.bytes.saturating_mul(100) / total).min(100);
            format!("Downloading {percent}%")
        }
        _ => "Downloading…".to_string(),
    }
}

fn model_button(
    tokens: &Tokens,
    label: SharedString,
    enabled: bool,
    cx: &mut Context<SettingsView>,
) -> impl IntoElement {
    div()
        .id("model-action")
        .h(px(32.))
        .px(px(12.))
        .flex()
        .items_center()
        .rounded(px(6.))
        .border_1()
        .border_color(tokens.hairline)
        .bg(tokens.fill)
        .text_sm()
        .text_color(if enabled { tokens.text } else { tokens.muted })
        .when(enabled, |el| {
            el.cursor_pointer()
                .hover(|style| style.bg(tokens.fill_hover))
                .on_click(cx.listener(|this, _event, _window, cx| {
                    this.capture.cancel();
                    let mut prefs = this.phase.prefs().clone();
                    prefs.pack = PackId::Light;
                    crate::begin_pack(cx, prefs);
                }))
        })
        .child(label)
}

fn language_row(
    tokens: &Tokens,
    selected: LanguagePref,
    open: bool,
    cx: &mut Context<SettingsView>,
) -> impl IntoElement {
    div()
        .w_full()
        .relative()
        .min_h(px(56.))
        .px(px(14.))
        .py(px(10.))
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap(px(16.))
        .child(div().text_sm().child("Language"))
        .child(
            div()
                .id("language-select")
                .w(px(156.))
                .h(px(32.))
                .px(px(10.))
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .rounded(px(6.))
                .border_1()
                .border_color(tokens.hairline)
                .bg(tokens.fill)
                .text_sm()
                .cursor_pointer()
                .hover(|style| style.bg(tokens.fill_hover))
                .on_click(cx.listener(|this, _event, _window, cx| {
                    this.language_open = !this.language_open;
                    cx.notify();
                }))
                .child(selected.label())
                .child(
                    svg()
                        .path("fluent/chevron-down.svg")
                        .size(px(14.))
                        .text_color(tokens.muted),
                ),
        )
        .when(open, |el| {
            el.child(
                div()
                    .absolute()
                    .bottom(px(46.))
                    .right(px(14.))
                    .w(px(156.))
                    .p(px(4.))
                    .flex()
                    .flex_col()
                    .rounded(px(7.))
                    .border_1()
                    .border_color(tokens.hairline)
                    .bg(tokens.elevated)
                    .shadow_md()
                    .children(LANGUAGES.into_iter().map(|language| {
                        let active = language == selected;
                        div()
                            .id(SharedString::from(format!(
                                "language-{}",
                                language.as_str()
                            )))
                            .h(px(30.))
                            .px(px(8.))
                            .flex()
                            .items_center()
                            .rounded(px(5.))
                            .text_sm()
                            .when(active, |row| {
                                row.bg(tokens.fill).font_weight(FontWeight::MEDIUM)
                            })
                            .when(!active, |row| {
                                row.cursor_pointer()
                                    .hover(|style| style.bg(tokens.fill_hover))
                                    .on_click(cx.listener(move |this, _event, _window, cx| {
                                        this.commit_language(language, cx);
                                    }))
                            })
                            .child(language.label())
                    })),
            )
        })
}

fn appearance_page(
    tokens: &Tokens,
    selected: AppearancePref,
    cx: &mut Context<SettingsView>,
) -> AnyElement {
    page(
        "Appearance",
        settings_group(tokens)
            .min_h(px(56.))
            .px(px(14.))
            .py(px(10.))
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap(px(16.))
            .child(div().text_sm().child("Theme"))
            .child(Segmented::new(
                *tokens,
                selected,
                THEMES.into_iter().map(|(pref, label)| {
                    Segment::new(pref, pref.element_id(), label).on_click(
                        cx,
                        move |this: &mut SettingsView, cx| {
                            this.commit_appearance(pref, cx);
                        },
                    )
                }),
            )),
    )
}

fn error_text(tokens: &Tokens, text: String) -> impl IntoElement {
    div().text_xs().text_color(tokens.muted).child(text)
}

fn pretty_hold(hold: &str) -> String {
    hold.split('+').collect::<Vec<_>>().join(" + ")
}

#[cfg(test)]
mod tests {
    use super::{download_progress, pretty_hold};
    use crate::phase::Progress;

    #[test]
    fn pretty_hold_spaces_tokens_and_leaves_a_bare_trigger() {
        assert_eq!(pretty_hold("Ctrl+Space"), "Ctrl + Space");
        assert_eq!(pretty_hold("Alt+a"), "Alt + a");
        assert_eq!(pretty_hold("F9"), "F9");
    }

    #[test]
    fn progress_is_compact_and_bounded() {
        assert_eq!(
            download_progress(&Progress {
                file: "encoder.onnx".into(),
                bytes: 25,
                total: Some(100),
            }),
            "Downloading 25%"
        );
        assert_eq!(
            download_progress(&Progress {
                file: "encoder.onnx".into(),
                bytes: 150,
                total: Some(100),
            }),
            "Downloading 100%"
        );
    }
}
