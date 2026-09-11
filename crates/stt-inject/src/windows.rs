use crate::ops::Stroke;

#[cfg(target_os = "windows")]
use crate::ops::{insert_strokes, replace_strokes};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WinStroke {
    UnicodeDown(u16),
    UnicodeUp(u16),
    BackspaceDown,
    BackspaceUp,
    ReturnDown,
    ReturnUp,
    TabDown,
    TabUp,
}

pub(crate) fn win_strokes(strokes: &[Stroke]) -> Vec<WinStroke> {
    let mut out = Vec::new();
    for stroke in strokes {
        match *stroke {
            Stroke::Backspace => {
                out.push(WinStroke::BackspaceDown);
                out.push(WinStroke::BackspaceUp);
            }
            Stroke::Return => {
                out.push(WinStroke::ReturnDown);
                out.push(WinStroke::ReturnUp);
            }
            Stroke::Tab => {
                out.push(WinStroke::TabDown);
                out.push(WinStroke::TabUp);
            }
            Stroke::Char(c) => {
                let mut buf = [0u16; 2];
                for &unit in &*c.encode_utf16(&mut buf) {
                    out.push(WinStroke::UnicodeDown(unit));
                    out.push(WinStroke::UnicodeUp(unit));
                }
            }
        }
    }
    out
}

#[cfg(target_os = "windows")]
mod sys {
    use std::mem::size_of;

    use stt_core::BoxError;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE,
        VK_BACK, VK_RETURN, VK_TAB,
    };

    use super::WinStroke;
    use crate::InjectError;

    pub(super) fn post(strokes: &[WinStroke]) -> Result<(), BoxError> {
        if strokes.is_empty() {
            return Ok(());
        }
        let inputs: Vec<INPUT> = strokes.iter().copied().map(to_input).collect();
        let sent = unsafe {
            SendInput(
                inputs.len() as u32,
                inputs.as_ptr(),
                size_of::<INPUT>() as i32,
            )
        };
        if sent as usize != inputs.len() {
            return Err(InjectError::Message(format!(
                "SendInput posted {sent} of {} events",
                inputs.len()
            ))
            .into());
        }
        Ok(())
    }

    fn to_input(stroke: WinStroke) -> INPUT {
        let (vk, scan, flags) = match stroke {
            WinStroke::UnicodeDown(unit) => (0, unit, KEYEVENTF_UNICODE),
            WinStroke::UnicodeUp(unit) => (0, unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP),
            WinStroke::BackspaceDown => (VK_BACK, 0, 0),
            WinStroke::BackspaceUp => (VK_BACK, 0, KEYEVENTF_KEYUP),
            WinStroke::ReturnDown => (VK_RETURN, 0, 0),
            WinStroke::ReturnUp => (VK_RETURN, 0, KEYEVENTF_KEYUP),
            WinStroke::TabDown => (VK_TAB, 0, 0),
            WinStroke::TabUp => (VK_TAB, 0, KEYEVENTF_KEYUP),
        };
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: scan,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }
}

#[cfg(target_os = "windows")]
pub(crate) struct WinInjector;

#[cfg(target_os = "windows")]
impl stt_core::TextInjector for WinInjector {
    fn insert(&mut self, text: &str) -> Result<(), stt_core::BoxError> {
        sys::post(&win_strokes(&insert_strokes(text)))
    }

    fn replace_last(&mut self, old: &str, new: &str) -> Result<(), stt_core::BoxError> {
        sys::post(&win_strokes(&replace_strokes(old, new)))
    }
}

#[cfg(test)]
mod tests {
    use super::{win_strokes, WinStroke};
    use crate::ops::{insert_strokes, replace_strokes};

    #[test]
    fn insert_emits_unicode_units_down_then_up() {
        assert_eq!(
            win_strokes(&insert_strokes("hi")),
            vec![
                WinStroke::UnicodeDown(u16::from(b'h')),
                WinStroke::UnicodeUp(u16::from(b'h')),
                WinStroke::UnicodeDown(u16::from(b'i')),
                WinStroke::UnicodeUp(u16::from(b'i')),
            ]
        );
    }

    #[test]
    fn insert_splits_supplementary_plane_into_surrogate_pair() {
        // U+1F600 😀 is one char, two UTF-16 units.
        let strokes = win_strokes(&insert_strokes("\u{1F600}"));
        assert_eq!(
            strokes,
            vec![
                WinStroke::UnicodeDown(0xD83D),
                WinStroke::UnicodeUp(0xD83D),
                WinStroke::UnicodeDown(0xDE00),
                WinStroke::UnicodeUp(0xDE00),
            ]
        );
    }

    #[test]
    fn replace_last_emits_backspaces_then_retype() {
        assert_eq!(
            win_strokes(&replace_strokes("hi", "o")),
            vec![
                WinStroke::BackspaceDown,
                WinStroke::BackspaceUp,
                WinStroke::BackspaceDown,
                WinStroke::BackspaceUp,
                WinStroke::UnicodeDown(u16::from(b'o')),
                WinStroke::UnicodeUp(u16::from(b'o')),
            ]
        );
    }

    #[test]
    fn newline_is_return_not_unicode() {
        assert_eq!(
            win_strokes(&insert_strokes("\n")),
            vec![WinStroke::ReturnDown, WinStroke::ReturnUp]
        );
    }
}
