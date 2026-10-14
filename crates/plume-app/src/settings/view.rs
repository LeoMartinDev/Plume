use crate::settings::i18n::tr;
use gpui::{
    bounds, canvas, div, fill, font, point, prelude::*, px, size, svg, AnyElement, Context, Div,
    FontStyle, FontWeight, ScrollHandle, SharedString, Stateful, StyledText, TextRun, Window,
};
use plume_ui::{eyebrow, Palette, Tokens, FONT_SANS, FONT_SERIF};

use crate::prefs::{AppearancePref, Scheme};

use super::dictation::dictation_page;
use super::history_view::history_page;
use super::model::model_page;
use super::{SettingsSection, SettingsView};
#[cfg(target_os = "macos")]
use super::{MACOS_TITLEBAR_HEIGHT, SETTINGS_TITLE};

const THEMES: [(AppearancePref, &str); 3] = [
    (AppearancePref::Auto, "ui.system"),
    (AppearancePref::Fixed(Scheme::Light), "ui.light"),
    (AppearancePref::Fixed(Scheme::Dark), "ui.dark"),
];

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
    // COLORREF is 0x00BBGGRR.
    let colorref = |color: gpui::Rgba| {
        let channel = |value: f32| (value * 255.).round() as u32;
        channel(color.r) | channel(color.g) << 8 | channel(color.b) << 16
    };
    let tokens = Tokens::new(palette);
    let dark = i32::from(palette == Palette::Dark);
    let (chrome, border, text) = (
        colorref(tokens.bg),
        colorref(tokens.border_strong),
        colorref(tokens.subtle),
    );

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

#[cfg(target_os = "macos")]
fn drag_macos_titlebar(window: &Window) {
    use cocoa::{
        appkit::NSApplication,
        base::{id, nil},
    };
    use objc::{runtime::Sel, Message};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return;
    };
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return;
    };
    unsafe {
        let native_view = handle.ns_view.as_ptr() as id;
        let Ok(native_window) = (&*native_view).send_message::<_, id>(Sel::register("window"), ())
        else {
            return;
        };
        if native_window.is_null() {
            return;
        }
        let app = NSApplication::sharedApplication(nil);
        let Ok(event) = (&*app).send_message::<_, id>(Sel::register("currentEvent"), ()) else {
            return;
        };
        if !event.is_null() {
            let _ = (&*native_window)
                .send_message::<_, ()>(Sel::register("performWindowDragWithEvent:"), (event,));
        }
    }
}

#[cfg(target_os = "macos")]
pub(super) fn macos_titlebar(tokens: &Tokens) -> impl IntoElement {
    use gpui::MouseButton;

    div()
        .id("settings-titlebar")
        .h(px(MACOS_TITLEBAR_HEIGHT))
        .w_full()
        .flex_shrink_0()
        .flex()
        .items_center()
        .pl(px(84.))
        .bg(tokens.bg)
        .text_color(tokens.subtle)
        .text_sm()
        .font_weight(FontWeight::MEDIUM)
        .child(SETTINGS_TITLE)
        .on_mouse_down(MouseButton::Left, |event, window, _| {
            if event.click_count == 2 {
                window.titlebar_double_click();
            } else {
                drag_macos_titlebar(window);
            }
        })
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _language = super::i18n::LanguageScope::new(self.phase.prefs().interface_language());
        if !self.capture.is_listening() {
            self.shortcut_edit.take();
        }
        let prefs = self.phase.prefs().clone();
        let palette = match prefs.appearance() {
            AppearancePref::Fixed(Scheme::Light) => Palette::Light,
            AppearancePref::Fixed(Scheme::Dark) => Palette::Dark,
            AppearancePref::Auto => Palette::from_window(window),
        };
        sync_native_titlebar(window, palette);
        let tokens = Tokens::new(palette);
        if self.onboarding.is_some() {
            if window.focused(cx).is_none() && !self.capture.is_listening() {
                window.focus(&self.hold_focus);
            }
            return super::onboarding::render(self, &tokens, cx).into_any_element();
        }
        let is_history = self.section == SettingsSection::History;
        if is_history && self.history_list_width != Some(window.viewport_size().width) {
            super::history_view::invalidate_list(&self.history_list, self.history.entries().len());
            self.history_list_width = Some(window.viewport_size().width);
        }
        let scroll = if is_history {
            ContentScroll::History(self.history_list.clone())
        } else {
            ContentScroll::Page(self.content_scroll.clone())
        };
        let content = match self.section {
            SettingsSection::Dictation => dictation_page(self, &tokens, &prefs, cx),
            SettingsSection::Model => model_page(self, &tokens, &prefs, cx),
            SettingsSection::Appearance => appearance_page(self, &tokens, prefs.appearance(), cx),
            SettingsSection::History => history_page(self, &tokens, cx),
            SettingsSection::About => super::updates::about_page(self, &tokens, cx),
        };

        let hold = prefs.hold().to_string();
        let body = div()
            .flex()
            .flex_row()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .child(sidebar(self.section, &self.update, &hold, &tokens, cx))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .relative()
                    .child(
                        div()
                            .id("settings-content")
                            .size_full()
                            .when(!is_history, |el| {
                                el.overflow_y_scroll()
                                    .track_scroll(&self.content_scroll)
                                    .pl(px(CONTENT_LEFT))
                                    .pr(px(CONTENT_RIGHT))
                                    .pt(px(CONTENT_TOP))
                                    .pb(px(28.))
                            })
                            .when(is_history, |el| el.overflow_hidden())
                            .child(content),
                    )
                    .child(scrollbar(scroll, self.scrollbar_drag.clone(), tokens)),
            );

        let shell = div()
            .id("settings-shell")
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(tokens.bg)
            .font_family(FONT_SANS)
            .text_color(tokens.text);
        #[cfg(target_os = "macos")]
        let shell = shell.child(macos_titlebar(&tokens));
        shell.child(body).into_any_element()
    }
}

