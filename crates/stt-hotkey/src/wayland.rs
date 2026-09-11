use stt_core::{BoxError, GlobalHotkey, HotkeyEvent};

use crate::HotkeyError;

pub struct WaylandHotkey;

impl GlobalHotkey for WaylandHotkey {
    fn register(&mut self, _shortcut: &str) -> Result<(), BoxError> {
        Err(HotkeyError::Unsupported(
            "Wayland global shortcuts need the XDG GlobalShortcuts portal (org.freedesktop.portal.GlobalShortcuts). This crate does not bind the portal yet.",
        )
        .into())
    }

    fn next_event(&mut self) -> Option<HotkeyEvent> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_names_the_global_shortcuts_portal() {
        let mut hotkey = WaylandHotkey;
        let err = hotkey.register("Ctrl+Space").unwrap_err();
        let message = err.to_string();
        assert!(
            message.contains("GlobalShortcuts"),
            "error should name the portal, got {message}"
        );
        assert!(hotkey.next_event().is_none());
    }
}
