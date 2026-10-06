use std::sync::mpsc;
use std::time::{Duration, Instant};

use gpui::{
    div, hsla, point, prelude::*, px, size, App, Application, AsyncApp, Bounds, BoxShadow, Context,
    Pixels, Size, TitlebarOptions, Window, WindowBackgroundAppearance, WindowBounds,
    WindowDecorations, WindowHandle, WindowKind, WindowOptions,
};
use plume_core::{Dictation, SessionState};
use plume_ui::BubbleFrame;

use crate::frame::hide_server_frame;
use crate::Bubble;

pub const WINDOW_TITLE: &str = "Plume — Dictation";

const FRAME_RECHECK_DELAYS: [Duration; 3] = [
    Duration::from_millis(40),
    Duration::from_millis(150),
    Duration::from_millis(400),
];

const BUBBLE_WIDTH: f32 = 96.;
const BUBBLE_HEIGHT: f32 = 36.;
const SHADOW_MARGIN: f32 = 12.;
const BUBBLE_BOTTOM_GAP: f32 = 48.;

struct BubbleView {
    bubble: Bubble,
    bars: [f32; 8],
    target_level: f32,
    last_animation_frame: Instant,
}

impl Render for BubbleView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let bars = if speaking(self.bubble.state()) {
            self.bars
        } else {
            [0.0; 8]
        };
        div()
            .flex()
            .items_center()
            .justify_center()
            .size_full()
            .child(
                div()
                    .w(px(BUBBLE_WIDTH))
                    .h(px(BUBBLE_HEIGHT))
                    .rounded_full()
                    .shadow(vec![
                        BoxShadow {
                            color: hsla(0., 0., 0., 0.28),
                            offset: point(px(0.), px(1.)),
                            blur_radius: px(4.),
                            spread_radius: px(0.),
                        },
                        BoxShadow {
                            color: hsla(0., 0., 0., 0.18),
                            offset: point(px(0.), px(2.)),
                            blur_radius: px(8.),
                            spread_radius: px(1.),
                        },
                    ])
                    .child(BubbleFrame::new(0.0).bars(bars)),
            )
    }
}

fn speaking(state: SessionState) -> bool {
    matches!(state, SessionState::Recording | SessionState::Streaming)
}

/// Exponential smoothing towards the audio target. Slightly different lags
/// per bar so they spread during transients instead of pumping in lockstep.
fn smooth_level_tuned(
    current: f32,
    target: f32,
    elapsed: Duration,
    attack: f32,
    release: f32,
) -> f32 {
    let target = target.clamp(0.0, 1.0);
    let delta = target - current;
    if delta.abs() < 0.002 {
        return target;
    }
    let time_constant = if delta > 0.0 { attack } else { release };
    let seconds = elapsed.as_secs_f32().min(0.05);
    let response = 1.0 - (-seconds / time_constant).exp();
    let next = current + delta * response;
    if target <= 0.02 && next < 0.07 {
        0.0
    } else {
        next
    }
}

fn bar_attack(index: usize) -> f32 {
    0.03 * (1.0 + 0.15 * index as f32)
}

fn bar_release(index: usize, target: f32) -> f32 {
    let base = if target <= 0.02 { 0.04 } else { 0.085 };
    base * (1.0 + 0.15 * index as f32)
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
        screen.origin.y + screen.size.height
            - window_size.height
            - px(BUBBLE_BOTTOM_GAP - SHADOW_MARGIN),
    );
    Bounds {
        origin,
        size: window_size,
    }
}

#[cfg(any(windows, target_os = "macos"))]
fn overlay_titlebar() -> Option<TitlebarOptions> {
    None
}

#[cfg(all(not(windows), not(target_os = "macos")))]
fn overlay_titlebar() -> Option<TitlebarOptions> {
    Some(TitlebarOptions {
        title: Some(WINDOW_TITLE.into()),
        appears_transparent: true,
        ..Default::default()
    })
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
    // compositor path may already have spawned SessionRuntime; that worker does not
    // read WAYLAND_DISPLAY or ZED_HEADLESS after Chord::bind.
    unsafe {
        std::env::remove_var("WAYLAND_DISPLAY");
        std::env::remove_var("ZED_HEADLESS");
    }
}

pub fn run() {
    prepare_display();
    Application::new().run(|cx: &mut App| {
        open_popup(cx, true);
        print_opened_line();
        cx.activate(true);
    });
}

pub fn run_with(bubbles: mpsc::Receiver<Bubble>, levels: mpsc::Receiver<f32>) {
    prepare_display();
    Application::new().run(|cx: &mut App| {
        attach(cx, bubbles, levels);
        print_opened_line();
        cx.activate(true);
    });
}

