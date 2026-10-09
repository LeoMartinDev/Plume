//! Native permission checks are separate from microphone/device readiness.
use std::sync::mpsc::{self, Receiver};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Permission {
    Granted,
    NotRequested,
    Denied,
}

#[derive(Clone, Copy, Debug)]
pub struct Permissions {
    pub microphone: Permission,
    pub accessibility: Permission,
}

impl Permissions {
    pub fn granted(self) -> bool {
        self.microphone == Permission::Granted && self.accessibility == Permission::Granted
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::{Permission, Permissions};
    use cocoa::base::nil;
    use cocoa::foundation::NSString;
    use objc::{
        runtime::{Class, Sel},
        Message,
    };

    #[link(name = "AVFoundation", kind = "framework")]
    extern "C" {}
    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrusted() -> bool;
    }

    pub fn snapshot() -> Permissions {
        // SAFETY: AVCaptureDevice is supplied by the linked AVFoundation framework.
        // This query neither creates a capture session nor prompts the user.
        unsafe {
            let audio = NSString::alloc(nil).init_str("soun");
            let status: isize = Class::get("AVCaptureDevice")
                .expect("AVFoundation loaded")
                .send_message(Sel::register("authorizationStatusForMediaType:"), (audio,))
                .unwrap();
            let _: () = (&*audio)
                .send_message(Sel::register("release"), ())
                .unwrap();
            Permissions {
                microphone: match status {
                    3 => Permission::Granted,
                    0 => Permission::NotRequested,
                    _ => Permission::Denied,
                },
                accessibility: if AXIsProcessTrusted() {
                    Permission::Granted
                } else {
                    Permission::Denied
                },
            }
        }
    }

    pub fn request_microphone(completed: std::sync::mpsc::Sender<()>) {
        // SAFETY: AVFoundation retains the copied completion block until the
        // asynchronous native prompt finishes. No UI state is touched by it.
        unsafe {
            let audio = NSString::alloc(nil).init_str("soun");
            if audio == nil {
                return;
            }
            let completion = block::ConcreteBlock::new(move |_granted: cocoa::base::BOOL| {
                let _ = completed.send(());
            })
            .copy();
            let _: () = Class::get("AVCaptureDevice")
                .expect("AVFoundation loaded")
                .send_message(
                    Sel::register("requestAccessForMediaType:completionHandler:"),
                    (audio, &*completion),
                )
                .unwrap();
            let _: () = (&*audio)
                .send_message(Sel::register("release"), ())
                .unwrap();
        }
    }
}

pub fn snapshot() -> Permissions {
    #[cfg(target_os = "macos")]
    {
        macos::snapshot()
    }
    #[cfg(not(target_os = "macos"))]
    {
        Permissions {
            microphone: Permission::Granted,
            accessibility: Permission::Granted,
        }
    }
}

/// Signals when the native prompt finishes, including when access is denied.
/// The receiver lets the UI handle completion on its own thread.
pub fn request_microphone() -> Receiver<()> {
    let (completed, completion) = mpsc::channel();
    #[cfg(target_os = "macos")]
    macos::request_microphone(completed);
    #[cfg(not(target_os = "macos"))]
    let _ = completed.send(());
    completion
}

pub fn microphone_settings_url() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"
    }
    #[cfg(windows)]
    {
        "ms-settings:privacy-microphone"
    }
    #[cfg(target_os = "linux")]
    {
        ""
    }
}

pub fn accessibility_settings_url() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
    }
    #[cfg(not(target_os = "macos"))]
    {
        ""
    }
}
