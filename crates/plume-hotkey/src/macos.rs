#![allow(dead_code)]

use crate::chord::{Edge, Mods};
use crate::shortcut::{KeyId, Trigger};
#[cfg(test)]
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

/// The keycode of a trigger key, for reading its physical state.
pub(crate) fn macos_keycode(id: KeyId) -> Option<u16> {
    (0u16..0x80).find(|&keycode| macos_key_id(keycode) == Some(id))
}

#[derive(Default)]
struct BindingFilter {
    set: crate::binding_set::BindingSet,
    /// Every tap callback, real key or not. See `BindingSet::reconcile`.
    edges: u64,
}
impl BindingFilter {
    fn register(&mut self, bindings: &[crate::HotkeyBinding]) -> Result<(), plume_core::BoxError> {
        self.set.register(bindings)
    }
    fn push(&mut self, kind: u32, keycode: u16, flags: u64) -> (Vec<crate::BindingEvent>, bool) {
        self.edges += 1;
        match decode_macos(kind, keycode, flags) {
            Some((mods, id, edge)) => self.set.push(Some(mods), id, edge),
            None => (Vec::new(), false),
        }
    }
    /// `key_down` reads the physical state of one keycode.
    fn reconcile(
        &mut self,
        flags: u64,
        key_down: impl Fn(u16) -> bool,
    ) -> Vec<crate::BindingEvent> {
        let released = self
            .set
            .engaged_triggers()
            .into_iter()
            .filter(|&id| macos_keycode(id).is_some_and(|keycode| !key_down(keycode)))
            .collect();
        let physical = crate::binding_set::Physical {
            mods: macos_mods(flags),
            trusted: Mods {
                ctrl: true,
                alt: true,
                shift: true,
                super_key: true,
                fn_key: true,
            },
            released,
        };
        self.set.reconcile(self.edges, physical)
    }
}

#[cfg(target_os = "macos")]
mod backend {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc::{self, Receiver, Sender};
    use std::sync::{Arc, Mutex, MutexGuard};
    use std::thread::{self, JoinHandle};
    use std::time::Instant;

