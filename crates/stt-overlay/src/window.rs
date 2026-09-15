use std::io::Write;
use std::sync::mpsc;
use std::time::Duration;

use gpui::{
    point, prelude::*, px, size, App, Application, AsyncApp, Bounds, Context, Pixels, Size,
    TitlebarOptions, Window, WindowBounds, WindowHandle, WindowKind, WindowOptions,
};
use stt_core::Dictation;
use stt_ui::{BubbleFrame, Palette, Tokens};

use crate::Bubble;

pub const WINDOW_TITLE: &str = "stt-overlay";

struct BubbleView {
    bubble: Bubble,
}

impl Render for BubbleView {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = Tokens::new(Palette::from_window(window));
        BubbleFrame::new(
            tokens,
            self.bubble.state().to_string(),
            self.bubble.text().to_string(),
        )
    }
}

fn bottom_center_bounds(window_size: Size<Pixels>, cx: &App) -> Bounds<Pixels> {
    let screen = cx
        .primary_display()
        .map(|display| display.bounds())
        .unwrap_or_else(|| Bounds {
            origin: point(px(0.), px(0.)),
            size: size(px(1280.), px(720.)),
        });
    let origin = point(
        screen.origin.x + (screen.size.width - window_size.width) / 2.,
        screen.origin.y + screen.size.height - window_size.height - px(48.),
    );
    Bounds {
        origin,
        size: window_size,
    }
}

/// Unset WAYLAND_DISPLAY and ZED_HEADLESS when DISPLAY is set.
/// Call before Application::new. Idempotent.
pub fn prepare_display() {
    let has_x11 = std::env::var_os("DISPLAY").is_some_and(|value| !value.is_empty());
    if !has_x11 {
        return;
    }
    // gpui 0.2.2 picks Wayland whenever WAYLAND_DISPLAY is set, even on WSLg
    // where the working path is X11.
    // SAFETY: this runs on the OS main thread before gpui starts. The env
    // compositor path may already have spawned Idle; that worker does not
    // read WAYLAND_DISPLAY or ZED_HEADLESS after Chord::bind.
    unsafe {
        std::env::remove_var("WAYLAND_DISPLAY");
        std::env::remove_var("ZED_HEADLESS");
    }
}

pub fn run() {
    prepare_display();
    Application::new().run(|cx: &mut App| {
        open_popup(cx);
        print_opened_line();
        cx.activate(true);
    });
}

pub fn run_with(rx: mpsc::Receiver<Bubble>) {
    prepare_display();
    Application::new().run(|cx: &mut App| {
        attach(cx, rx);
        print_opened_line();
        cx.activate(true);
    });
}

/// PopUp plus 16 ms poller inside an application that is already running.
/// Title stays WINDOW_TITLE. kind PopUp, focus false.
pub fn attach(cx: &mut App, rx: mpsc::Receiver<Bubble>) {
    let handle = open_popup(cx);
    cx.spawn(async move |cx: &mut AsyncApp| loop {
        cx.background_executor()
            .timer(Duration::from_millis(16))
            .await;
        let mut latest = None;
        loop {
            match rx.try_recv() {
                Ok(bubble) => latest = Some(bubble),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => return,
            }
        }
        if let Some(bubble) = latest {
            let _ = cx.update(|cx| {
                let _ = handle.update(cx, |view: &mut BubbleView, _window, cx| {
                    view.bubble = bubble;
                    cx.notify();
                });
            });
        }
    })
    .detach();
}

fn open_popup(cx: &mut App) -> WindowHandle<BubbleView> {
    let window_size = size(px(420.), px(56.));
    let bounds = bottom_center_bounds(window_size, cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some(WINDOW_TITLE.into()),
                ..Default::default()
            }),
            app_id: Some("stt-overlay".into()),
            kind: WindowKind::PopUp,
            focus: false,
            is_resizable: false,
            is_minimizable: false,
            ..Default::default()
        },
        |_window, cx| {
            cx.new(|_cx| BubbleView {
                bubble: Bubble::from_dictation(&Dictation::new()),
            })
        },
    )
    .expect("open overlay window")
}

fn print_opened_line() {
    eprintln!(
        "stt-overlay: window opened title={WINDOW_TITLE} display={}",
        std::env::var("DISPLAY").unwrap_or_else(|_| "<unset>".into())
    );
    let _ = std::io::stderr().flush();
}
