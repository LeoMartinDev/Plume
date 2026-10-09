use gpui::{App, Window};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem},
    Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent,
};
use windows_sys::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{
        IsIconic, IsWindow, IsWindowVisible, SetForegroundWindow, ShowWindow, SW_HIDE, SW_RESTORE,
        SW_SHOW,
    },
};

use super::{Ordering, ACTIONS, QUIT, SHOW};

const TRAY_ID: &str = "plume";

fn opens_settings(event: &TrayIconEvent) -> bool {
    event.id().0 == TRAY_ID
        && matches!(
            event,
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            }
        )
}

pub(crate) fn install(cx: &mut App) {
    match create() {
        Ok(tray) => super::keep_alive(cx, tray),
        Err(error) => tracing::warn!("plume-app: tray unavailable: {error}"),
    }
}

fn create() -> Result<tray_icon::TrayIcon, Box<dyn std::error::Error>> {
    // GPUI's Win32 message loop runs on this thread and dispatches tray messages.
    let menu = Menu::new();
    let settings = MenuItem::with_id("settings", "Settings…", true, None);
    let quit = MenuItem::with_id("quit", "Quit Plume", true, None);
    menu.append_items(&[&settings, &quit])?;
    MenuEvent::set_event_handler(Some(|event: MenuEvent| {
        let action = match event.id.0.as_str() {
            "settings" => SHOW,
            "quit" => QUIT,
            _ => return,
        };
        ACTIONS.fetch_or(action, Ordering::Relaxed);
    }));
    TrayIconEvent::set_event_handler(Some(|event: TrayIconEvent| {
        // Queue only the release, so a click opens settings once. Native
        // callbacks must not re-enter GPUI while its App is borrowed.
        if opens_settings(&event) {
            ACTIONS.fetch_or(SHOW, Ordering::Relaxed);
        }
    }));
    Ok(TrayIconBuilder::new()
        .with_id(TRAY_ID)
        .with_tooltip("Plume — Dictation")
        .with_icon(Icon::from_rgba(super::icon_rgba(), 32, 32)?)
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .build()?)
}

fn set_visible(window: &Window, visible: bool, cx: &App) {
    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return;
    };
    if let RawWindowHandle::Win32(handle) = handle.as_raw() {
        let hwnd = handle.hwnd.get();
        // ShowWindow sends WM_SHOWWINDOW synchronously, which asks GPUI to
        // draw. Run outside App/window updates so that callback can borrow App
        // and find the settings window in its window registry again.
        cx.foreground_executor()
            .spawn(async move {
                // SAFETY: the task runs on the owning UI thread. The window
                // may have closed before dispatch, so validate its HWND first.
                unsafe { set_native_visible(hwnd as _, visible) };
            })
            .detach();
    }
}

unsafe fn set_native_visible(hwnd: HWND, visible: bool) {
    if IsWindow(hwnd) == 0 {
        return;
    }
    if !visible {
        ShowWindow(hwnd, SW_HIDE);
        return;
    }
    if IsIconic(hwnd) != 0 {
        ShowWindow(hwnd, SW_RESTORE);
    } else if IsWindowVisible(hwnd) == 0 {
        ShowWindow(hwnd, SW_SHOW);
    }
    // A tray click only raises an already visible window. In particular, do
    // not reapply GPUI's initial placement or change its maximized state.
    SetForegroundWindow(hwnd);
}

pub(crate) fn hide_window(window: &Window, cx: &App) {
    set_visible(window, false, cx);
}
pub(crate) fn show_window(window: &Window, cx: &App) {
    set_visible(window, true, cx);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_can_be_revealed_after_hiding_or_minimizing() {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, IsZoomed, SW_MAXIMIZE, SW_MINIMIZE, WS_OVERLAPPEDWINDOW,
        };

        struct TestWindow(HWND);
        impl Drop for TestWindow {
            fn drop(&mut self) {
                // SAFETY: this test owns the window on the creating thread.
                unsafe { DestroyWindow(self.0) };
            }
        }

        // SAFETY: STATIC is a predefined Win32 class. All window operations
        // run on this test thread and the guard destroys the window on exit.
        unsafe {
            let window = TestWindow(CreateWindowExW(
                0,
                windows_sys::w!("STATIC"),
                windows_sys::w!("Plume tray regression test"),
                WS_OVERLAPPEDWINDOW,
                0,
                0,
                100,
                100,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            ));
            assert!(!window.0.is_null());
            for _ in 0..2 {
                set_native_visible(window.0, true);
                assert_ne!(IsWindowVisible(window.0), 0);
                assert_eq!(IsIconic(window.0), 0);
                set_native_visible(window.0, false);
                assert_eq!(IsWindowVisible(window.0), 0);
            }
            ShowWindow(window.0, SW_MINIMIZE);
            assert_ne!(IsIconic(window.0), 0);
            set_native_visible(window.0, false);
            set_native_visible(window.0, true);
            assert_ne!(IsWindowVisible(window.0), 0);
            assert_eq!(IsIconic(window.0), 0);
            ShowWindow(window.0, SW_MAXIMIZE);
            for _ in 0..2 {
                set_native_visible(window.0, true);
                assert_ne!(IsWindowVisible(window.0), 0);
                assert_ne!(IsZoomed(window.0), 0);
            }
        }
    }

    #[test]
    fn only_primary_release_on_plume_opens_settings() {
        for id in ["plume", "other"] {
            for button in [MouseButton::Left, MouseButton::Right, MouseButton::Middle] {
                for button_state in [MouseButtonState::Down, MouseButtonState::Up] {
                    let event = TrayIconEvent::Click {
                        id: id.into(),
                        position: Default::default(),
                        rect: Default::default(),
                        button,
                        button_state,
                    };
                    assert_eq!(
                        opens_settings(&event),
                        id == "plume"
                            && button == MouseButton::Left
                            && button_state == MouseButtonState::Up
                    );
                }
            }
        }
        assert!(!opens_settings(&TrayIconEvent::Move {
            id: "plume".into(),
            position: Default::default(),
            rect: Default::default(),
        }));
    }
}