    use core_foundation::base::TCFType;
    use core_foundation::mach_port::CFMachPortRef;
    use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop};
    use core_graphics::event::{
        CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType,
        CallbackResult, EventField,
    };
    use plume_core::{BoxError, GlobalHotkey, HotkeyEvent};

    use super::BindingFilter;
    use crate::binding_set::RECONCILE_INTERVAL;
    use crate::HotkeyError;

    /// `kCGEventSourceStateHIDSystemState`: the hardware state, which a
    /// session tap dropping an event does not change.
    const HID_SYSTEM_STATE: i32 = 1;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
        fn CGEventSourceKeyState(state: i32, key: u16) -> bool;
        fn CGEventSourceFlagsState(state: i32) -> u64;
    }

    pub struct MacosHotkey {
        events: Receiver<crate::BindingEvent>,
        tx: Sender<crate::BindingEvent>,
        filter: Arc<Mutex<BindingFilter>>,
        reconciled: Instant,
        runloop: Option<CFRunLoop>,
        thread: Option<JoinHandle<()>>,
    }

    fn lock(filter: &Mutex<BindingFilter>) -> MutexGuard<'_, BindingFilter> {
        filter.lock().unwrap_or_else(|err| err.into_inner())
    }

    impl MacosHotkey {
        pub(crate) fn new() -> Result<Self, BoxError> {
            let (tx, rx) = mpsc::channel();
            let (ready_tx, ready_rx) = mpsc::channel();
            let filter = Arc::new(Mutex::new(BindingFilter::default()));
            let thread_filter = Arc::clone(&filter);
            let thread_tx = tx.clone();
            let thread = thread::spawn(move || {
                // A filtering tap holds every key event until its callback
                // returns. Inference saturating the CPU must not delay it, or
                // macOS disables the tap and typing lags system-wide.
                unsafe {
                    libc::pthread_set_qos_class_self_np(
                        libc::qos_class_t::QOS_CLASS_USER_INTERACTIVE,
                        0,
                    );
                }
                let tx = thread_tx;
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
                tx,
                filter,
                reconciled: Instant::now(),
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
        // The callback re-enables its own tap, which only exists once the
        // callback has been handed over.
        let port = Arc::new(AtomicUsize::new(0));
        let callback_port = Arc::clone(&port);
        let Ok(tap) = CGEventTap::new(
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
                    // macOS turns a tap off when a callback was late, and
                    // keeps it off until asked. Edges in the gap are lost;
                    // the next reconciliation repairs a held chord.
                    CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput => {
                        let port = callback_port.load(Ordering::Acquire);
                        if port != 0 {
                            unsafe { CGEventTapEnable(port as CFMachPortRef, true) };
                        }
                        lock(&filter).edges += 1;
                        tracing::warn!(
                            "plume-hotkey: macOS {mode} keyboard tap was disabled; re-enabled"
                        );
                        return CallbackResult::Keep;
                    }
                    _ => return CallbackResult::Keep,
                };
                let mut filter = lock(&filter);
                if event.get_integer_value_field(EventField::EVENT_SOURCE_USER_DATA) == 0x504c554d45
                {
                    filter.edges += 1;
                    return CallbackResult::Keep;
                }
                let keycode =
                    event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE) as u16;
                let flags = event.get_flags().bits();
                let (signal, swallow) = filter.push(kind, keycode, flags);
                // Sent under the lock, so reconciliation edges stay in order.
                for signal in signal {
                    let _ = tx.send(signal);
                }
                drop(filter);
                if swallow {
                    CallbackResult::Drop
                } else {
                    CallbackResult::Keep
                }
            },
        ) else {
            return false;
        };
        port.store(
            tap.mach_port().as_concrete_TypeRef() as usize,
            Ordering::Release,
        );
        let Ok(source) = tap.mach_port().create_runloop_source(0) else {
            return false;
        };
        CFRunLoop::get_current().add_source(&source, unsafe { kCFRunLoopCommonModes });
        tap.enable();
        tracing::debug!("plume-hotkey: macOS {mode} keyboard tap ready");
        let _ = ready_tx.send(Ok(CFRunLoop::get_current()));
        CFRunLoop::run_current();
        true
    }

    impl MacosHotkey {
        pub fn register_bindings(
            &mut self,
            bindings: &[crate::HotkeyBinding],
        ) -> Result<(), BoxError> {
            lock(&self.filter).register(bindings)?;
            while self.events.try_recv().is_ok() {}
            Ok(())
        }
        pub fn next_binding_event(&mut self) -> Option<crate::BindingEvent> {
            if self.reconciled.elapsed() >= RECONCILE_INTERVAL {
                self.reconciled = Instant::now();
                self.reconcile();
            }
            self.events.try_recv().ok()
        }
        /// A key release can be lost: secure input (a password field) hides
        /// key events from every tap, and a disabled tap misses them all.
        /// The HID state still knows which keys are held.
        fn reconcile(&mut self) {
            let flags = unsafe { CGEventSourceFlagsState(HID_SYSTEM_STATE) };
            let mut filter = lock(&self.filter);
            let released = filter.reconcile(flags, |keycode| unsafe {
                CGEventSourceKeyState(HID_SYSTEM_STATE, keycode)
            });
            for event in released {
                tracing::info!(action = ?event.action, "plume-hotkey: release recovered from the keyboard state");
                let _ = self.tx.send(event);
            }
        }
        pub fn set_cancel_active(&mut self, active: bool) {
            lock(&self.filter).set.cancel_active = active;
        }
        pub fn set_passthrough(&mut self, active: bool) {
            lock(&self.filter).set.passthrough = active;
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
        filter.set.passthrough = true;
        assert!(!filter.push(KIND_KEY_DOWN, 0x31, FLAG_CONTROL).1);
        filter.set.passthrough = false;
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

    /// One hold binding, reported as its edge alone.
    struct CaptureFilter(BindingFilter);
    fn capture(shortcut: &str) -> CaptureFilter {
        let mut filter = BindingFilter::default();
        filter
            .register(&[crate::HotkeyBinding {
                action: crate::HotkeyAction::Hold,
                shortcut: shortcut.into(),
            }])
            .unwrap();
        CaptureFilter(filter)
    }
    impl CaptureFilter {
        fn push(&mut self, kind: u32, keycode: u16, flags: u64) -> (Option<HotkeyEvent>, bool) {
            let (events, swallow) = self.0.push(kind, keycode, flags);
            assert!(events.len() <= 1);
            (events.first().map(|e| e.edge), swallow)
        }
    }

    #[test]
    fn a_held_chord_ends_when_the_hid_state_shows_its_trigger_up() {
        let mut filter = capture("Ctrl+Space");
        assert_eq!(
            filter.push(KIND_KEY_DOWN, 0x31, FLAG_CONTROL),
            (Some(HotkeyEvent::Pressed), true)
        );
        let held = |keycode| keycode == 0x31;
        assert!(filter.0.reconcile(FLAG_CONTROL, held).is_empty());
        assert!(filter.0.reconcile(FLAG_CONTROL, held).is_empty());
        // Secure input hid the key-up from the tap.
        assert!(filter.0.reconcile(FLAG_CONTROL, |_| false).is_empty());
        let released = filter.0.reconcile(FLAG_CONTROL, |_| false);
        assert_eq!(
            released.iter().map(|e| e.edge).collect::<Vec<_>>(),
            [HotkeyEvent::Released]
        );
        assert_eq!(
            filter.push(KIND_KEY_DOWN, 0x31, FLAG_CONTROL),
            (Some(HotkeyEvent::Pressed), true)
        );
    }

    #[test]
    fn trigger_keycodes_round_trip() {
        for keycode in [0x31, 0x35, 0x22, 0x65, 0x19] {
            assert_eq!(macos_keycode(macos_key_id(keycode).unwrap()), Some(keycode));
        }
    }

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
        let mut filter = capture("Super+Shift+f");
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
        let mut filter = capture("Ctrl+Shift+F9");
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
            let mut filter = capture("Ctrl+Alt+Super");
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
