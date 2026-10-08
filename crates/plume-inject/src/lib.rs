//! Per-OS text injection for [`plume_core::TextInjector`].
//!
//! `replace_last` always deletes `old` with backspaces and retypes `new`.

use std::error::Error;
use std::fmt;

use plume_core::BoxError;

#[cfg(target_os = "macos")]
mod ax;
mod clipboard;
#[cfg(any(test, target_os = "macos"))]
mod macos;
mod ops;
mod transaction;
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
    fn field_context(&self) -> Option<plume_core::FieldContext> {
        #[cfg(target_os = "macos")]
        {
            ax::context()
        }
        #[cfg(target_os = "windows")]
        {
            windows::field_context()
        }
        #[cfg(target_os = "linux")]
        {
            None
        }
    }

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
        self.insert_checked(text, mode, &mut || true)
    }
    fn insert_checked(
        &mut self,
        text: &str,
        mode: plume_core::InsertionMode,
        before_dispatch: &mut dyn FnMut() -> bool,
    ) -> Result<plume_core::InjectionReport, BoxError> {
        let context = self.field_context();
        let (application, target) = self.target_info();
        reject_target(target)?;
        let mut transaction = DeliveryTarget {
            injector: self,
            context,
            application,
            target,
            before_dispatch,
        };
        let method = transaction::deliver(&mut transaction, text, mode)?;
        let application = transaction.application;
        let target = transaction.target;
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

fn reject_target(target: plume_core::TargetAssessment) -> Result<(), BoxError> {
    match target {
        plume_core::TargetAssessment::Sensitive => {
            Err(InjectError::Message("focused field is sensitive".into()).into())
        }
        plume_core::TargetAssessment::NonEditable => {
            Err(InjectError::Message("focused field is read-only".into()).into())
        }
        _ => Ok(()),
    }
}
struct DeliveryTarget<'a> {
    injector: &'a mut NativeInjector,
    context: Option<plume_core::FieldContext>,
    application: Option<String>,
    target: plume_core::TargetAssessment,
    before_dispatch: &'a mut dyn FnMut() -> bool,
}
impl transaction::Target for DeliveryTarget<'_> {
    type Snapshot = clipboard::Snapshot;
    fn snapshot(&mut self) -> Result<Self::Snapshot, BoxError> {
        clipboard::Snapshot::capture()
    }
    fn prepare(&mut self, text: &str) -> Result<String, BoxError> {
        let current = self.injector.field_context();
        (self.application, self.target) = self.injector.target_info();
        reject_target(self.target)?;
        let context = current.as_ref().filter(|now| {
            self.context
                .as_ref()
                .is_some_and(|old| now.target_id == old.target_id)
        });
        Ok(plume_core::boundary_spacing(text, context))
    }
    fn type_text(&mut self, text: &str) -> Result<(), BoxError> {
        self.injector.insert(text)
    }
    fn stage(&mut self, text: &str) -> Result<(), BoxError> {
        self.injector.set_clipboard(text)
    }
    fn version(&self) -> Result<u64, BoxError> {
        clipboard::version()
    }
    fn paste(&mut self) -> Result<(), BoxError> {
        self.injector.paste()
    }
    fn allow_dispatch(&mut self) -> bool {
        (self.before_dispatch)()
    }
    fn consumed(&mut self) {
        std::thread::sleep(std::time::Duration::from_millis(300));
    }
    fn restore(&mut self, snapshot: Self::Snapshot, token: u64) -> Result<(), BoxError> {
        #[cfg(windows)]
        {
            snapshot.restore_if_owned(token)
        }
        #[cfg(not(windows))]
        {
            let _ = token;
            snapshot.restore()
        }
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
