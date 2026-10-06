//! Per-OS text injection for [`plume_core::TextInjector`].
//!
//! `replace_last` always deletes `old` with backspaces and retypes `new`.

use std::error::Error;
use std::fmt;

use plume_core::BoxError;

#[cfg(any(test, target_os = "macos"))]
mod macos;
mod ops;
#[cfg(any(test, target_os = "windows"))]
mod windows;
#[cfg(target_os = "linux")]
mod x11;

pub use plume_core::TextInjector;

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
compile_error!("plume-inject supports linux, macos, and windows");

/// Native injector for the current OS session.
pub struct NativeInjector {
    inner: Inner,
    clipboard: Option<arboard::Clipboard>,
}

enum Inner {
    #[cfg(target_os = "linux")]
    X11(Box<x11::X11Injector>),
    #[cfg(target_os = "macos")]
    Mac(macos::MacInjector),
    #[cfg(target_os = "windows")]
    Win(windows::WinInjector),
}

#[derive(Debug)]
pub enum InjectError {
    Unsupported { reason: &'static str },
    Message(String),
}

impl fmt::Display for InjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported { reason } => f.write_str(reason),
            Self::Message(message) => f.write_str(message),
        }
    }
}

impl Error for InjectError {}

impl NativeInjector {
    /// Connect to the session's injection mechanism.
    ///
    /// Linux uses X11 XTEST when `DISPLAY` is set. A Wayland-only session
    /// (no `DISPLAY`) fails here, before an injector exists.
    pub fn connect() -> Result<Self, BoxError> {
        connect_os()
    }
}

#[cfg(target_os = "linux")]
fn connect_os() -> Result<NativeInjector, BoxError> {
    require_x11_session(
        std::env::var_os("DISPLAY").as_deref(),
        std::env::var_os("WAYLAND_DISPLAY").as_deref(),
    )?;
    Ok(NativeInjector {
        inner: Inner::X11(Box::new(x11::X11Injector::connect()?)),
        clipboard: arboard::Clipboard::new().ok(),
    })
}

#[cfg(target_os = "macos")]
fn connect_os() -> Result<NativeInjector, BoxError> {
    Ok(NativeInjector {
        inner: Inner::Mac(macos::MacInjector),
        clipboard: arboard::Clipboard::new().ok(),
    })
}

#[cfg(target_os = "windows")]
fn connect_os() -> Result<NativeInjector, BoxError> {
    Ok(NativeInjector {
        inner: Inner::Win(windows::WinInjector),
        clipboard: arboard::Clipboard::new().ok(),
    })
}

impl TextInjector for NativeInjector {
    fn insert(&mut self, text: &str) -> Result<(), BoxError> {
        match &mut self.inner {
            #[cfg(target_os = "linux")]
            Inner::X11(injector) => injector.insert(text),
            #[cfg(target_os = "macos")]
            Inner::Mac(injector) => injector.insert(text),
            #[cfg(target_os = "windows")]
            Inner::Win(injector) => injector.insert(text),
        }
    }

    fn replace_last(&mut self, old: &str, new: &str) -> Result<(), BoxError> {
        match &mut self.inner {
            #[cfg(target_os = "linux")]
            Inner::X11(injector) => injector.replace_last(old, new),
            #[cfg(target_os = "macos")]
            Inner::Mac(injector) => injector.replace_last(old, new),
            #[cfg(target_os = "windows")]
            Inner::Win(injector) => injector.replace_last(old, new),
        }
    }

    fn insert_with_mode(
        &mut self,
        text: &str,
        mode: plume_core::InsertionMode,
    ) -> Result<plume_core::InjectionReport, BoxError> {
        let (application, target) = self.target_info();
        match target {
            plume_core::TargetAssessment::Sensitive => {
                return Err(InjectError::Message("focused field is sensitive".into()).into())
            }
            plume_core::TargetAssessment::NonEditable => {
                return Err(InjectError::Message("focused field is read-only".into()).into())
            }
            plume_core::TargetAssessment::Editable | plume_core::TargetAssessment::Unknown => {}
        }

        let method = match mode {
            plume_core::InsertionMode::Typing => {
                self.insert(text)?;
                plume_core::InsertionMethod::Typing
            }
            plume_core::InsertionMode::Clipboard => {
                self.set_clipboard(text)?;
                self.paste()?;
                plume_core::InsertionMethod::Clipboard
            }
            plume_core::InsertionMode::Auto => match self.set_clipboard(text) {
                Ok(()) => {
                    // Once dispatch begins we cannot know whether the target consumed part of the
                    // shortcut. Do not type a second copy if posting the shortcut reports an error.
                    self.paste()?;
                    plume_core::InsertionMethod::Clipboard
                }
                Err(clipboard_error) => {
                    tracing::warn!(
                        "plume-inject: clipboard unavailable, using typing: {clipboard_error}"
                    );
                    self.insert(text)?;
                    plume_core::InsertionMethod::Typing
                }
            },
        };
        Ok(plume_core::InjectionReport {
            method,
            application,
            target,
        })
    }

    fn copy_text(&mut self, text: &str) -> Result<(), BoxError> {
        self.set_clipboard(text)
    }
}

impl NativeInjector {
    fn set_clipboard(&mut self, text: &str) -> Result<(), BoxError> {
        let clipboard = self
            .clipboard
            .as_mut()
            .ok_or_else(|| InjectError::Message("system clipboard is unavailable".into()))?;
        clipboard
            .set_text(text.to_string())
            .map_err(|err| InjectError::Message(format!("clipboard: {err}")))?;
        Ok(())
    }

    fn paste(&mut self) -> Result<(), BoxError> {
        match &mut self.inner {
            #[cfg(target_os = "linux")]
            Inner::X11(injector) => injector.paste(),
            #[cfg(target_os = "macos")]
            Inner::Mac(injector) => injector.paste(),
            #[cfg(target_os = "windows")]
            Inner::Win(injector) => injector.paste(),
        }
    }

    fn target_info(&self) -> (Option<String>, plume_core::TargetAssessment) {
        match &self.inner {
            #[cfg(target_os = "linux")]
            Inner::X11(injector) => injector.target_info(),
            #[cfg(target_os = "macos")]
            Inner::Mac(injector) => injector.target_info(),
            #[cfg(target_os = "windows")]
            Inner::Win(injector) => injector.target_info(),
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
) -> Result<(), InjectError> {
    match linux_session(display, wayland) {
        LinuxSession::X11 => Ok(()),
        LinuxSession::WaylandOnly => Err(InjectError::Unsupported {
            reason: LINUX_REQUIRES_X11,
        }),
        LinuxSession::Headless => Err(InjectError::Unsupported {
            reason: "neither DISPLAY nor WAYLAND_DISPLAY is set",
        }),
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::*;

    #[test]
    fn wayland_only_session_is_refused_before_an_injector_exists() {
        let err = require_x11_session(None, Some(OsStr::new("wayland-0"))).unwrap_err();
        assert_eq!(err.to_string(), LINUX_REQUIRES_X11);
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
            "neither DISPLAY nor WAYLAND_DISPLAY is set"
        );
    }
}