#[derive(Clone)]
enum ContentScroll {
    Page(ScrollHandle),
    History(gpui::ListState),
}

impl ContentScroll {
    fn metrics(&self) -> (f32, f32, f32) {
        let (maximum, viewport, offset) = match self {
            Self::Page(handle) => (
                handle.max_offset().height,
                handle.bounds().size.height,
                -handle.offset().y,
            ),
            Self::History(state) => (
                state.max_offset_for_scrollbar().height,
                state.viewport_bounds().size.height,
                -state.scroll_px_offset_for_scrollbar().y,
            ),
        };
        (maximum.into(), viewport.into(), offset.into())
    }

    fn set_offset(&self, offset: f32) {
        let position = point(px(0.), px(-offset));
        match self {
            Self::Page(handle) => handle.set_offset(position),
            Self::History(state) => state.set_offset_from_scrollbar(position),
        }
    }

    fn start_drag(&self) {
        if let Self::History(state) = self {
            state.scrollbar_drag_started();
        }
    }

    fn end_drag(&self) {
        if let Self::History(state) = self {
            state.scrollbar_drag_ended();
        }
    }
}

fn scrollbar(
    scroll: ContentScroll,
    drag: std::rc::Rc<std::cell::Cell<Option<f32>>>,
    tokens: Tokens,
) -> impl IntoElement {
    use gpui::{
        DispatchPhase, HitboxBehavior, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    };
    canvas(
        move |track, window, _| window.insert_hitbox(track, HitboxBehavior::Normal),
        move |track, hitbox, window, _| {
            let (maximum, viewport, offset) = scroll.metrics();
            if maximum <= 0. {
                return;
            }
            let height: f32 = track.size.height.into();
            let hovered = hitbox.is_hovered(window);
            let dragging = drag.get().is_some();
            let thumb_width = if hovered || dragging { 8. } else { 6. };
            let color = if dragging {
                tokens.scrollbar_active
            } else if hovered {
                tokens.scrollbar_hover
            } else {
                tokens.scrollbar
            };
            let thumb_height = (height * viewport / (viewport + maximum))
                .max(24.)
                .min(height);
            let travel = height - thumb_height;
            let top: f32 = track.top().into();
            let thumb_top = travel * (offset / maximum).clamp(0., 1.);
            window.paint_quad(
                fill(
                    bounds(
                        point(
                            track.left() + (track.size.width - px(thumb_width)) / 2.,
                            track.top() + px(thumb_top),
                        ),
                        size(px(thumb_width), px(thumb_height)),
                    ),
                    color,
                )
                .corner_radii(px(thumb_width / 2.)),
            );
            let down_scroll = scroll.clone();
            let down_drag = drag.clone();
            window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                if phase != DispatchPhase::Bubble
                    || event.button != MouseButton::Left
                    || !hitbox.is_hovered(window)
                {
                    return;
                }
                let y: f32 = event.position.y.into();
                let relative = y - top - thumb_top;
                let grab = if (0. ..=thumb_height).contains(&relative) {
                    relative
                } else {
                    thumb_height / 2.
                };
                down_scroll.start_drag();
                down_drag.set(Some(grab));
                if travel > 0. {
                    down_scroll.set_offset(((y - top - grab) / travel).clamp(0., 1.) * maximum);
                }
                cx.stop_propagation();
                window.refresh();
            });
            let move_scroll = scroll.clone();
            let move_drag = drag.clone();
            // Capture movement across the entire window, including outside the narrow track.
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                if phase != DispatchPhase::Capture {
                    return;
                }
                let Some(grab) = move_drag.get() else {
                    if hovered != track.contains(&event.position) {
                        window.refresh();
                    }
                    return;
                };
                if !event.dragging() {
                    move_drag.set(None);
                    move_scroll.end_drag();
                    window.refresh();
                    return;
                }
                let y: f32 = event.position.y.into();
                let (maximum, _, _) = move_scroll.metrics();
                if travel > 0. {
                    move_scroll.set_offset(((y - top - grab) / travel).clamp(0., 1.) * maximum);
                }
                cx.stop_propagation();
                window.refresh();
            });
            window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                if phase == DispatchPhase::Capture
                    && event.button == MouseButton::Left
                    && drag.take().is_some()
                {
                    scroll.end_drag();
                    cx.stop_propagation();
                    window.refresh();
                }
            });
        },
    )
    .absolute()
    .top(px(16.))
    .bottom(px(16.))
    .right(px(6.))
    .w(px(14.))
}

