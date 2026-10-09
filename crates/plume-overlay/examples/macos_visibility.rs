//! Native regression check: cargo run -p plume-overlay --example macos_visibility
//! Requires a macOS desktop session, but no microphone, shortcut or model.

#[cfg(not(target_os = "macos"))]
fn main() {}

#[cfg(target_os = "macos")]
fn main() {
    use std::{sync::mpsc, time::Duration};

    use cocoa::{
        appkit::{NSApplication, NSWindow, NSWindowOcclusionState, NSWindowStyleMask},
        base::{id, nil, BOOL, NO},
    };
    use gpui::{App, AppContext, Application, AsyncApp};
    use objc::{runtime::Sel, Message};
    use plume_core::Dictation;
    use plume_overlay::{Bubble, Feedback};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    struct BorrowErrors(std::sync::atomic::AtomicUsize);
    impl log::Log for BorrowErrors {
        fn enabled(&self, metadata: &log::Metadata) -> bool {
            metadata.level() == log::Level::Error
        }
        fn log(&self, record: &log::Record) {
            if self.enabled(record.metadata()) {
                eprintln!("{}: {}", record.file().unwrap_or("GPUI"), record.args());
                if record
                    .args()
                    .to_string()
                    .contains("RefCell already borrowed")
                {
                    self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
            }
        }
        fn flush(&self) {}
    }
    static BORROW_ERRORS: BorrowErrors = BorrowErrors(std::sync::atomic::AtomicUsize::new(0));
    log::set_logger(&BORROW_ERRORS).unwrap();
    log::set_max_level(log::LevelFilter::Error);

    struct SettingsPlaceholder;
    impl gpui::Render for SettingsPlaceholder {
        fn render(
            &mut self,
            _: &mut gpui::Window,
            _: &mut gpui::Context<Self>,
        ) -> impl gpui::IntoElement {
            gpui::div()
        }
    }

    Application::new().run(|cx: &mut App| {
        // Production creates Settings first, then attaches the overlay from
        // an async App update after the model has loaded.
        cx.open_window(
            gpui::WindowOptions {
                show: false,
                focus: false,
                ..Default::default()
            },
            |_, cx| cx.new(|_| SettingsPlaceholder),
        )
        .unwrap();
        let (bubbles_tx, bubbles_rx) = mpsc::channel();
        let (_levels_tx, levels_rx) = mpsc::channel();

        cx.spawn(async move |cx: &mut AsyncApp| {
            cx.background_executor()
                .timer(Duration::from_millis(100))
                .await;
            let handle = cx
                .update(|cx| {
                    plume_overlay::attach(cx, bubbles_rx, levels_rx);
                    *cx.windows().last().unwrap()
                })
                .unwrap();
            // Wait for the startup placement retries before exercising a session.
            cx.background_executor().timer(Duration::from_secs(1)).await;
            let mut dictation = Dictation::new();
            for (step, visible) in [false, true, true, true, false, true, false]
                .into_iter()
                .enumerate()
            {
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
                let bubble = match step {
                    5 => Bubble::feedback(
                        1,
                        Feedback::Error {
                            title: "Microphone unavailable".into(),
                            advice: "Regression check".into(),
                        },
                    ),
                    6 => Bubble::feedback(1, Feedback::Empty),
                    _ => Bubble::from_dictation(&dictation),
                };
                bubbles_tx.send(bubble).unwrap();
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
                            assert!(
                                !native
                                    .styleMask()
                                    .contains(NSWindowStyleMask::NSTitledWindowMask),
                                "overlay must be borderless"
                            );
                            let frame: cocoa::foundation::NSRect =
                                (&*view).send_message(Sel::register("frame"), ()).unwrap();
                            let viewport = window.viewport_size();
                            assert_eq!(
                                f64::from(f32::from(viewport.width)),
                                frame.size.width,
                                "GPUI missed a native width change"
                            );
                            assert_eq!(
                                f64::from(f32::from(viewport.height)),
                                frame.size.height,
                                "GPUI missed a native height change"
                            );
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
            assert_eq!(
                BORROW_ERRORS.0.load(std::sync::atomic::Ordering::Relaxed),
                0,
                "native overlay callbacks reentered a borrowed GPUI app"
            );
            eprintln!("macOS overlay: async startup, inactive display, release, redisplay, cancel and error card passed without borrow errors");
            cx.update(|cx| cx.quit()).unwrap();
        })
        .detach();
    });
}
