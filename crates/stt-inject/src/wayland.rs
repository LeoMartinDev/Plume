use stt_core::{BoxError, TextInjector};

use crate::InjectError;

const WAYLAND_UNSUPPORTED: &str = "Wayland text injection is not implemented. Compositors expose this through zwp_virtual_keyboard_v1 and the xdg-desktop-portal virtual-keyboard portal.";

pub(crate) struct WaylandInjector;

impl TextInjector for WaylandInjector {
    fn insert(&mut self, _text: &str) -> Result<(), BoxError> {
        Err(InjectError::Unsupported {
            reason: WAYLAND_UNSUPPORTED,
        }
        .into())
    }

    fn replace_last(&mut self, _old: &str, _new: &str) -> Result<(), BoxError> {
        Err(InjectError::Unsupported {
            reason: WAYLAND_UNSUPPORTED,
        }
        .into())
    }
}

#[cfg(test)]
mod tests {
    use super::WaylandInjector;
    use stt_core::TextInjector;

    #[test]
    fn insert_is_unsupported_and_names_the_virtual_keyboard_portal() {
        let mut injector = WaylandInjector;
        let err = injector.insert("hello").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("virtual-keyboard"),
            "unsupported error should name the portal, got {msg}"
        );
        assert!(
            injector.replace_last("hello", "hi").is_err(),
            "replace_last must also refuse on Wayland"
        );
    }
}