/// PopUp plus 16 ms poller inside an application that is already running.
/// Title stays WINDOW_TITLE. kind PopUp, focus false.
pub fn attach(cx: &mut App, bubbles: mpsc::Receiver<Bubble>, levels: mpsc::Receiver<f32>) {
    let handle = open_popup(cx, false);
    cx.spawn(async move |cx: &mut AsyncApp| loop {
        cx.background_executor()
            .timer(plume_ui::UI_REFRESH_INTERVAL)
            .await;
        let mut latest = None;
        loop {
            match bubbles.try_recv() {
                Ok(bubble) => latest = Some(bubble),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => return,
            }
        }
        let mut level = None;
        while let Ok(next) = levels.try_recv() {
            level = Some(next);
        }
        let _ = cx.update(|cx| {
            let _ = handle.update(cx, |view: &mut BubbleView, window, cx| {
                let mut changed = false;
                let now = Instant::now();
                let elapsed = now.saturating_duration_since(view.last_animation_frame);
                view.last_animation_frame = now;
                if let Some(bubble) = latest {
                    let still_speaking = speaking(bubble.state());
                    view.bubble = bubble;
                    if !still_speaking {
                        view.bars = [0.0; 8];
                        view.target_level = 0.0;
                    }
                    place_bubble(window, session_visible(view.bubble.state()));
                    changed = true;
                }
                if let Some(level) = level {
                    if speaking(view.bubble.state()) {
                        view.target_level = level;
                    }
                }
                if speaking(view.bubble.state()) {
                    for (index, bar) in view.bars.iter_mut().enumerate() {
                        let next = smooth_level_tuned(
                            *bar,
                            view.target_level,
                            elapsed,
                            bar_attack(index),
                            bar_release(index, view.target_level),
                        );
                        if next != *bar {
                            *bar = next;
                            changed = true;
                        }
                    }
                }
                if changed {
                    cx.notify();
                }
            });
        });
    })
    .detach();
}

fn open_popup(cx: &mut App, initially_visible: bool) -> WindowHandle<BubbleView> {
    let window_size = size(
        px(BUBBLE_WIDTH + SHADOW_MARGIN * 2.),
        px(BUBBLE_HEIGHT + SHADOW_MARGIN * 2.),
    );
    let bounds = bottom_center_bounds(window_size, cx);
    let handle = cx
        .open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: overlay_titlebar(),
                app_id: Some("plume".into()),
                kind: WindowKind::PopUp,
                focus: false,
                // GPUI needs the macOS panel mapped once before AppKit can
                // reliably order it in again. Hide it below before the event
                // loop draws the first frame. Windows can start hidden.
                show: initially_visible || !cfg!(windows),
                is_movable: false,
                is_resizable: false,
                is_minimizable: false,
                window_background: WindowBackgroundAppearance::Transparent,
                window_decorations: Some(WindowDecorations::Client),
                ..Default::default()
            },
            |_window, cx| {
                cx.new(|_cx| BubbleView {
                    bubble: Bubble::from_dictation(&Dictation::new()),
                    bars: [0.0; 8],
                    target_level: 0.0,
                    last_animation_frame: Instant::now(),
                })
            },
        )
        .expect("open overlay window");
    hide_server_frame(WINDOW_TITLE);
    #[cfg(target_os = "macos")]
    let _ = handle.update(cx, |_view, window, _cx| macos::configure(window));
    place_overlay(cx, handle, initially_visible);
    cx.spawn(async move |cx: &mut AsyncApp| {
        for delay in FRAME_RECHECK_DELAYS {
            cx.background_executor().timer(delay).await;
            hide_server_frame(WINDOW_TITLE);
            let _ = cx.update(|cx| place_overlay(cx, handle, initially_visible));
        }
    })
    .detach();
    handle
}

fn place_overlay(cx: &mut App, handle: WindowHandle<BubbleView>, preview_visible: bool) {
    let _ = handle.update(cx, |view, window, _cx| {
        place_bubble(
            window,
            preview_visible || session_visible(view.bubble.state()),
        );
    });
}

fn session_visible(state: SessionState) -> bool {
    // Releasing push-to-talk ends the visible capture immediately. Whisper
    // can take a noticeable time to produce its final transcript, but that
    // work should not leave the recording indicator on screen.
    matches!(state, SessionState::Recording | SessionState::Streaming)
}

fn place_bubble(window: &mut Window, visible: bool) {
    #[cfg(windows)]
    stack::place(window, visible);
    #[cfg(target_os = "macos")]
    macos::place(window, visible);
    #[cfg(all(not(windows), not(target_os = "macos")))]
    let _ = (window, visible);
}

