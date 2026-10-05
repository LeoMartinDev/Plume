use crate::BoxError;

/// Listens for the push-to-talk shortcut outside the focused window. One
/// implementation per operating system.
pub trait GlobalHotkey {
    fn register(&mut self, shortcut: &str) -> Result<(), BoxError>;
    fn next_event(&mut self) -> Option<HotkeyEvent>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotkeyEvent {
    Pressed,
    Released,
}
