use gpui::{App, Window};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem},
    Icon, TrayIconBuilder,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE, SW_SHOW};

use super::{Ordering, ACTIONS, QUIT, SHOW};

pub(crate) fn install(cx: &mut App) {
    match create() {
        Ok(tray) => super::keep_alive(cx, tray),
        Err(error) => eprintln!("stt-app: tray unavailable: {error}"),
    }
}

fn create() -> Result<tray_icon::TrayIcon, Box<dyn std::error::Error>> {
    // GPUI's Win32 message loop runs on this thread and dispatches tray messages.
    let menu = Menu::new();
    let settings = MenuItem::with_id("settings", "Settings…", true, None);
    let quit = MenuItem::with_id("quit", "Quit STT", true, None);
    menu.append_items(&[&settings, &quit])?;
    MenuEvent::set_event_handler(Some(|event: MenuEvent| {
        let action = match event.id.0.as_str() {
            "settings" => SHOW,
            "quit" => QUIT,
            _ => return,
        };
        ACTIONS.fetch_or(action, Ordering::Relaxed);
    }));
    Ok(TrayIconBuilder::new()
        .with_id("stt")
        .with_tooltip("STT — Dictation")
        .with_icon(Icon::from_rgba(super::icon_rgba(), 32, 32)?)
        .with_menu(Box::new(menu))
        .build()?)
}

fn set_visible(window: &Window, visible: bool) {
    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return;
    };
    if let RawWindowHandle::Win32(handle) = handle.as_raw() {
        // SAFETY: GPUI owns this HWND and this runs on its UI thread. Hiding
        // does not destroy the window, its settings state, or the STT session.
        unsafe {
            ShowWindow(
                handle.hwnd.get() as _,
                if visible { SW_SHOW } else { SW_HIDE },
            );
        }
    }
}

pub(crate) fn hide_window(window: &Window) {
    set_visible(window, false);
}
pub(crate) fn show_window(window: &Window) {
    set_visible(window, true);
}
