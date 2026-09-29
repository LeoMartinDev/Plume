use crate::ops::Stroke;

#[cfg(target_os = "macos")]
use crate::ops::{insert_strokes, replace_strokes};

/// HID virtual keycodes from Inside Macintosh / Events.h.
const KVK_RETURN: u16 = 0x24;
const KVK_TAB: u16 = 0x30;
const KVK_DELETE: u16 = 0x33;
#[cfg(target_os = "macos")]
const KVK_COMMAND: u16 = 0x37;
#[cfg(target_os = "macos")]
const KVK_V: u16 = 0x09;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MacStroke {
    pub keycode: u16,
    pub down: bool,
    pub unicode: Option<char>,
}

pub(crate) fn mac_strokes(strokes: &[Stroke]) -> Vec<MacStroke> {
    let mut out = Vec::new();
    for stroke in strokes {
        match *stroke {
            Stroke::Backspace => push_key(&mut out, KVK_DELETE, None),
            Stroke::Return => push_key(&mut out, KVK_RETURN, None),
            Stroke::Tab => push_key(&mut out, KVK_TAB, None),
            Stroke::Char(c) => push_key(&mut out, 0, Some(c)),
        }
    }
    out
}

fn push_key(out: &mut Vec<MacStroke>, keycode: u16, unicode: Option<char>) {
    out.push(MacStroke {
        keycode,
        down: true,
        unicode,
    });
    out.push(MacStroke {
        keycode,
        down: false,
        unicode,
    });
}

#[cfg(target_os = "macos")]
mod sys {
    use core_graphics::event::{CGEvent, CGEventTapLocation};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
    use stt_core::BoxError;

    use super::MacStroke;
    use crate::InjectError;

    pub(super) fn post(strokes: &[MacStroke]) -> Result<(), BoxError> {
        let mut events = Vec::with_capacity(strokes.len());
        for stroke in strokes {
            let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
                .map_err(|()| InjectError::Message("CGEventSource::new failed".into()))?;
            let event = CGEvent::new_keyboard_event(source, stroke.keycode, stroke.down)
                .map_err(|()| InjectError::Message("CGEvent::new_keyboard_event failed".into()))?;
            if let Some(ch) = stroke.unicode {
                let mut utf8 = [0u8; 4];
                event.set_string(ch.encode_utf8(&mut utf8));
            }
            events.push(event);
        }
        for event in events {
            event.post(CGEventTapLocation::HID);
        }
        Ok(())
    }
}

#[cfg(target_os = "macos")]
pub(crate) struct MacInjector;

#[cfg(target_os = "macos")]
impl MacInjector {
    pub(crate) fn paste(&mut self) -> Result<(), stt_core::BoxError> {
        sys::post(&[
            MacStroke {
                keycode: KVK_COMMAND,
                down: true,
                unicode: None,
            },
            MacStroke {
                keycode: KVK_V,
                down: true,
                unicode: None,
            },
            MacStroke {
                keycode: KVK_V,
                down: false,
                unicode: None,
            },
            MacStroke {
                keycode: KVK_COMMAND,
                down: false,
                unicode: None,
            },
        ])
    }

    pub(crate) fn target_info(&self) -> (Option<String>, stt_core::TargetAssessment) {
        (None, stt_core::TargetAssessment::Unknown)
    }
}

#[cfg(target_os = "macos")]
impl stt_core::TextInjector for MacInjector {
    fn insert(&mut self, text: &str) -> Result<(), stt_core::BoxError> {
        sys::post(&mac_strokes(&insert_strokes(text)))
    }

    fn replace_last(&mut self, old: &str, new: &str) -> Result<(), stt_core::BoxError> {
        sys::post(&mac_strokes(&replace_strokes(old, new)))
    }
}

#[cfg(test)]
mod tests {
    use super::{mac_strokes, MacStroke, KVK_DELETE, KVK_RETURN};
    use crate::ops::{insert_strokes, replace_strokes};

    fn down_up(keycode: u16, unicode: Option<char>) -> [MacStroke; 2] {
        [
            MacStroke {
                keycode,
                down: true,
                unicode,
            },
            MacStroke {
                keycode,
                down: false,
                unicode,
            },
        ]
    }

    #[test]
    fn insert_emits_unicode_down_up_per_char() {
        let mut expected = Vec::new();
        expected.extend(down_up(0, Some('é')));
        expected.extend(down_up(KVK_RETURN, None));
        assert_eq!(mac_strokes(&insert_strokes("é\n")), expected);
    }

    #[test]
    fn replace_last_emits_backspaces_then_retype() {
        let got = mac_strokes(&replace_strokes("ab", "c"));
        let mut expected = Vec::new();
        expected.extend(down_up(KVK_DELETE, None));
        expected.extend(down_up(KVK_DELETE, None));
        expected.extend(down_up(0, Some('c')));
        assert_eq!(got, expected);
    }
}