/// Content insets beside the sidebar; the history list applies the same ones per item.
pub(super) const CONTENT_LEFT: f32 = 12.;
pub(super) const CONTENT_RIGHT: f32 = 32.;
pub(super) const CONTENT_TOP: f32 = 16.;
pub(super) const PAGE_WIDTH: f32 = 500.;

fn sidebar(
    selected: SettingsSection,
    update: &super::updates::UpdateState,
    hold: &str,
    tokens: &Tokens,
    cx: &mut Context<SettingsView>,
) -> impl IntoElement {
    div()
        .w(px(196.))
        .h_full()
        .flex_shrink_0()
        .flex()
        .flex_col()
        .px(px(12.))
        .pt(px(14.))
        .pb(px(14.))
        .child(
            div()
                .px(px(10.))
                .pb(px(18.))
                .flex()
                .items_center()
                .gap(px(9.))
                .text_size(px(15.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(brand_mark(tokens, 26.))
                .child("Plume"),
        )
        .child(
            div().flex().flex_col().gap(px(2.)).children(
                [
                    SettingsSection::Dictation,
                    SettingsSection::Model,
                    SettingsSection::Appearance,
                    SettingsSection::History,
                ]
                .into_iter()
                .map(|section| nav_item(section, selected, false, tokens, cx)),
            ),
        )
        .child(
            div()
                .mt_auto()
                .flex()
                .flex_col()
                .gap(px(10.))
                .child(shortcut_card(tokens, hold, cx))
                .child(nav_item(
                    SettingsSection::About,
                    selected,
                    update.needs_attention(),
                    tokens,
                    cx,
                )),
        )
}

/// The app mark: the plume glyph on an ink tile, as on the website.
pub(super) fn brand_mark(tokens: &Tokens, size: f32) -> impl IntoElement {
    div()
        .size(px(size))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(size / 4.))
        .bg(tokens.btn_bg)
        .child(
            svg()
                .path("brand/plume.svg")
                .size(px(size * 0.66))
                .text_color(tokens.btn_fg),
        )
}

/// The push-to-talk shortcut, named and drawn as keys, inverted like the dictation
/// bubble; opens Dictation to change it.
fn shortcut_card(tokens: &Tokens, hold: &str, cx: &mut Context<SettingsView>) -> impl IntoElement {
    let tokens = *tokens;
    div()
        .id("sidebar-shortcut")
        .w_full()
        .px(px(12.))
        .pt(px(10.))
        .pb(px(12.))
        .flex()
        .flex_col()
        .gap(px(8.))
        .overflow_hidden()
        .rounded(px(14.))
        .bg(tokens.pill_bg)
        .cursor_pointer()
        .hover(|style| style.opacity(0.9))
        .on_click(cx.listener(|this, _event, window, cx| {
            this.capture.cancel();
            this.language_menu = None;
            this.history_menu = None;
            this.section = SettingsSection::Dictation;
            window.blur();
            cx.notify();
        }))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .text_size(px(12.))
                .text_color(tokens.pill_muted)
                .child(
                    svg()
                        .path("ui/mic.svg")
                        .size(px(13.))
                        .text_color(tokens.pill_muted),
                )
                .child(tr("ui.hold_to_speak")),
        )
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap(px(4.))
                .children(hold.split('+').map(move |key| {
                    div()
                        .h(px(24.))
                        .px(px(7.))
                        .flex()
                        .items_center()
                        .rounded(px(7.))
                        .border_1()
                        .border_b_2()
                        .border_color(tokens.pill_chip_border)
                        .bg(tokens.pill_chip)
                        .text_size(px(11.5))
                        .text_color(tokens.pill_chip_fg)
                        .whitespace_nowrap()
                        .child(plume_ui::key_label(key.trim()))
                })),
        )
}

