//! End-to-end GPUI overlay regression, without microphone or model.
//! cargo run -p plume-overlay --example windows_visibility
#[cfg(not(windows))]
fn main() {}

#[cfg(windows)]
fn main() {
    use gpui::{prelude::*, App, AppContext, Application, AsyncApp, WindowOptions};
    use plume_core::Dictation;
    use plume_overlay::{Bubble, Feedback};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use std::{sync::mpsc, time::Duration};
    use windows_sys::Win32::{
        Foundation::RECT,
        UI::{
            HiDpi::GetDpiForWindow,
            WindowsAndMessaging::{GetClientRect, GetForegroundWindow, IsWindowVisible},
        },
    };

    struct Anchor;
    impl Render for Anchor {
        fn render(
            &mut self,
            _window: &mut gpui::Window,
            _cx: &mut gpui::Context<Self>,
        ) -> impl IntoElement {
            gpui::div().child("Plume overlay regression test")
        }
    }
    Application::new().run(|cx: &mut App| {
        // Match the product: a normal settings window exists before the
        // hidden popup. Windows may foreground a process's first HWND.
        let anchor = cx
            .open_window(WindowOptions::default(), |_, cx| cx.new(|_| Anchor))
            .unwrap();
        let (tx, rx) = mpsc::channel();
        let (_levels, levels_rx) = mpsc::channel();
        plume_overlay::attach(cx, rx, levels_rx);
        let handle = cx
            .windows()
            .into_iter()
            .find(|window| *window != anchor.into())
            .unwrap();
        cx.spawn(async move |cx: &mut AsyncApp| {
            cx.background_executor().timer(Duration::from_secs(1)).await;
            let mut dictation = Dictation::new();
            for step in 0..6 {
                let bubble = match step {
                    0 => Bubble::from_dictation(&dictation),
                    1 => {
                        dictation.hold();
                        Bubble::from_dictation(&dictation)
                    }
                    2 => {
                        dictation.release();
                        Bubble::from_dictation(&dictation)
                    }
                    3 => Bubble::feedback(
                        0,
                        Feedback::InsertionFailed {
                            text: "Regression transcript".into(),
                            copied: false,
                        },
                    ),
                    4 => {
                        dictation = Dictation::new();
                        dictation.hold();
                        Bubble::from_dictation(&dictation).with_capture_id(1)
                    }
                    _ => {
                        dictation.cancel();
                        Bubble::from_dictation(&dictation).with_capture_id(1)
                    }
                };
                tx.send(bubble).unwrap();
                cx.background_executor()
                    .timer(Duration::from_millis(650))
                    .await;
                handle
                    .update(cx, |_, window, _| {
                        let RawWindowHandle::Win32(raw) = window.window_handle().unwrap().as_raw()
                        else {
                            panic!("expected Win32");
                        };
                        unsafe {
                            let hwnd = raw.hwnd.get() as _;
                            let visible = step != 0;
                            assert_eq!(
                                IsWindowVisible(hwnd) != 0,
                                visible,
                                "visibility at step {step}"
                            );
                            assert_ne!(
                                GetForegroundWindow(),
                                hwnd,
                                "overlay stole focus at step {step}"
                            );
                            if visible {
                                let mut rect: RECT = std::mem::zeroed();
                                assert_ne!(GetClientRect(hwnd, &mut rect), 0);
                                let scale = GetDpiForWindow(hwnd) as f32 / 96.;
                                let (width, height) = if step == 3 || step == 5 {
                                    (384., 112.)
                                } else {
                                    (120., 60.)
                                };
                                assert!(
                                    ((rect.right - rect.left) as f32 - width * scale).abs() <= 2.,
                                    "wrong drawable width at step {step}: {}",
                                    rect.right - rect.left
                                );
                                assert!(
                                    ((rect.bottom - rect.top) as f32 - height * scale).abs() <= 2.,
                                    "wrong drawable height at step {step}: {}",
                                    rect.bottom - rect.top
                                );
                            }
                        }
                    })
                    .unwrap();
            }
            eprintln!(
                "Windows GPUI overlay: visibility, focus, sizing, recovery and redisplay passed"
            );
            cx.update(|cx| cx.quit()).unwrap();
        })
        .detach();
    });
}
