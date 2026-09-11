//! Per-OS text injection for [`stt_core::TextInjector`].
//!
//! `replace_last` always deletes `old` with backspaces and retypes `new`.

use std::error::Error;
use std::fmt;

use stt_core::BoxError;

#[cfg(any(test, target_os = "macos"))]
mod macos;
mod ops;
#[cfg(any(test, target_os = "linux"))]
mod wayland;
#[cfg(any(test, target_os = "windows"))]
mod windows;
#[cfg(target_os = "linux")]
mod x11;

pub use stt_core::TextInjector;

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
compile_error!("stt-inject supports linux, macos, and windows");

/// Native injector for the current OS session.
pub struct NativeInjector {
    inner: Inner,
}

enum Inner {
    #[cfg(target_os = "linux")]
    X11(Box<x11::X11Injector>),
    #[cfg(target_os = "linux")]
    Wayland(wayland::WaylandInjector),
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
    /// (no `DISPLAY`) yields an injector whose methods return
    /// [`InjectError::Unsupported`] and name the virtual-keyboard portal.
    pub fn connect() -> Result<Self, BoxError> {
        connect_os()
    }
}

#[cfg(target_os = "linux")]
fn connect_os() -> Result<NativeInjector, BoxError> {
    if std::env::var_os("DISPLAY").is_some() {
        return Ok(NativeInjector {
            inner: Inner::X11(Box::new(x11::X11Injector::connect()?)),
        });
    }
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        return Ok(NativeInjector {
            inner: Inner::Wayland(wayland::WaylandInjector),
        });
    }
    Err(InjectError::Unsupported {
        reason: "neither DISPLAY nor WAYLAND_DISPLAY is set",
    }
    .into())
}

#[cfg(target_os = "macos")]
fn connect_os() -> Result<NativeInjector, BoxError> {
    Ok(NativeInjector {
        inner: Inner::Mac(macos::MacInjector),
    })
}

#[cfg(target_os = "windows")]
fn connect_os() -> Result<NativeInjector, BoxError> {
    Ok(NativeInjector {
        inner: Inner::Win(windows::WinInjector),
    })
}

impl TextInjector for NativeInjector {
    fn insert(&mut self, text: &str) -> Result<(), BoxError> {
        match &mut self.inner {
            #[cfg(target_os = "linux")]
            Inner::X11(injector) => injector.insert(text),
            #[cfg(target_os = "linux")]
            Inner::Wayland(injector) => injector.insert(text),
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
            #[cfg(target_os = "linux")]
            Inner::Wayland(injector) => injector.replace_last(old, new),
            #[cfg(target_os = "macos")]
            Inner::Mac(injector) => injector.replace_last(old, new),
            #[cfg(target_os = "windows")]
            Inner::Win(injector) => injector.replace_last(old, new),
        }
    }
}
