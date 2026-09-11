use stt_core::{BoxError, GlobalHotkey, HotkeyEvent};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConnectionExt, GrabMode, ModMask, Window};
#[cfg(test)]
use x11rb::protocol::xtest::ConnectionExt as XTestExt;
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;
#[cfg(test)]
use x11rb::NONE;

use crate::chord::{Edge, RepeatFilter};
use crate::shortcut::{Shortcut, Trigger};
use crate::HotkeyError;

const XK_SPACE: u32 = 0x0020;
const XK_ESCAPE: u32 = 0xff1b;
const XK_TAB: u32 = 0xff09;
const XK_RETURN: u32 = 0xff0d;
const XK_F1: u32 = 0xffbe;
#[cfg(test)]
const XK_CONTROL_L: u32 = 0xffe3;
#[cfg(test)]
const XK_SHIFT_L: u32 = 0xffe1;
#[cfg(test)]
const XK_ALT_L: u32 = 0xffe9;
#[cfg(test)]
const XK_SUPER_L: u32 = 0xffeb;

pub struct X11Hotkey {
    conn: RustConnection,
    root: Window,
    grabbed: Option<Grabbed>,
    filter: RepeatFilter,
}

struct Grabbed {
    trigger: u8,
    masks: Vec<ModMask>,
    #[cfg(test)]
    ctrl: Option<u8>,
    #[cfg(test)]
    alt: Option<u8>,
    #[cfg(test)]
    shift: Option<u8>,
    #[cfg(test)]
    super_key: Option<u8>,
}

impl X11Hotkey {
    pub(crate) fn connect() -> Result<Self, BoxError> {
        let (conn, screen_num) =
            x11rb::connect(None).map_err(|err| HotkeyError::Os(err.to_string()))?;
        let root = conn.setup().roots[screen_num].root;
        Ok(X11Hotkey {
            conn,
            root,
            grabbed: None,
            filter: RepeatFilter::new(),
        })
    }

    fn keycode(&self, keysym: u32) -> Result<u8, HotkeyError> {
        keycode_for(&self.conn, keysym)
            .ok_or_else(|| HotkeyError::Os(format!("no keycode for keysym {keysym:#x}")))
    }

    fn ungrab(&mut self) {
        if let Some(grabbed) = self.grabbed.take() {
            for mask in grabbed.masks {
                if let Ok(cookie) = self.conn.ungrab_key(grabbed.trigger, self.root, mask) {
                    let _ = cookie.check();
                }
            }
            let _ = self.conn.flush();
        }
        self.filter = RepeatFilter::new();
        drain(&self.conn);
    }

    #[cfg(test)]
    fn inject(&self, down: bool) -> Result<(), BoxError> {
        let grabbed = self
            .grabbed
            .as_ref()
            .ok_or_else(|| HotkeyError::Os("no shortcut is registered".to_string()))?;
        let typ = if down {
            x11rb::protocol::xproto::KEY_PRESS_EVENT
        } else {
            x11rb::protocol::xproto::KEY_RELEASE_EVENT
        };
        let mut keys = Vec::new();
        if let Some(code) = grabbed.ctrl {
            keys.push(code);
        }
        if let Some(code) = grabbed.alt {
            keys.push(code);
        }
        if let Some(code) = grabbed.shift {
            keys.push(code);
        }
        if let Some(code) = grabbed.super_key {
            keys.push(code);
        }
        if down {
            for code in &keys {
                fake_key(&self.conn, typ, *code)?;
            }
            fake_key(&self.conn, typ, grabbed.trigger)?;
        } else {
            fake_key(&self.conn, typ, grabbed.trigger)?;
            for code in keys.iter().rev() {
                fake_key(&self.conn, typ, *code)?;
            }
        }
        self.conn
            .flush()
            .map_err(|err| HotkeyError::Os(err.to_string()))?;
        Ok(())
    }
}

impl GlobalHotkey for X11Hotkey {
    fn register(&mut self, shortcut: &str) -> Result<(), BoxError> {
        let parsed = Shortcut::parse(shortcut)?;
        if parsed.fn_key || parsed.trigger == Trigger::Fn {
            return Err(HotkeyError::Unsupported("Fn is not a keysym on X11").into());
        }
        self.ungrab();

        let trigger = self.keycode(trigger_keysym(parsed.trigger)?)?;
        let mut base = ModMask::from(0u16);
        #[cfg(test)]
        let mut ctrl = None;
        #[cfg(test)]
        let mut alt = None;
        #[cfg(test)]
        let mut shift = None;
        #[cfg(test)]
        let mut super_key = None;
        if parsed.ctrl {
            base |= ModMask::CONTROL;
            #[cfg(test)]
            {
                ctrl = Some(self.keycode(XK_CONTROL_L)?);
            }
        }
        if parsed.alt {
            base |= ModMask::M1;
            #[cfg(test)]
            {
                alt = Some(self.keycode(XK_ALT_L)?);
            }
        }
        if parsed.shift {
            base |= ModMask::SHIFT;
            #[cfg(test)]
            {
                shift = Some(self.keycode(XK_SHIFT_L)?);
            }
        }
        if parsed.super_key {
            base |= ModMask::M4;
            #[cfg(test)]
            {
                super_key = Some(self.keycode(XK_SUPER_L)?);
            }
        }

        let mut masks = Vec::new();
        for mask in lock_combos(base) {
            if let Err(err) = self
                .conn
                .grab_key(
                    false,
                    self.root,
                    mask,
                    trigger,
                    GrabMode::ASYNC,
                    GrabMode::ASYNC,
                )
                .map_err(|err| HotkeyError::Os(err.to_string()))?
                .check()
            {
                for held in &masks {
                    let _ = self.conn.ungrab_key(trigger, self.root, *held);
                }
                let _ = self.conn.flush();
                return Err(HotkeyError::Os(err.to_string()).into());
            }
            masks.push(mask);
        }
        self.conn
            .flush()
            .map_err(|err| HotkeyError::Os(err.to_string()))?;
        drain(&self.conn);
        self.grabbed = Some(Grabbed {
            trigger,
            masks,
            #[cfg(test)]
            ctrl,
            #[cfg(test)]
            alt,
            #[cfg(test)]
            shift,
            #[cfg(test)]
            super_key,
        });
        Ok(())
    }

