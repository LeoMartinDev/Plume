use std::error::Error;
use std::fmt;

mod chord;
mod macos;
mod shortcut;
#[cfg(any(test, target_os = "linux"))]
mod wayland;
mod windows;

#[cfg(target_os = "linux")]
mod x11;

pub use stt_core::{BoxError, GlobalHotkey, HotkeyEvent};

/// Grammar check without an OS handle: parse-only, for config validation.
pub fn validate_shortcut(raw: &str) -> Result<(), HotkeyError> {
    shortcut::Shortcut::parse(raw).map(|_| ())
}

#[derive(Debug)]
pub enum HotkeyError {
    Unsupported(&'static str),
    InvalidShortcut(String),
    Os(String),
}

impl fmt::Display for HotkeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HotkeyError::Unsupported(reason) => write!(f, "unsupported hotkey: {reason}"),
            HotkeyError::InvalidShortcut(raw) => write!(f, "invalid shortcut {raw:?}"),
            HotkeyError::Os(detail) => write!(f, "hotkey os error: {detail}"),
        }
    }
}

impl Error for HotkeyError {}

pub enum PlatformHotkey {
    #[cfg(target_os = "linux")]
    X11(Box<x11::X11Hotkey>),
    #[cfg(target_os = "linux")]
    Wayland(wayland::WaylandHotkey),
    #[cfg(target_os = "macos")]
    Macos(macos::MacosHotkey),
    #[cfg(windows)]
    Windows(windows::WindowsHotkey),
}

pub fn global_hotkey() -> Result<PlatformHotkey, BoxError> {
    #[cfg(target_os = "linux")]
    {
        if std::env::var_os("DISPLAY").is_some() {
            return x11::X11Hotkey::connect()
                .map(Box::new)
                .map(PlatformHotkey::X11);
        }
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            return Ok(PlatformHotkey::Wayland(wayland::WaylandHotkey));
        }
        Err(HotkeyError::Unsupported("no DISPLAY or WAYLAND_DISPLAY is set").into())
    }

    #[cfg(target_os = "macos")]
    {
        macos::MacosHotkey::new().map(PlatformHotkey::Macos)
    }

    #[cfg(windows)]
    {
        windows::WindowsHotkey::new().map(PlatformHotkey::Windows)
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        Err(HotkeyError::Unsupported("global hotkeys are not implemented on this OS").into())
    }
}

impl GlobalHotkey for PlatformHotkey {
    fn register(&mut self, shortcut: &str) -> Result<(), BoxError> {
        match self {
            #[cfg(target_os = "linux")]
            PlatformHotkey::X11(inner) => inner.register(shortcut),
            #[cfg(target_os = "linux")]
            PlatformHotkey::Wayland(inner) => inner.register(shortcut),
            #[cfg(target_os = "macos")]
            PlatformHotkey::Macos(inner) => inner.register(shortcut),
            #[cfg(windows)]
            PlatformHotkey::Windows(inner) => inner.register(shortcut),
        }
    }

    fn next_event(&mut self) -> Option<HotkeyEvent> {
        match self {
            #[cfg(target_os = "linux")]
            PlatformHotkey::X11(inner) => inner.next_event(),
            #[cfg(target_os = "linux")]
            PlatformHotkey::Wayland(inner) => inner.next_event(),
            #[cfg(target_os = "macos")]
            PlatformHotkey::Macos(inner) => inner.next_event(),
            #[cfg(windows)]
            PlatformHotkey::Windows(inner) => inner.next_event(),
        }
    }
}
