use std::borrow::Cow;

use gpui::{point, px, rgb, rgba, App, BoxShadow, Global, Rgba, Window, WindowAppearance};

/// The website's typefaces (website/src/styles/global.css), bundled with the app.
pub const FONT_SANS: &str = "Geist";
pub const FONT_MONO: &str = "Geist Mono";
pub const FONT_SERIF: &str = "Instrument Serif";

const FONTS: [&[u8]; 5] = [
    include_bytes!("../assets/fonts/Geist-400.ttf"),
    include_bytes!("../assets/fonts/Geist-500.ttf"),
    include_bytes!("../assets/fonts/Geist-600.ttf"),
    include_bytes!("../assets/fonts/GeistMono-400.ttf"),
    include_bytes!("../assets/fonts/InstrumentSerif-Italic.ttf"),
];

struct FontsRegistered;
impl Global for FontsRegistered {}

/// Make the bundled typefaces available to every window. Idempotent.
pub fn register_fonts(cx: &mut App) {
    if cx.has_global::<FontsRegistered>() {
        return;
    }
    let fonts = FONTS.iter().map(|bytes| Cow::Borrowed(*bytes)).collect();
    if let Err(error) = cx.text_system().add_fonts(fonts) {
        eprintln!("plume-ui: cannot load the bundled fonts: {error}");
    }
    cx.set_global(FontsRegistered);
}

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

/// Colors shared with the website, so the app and the site read as one product.
#[derive(Clone, Copy, Debug)]
pub struct Tokens {
    pub palette: Palette,
    /// Window ground.
    pub bg: Rgba,
    /// Cards and groups laid on the ground.
    pub surface: Rgba,
    /// Fields, tracks and keycaps inside a surface.
    pub surface_2: Rgba,
    pub border: Rgba,
    pub border_strong: Rgba,
    pub text: Rgba,
    pub muted: Rgba,
    pub subtle: Rgba,
    pub faint: Rgba,
    /// Second line of display titles, set in the serif italic.
    pub title_dim: Rgba,
    pub accent: Rgba,
    /// Accent for text, legible on both grounds.
    pub accent_text: Rgba,
    pub accent_ring: Rgba,
    pub highlight: Rgba,
    pub on_accent: Rgba,
    pub btn_bg: Rgba,
    pub btn_fg: Rgba,
    pub hover: Rgba,
    pub island: Rgba,
    pub track: Rgba,
    pub switch_off: Rgba,
    /// The selected card: ink in light mode, accent in dark mode.
    pub invert_bg: Rgba,
    pub invert_fg: Rgba,
    pub invert_muted: Rgba,
    pub invert_chip: Rgba,
    pub invert_track: Rgba,
    pub warn: Rgba,
    pub warn_bg: Rgba,
    pub danger: Rgba,
    /// The dictation bubble, which inverts the window's scheme.
    pub pill_bg: Rgba,
    pub pill_fg: Rgba,
    pub pill_muted: Rgba,
    pub pill_chip: Rgba,
    pub pill_chip_fg: Rgba,
    /// Edge of a keycap drawn on the bubble.
    pub pill_chip_border: Rgba,
    pub pill_idle: Rgba,
    /// The dark stage the onboarding test runs on.
    pub stage: Rgba,
    pub stage_border: Rgba,
    pub stage_dots: Rgba,
    pub sheet: Rgba,
    pub sheet_border: Rgba,
    pub sheet_text: Rgba,
    pub sheet_title: Rgba,
    pub sheet_chrome: Rgba,
    pub scrollbar: Rgba,
    pub scrollbar_hover: Rgba,
    pub scrollbar_active: Rgba,
    shadow: Rgba,
    shadow_soft: Rgba,
}

