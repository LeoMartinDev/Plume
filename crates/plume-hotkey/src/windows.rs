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

/// Modifiers whose OS key state is physical. A Windows key the hook blocks
/// never reaches that state.
fn trusted_mods(blocks_super: bool) -> crate::chord::Mods {
    crate::chord::Mods {
        ctrl: true,
        alt: true,
        shift: true,
        super_key: !blocks_super,
        fn_key: false,
    }
}

#[cfg(windows)]
mod backend {
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::mpsc::{self, Receiver, Sender};
    use std::sync::{Mutex, MutexGuard, OnceLock};
    use std::thread::{self, JoinHandle};
    use std::time::Instant;

    use plume_core::{BoxError, GlobalHotkey, HotkeyEvent};
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::Security::{
        GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, TokenIntegrityLevel,
        TOKEN_MANDATORY_LABEL, TOKEN_QUERY,
    };
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::System::StationsAndDesktops::{
        CloseDesktop, OpenInputDesktop, DESKTOP_SWITCHDESKTOP,
    };
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, GetCurrentProcessId, GetCurrentThread, GetCurrentThreadId, OpenProcess,
        OpenProcessToken, SetThreadPriority, PROCESS_QUERY_LIMITED_INFORMATION,
        THREAD_PRIORITY_TIME_CRITICAL,
    };
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, GetForegroundWindow, GetMessageW,
        GetWindowThreadProcessId, PostThreadMessageW, SetWindowsHookExW, TranslateMessage,
        UnhookWindowsHookEx, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_QUIT,
    };

    use super::{decode_windows, should_block_windows_key, trusted_mods};
    use crate::binding_set::{BindingSet, Physical, RECONCILE_INTERVAL};
    use crate::chord::Mods;
    use crate::shortcut::Shortcut;
    use crate::HotkeyError;

    static HOOK_TX: OnceLock<Mutex<Option<Sender<crate::BindingEvent>>>> = OnceLock::new();
    static BINDINGS: OnceLock<Mutex<BindingSet>> = OnceLock::new();
    static CANCEL_ACTIVE: AtomicBool = AtomicBool::new(false);
    static PASSTHROUGH: AtomicBool = AtomicBool::new(false);
    static CANCEL_SUPER: AtomicBool = AtomicBool::new(false);
    static BLOCK_SUPER: AtomicBool = AtomicBool::new(false);
    static SUPER_DOWN: AtomicBool = AtomicBool::new(false);
    /// Every hook callback, injected or not. See `BindingSet::reconcile`.
    static EDGES: AtomicU64 = AtomicU64::new(0);

    fn hook_tx() -> &'static Mutex<Option<Sender<crate::BindingEvent>>> {
        HOOK_TX.get_or_init(|| Mutex::new(None))
    }

    fn bindings() -> MutexGuard<'static, BindingSet> {
        let set = BINDINGS.get_or_init(|| Mutex::new(BindingSet::default()));
        let mut set = set.lock().unwrap_or_else(|err| err.into_inner());
        set.cancel_active = CANCEL_ACTIVE.load(Ordering::Relaxed);
        set.passthrough = PASSTHROUGH.load(Ordering::Relaxed);
        set
    }

    /// Callers hold the bindings lock, so the hook and reconciliation never
    /// reorder the edges of one binding.
    fn send(events: Vec<crate::BindingEvent>) {
        if events.is_empty() {
            return;
        }
        if let Ok(guard) = hook_tx().lock() {
            if let Some(tx) = guard.as_ref() {
                for event in events {
                    let _ = tx.send(event);
                }
            }
        }
    }

    pub struct WindowsHotkey {
        events: Receiver<crate::BindingEvent>,
        thread_id: u32,
        hook_thread: Option<JoinHandle<()>>,
        reconciled: Instant,
        sight: Sight,
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
                // Every key on the desktop waits for this thread. Windows
                // silently removes a hook that answers late, and inference
                // can saturate every core.
                SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_TIME_CRITICAL);
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
                    reconciled: Instant::now(),
                    sight: Sight::new(),
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
            EDGES.fetch_add(1, Ordering::Relaxed);
            let info = &*(l_param as *const KBDLLHOOKSTRUCT);
            // Text insertion uses SendInput. Its synthetic Ctrl/trigger
            // events must never start or end a physical dictation hold.
            if super::is_injected(info.flags) {
                return CallNextHookEx(std::ptr::null_mut(), n_code, w_param, l_param);
            }
            let mut consumed = false;
            if let Some((id, edge)) = decode_windows(info.vkCode, info.flags, w_param as u32) {
                let mut set = bindings();
                let (events, swallow) = set.push(None, id, edge);
                consumed = swallow;
                send(events);
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
                blocks_super(),
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

    fn blocks_super() -> bool {
        BLOCK_SUPER.load(Ordering::Relaxed)
            || CANCEL_ACTIVE.load(Ordering::Relaxed) && CANCEL_SUPER.load(Ordering::Relaxed)
    }

    /// Modifier state as Windows reports it. The hook never hides a
    /// modifier, so this is the physical state, except for a Windows key the
    /// hook is blocking.
    fn async_mods() -> Mods {
        let down = |vk: u32| unsafe { GetAsyncKeyState(vk as i32) } as u16 & 0x8000 != 0;
        Mods {
            ctrl: down(super::VK_CONTROL),
            alt: down(super::VK_MENU),
            shift: down(super::VK_SHIFT),
            super_key: down(super::VK_LWIN) || down(super::VK_RWIN),
            fn_key: false,
        }
    }

    /// Whether the hook can observe the keyboard. User Interface Privilege
    /// Isolation hides every key from it while a window of a more privileged
    /// process (Task Manager, an elevated terminal) is in front, and the
    /// secure desktop (UAC prompt, lock screen) hides them all.
    struct Sight {
        own: Option<u32>,
        window: usize,
        pid: u32,
        blind: bool,
    }

    impl Sight {
        fn new() -> Self {
            Sight {
                own: unsafe { token_integrity(GetCurrentProcess()) },
                window: 0,
                pid: 0,
                blind: false,
            }
        }

        fn blind(&mut self) -> bool {
            unsafe {
                let desktop = OpenInputDesktop(0, 0, DESKTOP_SWITCHDESKTOP);
                if desktop.is_null() {
                    self.blind = true;
                    // Re-read the foreground once the desktop is back.
                    self.window = 0;
                    return true;
                }
                CloseDesktop(desktop);
                let window = GetForegroundWindow();
                if window.is_null() {
                    // Focus is moving between windows.
                    return self.blind;
                }
                let mut pid = 0;
                GetWindowThreadProcessId(window, &mut pid);
                if window as usize != self.window || pid != self.pid {
                    self.window = window as usize;
                    self.pid = pid;
                    self.blind = pid != 0
                        && pid != GetCurrentProcessId()
                        && matches!(
                            (self.own, process_integrity(pid)),
                            (Some(own), Some(theirs)) if theirs > own
                        );
                }
                self.blind
            }
        }
    }

    /// An unreadable process (a service, a protected process) counts as
    /// visible: a false "blind" would end every hold in that window.
    unsafe fn process_integrity(pid: u32) -> Option<u32> {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }
        let level = token_integrity(process);
        CloseHandle(process);
        level
    }

    unsafe fn token_integrity(process: HANDLE) -> Option<u32> {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(process, TOKEN_QUERY, &mut token) == 0 {
            return None;
        }
        // TOKEN_MANDATORY_LABEL followed by its SID, u64-aligned.
        let mut buffer = [0u64; 16];
        let mut written = 0u32;
        let read = GetTokenInformation(
            token,
            TokenIntegrityLevel,
            buffer.as_mut_ptr().cast(),
            std::mem::size_of_val(&buffer) as u32,
            &mut written,
        );
        CloseHandle(token);
        if read == 0 {
            return None;
        }
        let sid = (*buffer.as_ptr().cast::<TOKEN_MANDATORY_LABEL>()).Label.Sid;
        let count = *GetSidSubAuthorityCount(sid);
        if count == 0 {
            return None;
        }
        Some(*GetSidSubAuthority(sid, u32::from(count) - 1))
    }

    impl WindowsHotkey {
        pub fn set_passthrough(&mut self, active: bool) {
            PASSTHROUGH.store(active, Ordering::Relaxed);
        }
        pub fn register_bindings(
            &mut self,
            bindings: &[crate::HotkeyBinding],
        ) -> Result<(), BoxError> {
            let mut set = BindingSet::default();
            set.register(bindings)?;
            let mut block_super = false;
            let mut cancel_super = false;
            for b in bindings {
                let key = Shortcut::parse(&b.shortcut)?;
                if b.action == crate::HotkeyAction::Cancel {
                    cancel_super |= key.super_key;
                } else {
                    block_super |= key.super_key;
                }
            }
            BLOCK_SUPER.store(block_super, Ordering::Relaxed);
            CANCEL_SUPER.store(cancel_super, Ordering::Relaxed);
            *self::bindings() = set;
            while self.events.try_recv().is_ok() {}
            Ok(())
        }
        pub fn set_cancel_active(&mut self, active: bool) {
            CANCEL_ACTIVE.store(active, Ordering::Relaxed);
        }
        pub fn next_binding_event(&mut self) -> Option<crate::BindingEvent> {
            if self.reconciled.elapsed() >= RECONCILE_INTERVAL {
                self.reconciled = Instant::now();
                self.reconcile();
            }
            self.events.try_recv().ok()
        }

        /// Repair what the hook missed. A blocked trigger never reaches the
        /// OS key state, so only blindness reveals its lost release; the
        /// modifiers are read back directly.
        fn reconcile(&mut self) {
            let was_blind = self.sight.blind;
            let blind = self.sight.blind();
            let edges = EDGES.load(Ordering::Relaxed);
            let physical = Physical {
                mods: async_mods(),
                trusted: trusted_mods(blocks_super()),
                released: Vec::new(),
            };
            let mut set = bindings();
            let events = if blind && !was_blind {
                SUPER_DOWN.store(false, Ordering::Relaxed);
                tracing::info!(
                    "plume-hotkey: the keyboard is hidden from the hook (elevated window or secure desktop)"
                );
                set.forget()
            } else {
                set.reconcile(edges, physical)
            };
            for event in &events {
                tracing::info!(action = ?event.action, "plume-hotkey: release recovered from the keyboard state");
            }
            send(events);
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
            *bindings() = BindingSet::default();
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
