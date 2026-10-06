//! Native application/window identity, separate from the status-bar icon.

#[cfg(target_os = "macos")]
pub(crate) fn install(_window: &gpui::Window) {
    use cocoa::base::{id, nil};
    use objc::{
        runtime::{Class, Sel},
        Message,
    };

    // SAFETY: settings windows are created on the AppKit main thread. NSData
    // copies the embedded PNG and NSApplication retains its icon image.
    unsafe {
        let bytes = include_bytes!("../assets/brand/plume.png");
        let data: id = Class::get("NSData")
            .unwrap()
            .send_message(
                Sel::register("dataWithBytes:length:"),
                (bytes.as_ptr().cast::<std::ffi::c_void>(), bytes.len()),
            )
            .unwrap();
        let image: id = Class::get("NSImage")
            .unwrap()
            .send_message(Sel::register("alloc"), ())
            .unwrap();
        let image: id = (&*image)
            .send_message(Sel::register("initWithData:"), (data,))
            .unwrap();
        if image != nil {
            let app: id = Class::get("NSApplication")
                .unwrap()
                .send_message(Sel::register("sharedApplication"), ())
                .unwrap();
            let _: () = (&*app)
                .send_message(Sel::register("setApplicationIconImage:"), (image,))
                .unwrap();
            let _: () = (&*image)
                .send_message(Sel::register("release"), ())
                .unwrap();
        }
    }
}

// GPUI reads icon resource 1 embedded into plume.exe by build.rs.
#[cfg(windows)]
pub(crate) fn install(_window: &gpui::Window) {}

#[cfg(target_os = "linux")]
pub(crate) fn install(window: &gpui::Window) {
    if let Err(error) = install_x11(window) {
        eprintln!("plume: window icon unavailable: {error}");
    }
}

#[cfg(target_os = "linux")]
fn install_x11(window: &gpui::Window) -> Result<(), Box<dyn std::error::Error>> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use x11rb::{
        connection::Connection,
        protocol::xproto::{AtomEnum, ConnectionExt, PropMode},
        wrapper::ConnectionExt as _,
    };

    let handle = HasWindowHandle::window_handle(window).map_err(|error| error.to_string())?;
    let id = match handle.as_raw() {
        RawWindowHandle::Xcb(handle) => handle.window.get(),
        RawWindowHandle::Xlib(handle) => u32::try_from(handle.window)?,
        _ => return Ok(()),
    };
    let (connection, _) = x11rb::connect(None)?;
    let atom = connection
        .intern_atom(false, b"_NET_WM_ICON")?
        .reply()?
        .atom;
    let pixels = x11_icon_pixels();
    connection
        .change_property32(PropMode::REPLACE, id, atom, AtomEnum::CARDINAL, &pixels)?
        .check()?;
    connection.flush()?;
    Ok(())
}

#[cfg(any(target_os = "linux", test))]
fn x11_icon_pixels() -> Vec<u32> {
    let bytes = include_bytes!("../assets/brand/plume-window.rgba");
    let mut pixels = Vec::with_capacity(2 + 128 * 128);
    pixels.extend([128, 128]);
    pixels.extend(
        bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|&[r, g, b, a]| u32::from_be_bytes([a, r, g, b])),
    );
    pixels
}

#[cfg(test)]
mod tests {
    #[test]
    fn x11_icon_is_complete_opaque_argb() {
        let pixels = super::x11_icon_pixels();
        assert_eq!(&pixels[..2], &[128, 128]);
        assert_eq!(pixels.len(), 2 + 128 * 128);
        assert!(pixels[2..].iter().all(|pixel| pixel >> 24 == 255));
        assert!(pixels[2..].contains(&0xff000000));
        assert!(pixels[2..].contains(&0xffffffff));
    }
}