fn nav_item(
    section: SettingsSection,
    selected: SettingsSection,
    attention: bool,
    tokens: &Tokens,
    cx: &mut Context<SettingsView>,
) -> impl IntoElement {
    let is_selected = section == selected;
    let tokens = *tokens;
    div()
        .id(SharedString::from(format!("nav-{:?}", section)))
        .h(px(34.))
        .px(px(12.))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(10.))
        .rounded_full()
        .text_size(px(13.5))
        .text_color(if is_selected {
            tokens.text
        } else {
            tokens.muted
        })
        .when(is_selected, |el| {
            el.bg(tokens.surface)
                .shadow(tokens.raise())
                .font_weight(FontWeight::MEDIUM)
        })
        .when(!is_selected, |el| {
            el.cursor_pointer()
                .hover(move |style| style.bg(tokens.hover).text_color(tokens.text))
                .on_click(cx.listener(move |this, _event, window, cx| {
                    this.capture.cancel();
                    this.language_menu = None;
                    this.insertion_open = false;
                    this.history_menu = None;
                    this.scrollbar_drag.set(None);
                    this.history_list.scrollbar_drag_ended();
                    this.section = section;
                    window.blur();
                    cx.notify();
                }))
        })
        .children(section.icon().map(|icon| {
            svg().path(icon).size(px(16.)).text_color(if is_selected {
                tokens.text
            } else {
                tokens.muted
            })
        }))
        .child(div().flex_1().child(section.title()))
        .when(attention, |el| {
            el.child(div().size(px(6.)).rounded_full().bg(tokens.accent))
        })
}

/// Page title, with an optional lead whose end is set in the serif italic.
pub(super) fn page(
    tokens: &Tokens,
    title: &str,
    lead: Option<(&str, &str)>,
    body: impl IntoElement,
) -> AnyElement {
    div()
        .w_full()
        .max_w(px(PAGE_WIDTH))
        .flex()
        .flex_col()
        .gap(px(22.))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(
                    div()
                        .text_size(px(26.))
                        .line_height(px(30.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title.to_string()),
                )
                .children(lead.map(|(lead, serif)| serif_lead(tokens, lead, serif, 13.5))),
        )
        .child(body)
        .into_any_element()
}

/// GPUI may wrap a line after a typographic apostrophe ("l’|interface"); the
/// ASCII one counts as part of the word.
pub(super) fn prose(text: &str) -> String {
    text.replace('\u{2019}', "'")
}

