//! Native tray integration. Closing settings keeps its state and STT alive.

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
                .timer(stt_ui::UI_REFRESH_INTERVAL)
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
pub(crate) fn show_window(_window: &gpui::Window) {}

/// A small microphone on an opaque blue tile, visible on light and dark trays.
#[cfg(any(windows, target_os = "linux", test))]
fn icon_rgba() -> Vec<u8> {
    let mut pixels = Vec::with_capacity(32 * 32 * 4);
    for y in 0..32 {
        for x in 0..32 {
            let mic = (13..=18).contains(&x) && (5..=18).contains(&y);
            let cradle = ((10..=11).contains(&x) || (20..=21).contains(&x))
                && (14..=20).contains(&y)
                || (11..=20).contains(&x) && (21..=22).contains(&y);
            let stand = (15..=16).contains(&x) && (23..=26).contains(&y)
                || (11..=20).contains(&x) && y == 27;
            pixels.extend_from_slice(if mic || cradle || stand {
                &[255, 255, 255, 255]
            } else {
                &[42, 96, 210, 255]
            });
        }
    }
    pixels
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
            NSEventType, NSMenu, NSMenuItem, NSStatusBar, NSStatusItem, NSVariableStatusItemLength,
            NSWindow,
        },
        base::{id, nil},
        foundation::{NSInteger, NSString, NSUInteger},
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
            let mut class = ClassDecl::new("STTTrayTarget", Class::get("NSObject").unwrap())
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
                    Sel::register("quitSTT:"),
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
                let bar = NSStatusBar::systemStatusBar(nil);
                let item = bar.statusItemWithLength_(NSVariableStatusItemLength);
                let _: id = (&*item).send_message(Sel::register("retain"), ()).unwrap();
                let title = NSString::alloc(nil).init_str("STT");
                let _: () = (&*item.button())
                    .send_message(Sel::register("setTitle:"), (title,))
                    .unwrap();
                let _: () = (&*title)
                    .send_message(Sel::register("release"), ())
                    .unwrap();

                let menu = NSMenu::new(nil);
                for (label, action, key) in [
                    ("Settings…", "showSettings:", ""),
                    ("Quit STT", "quitSTT:", "q"),
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
                NSApplication::sharedApplication(nil).setActivationPolicy_(
                    NSApplicationActivationPolicy::NSApplicationActivationPolicyAccessory,
                );
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

    pub(crate) fn hide_window(window: &Window) {
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
