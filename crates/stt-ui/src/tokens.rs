use gpui::{div, prelude::*, rgb, Div, Rgba, Window, WindowAppearance};

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
    pub canvas: Rgba,
    pub elevated: Rgba,
    pub text: Rgba,
    pub muted: Rgba,
    pub hairline: Rgba,
    pub fill: Rgba,
    pub fill_hover: Rgba,
    pub accent: Rgba,
    pub on_accent: Rgba,
    pub status: Rgba,
}

impl Tokens {
    pub fn new(palette: Palette) -> Self {
        match palette {
            Palette::Light => Tokens {
                canvas: rgb(0xf7f7f7),
                elevated: rgb(0xffffff),
                text: rgb(0x141414),
                muted: rgb(0x6b6b6b),
                hairline: rgb(0xe5e5e5),
                fill: rgb(0xececec),
                fill_hover: rgb(0xe4e4e4),
                accent: rgb(0x3b6dff),
                on_accent: rgb(0xffffff),
                status: rgb(0x3b6dff),
            },
            Palette::Dark => Tokens {
                canvas: rgb(0x181818),
                elevated: rgb(0x1e1e1e),
                text: rgb(0xe4e4e4),
                muted: rgb(0x8a8a8a),
                hairline: rgb(0x2b2b2b),
                fill: rgb(0x2a2a2a),
                fill_hover: rgb(0x333333),
                accent: rgb(0x3b6dff),
                on_accent: rgb(0xffffff),
                status: rgb(0x9cdcfe),
            },
        }
    }

    pub fn page(self) -> Div {
        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(self.canvas)
            .text_color(self.text)
            .px_6()
            .py_5()
            .gap_4()
    }
}