/// "Plain words, *serif words.*" — the website's writing voice.
pub(super) fn serif_lead(tokens: &Tokens, lead: &str, serif: &str, size: f32) -> impl IntoElement {
    let (lead, serif) = (prose(lead), prose(serif));
    let text = if serif.is_empty() {
        lead.clone()
    } else {
        format!("{lead} {serif}")
    };
    let sans_len = if serif.is_empty() {
        text.len()
    } else {
        lead.len() + 1
    };
    let mut italic = font(FONT_SERIF);
    italic.style = FontStyle::Italic;
    let run = |len, font, color: gpui::Rgba| TextRun {
        len,
        font,
        color: color.into(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let mut runs = vec![run(sans_len, font(FONT_SANS), tokens.muted)];
    if !serif.is_empty() {
        runs.push(run(serif.len(), italic, tokens.title_dim));
    }
    div()
        .text_size(px(size))
        .line_height(px(size * 1.5))
        .child(StyledText::new(text).with_runs(runs))
}

/// A display title: sans first line, serif italic second line.
pub(super) fn display_title(
    tokens: &Tokens,
    first: &str,
    second: &str,
    size: f32,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .child(
            div()
                .text_size(px(size))
                .line_height(px(size * 1.08))
                .font_weight(FontWeight::SEMIBOLD)
                .child(first.to_string()),
        )
        .child(
            div()
                .font_family(FONT_SERIF)
                .italic()
                .text_size(px(size * 1.12))
                .line_height(px(size * 1.12))
                .text_color(tokens.title_dim)
                .child(second.to_string()),
        )
}

pub(super) fn settings_group(tokens: &Tokens) -> Div {
    div()
        .w_full()
        .flex()
        .flex_col()
        .rounded(px(18.))
        .border_1()
        .border_color(tokens.border)
        .bg(tokens.surface)
}

pub(super) fn settings_section(tokens: &Tokens, title: &str, body: impl IntoElement) -> Div {
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(8.))
        .child(section_title(tokens, title))
        .child(body)
}

pub(super) fn section_title(tokens: &Tokens, title: &str) -> Div {
    div()
        .px(px(4.))
        .child(eyebrow(title.to_string(), tokens.subtle))
}

pub(super) fn settings_row() -> Div {
    div()
        .w_full()
        .min_h(px(60.))
        .pl(px(18.))
        .pr(px(14.))
        .py(px(10.))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(14.))
}

/// The label side of a settings row: a name and an optional one-line explanation.
pub(super) fn row_label(tokens: &Tokens, title: &str, detail: Option<&str>) -> Div {
    div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(2.))
        .child(
            div()
                .text_size(px(14.))
                .font_weight(FontWeight::MEDIUM)
                .child(prose(title)),
        )
        .children(detail.map(|detail| {
            div()
                .text_size(px(12.5))
                .text_color(tokens.subtle)
                .child(prose(detail))
        }))
}

pub(super) fn settings_divider(tokens: &Tokens) -> Div {
    div()
        .h(px(1.))
        .flex_shrink_0()
        .mx(px(18.))
        .bg(tokens.border)
}

/// The closed state of a dropdown: a pill with a chevron.
pub(super) fn select_trigger(
    tokens: &Tokens,
    id: impl Into<gpui::ElementId>,
    label: impl Into<SharedString>,
    width: f32,
    enabled: bool,
) -> Stateful<Div> {
    let tokens = *tokens;
    div()
        .id(id)
        .w(px(width))
        .flex_shrink_0()
        .h(px(34.))
        .pl(px(14.))
        .pr(px(12.))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(8.))
        .rounded_full()
        .border_1()
        .border_color(tokens.border_strong)
        .bg(tokens.surface_2)
        .text_size(px(13.))
        .when(!enabled, |el| el.opacity(0.5))
        .when(enabled, |el| {
            el.cursor_pointer()
                .hover(move |style| style.border_color(tokens.faint))
        })
        .child(div().truncate().child(label.into()))
        .child(
            svg()
                .path("ui/chevron-down.svg")
                .size(px(12.))
                .flex_shrink_0()
                .text_color(tokens.subtle),
        )
}

/// The open state of a dropdown.
pub(super) fn menu_panel(tokens: &Tokens, width: f32) -> Div {
    div()
        .w(px(width))
        .p(px(4.))
        .flex()
        .flex_col()
        .rounded(px(14.))
        .border_1()
        .border_color(tokens.border_strong)
        .bg(tokens.surface)
        .shadow(tokens.float())
}

pub(super) fn menu_option(
    tokens: &Tokens,
    id: impl Into<gpui::ElementId>,
    label: impl Into<SharedString>,
    selected: bool,
    highlighted: bool,
) -> Stateful<Div> {
    let tokens = *tokens;
    div()
        .id(id)
        .h(px(32.))
        .flex_shrink_0()
        .px(px(10.))
        .flex()
        .items_center()
        .justify_between()
        .rounded(px(10.))
        .text_size(px(13.))
        .cursor_pointer()
        .when(highlighted, |el| el.bg(tokens.hover))
        .hover(move |style| style.bg(tokens.hover))
        .gap(px(8.))
        .when(selected, |el| el.font_weight(FontWeight::MEDIUM))
        .child(div().min_w_0().truncate().child(label.into()))
        .children(selected.then(|| {
            svg()
                .path("ui/check.svg")
                .size(px(12.))
                .text_color(tokens.accent_text)
        }))
}

