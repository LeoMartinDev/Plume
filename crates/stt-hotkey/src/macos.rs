#![allow(dead_code)]

use crate::chord::{Edge, Mods};
use crate::shortcut::{KeyId, Trigger};

const FLAG_SHIFT: u64 = 0x0002_0000;
const FLAG_CONTROL: u64 = 0x0004_0000;
const FLAG_ALTERNATE: u64 = 0x0008_0000;
const FLAG_COMMAND: u64 = 0x0010_0000;
const FLAG_SECONDARY_FN: u64 = 0x0080_0000;

const KIND_KEY_DOWN: u32 = 10;
const KIND_KEY_UP: u32 = 11;
const KIND_FLAGS_CHANGED: u32 = 12;

pub(crate) fn macos_mods(flags: u64) -> Mods {
    Mods {
        ctrl: flags & FLAG_CONTROL != 0,
        alt: flags & FLAG_ALTERNATE != 0,
        shift: flags & FLAG_SHIFT != 0,
        super_key: flags & FLAG_COMMAND != 0,
        fn_key: flags & FLAG_SECONDARY_FN != 0,
    }
}

pub(crate) fn macos_key_id(keycode: u16) -> Option<KeyId> {
    Some(match keycode {
        0x3B | 0x3E => KeyId::Ctrl,
        0x3A | 0x3D => KeyId::Alt,
        0x38 | 0x3C => KeyId::Shift,
        0x37 | 0x36 => KeyId::Super,
        0x3F => KeyId::Fn,
        0x31 => KeyId::Trigger(Trigger::Space),
        0x35 => KeyId::Trigger(Trigger::Escape),
        0x30 => KeyId::Trigger(Trigger::Tab),
        0x24 => KeyId::Trigger(Trigger::Return),
        0x7A => KeyId::Trigger(Trigger::F(1)),
        0x78 => KeyId::Trigger(Trigger::F(2)),
        0x63 => KeyId::Trigger(Trigger::F(3)),
        0x76 => KeyId::Trigger(Trigger::F(4)),
        0x60 => KeyId::Trigger(Trigger::F(5)),
        0x61 => KeyId::Trigger(Trigger::F(6)),
        0x62 => KeyId::Trigger(Trigger::F(7)),
        0x64 => KeyId::Trigger(Trigger::F(8)),
        0x65 => KeyId::Trigger(Trigger::F(9)),
        0x6D => KeyId::Trigger(Trigger::F(10)),
        0x67 => KeyId::Trigger(Trigger::F(11)),
        0x6F => KeyId::Trigger(Trigger::F(12)),
        0x69 => KeyId::Trigger(Trigger::F(13)),
        0x6B => KeyId::Trigger(Trigger::F(14)),
        0x71 => KeyId::Trigger(Trigger::F(15)),
        0x6A => KeyId::Trigger(Trigger::F(16)),
        0x40 => KeyId::Trigger(Trigger::F(17)),
        0x4F => KeyId::Trigger(Trigger::F(18)),
        0x50 => KeyId::Trigger(Trigger::F(19)),
        0x5A => KeyId::Trigger(Trigger::F(20)),
        0x00 => KeyId::Trigger(Trigger::Char('a')),
        0x0B => KeyId::Trigger(Trigger::Char('b')),
        0x08 => KeyId::Trigger(Trigger::Char('c')),
        0x02 => KeyId::Trigger(Trigger::Char('d')),
        0x0E => KeyId::Trigger(Trigger::Char('e')),
        0x03 => KeyId::Trigger(Trigger::Char('f')),
        0x05 => KeyId::Trigger(Trigger::Char('g')),
        0x04 => KeyId::Trigger(Trigger::Char('h')),
        0x22 => KeyId::Trigger(Trigger::Char('i')),
        0x26 => KeyId::Trigger(Trigger::Char('j')),
        0x28 => KeyId::Trigger(Trigger::Char('k')),
        0x25 => KeyId::Trigger(Trigger::Char('l')),
        0x2E => KeyId::Trigger(Trigger::Char('m')),
        0x2D => KeyId::Trigger(Trigger::Char('n')),
        0x1F => KeyId::Trigger(Trigger::Char('o')),
        0x23 => KeyId::Trigger(Trigger::Char('p')),
        0x0C => KeyId::Trigger(Trigger::Char('q')),
        0x0F => KeyId::Trigger(Trigger::Char('r')),
        0x01 => KeyId::Trigger(Trigger::Char('s')),
        0x11 => KeyId::Trigger(Trigger::Char('t')),
        0x20 => KeyId::Trigger(Trigger::Char('u')),
        0x09 => KeyId::Trigger(Trigger::Char('v')),
        0x0D => KeyId::Trigger(Trigger::Char('w')),
        0x07 => KeyId::Trigger(Trigger::Char('x')),
        0x10 => KeyId::Trigger(Trigger::Char('y')),
        0x06 => KeyId::Trigger(Trigger::Char('z')),
        0x1D => KeyId::Trigger(Trigger::Char('0')),
        0x12 => KeyId::Trigger(Trigger::Char('1')),
        0x13 => KeyId::Trigger(Trigger::Char('2')),
        0x14 => KeyId::Trigger(Trigger::Char('3')),
        0x15 => KeyId::Trigger(Trigger::Char('4')),
        0x17 => KeyId::Trigger(Trigger::Char('5')),
        0x16 => KeyId::Trigger(Trigger::Char('6')),
        0x1A => KeyId::Trigger(Trigger::Char('7')),
        0x1C => KeyId::Trigger(Trigger::Char('8')),
        0x19 => KeyId::Trigger(Trigger::Char('9')),
        _ => return None,
    })
}

