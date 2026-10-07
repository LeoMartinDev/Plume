//! Native tray integration. Closing settings keeps its state and Plume alive.

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

const SHOW: u8 = 1;
const QUIT: u8 = 2;
static ACTIONS: AtomicU8 = AtomicU8::new(0);
static AVAILABLE: AtomicBool = AtomicBool::new(false);

pub(crate) fn is_available() -> bool {
    AVAILABLE.load(Ordering::Relaxed)
}

fn keep_alive<T: 'static>(cx: &mut gpui::App, tray: T) {
    AVAILABLE.store(true, Ordering::Relaxed);
    cx.spawn(async move |cx| {
        let _tray = tray;
        loop {
            cx.background_executor()
                .timer(plume_ui::UI_REFRESH_INTERVAL)
                .await;
            let actions = ACTIONS.swap(0, Ordering::Relaxed);
            if actions == 0 {
                continue;
            }
            if cx
                .update(|cx| {
                    if actions & QUIT != 0 {
                        cx.quit();
                    } else if actions & SHOW != 0 {
                        crate::settings::show_settings(cx);
                    }
                })
                .is_err()
                || actions & QUIT != 0
            {
                break;
            }
        }
        AVAILABLE.store(false, Ordering::Relaxed);
    })
    .detach();
}

#[cfg(target_os = "macos")]
pub(crate) use macos::{hide_window, install};

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub(crate) use windows::{hide_window, install, show_window};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub(crate) use linux::{hide_window, install, show_window};

#[cfg(target_os = "macos")]
pub(crate) fn show_window(_window: &gpui::Window, _cx: &gpui::App) {}

/// The Plume mark on a black tile, visible on light and dark trays.
#[cfg(any(windows, target_os = "linux", test))]
fn icon_rgba() -> Vec<u8> {
    include_bytes!("../assets/brand/plume-tray.rgba").to_vec()
}

#[cfg(test)]
mod tests {
    #[test]
    fn tray_icon_has_valid_rgba_pixels_and_visible_contrast() {
        let pixels = super::icon_rgba();
        assert_eq!(pixels.len(), 32 * 32 * 4);
        let (pixels, remainder) = pixels.as_chunks::<4>();
        assert!(remainder.is_empty());
        assert!(pixels.iter().all(|pixel| pixel[3] == 255));
        assert!(pixels.iter().any(|pixel| pixel[..3] == [255, 255, 255]));
        assert!(pixels.iter().any(|pixel| pixel[..3] != [255, 255, 255]));
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::{Ordering, ACTIONS, QUIT, SHOW};
    use std::sync::OnceLock;

    use cocoa::{
        appkit::{
            NSApplication, NSApplicationActivationPolicy, NSEventMask, NSEventModifierFlags,
            NSEventType, NSMenu, NSMenuItem, NSStatusBar, NSStatusItem, NSWindow,
        },
        base::{id, nil, NO},
        foundation::{NSInteger, NSSize, NSString, NSUInteger},
    };
    use gpui::{App, Window};
    use objc::{
        declare::ClassDecl,
        runtime::{Class, Object, Sel},
        Message,
    };
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    // AppKit invokes these on the main thread. Queue actions instead of entering
    // GPUI from a native callback while its App may already be borrowed.
    extern "C" fn show_settings(_: &Object, _: Sel, _: id) {
        ACTIONS.fetch_or(SHOW, Ordering::Relaxed);
    }

    extern "C" fn quit(_: &Object, _: Sel, _: id) {
        ACTIONS.fetch_or(QUIT, Ordering::Relaxed);
    }

    extern "C" fn click(target: &Object, _: Sel, button: id) {
        // SAFETY: AppKit invokes the button action on the main thread. The
        // target's menu is owned by Tray for the lifetime of this callback.
        unsafe {
            let app = NSApplication::sharedApplication(nil);
            let event: id = (&*app)
                .send_message(Sel::register("currentEvent"), ())
                .unwrap();
            if event.is_null() {
                return;
            }
            let kind: NSUInteger = (&*event).send_message(Sel::register("type"), ()).unwrap();
            let modifiers: NSUInteger = (&*event)
                .send_message(Sel::register("modifierFlags"), ())
                .unwrap();
            if kind == NSEventType::NSRightMouseUp as NSUInteger
                || modifiers & NSEventModifierFlags::NSControlKeyMask.bits() != 0
            {
                let menu = *target.get_ivar::<id>("menu");
                let _: () = Class::get("NSMenu")
                    .unwrap()
                    .send_message(
                        Sel::register("popUpContextMenu:withEvent:forView:"),
                        (menu, event, button),
                    )
                    .unwrap();
            } else if kind == NSEventType::NSLeftMouseUp as NSUInteger {
                ACTIONS.fetch_or(SHOW, Ordering::Relaxed);
            }
        }
    }

    fn target_class() -> &'static Class {
        static CLASS: OnceLock<&'static Class> = OnceLock::new();
        CLASS.get_or_init(|| {
            let mut class = ClassDecl::new("PlumeTrayTarget", Class::get("NSObject").unwrap())
                .expect("register tray target");
            class.add_ivar::<id>("menu");
            // SAFETY: all selectors take one Objective-C object argument and
            // match their registered C ABI. The class lives for the process.
            unsafe {
                class.add_method(
                    Sel::register("trayClicked:"),
                    click as extern "C" fn(&Object, Sel, id),
                );
                class.add_method(
                    Sel::register("showSettings:"),
                    show_settings as extern "C" fn(&Object, Sel, id),
                );
                class.add_method(
                    Sel::register("quitPlume:"),
                    quit as extern "C" fn(&Object, Sel, id),
                );
            }
            class.register()
        })
    }

