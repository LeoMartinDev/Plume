use gpui::{App, Window};
use ksni::blocking::TrayMethods;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use x11rb::{connection::Connection, protocol::xproto::ConnectionExt};

use super::{Ordering, ACTIONS, QUIT, SHOW};

struct Tray;

impl ksni::Tray for Tray {
    // Hosts should send Activate on primary click; keep the context menu for
    // hosts that expose only menus (e.g. some AppIndicator extensions).
    const MENU_ON_ACTIVATE: bool = false;

    fn id(&self) -> String {
        "stt".into()
    }
    fn title(&self) -> String {
        "STT".into()
    }
    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        // StatusNotifierItem uses ARGB, unlike Windows' RGBA icon.
        let mut bytes = super::icon_rgba();
        for pixel in bytes.as_chunks_mut::<4>().0 {
            pixel.rotate_right(1);
        }
        vec![ksni::Icon {
            width: 32,
            height: 32,
            data: bytes,
        }]
    }
    fn activate(&mut self, _x: i32, _y: i32) {
        ACTIONS.fetch_or(SHOW, Ordering::Relaxed);
    }
    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        vec![
            ksni::menu::StandardItem {
                label: "Settings…".into(),
                activate: Box::new(|_| {
                    ACTIONS.fetch_or(SHOW, Ordering::Relaxed);
                }),
                ..Default::default()
            }
            .into(),
            ksni::menu::StandardItem {
                label: "Quit STT".into(),
                activate: Box::new(|_| {
                    ACTIONS.fetch_or(QUIT, Ordering::Relaxed);
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}

struct TrayGuard(ksni::blocking::Handle<Tray>);

impl Drop for TrayGuard {
    fn drop(&mut self) {
        self.0.shutdown().wait();
    }
}

pub(crate) fn install(cx: &mut App) {
    // Check for a working StatusNotifier host before allowing close-to-tray.
    // D-Bus runs on ksni's worker; GPUI keeps its own X11 event loop.
    match Tray.spawn() {
        Ok(handle) => super::keep_alive(cx, TrayGuard(handle)),
        Err(error) => {
            tracing::warn!("stt-app: tray unavailable; closing settings will quit: {error}")
        }
    }
}

fn set_visible(window: &Window, visible: bool) -> Result<(), Box<dyn std::error::Error>> {
    let handle = HasWindowHandle::window_handle(window).map_err(|error| error.to_string())?;
    let id = match handle.as_raw() {
        RawWindowHandle::Xcb(handle) => handle.window.get(),
        RawWindowHandle::Xlib(handle) => u32::try_from(handle.window)?,
        _ => return Err("close-to-tray requires the supported X11 backend".into()),
    };
    set_native_visible(id, visible)
}

fn set_native_visible(id: u32, visible: bool) -> Result<(), Box<dyn std::error::Error>> {
    let (connection, _) = x11rb::connect(None)?;
    if visible {
        connection.map_window(id)?.check()?;
    } else {
        connection.unmap_window(id)?.check()?;
    }
    connection.flush()?;
    Ok(())
}

pub(crate) fn hide_window(window: &Window) {
    if let Err(error) = set_visible(window, false) {
        tracing::warn!("stt-app: hide settings: {error}");
    }
}

pub(crate) fn show_window(window: &Window) {
    if let Err(error) = set_visible(window, true) {
        tracing::warn!("stt-app: show settings: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use x11rb::protocol::xproto::{CreateWindowAux, MapState, WindowClass};

    #[test]
    #[ignore = "requires an X11 DISPLAY; run with xvfb-run"]
    fn hidden_window_can_be_mapped_again() {
        let (connection, screen) = x11rb::connect(None).unwrap();
        let root = connection.setup().roots[screen].root;
        let id = connection.generate_id().unwrap();
        connection
            .create_window(
                x11rb::COPY_DEPTH_FROM_PARENT,
                id,
                root,
                0,
                0,
                100,
                100,
                0,
                WindowClass::INPUT_OUTPUT,
                0,
                &CreateWindowAux::new(),
            )
            .unwrap()
            .check()
            .unwrap();
        set_native_visible(id, true).unwrap();
        assert_eq!(
            connection
                .get_window_attributes(id)
                .unwrap()
                .reply()
                .unwrap()
                .map_state,
            MapState::VIEWABLE
        );
        set_native_visible(id, false).unwrap();
        assert_eq!(
            connection
                .get_window_attributes(id)
                .unwrap()
                .reply()
                .unwrap()
                .map_state,
            MapState::UNMAPPED
        );
        // The same native window still exists, so GPUI can keep settings and
        // session targets alive rather than creating a new window on reopen.
        set_native_visible(id, true).unwrap();
        assert_eq!(
            connection
                .get_window_attributes(id)
                .unwrap()
                .reply()
                .unwrap()
                .map_state,
            MapState::VIEWABLE
        );
    }

    #[test]
    #[ignore = "requires an isolated D-Bus session with no StatusNotifier host"]
    fn missing_tray_host_refuses_background_mode() {
        assert!(Tray.spawn().is_err());
    }
}
