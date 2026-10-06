mod controls;
mod tokens;

pub use controls::{AccentButton, BubbleFrame, InsetRow, ListGroup, Segment, Segmented};
pub use tokens::{Palette, Tokens};

/// Cadence for polling background updates displayed by GPUI.
pub const UI_REFRESH_INTERVAL: std::time::Duration = std::time::Duration::from_millis(16);