#[cfg(target_os = "macos")]
mod macos {
    use cocoa::{
        appkit::{NSColor, NSWindow, NSWindowStyleMask},
        base::{id, nil, NO},
    };
    use gpui::Window;
    use objc::{runtime::Sel, Message};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    fn native_window(window: &Window) -> Option<id> {
        let handle = HasWindowHandle::window_handle(window).ok()?;
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return None;
        };
        let view = handle.ns_view.as_ptr() as id;
        let native: id = unsafe { (&*view).send_message(Sel::register("window"), ()).ok()? };
        (!native.is_null()).then_some(native)
    }

    pub(crate) fn configure(window: &Window) {
        let Some(native) = native_window(window) else {
            return;
        };
        // GPUI's titlebar=None still creates a titled NSPanel. Keep its
        // non-activating bit while removing the title and window chrome.
        unsafe {
            // NSPanel hides when its app is inactive by default. Dictation
            // runs while another app has focus, so visibility must be driven
            // only by the session's orderFrontRegardless/orderOut calls.
            native.setHidesOnDeactivate_(NO);
            let Ok(style) = (&*native).send_message::<_, u64>(Sel::register("styleMask"), ())
            else {
                return;
            };
            let chrome = NSWindowStyleMask::NSTitledWindowMask
                | NSWindowStyleMask::NSClosableWindowMask
                | NSWindowStyleMask::NSMiniaturizableWindowMask
                | NSWindowStyleMask::NSResizableWindowMask
                | NSWindowStyleMask::NSFullSizeContentViewWindowMask;
            let borderless = style & !chrome.bits();
            let _ = (&*native).send_message::<_, ()>(Sel::register("setStyleMask:"), (borderless,));
            native.setHasShadow_(NO);
            native.setOpaque_(NO);
            native.setBackgroundColor_(NSColor::clearColor(nil));
        }
    }

    pub(crate) fn place(window: &Window, visible: bool) {
        let Some(native) = native_window(window) else {
            return;
        };
        unsafe {
            if visible {
                native.orderFrontRegardless();
            } else {
                native.orderOut_(nil);
            }
        }
    }
}

#[cfg(windows)]
mod stack {
    use gpui::Window;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::Foundation::{GetLastError, HWND};
    use windows_sys::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMNCRP_DISABLED, DWMWA_BORDER_COLOR, DWMWA_COLOR_NONE,
        DWMWA_NCRENDERING_POLICY, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND,
    };
    use windows_sys::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, ShowWindow, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
        SWP_SHOWWINDOW, SW_HIDE, SW_SHOWNA,
    };

    pub(crate) fn place(window: &mut Window, visible: bool) {
        let Ok(handle) = HasWindowHandle::window_handle(window) else {
            return;
        };
        let RawWindowHandle::Win32(win) = handle.as_raw() else {
            return;
        };
        let hwnd = win.hwnd.get() as HWND;
        unsafe {
            let non_client_policy = DWMNCRP_DISABLED;
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_NCRENDERING_POLICY as u32,
                (&non_client_policy as *const i32).cast(),
                std::mem::size_of_val(&non_client_policy) as u32,
            );
            let border_color = DWMWA_COLOR_NONE;
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_BORDER_COLOR as u32,
                (&border_color as *const u32).cast(),
                std::mem::size_of_val(&border_color) as u32,
            );
            let corner_preference = DWMWCP_DONOTROUND;
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE as u32,
                (&corner_preference as *const i32).cast(),
                std::mem::size_of_val(&corner_preference) as u32,
            );
            if visible {
                // Taille via GPUI (renderer suivi) plutot que resize externe
                // qui desynchronise le swapchain DirectX et rend la bulle
                // invisible. La position reste native (pas d'API move GPUI).
                window.resize(gpui::size(
                    gpui::px(super::BUBBLE_WIDTH + super::SHADOW_MARGIN * 2.0),
                    gpui::px(super::BUBBLE_HEIGHT + super::SHADOW_MARGIN * 2.0),
                ));
                match bottom_center_on_nearest_monitor(hwnd) {
                    Some((x, y, w, h)) => {
                        let ok = SetWindowPos(
                            hwnd,
                            HWND_TOPMOST,
                            x,
                            y,
                            0,
                            0,
                            SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
                        );
                        // Force l'affichage sans activation : si SetWindowPos
                        // seul ne suffit pas (fenetre demarree cachee), ce
                        // second appel garantit que la bulle devient visible.
                        let _ = ShowWindow(hwnd, SW_SHOWNA);
                        tracing::debug!(
                            "plume-overlay: place visible x={x} y={y} w={w} h={h} setwindowpos={ok} lasterror={} hwnd={hwnd:?}",
                            GetLastError()
                        );
                    }
                    None => {
                        let ok = SetWindowPos(
                            hwnd,
                            HWND_TOPMOST,
                            0,
                            0,
                            0,
                            0,
                            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
                        );
                        let _ = ShowWindow(hwnd, SW_SHOWNA);
                        tracing::debug!(
                            "plume-overlay: place visible fallback setwindowpos={ok} lasterror={} hwnd={hwnd:?}",
                            GetLastError()
                        );
                    }
                }
            } else {
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
        }
    }

    fn bottom_center_on_nearest_monitor(hwnd: HWND) -> Option<(i32, i32, i32, i32)> {
        unsafe {
            // La fenetre demarre cachee avec une taille par defaut Windows
            // (CW_USEDEFAULT). On impose donc la taille reelle de la bulle
            // a chaque affichage, sinon GetWindowRect renvoie une grande
            // taille et la bulle finit vers le centre de l'ecran.
            let dpi = GetDpiForWindow(hwnd);
            let scale = if dpi == 0 { 1.0 } else { dpi as f32 / 96.0 };
            let win_w = ((super::BUBBLE_WIDTH + super::SHADOW_MARGIN * 2.0) * scale).round() as i32;
            let win_h =
                ((super::BUBBLE_HEIGHT + super::SHADOW_MARGIN * 2.0) * scale).round() as i32;
            if win_w <= 0 || win_h <= 0 {
                return None;
            }
            let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
            if monitor.is_null() {
                return None;
            }
            let mut info: MONITORINFO = std::mem::zeroed();
            info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
            if GetMonitorInfoW(monitor, &mut info) == 0 {
                return None;
            }
            let work = info.rcWork;
            let gap = ((super::BUBBLE_BOTTOM_GAP - super::SHADOW_MARGIN) * scale).round() as i32;
            let x = work.left + (work.right - work.left - win_w) / 2;
            let y = work.bottom - win_h - gap;
            Some((x, y, win_w, win_h))
        }
    }
}