pub(crate) fn decode_macos(kind: u32, keycode: u16, flags: u64) -> Option<(Mods, KeyId, Edge)> {
    let mods = macos_mods(flags);
    let id = macos_key_id(keycode)?;
    let edge = match kind {
        KIND_KEY_DOWN => Edge::Down,
        KIND_KEY_UP => Edge::Up,
        KIND_FLAGS_CHANGED => {
            let down = match id {
                KeyId::Ctrl => mods.ctrl,
                KeyId::Alt => mods.alt,
                KeyId::Shift => mods.shift,
                KeyId::Super => mods.super_key,
                KeyId::Fn => mods.fn_key,
                KeyId::Trigger(_) => return None,
            };
            if down {
                Edge::Down
            } else {
                Edge::Up
            }
        }
        _ => return None,
    };
    Some((mods, id, edge))
}

#[cfg(target_os = "macos")]
mod backend {
    use std::sync::mpsc::{self, Receiver, TryRecvError};
    use std::thread::{self, JoinHandle};

    use core_foundation::runloop::CFRunLoop;
    use core_graphics::event::{
        CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType,
        CallbackResult, EventField,
    };
    use stt_core::{BoxError, GlobalHotkey, HotkeyEvent};

    use super::{decode_macos, KIND_FLAGS_CHANGED, KIND_KEY_DOWN, KIND_KEY_UP};
    use crate::chord::ChordTracker;
    use crate::shortcut::Shortcut;
    use crate::HotkeyError;

    struct MacRaw {
        kind: u32,
        keycode: u16,
        flags: u64,
    }

    pub struct MacosHotkey {
        events: Receiver<MacRaw>,
        runloop: Option<CFRunLoop>,
        thread: Option<JoinHandle<()>>,
        tracker: Option<ChordTracker>,
    }

