#![allow(dead_code)]

use crate::chord::{Edge, Mods};
use crate::shortcut::{KeyId, Trigger};
use plume_core::HotkeyEvent;

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
    let mut mods = macos_mods(flags);
    let id = macos_key_id(keycode)?;
    // macOS also marks function-key events with the secondary-Fn flag.
    // Match GPUI's normalization so a captured Ctrl+Shift+F9 does not
    // require a spurious Fn modifier in the global listener.
    if matches!(id, KeyId::Trigger(Trigger::F(_))) {
        mods.fn_key = false;
    }
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

#[derive(Default)]
struct CaptureFilter {
    tracker: Option<crate::chord::ChordTracker>,
    swallowed: Option<KeyId>,
}

impl CaptureFilter {
    fn register(&mut self, shortcut: crate::shortcut::Shortcut) {
        self.tracker = Some(crate::chord::ChordTracker::new(shortcut));
        self.swallowed = None;
    }

    fn push(&mut self, kind: u32, keycode: u16, flags: u64) -> (Option<HotkeyEvent>, bool) {
        let Some((mods, id, edge)) = decode_macos(kind, keycode, flags) else {
            return (None, false);
        };
        let Some(tracker) = self.tracker.as_mut() else {
            return (None, false);
        };
        let was_swallowed = self.swallowed == Some(id);
        let signal = tracker.push_macos(mods, id, edge);
        // Modifier flags must reach applications; dropping one flags-changed
        // event leaves their keyboard state out of sync with the physical keys.
        let can_swallow = matches!(id, KeyId::Trigger(_));
        if signal == Some(HotkeyEvent::Pressed) && can_swallow {
            self.swallowed = Some(id);
        } else if was_swallowed && edge == Edge::Up {
            self.swallowed = None;
        }
        (
            signal,
            was_swallowed || (signal == Some(HotkeyEvent::Pressed) && can_swallow),
        )
    }
}

#[derive(Default)]
struct BindingFilter {
    filters: Vec<(crate::HotkeyAction, CaptureFilter)>,
    cancel_active: bool,
    passthrough: bool,
}
impl BindingFilter {
    fn register(&mut self, bindings: &[crate::HotkeyBinding]) -> Result<(), plume_core::BoxError> {
        let mut filters = Vec::new();
        for b in bindings {
            let mut f = CaptureFilter::default();
            f.register(crate::shortcut::Shortcut::parse(&b.shortcut)?);
            filters.push((b.action, f));
        }
        self.filters = filters;
        Ok(())
    }
    fn push(&mut self, kind: u32, keycode: u16, flags: u64) -> (Vec<crate::BindingEvent>, bool) {
        let mut result = Vec::new();
        let mut swallow = false;
        for (action, filter) in &mut self.filters {
            let (event, owns) = filter.push(kind, keycode, flags);
            if self.passthrough {
                filter.swallowed = None;
            }
            let enabled = *action != crate::HotkeyAction::Cancel || self.cancel_active;
            swallow |= owns && enabled;
            if let Some(edge) = event {
                if enabled {
                    result.push(crate::BindingEvent {
                        action: *action,
                        edge,
                    });
                }
            }
        }
        (result, swallow && !self.passthrough)
    }
}

#[cfg(target_os = "macos")]
mod backend {
    use std::sync::mpsc::{self, Receiver, Sender};
    use std::sync::{Arc, Mutex};
    use std::thread::{self, JoinHandle};

    use core_foundation::runloop::CFRunLoop;
    use core_graphics::event::{
        CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType,
        CallbackResult, EventField,
    };
    use plume_core::{BoxError, GlobalHotkey, HotkeyEvent};

    use super::BindingFilter;
    use crate::HotkeyError;

    pub struct MacosHotkey {
        events: Receiver<crate::BindingEvent>,
        filter: Arc<Mutex<BindingFilter>>,
        runloop: Option<CFRunLoop>,
        thread: Option<JoinHandle<()>>,
    }

