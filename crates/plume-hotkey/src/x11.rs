use plume_core::{BoxError, GlobalHotkey, HotkeyEvent};
use x11rb::connection::Connection;
use x11rb::protocol::xkb::{ConnectionExt as XkbExt, PerClientFlag, ID};
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
const XK_CONTROL_L: u32 = 0xffe3;
const XK_SHIFT_L: u32 = 0xffe1;
const XK_ALT_L: u32 = 0xffe9;
const XK_SUPER_L: u32 = 0xffeb;

pub struct X11Hotkey {
    conn: RustConnection,
    root: Window,
    grabs: Vec<Grabbed>,
    bindings: Vec<crate::HotkeyBinding>,
    cancel_active: bool,
    pending: std::collections::VecDeque<crate::BindingEvent>,
    passthrough: bool,
}
struct Grabbed {
    action: crate::HotkeyAction,
    trigger: u8,
    base: ModMask,
    masks: Vec<ModMask>,
    filter: RepeatFilter,
    held: bool,
    rearm_pending: bool,
    #[cfg(test)]
    ctrl: Option<u8>,
    #[cfg(test)]
    alt: Option<u8>,
    #[cfg(test)]
    shift: Option<u8>,
    #[cfg(test)]
    super_key: Option<u8>,
}
impl Grabbed {
    fn push(&mut self, detail: u8, state: u16, edge: Edge) -> Option<crate::BindingEvent> {
        if detail != self.trigger {
            return None;
        }
        if edge == Edge::Up {
            self.rearm_pending = false;
        }
        // Every binding observes the physical trigger even if its modifiers
        // differ. Repeats after changing modifiers cannot become a new press.
        let signal = self.filter.push(edge)?;
        match signal {
            HotkeyEvent::Pressed if state == u16::from(self.base) => self.held = true,
            HotkeyEvent::Released if self.held => self.held = false,
            _ => return None,
        }
        Some(crate::BindingEvent {
            action: self.action,
            edge: signal,
        })
    }
}
impl X11Hotkey {
    pub(crate) fn connect() -> Result<Self, BoxError> {
        let (conn, screen) = x11rb::connect(None)?;
        // Traditional X11 autorepeat synthesizes release/press pairs. Request
        // physical releases so a held shortcut cannot stop or restart a take.
        if !conn.xkb_use_extension(1, 0)?.reply()?.supported {
            return Err(
                HotkeyError::Unsupported("XKB is required for physical key releases").into(),
            );
        }
        let repeat = PerClientFlag::DETECTABLE_AUTO_REPEAT;
        let flags = conn
            .xkb_per_client_flags(
                ID::USE_CORE_KBD.into(),
                repeat,
                repeat,
                0u32.into(),
                0u32.into(),
                0u32.into(),
            )?
            .reply()?;
        if u32::from(flags.value) & u32::from(repeat) == 0 {
            return Err(
                HotkeyError::Unsupported("XKB detectable autorepeat is unavailable").into(),
            );
        }
        let root = conn.setup().roots[screen].root;
        Ok(Self {
            conn,
            root,
            grabs: Vec::new(),
            bindings: Vec::new(),
            cancel_active: false,
            pending: Default::default(),
            passthrough: false,
        })
    }
    fn keycode(&self, sym: u32) -> Result<u8, BoxError> {
        keycode_for(&self.conn, sym)
            .ok_or_else(|| HotkeyError::Os(format!("no keycode for {sym}")).into())
    }
    fn add(&mut self, binding: &crate::HotkeyBinding) -> Result<(), BoxError> {
        let key = Shortcut::parse(&binding.shortcut)?;
        let trigger = self.keycode(trigger_keysym(key.trigger)?)?;
        let mut base = ModMask::from(0u16);
        if key.ctrl && key.trigger != Trigger::Ctrl {
            base |= ModMask::CONTROL;
        }
        if key.alt && key.trigger != Trigger::Alt {
            base |= ModMask::M1;
        }
        if key.shift && key.trigger != Trigger::Shift {
            base |= ModMask::SHIFT;
        }
        if key.super_key && key.trigger != Trigger::Super {
            base |= ModMask::M4;
        }
        let mut masks = Vec::new();
        for mask in lock_combos(base) {
            if let Err(error) = self
                .conn
                .grab_key(
                    false,
                    self.root,
                    mask,
                    trigger,
                    GrabMode::ASYNC,
                    GrabMode::ASYNC,
                )?
                .check()
            {
                for mask in masks {
                    self.conn.ungrab_key(trigger, self.root, mask)?.check()?;
                }
                self.conn.flush()?;
                return Err(error.into());
            }
            masks.push(mask);
        }
        self.grabs.push(Grabbed {
            action: binding.action,
            trigger,
            base,
            masks,
            filter: RepeatFilter::new(),
            held: false,
            rearm_pending: false,
            #[cfg(test)]
            ctrl: if key.ctrl && key.trigger != Trigger::Ctrl {
                Some(self.keycode(XK_CONTROL_L)?)
            } else {
                None
            },
            #[cfg(test)]
            alt: if key.alt && key.trigger != Trigger::Alt {
                Some(self.keycode(XK_ALT_L)?)
            } else {
                None
            },
            #[cfg(test)]
            shift: if key.shift && key.trigger != Trigger::Shift {
                Some(self.keycode(XK_SHIFT_L)?)
            } else {
                None
            },
            #[cfg(test)]
            super_key: if key.super_key && key.trigger != Trigger::Super {
                Some(self.keycode(XK_SUPER_L)?)
            } else {
                None
            },
        });
        self.conn.flush()?;
        Ok(())
    }
    fn ungrab(&mut self) {
        self.pending.clear();
        for g in self.grabs.drain(..) {
            for mask in g.masks {
                if let Ok(cookie) = self.conn.ungrab_key(g.trigger, self.root, mask) {
                    let _ = cookie.check();
                }
            }
        }
        let _ = self.conn.flush();
        drain(&self.conn);
    }
    pub fn register_bindings(&mut self, bindings: &[crate::HotkeyBinding]) -> Result<(), BoxError> {
        crate::validate_bindings(bindings)?;
        for b in bindings {
            let k = Shortcut::parse(&b.shortcut)?;
            if k.fn_key {
                return Err(HotkeyError::Unsupported("Fn is not a keysym on X11").into());
            }
        }
        let old = self.bindings.clone();
        self.ungrab();
        if self.passthrough {
            self.bindings = bindings.to_vec();
            return Ok(());
        }
        for binding in bindings {
            if binding.action != crate::HotkeyAction::Cancel || self.cancel_active {
                if let Err(error) = self.add(binding) {
                    self.ungrab();
                    for b in &old {
                        if b.action != crate::HotkeyAction::Cancel || self.cancel_active {
                            let _ = self.add(b);
                        }
                    }
                    return Err(error);
                }
            }
        }
        self.bindings = bindings.to_vec();
        drain(&self.conn);
        Ok(())
    }
    pub fn set_cancel_active(&mut self, active: bool) {
        if self.cancel_active == active {
            return;
        }
        self.cancel_active = active;
        if active {
            if let Some(binding) = self
                .bindings
                .iter()
                .find(|b| b.action == crate::HotkeyAction::Cancel)
                .cloned()
            {
                if let Err(error) = self.add(&binding) {
                    tracing::warn!("cancel shortcut unavailable: {error}");
                }
            }
        } else if let Some(index) = self
            .grabs
            .iter()
            .position(|g| g.action == crate::HotkeyAction::Cancel)
        {
            let g = self.grabs.remove(index);
            for mask in g.masks {
                if let Ok(cookie) = self.conn.ungrab_key(g.trigger, self.root, mask) {
                    let _ = cookie.check();
                }
            }
            let _ = self.conn.flush();
        }
    }
    pub fn next_binding_event(&mut self) -> Option<crate::BindingEvent> {
        if self.grabs.iter().any(|g| g.rearm_pending) {
            if let Ok(cookie) = self.conn.query_keymap() {
                if let Ok(keys) = cookie.reply() {
                    for g in &mut self.grabs {
                        if g.rearm_pending
                            && keys.keys[usize::from(g.trigger / 8)] & (1 << (g.trigger % 8)) == 0
                        {
                            g.filter.push(Edge::Up);
                            g.rearm_pending = false;
                        }
                    }
                }
            }
        }
        if let Some(event) = self.pending.pop_front() {
            return Some(event);
        }
        while let Ok(Some(event)) = self.conn.poll_for_event() {
            let (detail, state, edge) = match event {
                Event::KeyPress(e) => (e.detail, e.state, Edge::Down),
                Event::KeyRelease(e) => (e.detail, e.state, Edge::Up),
                _ => continue,
            };
            let state = u16::from(state)
                & !(u16::from(ModMask::LOCK) | u16::from(ModMask::M2) | u16::from(ModMask::M5));
            for g in &mut self.grabs {
                if let Some(event) = g.push(detail, state, edge) {
                    self.pending.push_back(event);
                }
            }
            if let Some(event) = self.pending.pop_front() {
                return Some(event);
            }
        }
        None
    }
    pub fn set_passthrough(&mut self, active: bool) -> Result<(), BoxError> {
        if active == self.passthrough {
            return Ok(());
        }
        self.passthrough = active;
        if active {
            self.ungrab();
        } else {
            let bindings = self.bindings.clone();
            self.register_bindings(&bindings)?;
            let keys = self.conn.query_keymap()?.reply()?;
            for g in &mut self.grabs {
                if keys.keys[usize::from(g.trigger / 8)] & (1 << (g.trigger % 8)) != 0 {
                    g.filter.push(Edge::Down);
                    g.rearm_pending = true;
                }
            }
        }
        Ok(())
    }
    #[cfg(test)]
    fn inject(&self, down: bool) -> Result<(), BoxError> {
        let g = self.grabs.first().ok_or("no shortcut")?;
        let typ = if down {
            x11rb::protocol::xproto::KEY_PRESS_EVENT
        } else {
            x11rb::protocol::xproto::KEY_RELEASE_EVENT
        };
        let keys: Vec<_> = [g.ctrl, g.alt, g.shift, g.super_key]
            .into_iter()
            .flatten()
            .collect();
        if down {
            for k in &keys {
                fake_key(&self.conn, typ, *k)?;
            }
            fake_key(&self.conn, typ, g.trigger)?;
        } else {
            fake_key(&self.conn, typ, g.trigger)?;
            for k in keys.iter().rev() {
                fake_key(&self.conn, typ, *k)?;
            }
        }
        self.conn.flush()?;
        Ok(())
    }
}
impl GlobalHotkey for X11Hotkey {
    fn register(&mut self, shortcut: &str) -> Result<(), BoxError> {
        self.register_bindings(&[crate::HotkeyBinding {
            action: crate::HotkeyAction::Hold,
            shortcut: shortcut.into(),
        }])
    }
    fn next_event(&mut self) -> Option<HotkeyEvent> {
        self.next_binding_event().map(|e| e.edge)
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
        Trigger::Ctrl => XK_CONTROL_L,
        Trigger::Alt => XK_ALT_L,
        Trigger::Shift => XK_SHIFT_L,
        Trigger::Super => XK_SUPER_L,
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

    #[test]
    fn shared_trigger_does_not_rearm_when_modifiers_change_during_repeat() {
        let make = |action, base| Grabbed {
            action,
            trigger: 65,
            base,
            masks: Vec::new(),
            filter: RepeatFilter::new(),
            held: false,
            rearm_pending: false,
            ctrl: None,
            alt: None,
            shift: None,
            super_key: None,
        };
        let mut hold = make(crate::HotkeyAction::Hold, ModMask::CONTROL);
        let mut toggle = make(
            crate::HotkeyAction::Toggle,
            ModMask::CONTROL | ModMask::SHIFT,
        );
        let combined = u16::from(ModMask::CONTROL | ModMask::SHIFT);
        let control = u16::from(ModMask::CONTROL);
        assert!(hold.push(65, combined, Edge::Down).is_none());
        assert_eq!(
            toggle.push(65, combined, Edge::Down).unwrap().edge,
            HotkeyEvent::Pressed
        );
        assert!(hold.push(65, control, Edge::Down).is_none());
        assert!(toggle.push(65, control, Edge::Down).is_none());
        assert!(hold.push(65, control, Edge::Up).is_none());
        assert_eq!(
            toggle.push(65, control, Edge::Up).unwrap().edge,
            HotkeyEvent::Released
        );
        assert_eq!(
            hold.push(65, control, Edge::Down).unwrap().edge,
            HotkeyEvent::Pressed
        );
    }

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
