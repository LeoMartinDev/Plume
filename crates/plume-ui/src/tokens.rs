use gpui::{div, prelude::*, rgb, Div, Rgba, Stateful, Window, WindowAppearance};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Palette {
    Light,
    Dark,
}

impl Palette {
    pub fn from_window(window: &Window) -> Self {
        Self::from_appearance(window.appearance())
    }

    pub fn from_appearance(appearance: WindowAppearance) -> Self {
        match appearance {
            WindowAppearance::Light | WindowAppearance::VibrantLight => Palette::Light,
            WindowAppearance::Dark | WindowAppearance::VibrantDark => Palette::Dark,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Tokens {
    pub chrome: Rgba,
    pub content: Rgba,
    pub canvas: Rgba,
    pub group: Rgba,
    pub elevated: Rgba,
    pub text: Rgba,
    pub muted: Rgba,
    pub scrollbar: Rgba,
    pub scrollbar_hover: Rgba,
    pub scrollbar_active: Rgba,
    pub hairline: Rgba,
    pub fill: Rgba,
    pub fill_hover: Rgba,
    pub accent: Rgba,
    pub accent_soft: Rgba,
    pub on_accent: Rgba,
    pub status: Rgba,
}

impl Tokens {
    pub fn new(palette: Palette) -> Self {
        match palette {
            Palette::Light => Tokens {
                chrome: rgb(0xf3f3f3),
                content: rgb(0xffffff),
                canvas: rgb(0xf7f7f7),
                group: rgb(0xf3f3f3),
                elevated: rgb(0xffffff),
                text: rgb(0x141414),
                muted: rgb(0x6b6b6b),
                scrollbar: rgb(0xc4c4c4),
                scrollbar_hover: rgb(0xa3a3a3),
                scrollbar_active: rgb(0x858585),
                hairline: rgb(0xe5e5e5),
                fill: rgb(0xececec),
                fill_hover: rgb(0xe4e4e4),
                accent: rgb(0x3b6dff),
                accent_soft: rgb(0xf2f6ff),
                on_accent: rgb(0xffffff),
                status: rgb(0x3b6dff),
            },
            Palette::Dark => Tokens {
                chrome: rgb(0x202020),
                content: rgb(0x141414),
                canvas: rgb(0x161616),
                group: rgb(0x202020),
                elevated: rgb(0x1e1e1e),
                text: rgb(0xe8e8e8),
                muted: rgb(0x999999),
                scrollbar: rgb(0x484848),
                scrollbar_hover: rgb(0x626262),
                scrollbar_active: rgb(0x7a7a7a),
                hairline: rgb(0x2b2b2b),
                fill: rgb(0x2a2a2a),
                fill_hover: rgb(0x353535),
                accent: rgb(0x3b6dff),
                accent_soft: rgb(0x202733),
                on_accent: rgb(0xffffff),
                status: rgb(0x9cdcfe),
            },
        }
    }

    pub fn page(self) -> Stateful<Div> {
        div()
            .id("settings-page")
            .flex()
            .flex_col()
            .size_full()
            .overflow_y_scroll()
            .bg(self.canvas)
            .text_color(self.text)
            .px_6()
            .py_5()
            .gap_4()
    }
}
