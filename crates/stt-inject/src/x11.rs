use stt_core::{BoxError, TextInjector};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConnectionExt as XProtoExt, KEY_PRESS_EVENT, KEY_RELEASE_EVENT};
use x11rb::protocol::xtest::ConnectionExt as XTestExt;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;
use x11rb::{CURRENT_TIME, NONE};

use crate::ops::{insert_strokes, replace_strokes, Stroke};
use crate::InjectError;

const XK_BACKSPACE: u32 = 0xff08;
const XK_TAB: u32 = 0xff09;
const XK_RETURN: u32 = 0xff0d;
const XK_SHIFT_L: u32 = 0xffe1;
const XK_SHIFT_R: u32 = 0xffe2;
const UNICODE_KEYSYM_BASE: u32 = 0x0100_0000;

pub(crate) struct X11Injector {
    conn: RustConnection,
}

impl X11Injector {
    pub(crate) fn connect() -> Result<Self, BoxError> {
        let (conn, _) = x11rb::connect(None)?;
        let ext = conn
            .query_extension(x11rb::protocol::xtest::X11_EXTENSION_NAME.as_bytes())?
            .reply()?;
        if !ext.present {
            return Err(InjectError::Message(
                "XTEST extension is not present on this X server".into(),
            )
            .into());
        }
        Ok(Self { conn })
    }

    fn play(&mut self, strokes: &[Stroke]) -> Result<(), BoxError> {
        if strokes.is_empty() {
            return Ok(());
        }
        let map = Keymap::load(&self.conn)?;
        for stroke in strokes {
            match *stroke {
                Stroke::Backspace => tap(&self.conn, map.backspace, false, map.shift)?,
                Stroke::Return => {
                    let (keycode, shift) = map.lookup(XK_RETURN)?;
                    tap(&self.conn, keycode, shift, map.shift)?;
                }
                Stroke::Tab => {
                    let (keycode, shift) = map.lookup(XK_TAB)?;
                    tap(&self.conn, keycode, shift, map.shift)?;
                }
                Stroke::Char(c) => {
                    let keysym = char_to_keysym(c).ok_or_else(|| {
                        InjectError::Message(format!(
                            "cannot map U+{:04X} to an X11 keysym",
                            u32::from(c)
                        ))
                    })?;
                    let (keycode, shift) = map.lookup(keysym)?;
                    tap(&self.conn, keycode, shift, map.shift)?;
                }
            }
        }
        self.conn.sync()?;
        Ok(())
    }
}

impl TextInjector for X11Injector {
    fn insert(&mut self, text: &str) -> Result<(), BoxError> {
        self.play(&insert_strokes(text))
    }

    fn replace_last(&mut self, old: &str, new: &str) -> Result<(), BoxError> {
        self.play(&replace_strokes(old, new))
    }
}

struct Keymap {
    min: u8,
    per: usize,
    keysyms: Vec<u32>,
    shift: u8,
    backspace: u8,
}

impl Keymap {
    fn load(conn: &RustConnection) -> Result<Self, BoxError> {
        let setup = conn.setup();
        let min = setup.min_keycode;
        let count = setup.max_keycode.saturating_sub(min).saturating_add(1);
        let reply = conn.get_keyboard_mapping(min, count)?.reply()?;
        let per = usize::from(reply.keysyms_per_keycode);
        if per == 0 {
            return Err(InjectError::Message(
                "GetKeyboardMapping returned 0 keysyms_per_keycode".into(),
            )
            .into());
        }
        let keysyms = reply.keysyms;
        let shift = lookup_keysym(&keysyms, min, per, XK_SHIFT_L)
            .or_else(|| lookup_keysym(&keysyms, min, per, XK_SHIFT_R))
            .ok_or_else(|| InjectError::Message("Shift is not on the current X11 keymap".into()))?
            .0;
        let backspace = lookup_keysym(&keysyms, min, per, XK_BACKSPACE)
            .ok_or_else(|| {
                InjectError::Message("BackSpace is not on the current X11 keymap".into())
            })?
            .0;
        Ok(Self {
            min,
            per,
            keysyms,
            shift,
            backspace,
        })
    }

    fn lookup(&self, keysym: u32) -> Result<(u8, bool), BoxError> {
        lookup_keysym(&self.keysyms, self.min, self.per, keysym).ok_or_else(|| {
            InjectError::Message(format!(
                "keysym {keysym:#x} is not on the current X11 keymap"
            ))
            .into()
        })
    }
}

fn lookup_keysym(keysyms: &[u32], min: u8, per: usize, target: u32) -> Option<(u8, bool)> {
    let count = keysyms.len() / per;
    for i in 0..count {
        let base = i * per;
        let slot0 = keysyms.get(base).copied();
        let slot1 = keysyms.get(base + 1).copied();
        if slot0 == Some(target) {
            return Some((min + i as u8, false));
        }
        if slot1 == Some(target) {
            return Some((min + i as u8, true));
        }
    }
    None
}

pub(crate) fn char_to_keysym(c: char) -> Option<u32> {
    match u32::from(c) {
        0x20..=0xff => Some(u32::from(c)),
        cp if cp > 0xff => Some(UNICODE_KEYSYM_BASE | cp),
        _ => None,
    }
}

fn tap(conn: &RustConnection, keycode: u8, need_shift: bool, shift: u8) -> Result<(), BoxError> {
    if need_shift {
        fake_key(conn, KEY_PRESS_EVENT, shift)?;
    }
    fake_key(conn, KEY_PRESS_EVENT, keycode)?;
    fake_key(conn, KEY_RELEASE_EVENT, keycode)?;
    if need_shift {
        fake_key(conn, KEY_RELEASE_EVENT, shift)?;
    }
    Ok(())
}

fn fake_key(conn: &RustConnection, event_type: u8, keycode: u8) -> Result<(), BoxError> {
    conn.xtest_fake_input(event_type, keycode, CURRENT_TIME, NONE, 0, 0, 0)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::char_to_keysym;

    #[test]
    fn latin1_and_unicode_keysyms() {
        assert_eq!(char_to_keysym('a'), Some(0x61));
        assert_eq!(char_to_keysym('A'), Some(0x41));
        assert_eq!(char_to_keysym('é'), Some(0xe9));
        assert_eq!(char_to_keysym('€'), Some(0x0100_20ac));
        assert_eq!(char_to_keysym('\u{7}'), None);
    }
}

#[cfg(test)]
mod loopback;
