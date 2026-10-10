use crate::settings::i18n::tr;
use gpui::{
    bounds, canvas, div, fill, point, prelude::*, px, size, svg, AnyElement, Context, Div,
    FontWeight, ScrollHandle, SharedString, Window,
};
use plume_ui::{Palette, Segment, Segmented, Tokens};

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
        .bg(tokens.chrome)
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

        let body = div()
            .flex()
            .flex_row()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .child(sidebar(self.section, &self.update, &tokens, cx))
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .relative()
                    .p(px(8.))
                    .child(
                        div()
                            .id("settings-content")
                            .size_full()
                            .when(!is_history, |el| {
                                el.overflow_y_scroll().track_scroll(&self.content_scroll)
                            })
                            .when(is_history, |el| el.overflow_hidden())
                            .rounded(px(12.))
                            .border_1()
                            .border_color(tokens.hairline)
                            .bg(tokens.content)
                            .when(!is_history, |el| el.px(px(28.)).py(px(24.)))
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
            .bg(tokens.chrome)
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
    .top(px(20.))
    .bottom(px(20.))
    .right(px(12.))
    .w(px(14.))
}

fn sidebar(
    selected: SettingsSection,
    update: &super::updates::UpdateState,
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
                .flex()
                .items_center()
                .gap(px(8.))
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .child(
                    svg()
                        .path("brand/plume.svg")
                        .size(px(28.))
                        .text_color(tokens.text),
                )
                .child("Plume"),
        )
        .child(
            div().flex().flex_col().gap(px(4.)).children(
                [
                    SettingsSection::Dictation,
                    SettingsSection::Model,
                    SettingsSection::Appearance,
                    SettingsSection::History,
                ]
                .into_iter()
                .map(|section| nav_item(section, selected, tokens, cx)),
            ),
        )
        .child(
            div().mt_auto().child(
                div()
                    .relative()
                    .child(nav_item(SettingsSection::About, selected, tokens, cx))
                    .when(update.needs_attention(), |row| {
                        row.child(
                            div()
                                .absolute()
                                .right(px(10.))
                                .top(px(14.))
                                .size(px(6.))
                                .rounded_full()
                                .bg(tokens.accent),
                        )
                    }),
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
        .when(section != SettingsSection::About, |el| {
            el.child(
                svg()
                    .path(section.icon())
                    .size(px(16.))
                    .text_color(if is_selected {
                        tokens.text
                    } else {
                        tokens.muted
                    }),
            )
        })
        .child(section.title())
}

pub(super) fn page(title: &'static str, body: impl IntoElement) -> AnyElement {
    div()
        .w_full()
        .max_w(px(480.))
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

pub(super) fn settings_group(tokens: &Tokens) -> Div {
    div()
        .w_full()
        .rounded(px(10.))
        .border_1()
        .border_color(tokens.hairline)
        .bg(tokens.group)
}

pub(super) fn settings_section(
    tokens: &Tokens,
    title: &'static str,
    body: impl IntoElement,
) -> Div {
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(8.))
        .child(section_title(tokens, title))
        .child(body)
}

pub(super) fn section_title(tokens: &Tokens, title: &'static str) -> Div {
    div()
        .text_xs()
        .font_weight(FontWeight::MEDIUM)
        .text_color(tokens.muted)
        .child(title)
}

pub(super) fn settings_row() -> Div {
    div()
        .w_full()
        .min_h(px(58.))
        .px(px(14.))
        .py(px(8.))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(14.))
}

pub(super) fn settings_divider(tokens: &Tokens) -> Div {
    div()
        .h(px(1.))
        .flex_shrink_0()
        .mx(px(14.))
        .bg(tokens.hairline)
}

fn appearance_page(
    view: &SettingsView,
    tokens: &Tokens,
    selected: AppearancePref,
    cx: &mut Context<SettingsView>,
) -> AnyElement {
    page(
        tr("interface.title"),
        settings_section(
            tokens,
            tr("interface.title"),
            settings_group(tokens)
                .child(super::languages::interface_language_row(view, tokens, cx))
                .child(settings_divider(tokens))
                .child(
                    settings_row()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_sm()
                                .child(tr("interface.theme")),
                        )
                        .child(Segmented::new(
                            *tokens,
                            selected,
                            THEMES.into_iter().map(|(pref, label)| {
                                Segment::new(pref, pref.element_id(), tr(label)).on_click(
                                    cx,
                                    move |this: &mut SettingsView, cx| {
                                        this.commit_appearance(pref, cx);
                                    },
                                )
                            }),
                        )),
                ),
        ),
    )
}

pub(super) fn error_text(tokens: &Tokens, text: String) -> impl IntoElement {
    div()
        .w_full()
        .px(px(14.))
        .py(px(10.))
        .rounded(px(8.))
        .border_1()
        .border_color(tokens.hairline)
        .bg(tokens.accent_soft)
        .text_xs()
        .text_color(tokens.text)
        .child(tr(&text).to_owned())
}