    struct Tray {
        item: id,
        target: id,
        menu: id,
    }

    fn install_logo(button: id) {
        // SAFETY: called on the AppKit main thread. NSData copies the embedded
        // PNG; the button retains its NSImage after setImage:.
        unsafe {
            let bytes = include_bytes!("../assets/brand/plume-menubar.png");
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
            if image == nil {
                let title = NSString::alloc(nil).init_str("Plume");
                let _: () = (&*button)
                    .send_message(Sel::register("setTitle:"), (title,))
                    .unwrap();
                let _: () = (&*title)
                    .send_message(Sel::register("release"), ())
                    .unwrap();
                return;
            }
            let _: () = (&*image)
                .send_message(Sel::register("setSize:"), (NSSize::new(18., 18.),))
                .unwrap();
            let _: () = (&*image)
                .send_message(Sel::register("setTemplate:"), (NO,))
                .unwrap();
            // NSImageOnly: do not depend on the status button's default
            // leading-image/text layout when the button has no title.
            let _: () = (&*button)
                .send_message(Sel::register("setImagePosition:"), (1 as NSInteger,))
                .unwrap();
            let _: () = (&*button)
                .send_message(Sel::register("setImage:"), (image,))
                .unwrap();
            let _: () = (&*image)
                .send_message(Sel::register("release"), ())
                .unwrap();
        }
    }

    impl Tray {
        /// Called on GPUI's main thread after AppKit finishes launching.
        fn new() -> Self {
            // SAFETY: all AppKit objects are created and used on the main thread.
            // Retain the status item (the system bar does not own it) and the
            // action target (button and menu targets are not retained).
            unsafe {
                let target: id = target_class()
                    .send_message(Sel::register("new"), ())
                    .unwrap();
                NSApplication::sharedApplication(nil).setActivationPolicy_(
                    NSApplicationActivationPolicy::NSApplicationActivationPolicyAccessory,
                );
                let bar = NSStatusBar::systemStatusBar(nil);
                let item = bar.statusItemWithLength_(28.);
                let _: id = (&*item).send_message(Sel::register("retain"), ()).unwrap();
                install_logo(item.button());
                let position: NSInteger = (&*item.button())
                    .send_message(Sel::register("imagePosition"), ())
                    .unwrap();
                let frame: cocoa::foundation::NSRect = (&*item.button())
                    .send_message(Sel::register("frame"), ())
                    .unwrap();
                tracing::debug!(
                    position,
                    width = frame.size.width,
                    height = frame.size.height,
                    "Plume status icon layout"
                );
                let title = NSString::alloc(nil).init_str("Plume");
                let _: () = (&*item.button())
                    .send_message(Sel::register("setToolTip:"), (title,))
                    .unwrap();
                let _: () = (&*title)
                    .send_message(Sel::register("release"), ())
                    .unwrap();

                let menu = NSMenu::new(nil);
                for (label, action, key) in [
                    ("Settings…", "showSettings:", ""),
                    ("Quit Plume", "quitPlume:", "q"),
                ] {
                    let title = NSString::alloc(nil).init_str(label);
                    let key = NSString::alloc(nil).init_str(key);
                    let entry = menu.addItemWithTitle_action_keyEquivalent(
                        title,
                        Sel::register(action),
                        key,
                    );
                    entry.setTarget_(target);
                    let _: () = (&*title)
                        .send_message(Sel::register("release"), ())
                        .unwrap();
                    let _: () = (&*key).send_message(Sel::register("release"), ()).unwrap();
                }
                // Attaching a menu to the status item consumes primary clicks.
                // Instead, the button opens settings and shows the menu only
                // on secondary clicks, including the macOS Ctrl-click gesture.
                (*target).set_ivar("menu", menu);
                let button = item.button();
                let _: () = (&*button)
                    .send_message(Sel::register("setTarget:"), (target,))
                    .unwrap();
                let _: () = (&*button)
                    .send_message(
                        Sel::register("setAction:"),
                        (Sel::register("trayClicked:"),),
                    )
                    .unwrap();
                let mask = NSEventMask::NSLeftMouseUpMask | NSEventMask::NSRightMouseUpMask;
                let _: NSInteger = (&*button)
                    .send_message(Sel::register("sendActionOn:"), (mask.bits(),))
                    .unwrap();
                Self { item, target, menu }
            }
        }
    }

    impl Drop for Tray {
        fn drop(&mut self) {
            // SAFETY: the foreground task owns Tray, so cleanup stays on the
            // main thread. Remove the menu before releasing its action target.
            unsafe {
                NSStatusBar::systemStatusBar(nil).removeStatusItem_(self.item);
                let _ = (&*self.item).send_message::<_, ()>(Sel::register("release"), ());
                let _ = (&*self.menu).send_message::<_, ()>(Sel::register("release"), ());
                let _ = (&*self.target).send_message::<_, ()>(Sel::register("release"), ());
            }
        }
    }

    pub(crate) fn install(cx: &mut App) {
        super::keep_alive(cx, Tray::new());
    }

    pub(crate) fn hide_window(window: &Window, _cx: &App) {
        let Ok(handle) = HasWindowHandle::window_handle(window) else {
            return;
        };
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return;
        };
        // SAFETY: GPUI owns the NSView for the lifetime of this Window. Only
        // order out settings; hiding the whole app would also hide dictation.
        unsafe {
            let view = handle.ns_view.as_ptr() as id;
            if let Ok(native) = (&*view).send_message::<_, id>(Sel::register("window"), ()) {
                if !native.is_null() {
                    native.orderOut_(nil);
                }
            }
        }
    }
}