fn print_opened_line() {
    tracing::debug!(
        "plume-overlay: window opened title={WINDOW_TITLE} display={}",
        std::env::var("DISPLAY").unwrap_or_else(|_| "<unset>".into())
    );
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use plume_core::SessionState;

    use super::{bar_attack, session_visible, smooth_level_tuned};

    #[test]
    fn bubble_shows_only_while_a_session_runs() {
        assert!(!session_visible(SessionState::Idle));
        assert!(session_visible(SessionState::Recording));
        assert!(session_visible(SessionState::Streaming));
        assert!(!session_visible(SessionState::Finalizing));
        assert!(!session_visible(SessionState::Cancelled));
    }

    #[test]
    fn voice_level_is_responsive_in_both_directions() {
        let frame = plume_ui::UI_REFRESH_INTERVAL;
        let rising = smooth_level_tuned(0.0, 1.0, frame, 0.04, 0.085);
        let falling = smooth_level_tuned(1.0, 0.0, frame, 0.04, 0.04);
        assert!(rising > 0.32 && rising < 0.34);
        assert!(falling > 0.67 && falling < 0.68);
        assert_eq!(smooth_level_tuned(0.5, 0.501, frame, 0.04, 0.085), 0.501);
    }

    #[test]
    fn voice_level_snaps_cleanly_to_silence() {
        let frame = plume_ui::UI_REFRESH_INTERVAL;
        assert_eq!(smooth_level_tuned(0.08, 0.0, frame, 0.04, 0.04), 0.0);
        assert!(smooth_level_tuned(0.5, 0.0, frame, 0.04, 0.04) > 0.3);
    }

    #[test]
    fn voice_smoothing_uses_elapsed_time_and_caps_long_frames() {
        let one_frame = smooth_level_tuned(0.0, 1.0, plume_ui::UI_REFRESH_INTERVAL, 0.04, 0.085);
        let two_frames = smooth_level_tuned(0.0, 1.0, Duration::from_millis(32), 0.04, 0.085);
        let capped = smooth_level_tuned(0.0, 1.0, Duration::from_secs(1), 0.04, 0.085);
        assert!(two_frames > one_frame);
        assert_eq!(
            capped,
            smooth_level_tuned(0.0, 1.0, Duration::from_millis(50), 0.04, 0.085)
        );
    }

    #[test]
    fn voice_bars_lag_differently_per_bar() {
        let frame = plume_ui::UI_REFRESH_INTERVAL;
        let fast = smooth_level_tuned(0.0, 1.0, frame, bar_attack(0), 0.085);
        let slow = smooth_level_tuned(0.0, 1.0, frame, bar_attack(7), 0.085);
        assert!(bar_attack(7) > bar_attack(0));
        assert!(fast > slow);
        assert!(slow > 0.0);
    }
}