    impl MacosHotkey {
        pub(crate) fn new() -> Result<Self, BoxError> {
            let (tx, rx) = mpsc::channel();
            let (ready_tx, ready_rx) = mpsc::channel();
            let filter = Arc::new(Mutex::new(BindingFilter::default()));
            let thread_filter = Arc::clone(&filter);
            let thread = thread::spawn(move || {
                // A filtering session tap owns the configured shortcut, so
                // Finder and other apps cannot act on the same key press.
                // Keep passive monitoring as a fallback when macOS denies a
                // filtering tap but permits Input Monitoring.
                for (location, options, mode) in [
                    (
                        CGEventTapLocation::Session,
                        CGEventTapOptions::Default,
                        "filtering",
                    ),
                    (
                        CGEventTapLocation::HID,
                        CGEventTapOptions::ListenOnly,
                        "passive",
                    ),
                ] {
                    if run_tap(
                        location,
                        options,
                        mode,
                        tx.clone(),
                        Arc::clone(&thread_filter),
                        &ready_tx,
                    ) {
                        return;
                    }
                }
                let _ = ready_tx.send(Err(HotkeyError::Os(
                    "CGEventTapCreate failed; grant Accessibility permission".to_string(),
                )));
            });
            let runloop = ready_rx
                .recv()
                .map_err(|err| HotkeyError::Os(err.to_string()))??;
            Ok(MacosHotkey {
                events: rx,
                filter,
                runloop: Some(runloop),
                thread: Some(thread),
            })
        }
    }

