use gpui::{
    bounds, canvas, div, fill, point, prelude::*, px, size, svg, AnyElement, Context, Div,
    FontWeight, Rgba, ScrollHandle, SharedString, Window,
};
use stt_ui::{Palette, Segment, Segmented, Tokens};

use crate::prefs::{AppearancePref, Scheme};

use super::dictation::dictation_page;
use super::history_view::history_page;
use super::model::model_page;
use super::{SettingsSection, SettingsView};

const THEMES: [(AppearancePref, &str); 3] = [
    (AppearancePref::Auto, "System"),
    (AppearancePref::Fixed(Scheme::Light), "Light"),
    (AppearancePref::Fixed(Scheme::Dark), "Dark"),
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
            SettingsSection::History => history_page(self, &tokens, cx),
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
                div()
                    .flex_1()
                    .h_full()
                    .relative()
                    .p(px(8.))
                    .child(
                        div()
                            .id("settings-content")
                            .size_full()
                            .overflow_y_scroll()
                            .track_scroll(&self.content_scroll)
                            .rounded(px(12.))
                            .border_1()
                            .border_color(tokens.hairline)
                            .bg(tokens.content)
                            .px(px(28.))
                            .py(px(24.))
                            .child(content),
                    )
                    .child(scrollbar(self.content_scroll.clone(), tokens.muted)),
            )
    }
}

fn scrollbar(scroll: ScrollHandle, color: Rgba) -> impl IntoElement {
    canvas(
        move |_, _, _| (),
        move |track, _, window, _| {
            let max_offset: f32 = scroll.max_offset().height.into();
            if max_offset <= 0. {
                return;
            }

            let viewport_height: f32 = scroll.bounds().size.height.into();
            let track_height: f32 = track.size.height.into();
            let thumb_height = (track_height * viewport_height / (viewport_height + max_offset))
                .max(24.)
                .min(track_height);
            let offset: f32 = (-scroll.offset().y).into();
            let thumb_top = (track_height - thumb_height) * (offset / max_offset);

            window.paint_quad(fill(
                bounds(
                    point(track.left(), track.top() + px(thumb_top)),
                    size(track.size.width, px(thumb_height)),
                ),
                color,
            ));
        },
    )
    .absolute()
    .top(px(16.))
    .bottom(px(16.))
    .right(px(12.))
    .w(px(4.))
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
                    SettingsSection::History,
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
                    this.insertion_open = false;
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

pub(super) fn page(title: &'static str, body: impl IntoElement) -> AnyElement {
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

pub(super) fn settings_group(tokens: &Tokens) -> Div {
    div()
        .w_full()
        .rounded(px(10.))
        .border_1()
        .border_color(tokens.hairline)
        .bg(tokens.group)
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

pub(super) fn error_text(tokens: &Tokens, text: String) -> impl IntoElement {
    div().text_xs().text_color(tokens.muted).child(text)
}
