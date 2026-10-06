use std::error::Error;
use std::fmt;

mod chord;
mod macos;
mod shortcut;
mod windows;

#[cfg(target_os = "linux")]
mod x11;

pub use plume_core::{BoxError, GlobalHotkey, HotkeyEvent};

#[derive(Debug)]
pub enum HotkeyError {
    Unsupported(&'static str),
    InvalidShortcut(String),
    TooManyKeys(usize),
    Os(String),
}

impl fmt::Display for HotkeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HotkeyError::Unsupported(reason) => write!(f, "unsupported hotkey: {reason}"),
            HotkeyError::InvalidShortcut(raw) => write!(f, "invalid shortcut {raw:?}"),
            HotkeyError::TooManyKeys(count) => {
                write!(f, "shortcut can contain at most 3 keys (got {count})")
            }
            HotkeyError::Os(detail) => write!(f, "hotkey os error: {detail}"),
        }
    }
}

impl Error for HotkeyError {}

pub enum PlatformHotkey {
    #[cfg(target_os = "linux")]
    X11(Box<x11::X11Hotkey>),
    #[cfg(target_os = "macos")]
    Macos(macos::MacosHotkey),
    #[cfg(windows)]
    Windows(windows::WindowsHotkey),
}

/// Check a shortcut string against the `register` grammar without grabbing
/// anything. Config boundaries call this so a bad chord fails before any OS
/// handle exists.
pub fn validate_shortcut(raw: &str) -> Result<(), HotkeyError> {
    shortcut::Shortcut::parse(raw).map(|_| ())
}

pub fn global_hotkey() -> Result<PlatformHotkey, BoxError> {
    #[cfg(target_os = "linux")]
    {
        require_x11_session(
            std::env::var_os("DISPLAY").as_deref(),
            std::env::var_os("WAYLAND_DISPLAY").as_deref(),
        )?;
        x11::X11Hotkey::connect()
            .map(Box::new)
            .map(PlatformHotkey::X11)
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
            #[cfg(target_os = "macos")]
            PlatformHotkey::Macos(inner) => inner.next_event(),
            #[cfg(windows)]
            PlatformHotkey::Windows(inner) => inner.next_event(),
        }
    }
}

#[cfg(any(test, target_os = "linux"))]
const LINUX_REQUIRES_X11: &str =
    "Linux dictation requires an X11 session (DISPLAY). Wayland is not supported yet.";

#[cfg(any(test, target_os = "linux"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LinuxSession {
    X11,
    WaylandOnly,
    Headless,
}

#[cfg(any(test, target_os = "linux"))]
fn linux_session(
    display: Option<&std::ffi::OsStr>,
    wayland: Option<&std::ffi::OsStr>,
) -> LinuxSession {
    if display.is_some() {
        LinuxSession::X11
    } else if wayland.is_some() {
        LinuxSession::WaylandOnly
    } else {
        LinuxSession::Headless
    }
}

#[cfg(any(test, target_os = "linux"))]
fn require_x11_session(
    display: Option<&std::ffi::OsStr>,
    wayland: Option<&std::ffi::OsStr>,
) -> Result<(), HotkeyError> {
    match linux_session(display, wayland) {
        LinuxSession::X11 => Ok(()),
        LinuxSession::WaylandOnly => Err(HotkeyError::Unsupported(LINUX_REQUIRES_X11)),
        LinuxSession::Headless => Err(HotkeyError::Unsupported(
            "no DISPLAY or WAYLAND_DISPLAY is set",
        )),
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::*;

    #[test]
    fn wayland_only_session_is_refused_before_a_backend_exists() {
        let err = require_x11_session(None, Some(OsStr::new("wayland-0"))).unwrap_err();
        assert_eq!(
            err.to_string(),
            format!("unsupported hotkey: {LINUX_REQUIRES_X11}")
        );
    }

    #[test]
    fn display_selects_x11_even_when_wayland_is_set() {
        require_x11_session(Some(OsStr::new(":0")), Some(OsStr::new("wayland-0"))).unwrap();
    }

    #[test]
    fn empty_env_stays_headless() {
        let err = require_x11_session(None, None).unwrap_err();
        assert_eq!(
            err.to_string(),
            "unsupported hotkey: no DISPLAY or WAYLAND_DISPLAY is set"
        );
    }
}