fn appearance_page(
    view: &SettingsView,
    tokens: &Tokens,
    selected: AppearancePref,
    cx: &mut Context<SettingsView>,
) -> AnyElement {
    page(
        tokens,
        tr("interface.title"),
        Some((
            tr("settings.interface.lead"),
            tr("settings.interface.serif"),
        )),
        div()
            .flex()
            .flex_col()
            .gap(px(22.))
            .child(settings_section(
                tokens,
                tr("interface.theme"),
                div().flex().gap(px(10.)).children(
                    THEMES
                        .into_iter()
                        .map(|(pref, label)| theme_choice(tokens, pref, label, selected, cx)),
                ),
            ))
            .child(settings_section(
                tokens,
                tr("ui.languages"),
                settings_group(tokens)
                    .child(super::languages::interface_language_row(view, tokens, cx))
                    .child(settings_divider(tokens))
                    .child(super::languages::dictation_language_row(view, tokens, cx)),
            )),
    )
}

/// A theme drawn as a small window: system is half light, half dark.
fn theme_choice(
    tokens: &Tokens,
    pref: AppearancePref,
    label: &'static str,
    selected: AppearancePref,
    cx: &mut Context<SettingsView>,
) -> impl IntoElement {
    let active = pref == selected;
    let light = Tokens::new(Palette::Light);
    let dark = Tokens::new(Palette::Dark);
    let (left, right) = match pref {
        AppearancePref::Auto => (light, dark),
        AppearancePref::Fixed(Scheme::Light) => (light, light),
        AppearancePref::Fixed(Scheme::Dark) => (dark, dark),
    };
    let half = |side: Tokens, first: bool| {
        let card = div()
            .w_full()
            .h(px(34.))
            .bg(side.surface)
            .border_1()
            .border_color(side.border_strong);
        let pill = div().w(px(26.)).h(px(10.)).bg(side.pill_bg);
        div()
            .flex_1()
            .h_full()
            .flex()
            .flex_col()
            .justify_between()
            .py(px(12.))
            .bg(side.bg)
            // GPUI clips children to a rectangle, not to the frame's rounded corners:
            // each half rounds its own outer corners (frame radius minus its border).
            .when(first, |el| el.pl(px(12.)).items_end().rounded_l(px(13.)))
            .when(!first, |el| el.pr(px(12.)).items_start().rounded_r(px(13.)))
            .child(if first {
                card.rounded_l(px(10.)).border_r_0()
            } else {
                card.rounded_r(px(10.)).border_l_0()
            })
            .child(if first {
                pill.rounded_l(px(5.))
            } else {
                pill.rounded_r(px(5.))
            })
    };
    let tokens = *tokens;
    div()
        .id(pref.element_id())
        .flex_1()
        .flex()
        .flex_col()
        .gap(px(8.))
        .when(!active, |el| {
            el.cursor_pointer()
                .on_click(cx.listener(move |this, _, _, cx| this.commit_appearance(pref, cx)))
        })
        .child(
            div()
                .h(px(92.))
                .flex()
                .rounded(px(14.))
                .overflow_hidden()
                .border_1()
                .border_color(if active { tokens.accent } else { tokens.border })
                .when(active, |el| el.shadow(tokens.ring()))
                .when(!active, |el| {
                    el.hover(move |style| style.border_color(tokens.faint))
                })
                .child(half(left, true))
                .child(half(right, false)),
        )
        .child(
            div()
                .pl(px(4.))
                .text_size(px(13.))
                .text_color(if active { tokens.text } else { tokens.muted })
                .when(active, |el| el.font_weight(FontWeight::MEDIUM))
                .child(tr(label)),
        )
}

pub(super) fn error_text(tokens: &Tokens, text: String) -> impl IntoElement {
    div()
        .w_full()
        .px(px(16.))
        .py(px(12.))
        .flex()
        .gap(px(10.))
        .rounded(px(14.))
        .border_1()
        .border_color(tokens.border)
        .bg(tokens.surface)
        .text_size(px(12.5))
        .text_color(tokens.text)
        .child(
            svg()
                .path("ui/info.svg")
                .size(px(15.))
                .flex_shrink_0()
                .text_color(tokens.warn),
        )
        .child(div().flex_1().min_w_0().child(prose(tr(&text))))
}
