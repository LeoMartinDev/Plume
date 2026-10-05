//! Native regression check: cargo run -p stt-overlay --example macos_visibility
//! Requires a macOS desktop session, but no microphone, shortcut or model.

#[cfg(not(target_os = "macos"))]
fn main() {}

#[cfg(target_os = "macos")]
fn main() {
    use std::{sync::mpsc, time::Duration};

    use cocoa::{
        appkit::{NSApplication, NSWindow, NSWindowOcclusionState},
        base::{id, nil, BOOL, NO},
    };
    use gpui::{App, Application, AsyncApp};
    use objc::{runtime::Sel, Message};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use stt_core::Dictation;
    use stt_overlay::Bubble;

    Application::new().run(|cx: &mut App| {
        let (bubbles_tx, bubbles_rx) = mpsc::channel();
        let (_levels_tx, levels_rx) = mpsc::channel();
        stt_overlay::attach(cx, bubbles_rx, levels_rx);
        let handle = cx.windows()[0];

        cx.spawn(async move |cx: &mut AsyncApp| {
            // Wait for the startup placement retries before exercising a session.
            cx.background_executor().timer(Duration::from_secs(1)).await;
            let mut dictation = Dictation::new();
            for (step, visible) in [false, true, false, true, false].into_iter().enumerate() {
                match step {
                    1 => dictation.hold(),
                    2 => dictation.release(),
                    3 => {
                        dictation = Dictation::new();
                        dictation.hold();
                    }
                    4 => dictation.cancel(),
                    _ => {}
                }
                bubbles_tx.send(Bubble::from_dictation(&dictation)).unwrap();
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                handle
                    .update(cx, |_, window, _| {
                        let RawWindowHandle::AppKit(raw) = window.window_handle().unwrap().as_raw()
                        else {
                            panic!("expected an AppKit window");
                        };
                        let view = raw.ns_view.as_ptr() as id;
                        unsafe {
                            let native: id =
                                (&*view).send_message(Sel::register("window"), ()).unwrap();
                            assert_eq!(
                                native.hidesOnDeactivate(),
                                NO,
                                "panel must survive app inactivity"
                            );
                            let app = NSApplication::sharedApplication(nil);
                            let active: BOOL =
                                (&*app).send_message(Sel::register("isActive"), ()).unwrap();
                            assert_eq!(active, NO, "overlay stole app focus");
                            assert_eq!(native.isKeyWindow(), NO, "overlay stole keyboard focus");
                            assert_eq!(
                                native.isVisible() != NO,
                                visible,
                                "wrong visibility at step {step}"
                            );
                            if visible {
                                assert!(
                                    native.occlusionState().contains(
                                        NSWindowOcclusionState::NSWindowOcclusionStateVisible
                                    ),
                                    "panel is ordered in but not displayed"
                                );
                            }
                        }
                    })
                    .unwrap();
            }
            eprintln!("macOS overlay: inactive display, release, redisplay and cancel passed");
            cx.update(|cx| cx.quit()).unwrap();
        })
        .detach();
    });
}
