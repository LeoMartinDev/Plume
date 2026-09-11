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
    let id = windows_key_id(vk)?;
    let up = flags & LLKHF_UP != 0 || msg == WM_KEYUP || msg == WM_SYSKEYUP;
    Some((id, if up { Edge::Up } else { Edge::Down }))
}

#[cfg(windows)]
mod backend {
    use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
    use std::sync::{Mutex, OnceLock};
    use std::thread::{self, JoinHandle};

    use stt_core::{BoxError, GlobalHotkey, HotkeyEvent};
    use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, GetMessageW, PostThreadMessageW, SetWindowsHookExW,
        TranslateMessage, UnhookWindowsHookEx, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_QUIT,
    };

    use super::decode_windows;
    use crate::chord::ChordTracker;
    use crate::shortcut::Shortcut;
    use crate::HotkeyError;

    struct WinRaw {
        vk: u32,
        flags: u32,
        msg: u32,
    }

    static HOOK_TX: OnceLock<Mutex<Option<Sender<WinRaw>>>> = OnceLock::new();

    fn hook_tx() -> &'static Mutex<Option<Sender<WinRaw>>> {
        HOOK_TX.get_or_init(|| Mutex::new(None))
    }

    pub struct WindowsHotkey {
        events: Receiver<WinRaw>,
        thread_id: u32,
        hook_thread: Option<JoinHandle<()>>,
        tracker: Option<ChordTracker>,
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
                    tracker: None,
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
            if let Ok(guard) = hook_tx().lock() {
                if let Some(tx) = guard.as_ref() {
                    let _ = tx.send(WinRaw {
                        vk: info.vkCode,
                        flags: info.flags,
                        msg: w_param as u32,
                    });
                }
            }
        }
        CallNextHookEx(std::ptr::null_mut(), n_code, w_param, l_param)
    }

    impl GlobalHotkey for WindowsHotkey {
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
                        if let Some((id, edge)) = decode_windows(raw.vk, raw.flags, raw.msg) {
                            if let Some(event) = tracker.push(id, edge) {
                                return Some(event);
                            }
                        }
                    }
                    Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => return None,
                }
            }
        }
    }

    impl Drop for WindowsHotkey {
        fn drop(&mut self) {
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
    use stt_core::HotkeyEvent;

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
}