impl Tokens {
    pub fn new(palette: Palette) -> Self {
        match palette {
            Palette::Light => Tokens {
                palette,
                bg: rgb(0xf4f4f2),
                surface: rgb(0xffffff),
                surface_2: rgb(0xf4f4f2),
                border: rgb(0xe8e8e4),
                border_strong: rgb(0xdcdcd7),
                text: rgb(0x141414),
                muted: rgb(0x55555a),
                subtle: rgb(0x6b6b70),
                faint: rgb(0x86868c),
                title_dim: rgb(0x7c7c82),
                accent: rgb(0x3b6dff),
                accent_text: rgb(0x2d57e0),
                accent_ring: rgba(0x3b6dff4d),
                highlight: rgb(0xe6ecff),
                on_accent: rgb(0xffffff),
                btn_bg: rgb(0x141414),
                btn_fg: rgb(0xffffff),
                hover: rgba(0x1414140f),
                island: rgba(0xffffffd6),
                track: rgb(0xe8e8e4),
                switch_off: rgb(0xdcdcd7),
                invert_bg: rgb(0x141414),
                invert_fg: rgb(0xffffff),
                invert_muted: rgb(0xb4b4ba),
                invert_chip: rgb(0x2a2a2e),
                invert_track: rgb(0x3a3a40),
                warn: rgb(0x9a5b00),
                warn_bg: rgb(0xfdf1dc),
                danger: rgb(0xd13438),
                pill_bg: rgb(0x141414),
                pill_fg: rgb(0xffffff),
                pill_muted: rgb(0xc9c9ce),
                pill_chip: rgb(0x2a2a2e),
                pill_chip_fg: rgb(0xe4e4e8),
                pill_chip_border: rgb(0x3a3a40),
                pill_idle: rgb(0x6e6e76),
                stage: rgb(0x141414),
                stage_border: rgb(0x141414),
                stage_dots: rgba(0xffffff12),
                sheet: rgb(0xfbfbfa),
                sheet_border: rgb(0xecece8),
                sheet_text: rgb(0x2a2a2e),
                sheet_title: rgb(0x141414),
                sheet_chrome: rgb(0xdcdcd7),
                scrollbar: rgb(0xd2d2cd),
                scrollbar_hover: rgb(0xb4b4ae),
                scrollbar_active: rgb(0x96968f),
                shadow: rgba(0x14141414),
                shadow_soft: rgba(0x1414140a),
            },
            Palette::Dark => Tokens {
                palette,
                bg: rgb(0x0b0b0c),
                surface: rgb(0x131315),
                surface_2: rgb(0x1f1f22),
                border: rgb(0x222225),
                border_strong: rgb(0x2a2a2e),
                text: rgb(0xf2f2f0),
                muted: rgb(0xa3a3aa),
                subtle: rgb(0x8a8a92),
                faint: rgb(0x6c6c74),
                title_dim: rgb(0xb4b4bc),
                accent: rgb(0x3b6dff),
                accent_text: rgb(0x8fa9ff),
                accent_ring: rgba(0x3b6dff73),
                highlight: rgba(0x3b6dff3d),
                on_accent: rgb(0xffffff),
                btn_bg: rgb(0xf2f2f0),
                btn_fg: rgb(0x0b0b0c),
                hover: rgba(0xffffff14),
                island: rgba(0x161618b8),
                track: rgb(0x2a2a2e),
                switch_off: rgb(0x2e2e33),
                invert_bg: rgb(0x3b6dff),
                invert_fg: rgb(0xffffff),
                invert_muted: rgb(0xe8edff),
                invert_chip: rgba(0xffffff29),
                invert_track: rgba(0xffffff47),
                warn: rgb(0xf0c47b),
                warn_bg: rgba(0xf0c47b1f),
                danger: rgb(0xff6369),
                pill_bg: rgb(0xf2f2f0),
                pill_fg: rgb(0x0b0b0c),
                pill_muted: rgb(0x45454b),
                pill_chip: rgb(0xe2e2df),
                pill_chip_fg: rgb(0x2a2a2e),
                pill_chip_border: rgb(0xcfcfca),
                pill_idle: rgb(0x8a8a90),
                stage: rgb(0x101012),
                stage_border: rgb(0x1f1f22),
                stage_dots: rgba(0xffffff0d),
                sheet: rgb(0x161618),
                sheet_border: rgb(0x2a2a2e),
                sheet_text: rgb(0xc9c9ce),
                sheet_title: rgb(0xf2f2f0),
                sheet_chrome: rgb(0x2e2e33),
                scrollbar: rgb(0x2e2e33),
                scrollbar_hover: rgb(0x46464c),
                scrollbar_active: rgb(0x5e5e66),
                shadow: rgba(0x00000066),
                shadow_soft: rgba(0x00000066),
            },
        }
    }

    /// A selected chip lifted off its track.
    pub fn raise(&self) -> Vec<BoxShadow> {
        vec![
            BoxShadow {
                color: self.shadow_soft.into(),
                offset: point(px(0.), px(1.)),
                blur_radius: px(2.),
                spread_radius: px(0.),
            },
            BoxShadow {
                color: self.shadow_soft.into(),
                offset: point(px(0.), px(4.)),
                blur_radius: px(12.),
                spread_radius: px(0.),
            },
        ]
    }

    /// Floating elements: the step island, menus.
    pub fn float(&self) -> Vec<BoxShadow> {
        vec![
            BoxShadow {
                color: self.shadow.into(),
                offset: point(px(0.), px(8.)),
                blur_radius: px(32.),
                spread_radius: px(0.),
            },
            BoxShadow {
                color: self.shadow_soft.into(),
                offset: point(px(0.), px(1.)),
                blur_radius: px(2.),
                spread_radius: px(0.),
            },
        ]
    }

    /// The focus or editing ring around a field or a card.
    pub fn ring(&self) -> Vec<BoxShadow> {
        vec![BoxShadow {
            color: self.accent_ring.into(),
            offset: point(px(0.), px(0.)),
            blur_radius: px(0.),
            spread_radius: px(3.),
        }]
    }
}
