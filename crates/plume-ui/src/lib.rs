mod controls;
mod tokens;

pub use controls::{
    eyebrow, key_label, keycaps, Button, ButtonSize, ButtonVariant, IconButton, KeySize, Segment,
    Segmented, Switch,
};
pub use tokens::{register_fonts, Palette, Tokens, FONT_MONO, FONT_SANS, FONT_SERIF};

/// Cadence for polling background updates displayed by GPUI.
pub const UI_REFRESH_INTERVAL: std::time::Duration = std::time::Duration::from_millis(16);