    impl MacosHotkey {
        pub(crate) fn new() -> Result<Self, BoxError> {
            let (tx, rx) = mpsc::channel();
            let (ready_tx, ready_rx) = mpsc::channel();
            let thread = thread::spawn(move || {
                let result = CGEventTap::with_enabled(
                    CGEventTapLocation::HID,
                    CGEventTapPlacement::HeadInsertEventTap,
                    CGEventTapOptions::ListenOnly,
                    vec![
                        CGEventType::KeyDown,
                        CGEventType::KeyUp,
                        CGEventType::FlagsChanged,
                    ],
                    move |_proxy, etype, event| {
                        let kind = match etype {
                            CGEventType::KeyDown => KIND_KEY_DOWN,
                            CGEventType::KeyUp => KIND_KEY_UP,
                            CGEventType::FlagsChanged => KIND_FLAGS_CHANGED,
                            _ => return CallbackResult::Keep,
                        };
                        let keycode = event
                            .get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE)
                            as u16;
                        let flags = event.get_flags().bits();
                        let _ = tx.send(MacRaw {
                            kind,
                            keycode,
                            flags,
                        });
                        CallbackResult::Keep
                    },
                    || {
                        let _ = ready_tx.send(Ok(CFRunLoop::get_current()));
                        CFRunLoop::run_current();
                    },
                );
                if result.is_err() {
                    let _ = ready_tx.send(Err(HotkeyError::Os(
                        "CGEventTapCreate failed; grant Accessibility permission".to_string(),
                    )));
                }
            });
            let runloop = ready_rx
                .recv()
                .map_err(|err| HotkeyError::Os(err.to_string()))??;
            Ok(MacosHotkey {
                events: rx,
                runloop: Some(runloop),
                thread: Some(thread),
                tracker: None,
            })
        }
    }

    impl GlobalHotkey for MacosHotkey {
        fn register(&mut self, shortcut: &str) -> Result<(), BoxError> {
            self.tracker = Some(ChordTracker::new(Shortcut::parse(shortcut)?));
            while self.events.try_recv().is_ok() {}
            Ok(())
        }

        fn next_event(&mut self) -> Option<HotkeyEvent> {
            let tracker = self.tracker.as_mut()?;
            loop {
                match self.events.try_recv() {
                    Ok(raw) => {
                        if let Some((mods, id, edge)) =
                            decode_macos(raw.kind, raw.keycode, raw.flags)
                        {
                            if let Some(event) = tracker.push_macos(mods, id, edge) {
                                return Some(event);
                            }
                        }
                    }
                    Err(TryRecvError::Empty) => return None,
                    Err(TryRecvError::Disconnected) => return None,
                }
            }
        }
    }

    impl Drop for MacosHotkey {
        fn drop(&mut self) {
            if let Some(runloop) = self.runloop.take() {
                runloop.stop();
            }
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }
}

#[cfg(target_os = "macos")]
pub(crate) use backend::MacosHotkey;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chord::ChordTracker;
    use crate::shortcut::Shortcut;
    use stt_core::HotkeyEvent;

    #[test]
    fn decode_ctrl_space_keydown() {
        let (mods, id, edge) = decode_macos(KIND_KEY_DOWN, 0x31, FLAG_CONTROL).unwrap();
        assert!(mods.ctrl);
        assert_eq!(id, KeyId::Trigger(Trigger::Space));
        assert_eq!(edge, Edge::Down);
    }

    #[test]
    fn decode_flags_changed_control() {
        let (mods, id, edge) = decode_macos(KIND_FLAGS_CHANGED, 0x3B, FLAG_CONTROL).unwrap();
        assert!(mods.ctrl);
        assert_eq!(id, KeyId::Ctrl);
        assert_eq!(edge, Edge::Down);

        let (_, _, up) = decode_macos(KIND_FLAGS_CHANGED, 0x3B, 0).unwrap();
        assert_eq!(up, Edge::Up);
    }

    #[test]
    fn ctrl_space_hold_filters_auto_repeat() {
        let mut tracker = ChordTracker::new(Shortcut::parse("Ctrl+Space").unwrap());
        let (mods, id, edge) = decode_macos(KIND_KEY_DOWN, 0x31, FLAG_CONTROL).unwrap();
        assert_eq!(
            tracker.push_macos(mods, id, edge),
            Some(HotkeyEvent::Pressed)
        );
        let (mods, id, edge) = decode_macos(KIND_KEY_DOWN, 0x31, FLAG_CONTROL).unwrap();
        assert_eq!(tracker.push_macos(mods, id, edge), None);
        let (mods, id, edge) = decode_macos(KIND_KEY_UP, 0x31, FLAG_CONTROL).unwrap();
        assert_eq!(
            tracker.push_macos(mods, id, edge),
            Some(HotkeyEvent::Released)
        );
    }

    #[test]
    fn fn_flags_changed_is_the_fn_trigger() {
        let (mods, id, edge) = decode_macos(KIND_FLAGS_CHANGED, 0x3F, FLAG_SECONDARY_FN).unwrap();
        assert!(mods.fn_key);
        assert_eq!(id, KeyId::Fn);
        assert_eq!(edge, Edge::Down);
        let mut tracker = ChordTracker::new(Shortcut::parse("Fn").unwrap());
        assert_eq!(
            tracker.push_macos(mods, id, edge),
            Some(HotkeyEvent::Pressed)
        );
    }
}
