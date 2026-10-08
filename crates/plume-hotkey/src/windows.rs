#![allow(dead_code)]

use crate::chord::Edge;
use crate::shortcut::{KeyId, Trigger};

const VK_TAB: u32 = 0x09;
const VK_RETURN: u32 = 0x0D;
const VK_SHIFT: u32 = 0x10;
const VK_CONTROL: u32 = 0x11;
const VK_MENU: u32 = 0x12;
const VK_ESCAPE: u32 = 0x1B;
const VK_SPACE: u32 = 0x20;
const VK_LWIN: u32 = 0x5B;
const VK_RWIN: u32 = 0x5C;
const VK_F1: u32 = 0x70;
const VK_F24: u32 = 0x87;
const VK_LSHIFT: u32 = 0xA0;
const VK_RSHIFT: u32 = 0xA1;
const VK_LCONTROL: u32 = 0xA2;
const VK_RCONTROL: u32 = 0xA3;
const VK_LMENU: u32 = 0xA4;
const VK_RMENU: u32 = 0xA5;

const WM_KEYDOWN: u32 = 0x0100;
const WM_KEYUP: u32 = 0x0101;
const WM_SYSKEYDOWN: u32 = 0x0104;
const WM_SYSKEYUP: u32 = 0x0105;
const LLKHF_UP: u32 = 0x80;
const LLKHF_INJECTED: u32 = 0x10;
const LLKHF_LOWER_IL_INJECTED: u32 = 0x02;

fn is_injected(flags: u32) -> bool {
    flags & (LLKHF_INJECTED | LLKHF_LOWER_IL_INJECTED) != 0
}

pub(crate) fn windows_key_id(vk: u32) -> Option<KeyId> {
    Some(match vk {
        VK_CONTROL | VK_LCONTROL | VK_RCONTROL => KeyId::Ctrl,
        VK_MENU | VK_LMENU | VK_RMENU => KeyId::Alt,
        VK_SHIFT | VK_LSHIFT | VK_RSHIFT => KeyId::Shift,
        VK_LWIN | VK_RWIN => KeyId::Super,
        VK_SPACE => KeyId::Trigger(Trigger::Space),
        VK_ESCAPE => KeyId::Trigger(Trigger::Escape),
        VK_TAB => KeyId::Trigger(Trigger::Tab),
        VK_RETURN => KeyId::Trigger(Trigger::Return),
        vk if (VK_F1..=VK_F24).contains(&vk) => KeyId::Trigger(Trigger::F((vk - VK_F1 + 1) as u8)),
        vk if (0x30..=0x39).contains(&vk) => {
            KeyId::Trigger(Trigger::Char(char::from((b'0') + (vk - 0x30) as u8)))
        }
        vk if (0x41..=0x5A).contains(&vk) => {
            KeyId::Trigger(Trigger::Char(char::from((b'a') + (vk - 0x41) as u8)))
        }
        _ => return None,
    })
}

pub(crate) fn decode_windows(vk: u32, flags: u32, msg: u32) -> Option<(KeyId, Edge)> {
    if is_injected(flags) {
        return None;
    }
    let id = windows_key_id(vk)?;
    let up = flags & LLKHF_UP != 0 || msg == WM_KEYUP || msg == WM_SYSKEYUP;
    Some((id, if up { Edge::Up } else { Edge::Down }))
}

fn should_block_windows_key(vk: u32, block_super: bool, super_down: bool) -> bool {
    block_super && (super_down || matches!(windows_key_id(vk), Some(KeyId::Super)))
}

#[cfg(windows)]
mod backend {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::{self, Receiver, Sender};
    use std::sync::{Mutex, OnceLock};
    use std::thread::{self, JoinHandle};