    fn run_tap(
        location: CGEventTapLocation,
        options: CGEventTapOptions,
        mode: &'static str,
        tx: Sender<crate::BindingEvent>,
        filter: Arc<Mutex<BindingFilter>>,
        ready_tx: &Sender<Result<CFRunLoop, HotkeyError>>,
    ) -> bool {
        CGEventTap::with_enabled(
            location,
            CGEventTapPlacement::HeadInsertEventTap,
            options,
            vec![
                CGEventType::KeyDown,
                CGEventType::KeyUp,
                CGEventType::FlagsChanged,
            ],
            move |_proxy, etype, event| {
                let kind = match etype {
                    CGEventType::KeyDown => super::KIND_KEY_DOWN,
                    CGEventType::KeyUp => super::KIND_KEY_UP,
                    CGEventType::FlagsChanged => super::KIND_FLAGS_CHANGED,
                    _ => return CallbackResult::Keep,
                };
                if event.get_integer_value_field(EventField::EVENT_SOURCE_USER_DATA) == 0x504c554d45
                {
                    return CallbackResult::Keep;
                }
                let keycode =
                    event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE) as u16;
                let flags = event.get_flags().bits();
                let Ok(mut filter) = filter.lock() else {
                    return CallbackResult::Keep;
                };
                let (signal, swallow) = filter.push(kind, keycode, flags);
                drop(filter);
                for signal in signal {
                    let _ = tx.send(signal);
                }
                if swallow {
                    CallbackResult::Drop
                } else {
                    CallbackResult::Keep
                }
            },
            || {
                tracing::debug!("plume-hotkey: macOS {mode} keyboard tap ready");
                let _ = ready_tx.send(Ok(CFRunLoop::get_current()));
                CFRunLoop::run_current();
            },
        )
        .is_ok()
    }

    impl MacosHotkey {
        pub fn register_bindings(
            &mut self,
            bindings: &[crate::HotkeyBinding],
        ) -> Result<(), BoxError> {
            self.filter.lock().unwrap().register(bindings)?;
            while self.events.try_recv().is_ok() {}
            Ok(())
        }
        pub fn next_binding_event(&mut self) -> Option<crate::BindingEvent> {
            self.events.try_recv().ok()
        }
        pub fn set_cancel_active(&mut self, active: bool) {
            self.filter.lock().unwrap().cancel_active = active;
        }
        pub fn set_passthrough(&mut self, active: bool) {
            self.filter.lock().unwrap().passthrough = active;
        }
    }
    impl GlobalHotkey for MacosHotkey {
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
    #[test]
    fn shortcut_editor_receives_keys_and_resume_requires_a_new_press() {
        use super::*;
        use crate::{HotkeyAction, HotkeyBinding};
        let mut filter = BindingFilter::default();
        filter
            .register(&[HotkeyBinding {
                action: HotkeyAction::Hold,
                shortcut: "Ctrl+Space".into(),
            }])
            .unwrap();
        filter.passthrough = true;
        assert!(!filter.push(KIND_KEY_DOWN, 0x31, FLAG_CONTROL).1);
        filter.passthrough = false;
        assert_eq!(
            filter.push(KIND_KEY_DOWN, 0x31, FLAG_CONTROL),
            (Vec::new(), false)
        );
        assert!(!filter.push(KIND_KEY_UP, 0x31, FLAG_CONTROL).1);
        assert!(filter.push(KIND_KEY_DOWN, 0x31, FLAG_CONTROL).1);
    }
    #[test]
    fn shared_trigger_keeps_each_binding_released_after_modifier_changes() {
        use super::*;
        use crate::{BindingEvent, HotkeyAction, HotkeyBinding};
        let mut filter = BindingFilter::default();
        filter
            .register(&[
                HotkeyBinding {
                    action: HotkeyAction::Hold,
                    shortcut: "Ctrl+Space".into(),
                },
                HotkeyBinding {
                    action: HotkeyAction::Toggle,
                    shortcut: "Ctrl+Shift+Space".into(),
                },
            ])
            .unwrap();
        let event = |action, edge| BindingEvent { action, edge };
        let combined = FLAG_CONTROL | FLAG_SHIFT;
        assert!(filter
            .push(KIND_FLAGS_CHANGED, 0x3b, FLAG_CONTROL)
            .0
            .is_empty());
        assert!(filter.push(KIND_FLAGS_CHANGED, 0x38, combined).0.is_empty());
        assert_eq!(
            filter.push(KIND_KEY_DOWN, 0x31, combined).0,
            [event(HotkeyAction::Toggle, HotkeyEvent::Pressed)]
        );
        assert!(filter
            .push(KIND_FLAGS_CHANGED, 0x38, FLAG_CONTROL)
            .0
            .is_empty());
        assert!(filter.push(KIND_KEY_DOWN, 0x31, FLAG_CONTROL).0.is_empty());
        assert_eq!(
            filter.push(KIND_KEY_UP, 0x31, FLAG_CONTROL).0,
            [event(HotkeyAction::Toggle, HotkeyEvent::Released)]
        );
        assert_eq!(
            filter.push(KIND_KEY_DOWN, 0x31, FLAG_CONTROL).0,
            [event(HotkeyAction::Hold, HotkeyEvent::Pressed)]
        );
    }
    use super::*;
    use crate::chord::ChordTracker;
    use crate::shortcut::Shortcut;
    use plume_core::HotkeyEvent;

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
    fn super_shift_f_hold_uses_command_and_shift_flags() {
        let mut tracker = ChordTracker::new(Shortcut::parse("Super+Shift+f").unwrap());
        for (kind, keycode, flags, expected) in [
            (KIND_FLAGS_CHANGED, 0x37, FLAG_COMMAND, None),
            (KIND_FLAGS_CHANGED, 0x38, FLAG_COMMAND | FLAG_SHIFT, None),
            (
                KIND_KEY_DOWN,
                0x03,
                FLAG_COMMAND | FLAG_SHIFT,
                Some(HotkeyEvent::Pressed),
            ),
            (
                KIND_KEY_UP,
                0x03,
                FLAG_COMMAND | FLAG_SHIFT,
                Some(HotkeyEvent::Released),
            ),
        ] {
            let (mods, id, edge) = decode_macos(kind, keycode, flags).unwrap();
            assert_eq!(tracker.push_macos(mods, id, edge), expected);
        }
    }

    #[test]
    fn filter_consumes_only_the_registered_chord_until_key_up() {
        let mut filter = CaptureFilter::default();
        filter.register(Shortcut::parse("Super+Shift+f").unwrap());
        let modifiers = FLAG_COMMAND | FLAG_SHIFT;
        assert_eq!(
            filter.push(KIND_FLAGS_CHANGED, 0x37, FLAG_COMMAND),
            (None, false)
        );
        assert_eq!(
            filter.push(KIND_FLAGS_CHANGED, 0x38, modifiers),
            (None, false)
        );
        assert_eq!(filter.push(KIND_KEY_DOWN, 0x00, modifiers), (None, false));
        assert_eq!(
            filter.push(KIND_KEY_DOWN, 0x03, modifiers),
            (Some(HotkeyEvent::Pressed), true)
        );
        assert_eq!(filter.push(KIND_KEY_DOWN, 0x03, modifiers), (None, true));
        assert_eq!(
            filter.push(KIND_KEY_UP, 0x03, modifiers),
            (Some(HotkeyEvent::Released), true)
        );
        assert_eq!(filter.push(KIND_KEY_UP, 0x00, modifiers), (None, false));
        assert_eq!(
            filter.push(KIND_KEY_DOWN, 0x03, modifiers),
            (Some(HotkeyEvent::Pressed), true)
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

    #[test]
    fn three_key_function_chord_ignores_the_implicit_fn_flag() {
        let mut filter = CaptureFilter::default();
        filter.register(Shortcut::parse("Ctrl+Shift+F9").unwrap());
        let flags = FLAG_CONTROL | FLAG_SHIFT | FLAG_SECONDARY_FN;
        assert_eq!(
            filter.push(KIND_KEY_DOWN, 0x65, flags),
            (Some(HotkeyEvent::Pressed), true)
        );
        assert_eq!(filter.push(KIND_KEY_DOWN, 0x65, flags), (None, true));
        assert_eq!(
            filter.push(KIND_KEY_UP, 0x65, flags),
            (Some(HotkeyEvent::Released), true)
        );
    }

    #[test]
    fn three_modifiers_use_the_snapshot_even_if_trigger_event_was_missed() {
        let mut tracker = ChordTracker::new(Shortcut::parse("Ctrl+Alt+Super").unwrap());
        let (mods, id, edge) = decode_macos(
            KIND_FLAGS_CHANGED,
            0x3A,
            FLAG_CONTROL | FLAG_ALTERNATE | FLAG_COMMAND,
        )
        .unwrap();
        assert_eq!(
            tracker.push_macos(mods, id, edge),
            Some(HotkeyEvent::Pressed)
        );
        // The next snapshot also repairs a missed Command release.
        let (mods, id, edge) = decode_macos(KIND_FLAGS_CHANGED, 0x3B, 0).unwrap();
        assert_eq!(
            tracker.push_macos(mods, id, edge),
            Some(HotkeyEvent::Released)
        );
    }

    #[test]
    fn three_modifier_chord_works_in_every_press_order_and_keeps_flags() {
        let keys = [
            (0x3B, FLAG_CONTROL),
            (0x3A, FLAG_ALTERNATE),
            (0x37, FLAG_COMMAND),
        ];
        for order in [
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ] {
            let mut filter = CaptureFilter::default();
            filter.register(Shortcut::parse("Ctrl+Alt+Super").unwrap());
            let mut flags = 0;
            for (step, index) in order.into_iter().enumerate() {
                let (keycode, flag) = keys[index];
                flags |= flag;
                assert_eq!(
                    filter.push(KIND_FLAGS_CHANGED, keycode, flags),
                    ((step == 2).then_some(HotkeyEvent::Pressed), false)
                );
            }
            assert_eq!(
                filter.push(KIND_FLAGS_CHANGED, 0x37, FLAG_CONTROL | FLAG_ALTERNATE),
                (Some(HotkeyEvent::Released), false)
            );
        }
    }
}
