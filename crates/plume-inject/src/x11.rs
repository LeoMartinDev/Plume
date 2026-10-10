use plume_core::{BoxError, TextInjector};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    AtomEnum, ConnectionExt as XProtoExt, Window, KEY_PRESS_EVENT, KEY_RELEASE_EVENT,
};
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
const XK_CONTROL_L: u32 = 0xffe3;
const XK_CONTROL_R: u32 = 0xffe4;
const UNICODE_KEYSYM_BASE: u32 = 0x0100_0000;

pub(crate) struct X11Injector {
    conn: RustConnection,
    root: Window,
}

impl X11Injector {
    pub(crate) fn connect() -> Result<Self, BoxError> {
        let (conn, screen_num) = x11rb::connect(None)?;
        let ext = conn
            .query_extension(x11rb::protocol::xtest::X11_EXTENSION_NAME.as_bytes())?
            .reply()?;
        if !ext.present {
            return Err(InjectError::Message(
                "XTEST extension is not present on this X server".into(),
            )
            .into());
        }
        let root = conn.setup().roots[screen_num].root;
        Ok(Self { conn, root })
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

    pub(crate) fn paste(&mut self) -> Result<(), BoxError> {
        let map = Keymap::load(&self.conn)?;
        let (v, needs_shift) = map.lookup(u32::from('v'))?;
        if needs_shift {
            return Err(
                InjectError::Message("lowercase v requires Shift on this keymap".into()).into(),
            );
        }
        fake_key(&self.conn, KEY_PRESS_EVENT, map.control)?;
        fake_key(&self.conn, KEY_PRESS_EVENT, v)?;
        fake_key(&self.conn, KEY_RELEASE_EVENT, v)?;
        fake_key(&self.conn, KEY_RELEASE_EVENT, map.control)?;
        self.conn.sync()?;
        Ok(())
    }

    pub(crate) fn target_info(&self) -> (Option<String>, plume_core::TargetAssessment) {
        match self.active_window() {
            // The window manager reports no active window: nothing can take text.
            Ok(None) => (None, plume_core::TargetAssessment::NoFocus),
            Ok(Some(window)) => (
                self.application(window).ok().flatten(),
                plume_core::TargetAssessment::Unknown,
            ),
            Err(_) => (None, plume_core::TargetAssessment::Unknown),
        }
    }

    fn active_window(&self) -> Result<Option<u32>, BoxError> {
        let active_atom = self
            .conn
            .intern_atom(false, b"_NET_ACTIVE_WINDOW")?
            .reply()?
            .atom;
        Ok(self
            .conn
            .get_property(false, self.root, active_atom, AtomEnum::WINDOW, 0, 1)?
            .reply()?
            .value32()
            .and_then(|mut values| values.next())
            .filter(|window| *window != 0))
    }

    fn application(&self, window: u32) -> Result<Option<String>, BoxError> {
        let class = self
            .conn
            .get_property(false, window, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 1024)?
            .reply()?
            .value;
        let names: Vec<&[u8]> = class
            .split(|byte| *byte == 0)
            .filter(|name| !name.is_empty())
            .collect();
        Ok(names
            .get(1)
            .or_else(|| names.first())
            .map(|name| String::from_utf8_lossy(name).into_owned()))
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
    control: u8,
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
        let control = lookup_keysym(&keysyms, min, per, XK_CONTROL_L)
            .or_else(|| lookup_keysym(&keysyms, min, per, XK_CONTROL_R))
            .ok_or_else(|| InjectError::Message("Control is not on the current X11 keymap".into()))?
            .0;
        Ok(Self {
            min,
            per,
            keysyms,
            shift,
            control,
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

struct ClipboardWatch {
    conn: RustConnection,
    window: Window,
    selection: u32,
    timestamp: u32,
}
impl ClipboardWatch {
    fn new() -> Result<Self, BoxError> {
        use x11rb::protocol::{
            xfixes::{ConnectionExt as _, SelectionEventMask},
            xproto::{CreateWindowAux, WindowClass},
        };
        let (conn, screen) = x11rb::connect(None)?;
        conn.xfixes_query_version(5, 0)?.reply()?;
        let window = conn.generate_id()?;
        conn.create_window(
            0,
            window,
            conn.setup().roots[screen].root,
            0,
            0,
            1,
            1,
            0,
            WindowClass::INPUT_ONLY,
            0,
            &CreateWindowAux::new(),
        )?
        .check()?;
        let selection = conn.intern_atom(false, b"CLIPBOARD")?.reply()?.atom;
        conn.xfixes_select_selection_input(
            window,
            selection,
            SelectionEventMask::SET_SELECTION_OWNER
                | SelectionEventMask::SELECTION_WINDOW_DESTROY
                | SelectionEventMask::SELECTION_CLIENT_CLOSE,
        )?
        .check()?;
        Ok(Self {
            conn,
            window,
            selection,
            timestamp: 0,
        })
    }
    fn version(&mut self) -> Result<u64, BoxError> {
        let owner = self
            .conn
            .get_selection_owner(self.selection)?
            .reply()?
            .owner;
        while let Some(event) = self.conn.poll_for_event()? {
            if let x11rb::protocol::Event::XfixesSelectionNotify(event) = event {
                self.timestamp = event.selection_timestamp;
            }
        }
        Ok((u64::from(owner) << 32) | u64::from(self.timestamp))
    }
}
impl Drop for ClipboardWatch {
    fn drop(&mut self) {
        let _ = self.conn.destroy_window(self.window);
        let _ = self.conn.flush();
    }
}
thread_local! {static CLIPBOARD_WATCH:std::cell::RefCell<Option<ClipboardWatch>>=const{std::cell::RefCell::new(None)};}
pub(crate) fn clipboard_version() -> Result<u64, BoxError> {
    CLIPBOARD_WATCH.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.is_none() {
            *slot = Some(ClipboardWatch::new()?);
        }
        slot.as_mut().unwrap().version()
    })
}
pub(crate) fn clipboard_empty() -> Result<bool, BoxError> {
    Ok(clipboard_version()? >> 32 == 0)
}

pub(crate) fn clipboard_formats_preservable() -> Result<bool, BoxError> {
    CLIPBOARD_WATCH.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.is_none() {
            *slot = Some(ClipboardWatch::new()?);
        }
        let watch = slot.as_mut().unwrap();
        if watch
            .conn
            .get_selection_owner(watch.selection)?
            .reply()?
            .owner
            == NONE
        {
            return Ok(true);
        }
        let targets = watch.conn.intern_atom(false, b"TARGETS")?.reply()?.atom;
        let property = watch
            .conn
            .intern_atom(false, b"PLUME_CLIPBOARD_TARGETS")?
            .reply()?
            .atom;
        watch
            .conn
            .convert_selection(
                watch.window,
                watch.selection,
                targets,
                property,
                CURRENT_TIME,
            )?
            .check()?;
        watch.conn.flush()?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(200);
        loop {
            while let Some(event) = watch.conn.poll_for_event()? {
                match event {
                    x11rb::protocol::Event::XfixesSelectionNotify(e) => {
                        watch.timestamp = e.selection_timestamp
                    }
                    x11rb::protocol::Event::SelectionNotify(e)
                        if e.requestor == watch.window && e.target == targets =>
                    {
                        if e.property == NONE {
                            return Ok(false);
                        }
                        let reply = watch
                            .conn
                            .get_property(true, watch.window, property, AtomEnum::ATOM, 0, 1024)?
                            .reply()?;
                        let Some(atoms) = reply.value32() else {
                            return Ok(false);
                        };
                        for atom in atoms {
                            let name = watch.conn.get_atom_name(atom)?.reply()?.name;
                            let name = std::str::from_utf8(&name)?;
                            if !matches!(
                                name,
                                "TARGETS"
                                    | "TIMESTAMP"
                                    | "MULTIPLE"
                                    | "SAVE_TARGETS"
                                    | "INCR"
                                    | "UTF8_STRING"
                                    | "TEXT"
                                    | "STRING"
                                    | "COMPOUND_TEXT"
                                    | "text/plain"
                                    | "text/plain;charset=utf-8"
                                    | "text/plain;charset=UTF-8"
                                    | "text/html"
                                    | "image/png"
                                    | "image/bmp"
                                    | "image/jpeg"
                                    | "image/tiff"
                            ) {
                                return Ok(false);
                            }
                        }
                        return Ok(true);
                    }
                    _ => {}
                }
            }
            if std::time::Instant::now() >= deadline {
                return Err("clipboard owner did not supply supported formats".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    })
}