    use plume_core::{BoxError, GlobalHotkey, HotkeyEvent};
    use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, GetMessageW, PostThreadMessageW, SetWindowsHookExW,
        TranslateMessage, UnhookWindowsHookEx, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_QUIT,
    };

    use super::{decode_windows, should_block_windows_key};
    use crate::chord::ChordTracker;
    use crate::shortcut::Shortcut;
    use crate::HotkeyError;

    struct WinRaw {
        vk: u32,
        flags: u32,
        msg: u32,
    }

    static HOOK_TX: OnceLock<Mutex<Option<Sender<crate::BindingEvent>>>> = OnceLock::new();
    type TrackedBinding = (crate::HotkeyAction, ChordTracker, bool);
    static TRACKERS: OnceLock<Mutex<Vec<TrackedBinding>>> = OnceLock::new();
    static CANCEL_ACTIVE: AtomicBool = AtomicBool::new(false);
    static PASSTHROUGH: AtomicBool = AtomicBool::new(false);
    static CANCEL_SUPER: AtomicBool = AtomicBool::new(false);
    static BLOCK_SUPER: AtomicBool = AtomicBool::new(false);
    static SUPER_DOWN: AtomicBool = AtomicBool::new(false);

    fn hook_tx() -> &'static Mutex<Option<Sender<crate::BindingEvent>>> {
        HOOK_TX.get_or_init(|| Mutex::new(None))
    }

    pub struct WindowsHotkey {
        events: Receiver<crate::BindingEvent>,
        thread_id: u32,
        hook_thread: Option<JoinHandle<()>>,
    }

    impl WindowsHotkey {
        pub(crate) fn new() -> Result<Self, BoxError> {
            let (tx, rx) = mpsc::channel();
            {
                let mut slot = hook_tx().lock().unwrap_or_else(|err| err.into_inner());
                if slot.is_some() {
                    return Err(HotkeyError::Os(
                        "a Windows hotkey hook is already installed".to_string(),
                    )
                    .into());
                }
                *slot = Some(tx);
            }
            let (ready_tx, ready_rx) = mpsc::channel();
            // The hook callback and GetMessageW share this thread. MSG is written by GetMessageW first.
            let hook_thread = thread::spawn(move || unsafe {
                let module = GetModuleHandleW(std::ptr::null());
                let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(low_level_proc), module, 0);
                if hook.is_null() {
                    let _ = ready_tx.send(Err(HotkeyError::Os(
                        "SetWindowsHookExW(WH_KEYBOARD_LL) failed".to_string(),
                    )));
                    return;
                }
                let thread_id = GetCurrentThreadId();
                let _ = ready_tx.send(Ok(thread_id));
                let mut msg = std::mem::zeroed::<MSG>();
                while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                let _ = UnhookWindowsHookEx(hook);
            });
            match ready_rx.recv() {
                Ok(Ok(thread_id)) => Ok(WindowsHotkey {
                    events: rx,
                    thread_id,
                    hook_thread: Some(hook_thread),
                }),
                Ok(Err(err)) => Err(err.into()),
                Err(err) => Err(HotkeyError::Os(err.to_string()).into()),
            }
        }
    }

    // lParam is a KBDLLHOOKSTRUCT for nCode == HC_ACTION, as WH_KEYBOARD_LL documents.
    unsafe extern "system" fn low_level_proc(
        n_code: i32,
        w_param: WPARAM,
        l_param: LPARAM,
    ) -> LRESULT {
        if n_code == 0 && l_param != 0 {
            let info = &*(l_param as *const KBDLLHOOKSTRUCT);
            // Text insertion uses SendInput. Its synthetic Ctrl/trigger
            // events must never start or end a physical dictation hold.
            if super::is_injected(info.flags) {
                return CallNextHookEx(std::ptr::null_mut(), n_code, w_param, l_param);
            }
            let mut consumed = false;
            if let Some((id, edge)) = decode_windows(info.vkCode, info.flags, w_param as u32) {
                if let Ok(mut trackers) = TRACKERS.get_or_init(|| Mutex::new(Vec::new())).lock() {
                    for (action, tracker, owned) in trackers.iter_mut() {
                        let enabled = *action != crate::HotkeyAction::Cancel
                            || CANCEL_ACTIVE.load(Ordering::Relaxed);
                        let signal = tracker.push(id, edge);
                        let trigger =
                            matches!(id, super::KeyId::Trigger(_)) && tracker.owns_trigger(id);
                        if enabled
                            && trigger
                            && signal == Some(HotkeyEvent::Pressed)
                            && !PASSTHROUGH.load(Ordering::Relaxed)
                        {
                            *owned = true;
                        }
                        consumed |= trigger && *owned;
                        if trigger && edge == super::Edge::Up {
                            *owned = false;
                        }
                        if enabled {
                            if let Some(edge) = signal {
                                if let Ok(guard) = hook_tx().lock() {
                                    if let Some(tx) = guard.as_ref() {
                                        let _ = tx.send(crate::BindingEvent {
                                            action: *action,
                                            edge,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
            let is_super = matches!(
                super::windows_key_id(info.vkCode),
                Some(super::KeyId::Super)
            );
            let is_up = info.flags & super::LLKHF_UP != 0;
            if is_super && !is_up {
                SUPER_DOWN.store(true, Ordering::Relaxed);
            }
            let block = should_block_windows_key(
                info.vkCode,
                BLOCK_SUPER.load(Ordering::Relaxed)
                    || CANCEL_ACTIVE.load(Ordering::Relaxed)
                        && CANCEL_SUPER.load(Ordering::Relaxed),
                SUPER_DOWN.load(Ordering::Relaxed),
            );
            if is_super && is_up {
                SUPER_DOWN.store(false, Ordering::Relaxed);
            }
            if !PASSTHROUGH.load(Ordering::Relaxed) && (consumed || block) {
                return 1;
            }
        }
        CallNextHookEx(std::ptr::null_mut(), n_code, w_param, l_param)
    }

    impl WindowsHotkey {
        pub fn set_passthrough(&mut self, active: bool) {
            PASSTHROUGH.store(active, Ordering::Relaxed);
        }
        pub fn register_bindings(
            &mut self,
            bindings: &[crate::HotkeyBinding],
        ) -> Result<(), BoxError> {
            let mut trackers = Vec::new();
            let mut block_super = false;
            let mut cancel_super = false;
            for b in bindings {
                let key = Shortcut::parse(&b.shortcut)?;
                if b.action == crate::HotkeyAction::Cancel {
                    cancel_super |= key.super_key;
                } else {
                    block_super |= key.super_key;
                }
                trackers.push((b.action, ChordTracker::new(key), false));
            }
            BLOCK_SUPER.store(block_super, Ordering::Relaxed);
            CANCEL_SUPER.store(cancel_super, Ordering::Relaxed);
            *TRACKERS
                .get_or_init(|| Mutex::new(Vec::new()))
                .lock()
                .unwrap() = trackers;
            while self.events.try_recv().is_ok() {}
            Ok(())
        }
        pub fn set_cancel_active(&mut self, active: bool) {
            CANCEL_ACTIVE.store(active, Ordering::Relaxed);
        }
        pub fn next_binding_event(&mut self) -> Option<crate::BindingEvent> {
            self.events.try_recv().ok()
        }
    }
    impl GlobalHotkey for WindowsHotkey {
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

    impl Drop for WindowsHotkey {
        fn drop(&mut self) {
            PASSTHROUGH.store(false, Ordering::Relaxed);
            CANCEL_ACTIVE.store(false, Ordering::Relaxed);
            CANCEL_SUPER.store(false, Ordering::Relaxed);
            BLOCK_SUPER.store(false, Ordering::Relaxed);
            SUPER_DOWN.store(false, Ordering::Relaxed);
            {
                let mut slot = hook_tx().lock().unwrap_or_else(|err| err.into_inner());
                *slot = None;
            }
            // thread_id is the hook thread, still in GetMessageW until WM_QUIT arrives.
            unsafe {
                let _ = PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0);
            }
            if let Some(thread) = self.hook_thread.take() {
                let _ = thread.join();
            }
        }
    }
}

#[cfg(windows)]
pub(crate) use backend::WindowsHotkey;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chord::ChordTracker;
    use crate::shortcut::Shortcut;
    use plume_core::HotkeyEvent;

    #[test]
    fn injected_shortcuts_do_not_start_or_release_a_physical_hold() {
        for flags in [LLKHF_INJECTED, LLKHF_LOWER_IL_INJECTED] {
            assert_eq!(decode_windows(VK_CONTROL, flags, WM_KEYDOWN), None);
            assert_eq!(decode_windows(VK_CONTROL, flags | LLKHF_UP, WM_KEYUP), None);
            assert_eq!(decode_windows(VK_SPACE, flags, WM_KEYDOWN), None);
            assert_eq!(decode_windows(VK_SPACE, flags | LLKHF_UP, WM_KEYUP), None);
        }
    }

    #[test]
    fn decode_space_down_and_up() {
        let (id, edge) = decode_windows(VK_SPACE, 0, WM_SYSKEYDOWN).unwrap();
        assert_eq!(id, KeyId::Trigger(Trigger::Space));
        assert_eq!(edge, Edge::Down);

        let (id, edge) = decode_windows(VK_SPACE, LLKHF_UP, WM_KEYUP).unwrap();
        assert_eq!(id, KeyId::Trigger(Trigger::Space));
        assert_eq!(edge, Edge::Up);
    }

    #[test]
    fn left_and_right_control_are_ctrl() {
        assert_eq!(windows_key_id(VK_LCONTROL), Some(KeyId::Ctrl));
        assert_eq!(windows_key_id(VK_RCONTROL), Some(KeyId::Ctrl));
    }

    #[test]
    fn ctrl_space_hold_filters_auto_repeat() {
        let mut tracker = ChordTracker::new(Shortcut::parse("Ctrl+Space").unwrap());
        let (ctrl, down) = decode_windows(VK_LCONTROL, 0, WM_KEYDOWN).unwrap();
        assert_eq!(tracker.push(ctrl, down), None);
        let (space, down) = decode_windows(VK_SPACE, 0, WM_KEYDOWN).unwrap();
        assert_eq!(tracker.push(space, down), Some(HotkeyEvent::Pressed));
        let (space, down) = decode_windows(VK_SPACE, 0, WM_KEYDOWN).unwrap();
        assert_eq!(tracker.push(space, down), None);
        let (space, up) = decode_windows(VK_SPACE, LLKHF_UP, WM_KEYUP).unwrap();
        assert_eq!(tracker.push(space, up), Some(HotkeyEvent::Released));
    }

    #[test]
    fn f9_virtual_key_maps_to_f9() {
        assert_eq!(windows_key_id(0x78), Some(KeyId::Trigger(Trigger::F(9))));
    }

    #[test]
    fn windows_key_is_blocked_only_when_the_registered_chord_uses_it() {
        assert!(should_block_windows_key(VK_LWIN, true, false));
        assert!(should_block_windows_key(VK_RWIN, true, false));
        assert!(!should_block_windows_key(VK_LWIN, false, false));
        assert!(!should_block_windows_key(VK_SPACE, true, false));
        assert!(should_block_windows_key(VK_SPACE, true, true));
    }
}