    fn next_event(&mut self) -> Option<HotkeyEvent> {
        loop {
            match self.conn.poll_for_event() {
                Ok(Some(Event::KeyPress(_))) => {
                    if let Some(event) = self.filter.push(Edge::Down) {
                        return Some(event);
                    }
                }
                Ok(Some(Event::KeyRelease(_))) => {
                    if let Some(event) = self.filter.push(Edge::Up) {
                        return Some(event);
                    }
                }
                Ok(Some(_)) => {}
                Ok(None) => return None,
                Err(_) => return None,
            }
        }
    }
}

impl Drop for X11Hotkey {
    fn drop(&mut self) {
        self.ungrab();
    }
}

fn drain(conn: &RustConnection) {
    while let Ok(Some(_)) = conn.poll_for_event() {}
}

#[cfg(test)]
fn fake_key(conn: &RustConnection, typ: u8, detail: u8) -> Result<(), HotkeyError> {
    conn.xtest_fake_input(typ, detail, 0, NONE, 0, 0, 0)
        .map_err(|err| HotkeyError::Os(err.to_string()))?;
    Ok(())
}

// CapsLock, NumLock, and ScrollLock sit in extra modifier bits, so a grab of
// Control|Shift alone misses the same physical chord when a lock is on.
fn lock_combos(base: ModMask) -> [ModMask; 8] {
    let extras = [ModMask::LOCK, ModMask::M2, ModMask::M5];
    let mut out = [base; 8];
    for (index, slot) in out.iter_mut().enumerate() {
        let mut mask = base;
        if index & 1 != 0 {
            mask |= extras[0];
        }
        if index & 2 != 0 {
            mask |= extras[1];
        }
        if index & 4 != 0 {
            mask |= extras[2];
        }
        *slot = mask;
    }
    out
}

fn trigger_keysym(trigger: Trigger) -> Result<u32, HotkeyError> {
    Ok(match trigger {
        Trigger::Space => XK_SPACE,
        Trigger::Escape => XK_ESCAPE,
        Trigger::Tab => XK_TAB,
        Trigger::Return => XK_RETURN,
        Trigger::Char(ch) if ch.is_ascii_lowercase() || ch.is_ascii_digit() => u32::from(ch),
        Trigger::F(n) if (1..=24).contains(&n) => XK_F1 + u32::from(n) - 1,
        Trigger::Fn => {
            return Err(HotkeyError::Unsupported("Fn is not a keysym on X11"));
        }
        Trigger::Char(_) | Trigger::F(_) => {
            return Err(HotkeyError::InvalidShortcut(format!("{trigger:?}")));
        }
    })
}

fn keycode_for(conn: &RustConnection, keysym: u32) -> Option<u8> {
    let setup = conn.setup();
    let min = setup.min_keycode;
    let count = setup.max_keycode.saturating_sub(min).saturating_add(1);
    let mapping = conn.get_keyboard_mapping(min, count).ok()?.reply().ok()?;
    let per = mapping.keysyms_per_keycode as usize;
    if per == 0 {
        return None;
    }
    mapping
        .keysyms
        .chunks(per)
        .position(|chunk| chunk.contains(&keysym))
        .map(|index| min + index as u8)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn wait_event(hotkey: &mut X11Hotkey, want: HotkeyEvent, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if hotkey.next_event() == Some(want) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        false
    }

    #[test]
    fn x11_live_pressed_and_released() {
        let mut hotkey = match X11Hotkey::connect() {
            Ok(hotkey) => hotkey,
            Err(err) => {
                eprintln!("skip x11 live: {err}");
                return;
            }
        };
        hotkey
            .register("Ctrl+Shift+F9")
            .expect("grab Ctrl+Shift+F9");
        hotkey.inject(true).expect("XTEST key down");
        hotkey.inject(true).expect("XTEST auto-repeat down");
        let pressed = wait_event(&mut hotkey, HotkeyEvent::Pressed, Duration::from_secs(2));
        hotkey.inject(false).expect("XTEST key up");
        let released = wait_event(&mut hotkey, HotkeyEvent::Released, Duration::from_secs(2));
        eprintln!(
            "x11 live proof shortcut=Ctrl+Shift+F9 synthesis=XTEST pressed={pressed} released={released}"
        );
        assert!(pressed, "expected HotkeyEvent::Pressed");
        assert!(released, "expected HotkeyEvent::Released");
        assert!(
            hotkey.next_event().is_none(),
            "auto-repeat must not emit a second Pressed"
        );
    }
}
