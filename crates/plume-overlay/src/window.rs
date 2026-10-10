use std::sync::mpsc;
use std::time::{Duration, Instant};

use gpui::{
    div, point, prelude::*, px, size, App, Application, AsyncApp, Bounds, Context, Pixels,
    SharedString, Size, TitlebarOptions, Window, WindowBackgroundAppearance, WindowBounds,
    WindowDecorations, WindowHandle, WindowKind, WindowOptions,
};
use plume_core::Dictation;
use plume_ui::Palette;

use crate::feedback::{FeedbackState, Phase};
use crate::frame::hide_server_frame;
use crate::pill::{self, Lead, Measure, PillSpec, Tone, PILL_HEIGHT, WAVE_BARS};
use crate::{Appearance, Bubble, Feedback};

pub const WINDOW_TITLE: &str = "Plume — Dictation";

const FRAME_RECHECK_DELAYS: [Duration; 3] = [
    Duration::from_millis(40),
    Duration::from_millis(150),
    Duration::from_millis(400),
];

const SHADOW_MARGIN: f32 = 20.;
const BUBBLE_BOTTOM_GAP: f32 = 48.;
const INITIAL_WIDTH: f32 = 160.;
const SHAKE_DURATION: Duration = Duration::from_millis(600);
const SHAKE_AMPLITUDE: f32 = 7.;
const SHAKE_OSCILLATIONS: f32 = 3.;
const PASTE_SHORTCUT: &str = if cfg!(target_os = "macos") {
    "Cmd+V"
} else {
    "Ctrl+V"
};
const FADE_DURATION: Duration = Duration::from_millis(315);

struct BubbleView {
    feedback: FeedbackState,
    bars: [f32; WAVE_BARS],
    target_level: f32,
    last_animation_frame: Instant,
    reduced_motion: bool,
    placed_phase: Phase,
    palette: Palette,
    width: f32,
    height: f32,
    measured: Option<(PillSpec, Measure)>,
}

impl BubbleView {
    fn spec(&self) -> PillSpec {
        match self.feedback.phase() {
            Phase::Hidden => PillSpec::new(Lead::Idle, "Ready"),
            Phase::Starting => PillSpec::new(Lead::Idle, "Starting…"),
            Phase::Cancelling => PillSpec::new(Lead::Idle, "Cancelling…"),
            Phase::NoSpeech => PillSpec::new(Lead::Idle, "No speech detected"),
            Phase::Recording => {
                let (limit, cancel) = match &self.feedback.bubble.feedback {
                    Feedback::RecordingNotice { limit, cancel, .. } => (*limit, cancel.as_str()),
                    _ => (false, ""),
                };
                PillSpec {
                    tone: if limit { Tone::Warning } else { Tone::Muted },
                    keys: shortcut_keys(cancel),
                    ..PillSpec::new(
                        Lead::Listening,
                        if limit { "1 minute left" } else { "Listening" },
                    )
                }
            }
            Phase::Transcribing | Phase::Inserting => {
                PillSpec::new(Lead::Thinking, "Transcribing on-device")
            }
            Phase::Success if self.feedback.bubble.feedback == Feedback::Copied => PillSpec {
                keys: shortcut_keys(PASTE_SHORTCUT),
                ..PillSpec::new(Lead::Done, "Copied")
            },
            Phase::Success => PillSpec::new(Lead::Done, "Inserted"),
            Phase::Attention => self.attention(),
        }
    }

    fn attention(&self) -> PillSpec {
        if let Some(recovery) = self.feedback.recoveries.front() {
            let spec = if recovery.copy_failed {
                notice(
                    Lead::Alert,
                    "Clipboard unavailable",
                    "Your text is safe. Try Copy again.",
                )
            } else if recovery.copied {
                notice(
                    Lead::Done,
                    "Text copied",
                    if cfg!(target_os = "macos") {
                        "Press Cmd+V to paste into your app."
                    } else {
                        "Press Ctrl+V to paste into your app."
                    },
                )
            } else {
                notice(
                    Lead::Caution,
                    "Insertion unavailable",
                    "Your transcription is saved.",
                )
            };
            return PillSpec {
                action: Some("Copy".into()),
                ..spec
            };
        }
        match &self.feedback.bubble.feedback {
            Feedback::Error { title, advice } => notice(Lead::Alert, title, advice),
            _ => notice(
                Lead::Caution,
                "Insertion unavailable",
                "Your transcription is saved.",
            ),
        }
    }
}

/// Two-line pill with a dismiss button. Errors can carry raw multi-line
/// messages; the pill shows each part on one line.
fn notice(lead: Lead, title: &str, detail: &str) -> PillSpec {
    let line = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");
    let detail = line(detail);
    PillSpec {
        tone: Tone::Strong,
        detail: (!detail.is_empty()).then(|| detail.into()),
        dismiss: true,
        ..PillSpec::new(lead, line(title))
    }
}

/// "Ctrl+Shift+X" → one key cap per key.
fn shortcut_keys(shortcut: &str) -> Vec<SharedString> {
    shortcut
        .split('+')
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .map(|key| SharedString::from(key.to_owned()))
        .collect()
}

impl Render for BubbleView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let phase = self.feedback.phase();
        let elapsed = self.feedback.since.elapsed();
        let motion = terminal_motion(
            phase,
            self.feedback.lifetime(),
            elapsed,
            self.reduced_motion,
        );
        let spec = self.spec();
        let detail = match &self.measured {
            Some((measured, measure)) if *measured == spec => measure.detail.clone(),
            _ => spec.detail.clone(),
        };
        let frame = pill::Frame {
            width: self.width,
            height: self.height,
            offset_x: motion.offset_x,
            opacity: motion.opacity,
            time: if self.reduced_motion {
                0.
            } else {
                elapsed.as_secs_f32()
            },
            animate: !self.reduced_motion,
            bars: &self.bars,
            detail,
        };
        let on_copy = cx.listener(|view, _, _, cx| {
            if let Some(recovery) = view.feedback.recoveries.front_mut() {
                let result = arboard::Clipboard::new()
                    .and_then(|mut clipboard| clipboard.set_text(recovery.text.clone()));
                recovery.copied = result.is_ok();
                recovery.copy_failed = result.is_err();
            }
            cx.notify();
        });
        let on_dismiss = cx.listener(|view, _, _, cx| {
            view.feedback.dismiss();
            cx.notify();
        });
        div()
            .flex()
            .items_center()
            .justify_center()
            .size_full()
            .child(pill::render(
                &spec,
                frame,
                &pill::colors(self.palette),
                Box::new(on_copy),
                Box::new(on_dismiss),
            ))
    }
}

struct TerminalMotion {
    offset_x: f32,
    opacity: f32,
}

/// `lifetime` is how long the terminal state stays; the pill fades out at
/// its end. No lifetime: the pill holds still.
fn terminal_motion(
    phase: Phase,
    lifetime: Option<Duration>,
    elapsed: Duration,
    reduced_motion: bool,
) -> TerminalMotion {
    let mut motion = TerminalMotion {
        offset_x: 0.,
        opacity: 1.,
    };
    if reduced_motion {
        return motion;
    }
    let Some(duration) = lifetime else {
        return motion;
    };
    if phase == Phase::NoSpeech {
        // A head-shake "no": three swings that settle well before the fade.
        let shake = (elapsed.as_secs_f32() / SHAKE_DURATION.as_secs_f32()).clamp(0., 1.);
        motion.offset_x = (shake * std::f32::consts::TAU * SHAKE_OSCILLATIONS).sin()
            * SHAKE_AMPLITUDE
            * (1. - shake).powf(1.4);
    }
    let fade_start = duration.saturating_sub(FADE_DURATION);
    let fade = (elapsed.saturating_sub(fade_start).as_secs_f32() / FADE_DURATION.as_secs_f32())
        .clamp(0., 1.);
    motion.opacity = 1. - fade * fade * (3. - 2. * fade);
    motion
}

fn palette(appearance: Appearance, window: &Window) -> Palette {
    match appearance {
        Appearance::Light => Palette::Light,
        Appearance::Dark => Palette::Dark,
        Appearance::System => Palette::from_window(window),
    }
}

/// Exponential ease of the pill's size towards its measured content.
fn approach(current: f32, target: f32, elapsed: Duration, snap: bool) -> f32 {
    if snap || (target - current).abs() < 0.5 {
        target
    } else {
        current + (target - current) * (elapsed.as_secs_f32() * 18.).min(1.)
    }
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
        let mut updates = Vec::new();
        loop {
            match bubbles.try_recv() {
                Ok(bubble) => updates.push(bubble),
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
                for bubble in updates {
                    view.feedback.receive(bubble, now);
                    let still_speaking = view.feedback.phase() == Phase::Recording;
                    if !still_speaking {
                        view.bars = [0.0; WAVE_BARS];
                        view.target_level = 0.0;
                    }
                    changed = true;
                }
                view.feedback.tick(now);
                let phase = view.feedback.phase();
                let appearance = cx.try_global::<Appearance>().copied().unwrap_or_default();
                let palette = palette(appearance, window);
                if palette != view.palette {
                    view.palette = palette;
                    changed = true;
                }
                // Shaping text is only needed when the pill's content changes.
                let spec = view.spec();
                let target = match &view.measured {
                    Some((measured, measure)) if *measured == spec => {
                        (measure.width, measure.height)
                    }
                    _ => {
                        let measure = spec.measure(window);
                        let size = (measure.width, measure.height);
                        view.measured = Some((spec, measure));
                        size
                    }
                };
                // A pill that appears takes its size at once; between two
                // visible states it eases to the new content.
                let snap = view.reduced_motion || view.placed_phase == Phase::Hidden;
                let previous = (view.width, view.height);
                view.width = approach(view.width, target.0, elapsed, snap);
                view.height = approach(view.height, target.1, elapsed, snap);
                changed |= (view.width, view.height) != previous;
                if changed || phase != view.placed_phase {
                    place_bubble(window, phase != Phase::Hidden, view.width, view.height, cx);
                    changed = true;
                    view.placed_phase = phase;
                }
                if !view.reduced_motion
                    && matches!(
                        phase,
                        Phase::Recording
                            | Phase::Transcribing
                            | Phase::Inserting
                            | Phase::NoSpeech
                            | Phase::Success
                    )
                {
                    changed = true;
                }
                if let Some(level) = level {
                    if phase == Phase::Recording {
                        view.target_level = level;
                    }
                }
                if phase == Phase::Recording {
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
    let window_size = window_size(INITIAL_WIDTH, PILL_HEIGHT);
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
            |window, cx| {
                let appearance = cx.try_global::<Appearance>().copied().unwrap_or_default();
                cx.new(|_cx| BubbleView {
                    feedback: FeedbackState::new(
                        Bubble::from_dictation(&Dictation::new()),
                        Instant::now(),
                    ),
                    bars: [0.0; WAVE_BARS],
                    target_level: 0.0,
                    last_animation_frame: Instant::now(),
                    reduced_motion: reduced_motion(),
                    placed_phase: Phase::Hidden,
                    palette: palette(appearance, window),
                    width: INITIAL_WIDTH,
                    height: PILL_HEIGHT,
                    measured: None,
                })
            },
        )
        .expect("open overlay window");
    hide_server_frame(WINDOW_TITLE);
    #[cfg(target_os = "macos")]
    let _ = handle.update(cx, |_view, window, cx| macos::configure(window, cx));
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
    let _ = handle.update(cx, |view, window, cx| {
        place_bubble(
            window,
            preview_visible || view.feedback.phase() != Phase::Hidden,
            view.width,
            view.height,
            cx,
        );
    });
}

/// The pill plus room for its shadow, in whole pixels so the window does not
/// jitter while the pill eases between sizes.
fn window_size(width: f32, height: f32) -> Size<Pixels> {
    size(
        px((width + SHADOW_MARGIN * 2.).ceil()),
        px((height + SHADOW_MARGIN * 2.).ceil()),
    )
}

fn place_bubble(window: &mut Window, visible: bool, width: f32, height: f32, cx: &App) {
    let dimensions = window_size(width, height);
    if window.bounds().size != dimensions {
        window.resize(dimensions);
    }
    #[cfg(windows)]
    stack::place(window, visible, dimensions, cx);
    #[cfg(target_os = "macos")]
    macos::place(window, visible, dimensions, cx);
    #[cfg(target_os = "linux")]
    linux::place(window, visible, bottom_center_bounds(dimensions, cx));
}

fn reduced_motion() -> bool {
    if std::env::var("PLUME_REDUCED_MOTION").is_ok_and(|value| value == "1") {
        return true;
    }
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SystemParametersInfoW, SPI_GETCLIENTAREAANIMATION,
        };
        let mut enabled: i32 = 1;
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            (&mut enabled as *mut i32).cast(),
            0,
        );
        enabled == 0
    }
    #[cfg(target_os = "macos")]
    unsafe {
        use objc::{
            runtime::{Class, Sel},
            Message,
        };
        let workspace: cocoa::base::id = Class::get("NSWorkspace")
            .unwrap()
            .send_message(Sel::register("sharedWorkspace"), ())
            .unwrap();
        (&*workspace)
            .send_message::<_, cocoa::base::BOOL>(
                Sel::register("accessibilityDisplayShouldReduceMotion"),
                (),
            )
            .unwrap_or(cocoa::base::NO)
            != cocoa::base::NO
    }
    #[cfg(target_os = "linux")]
    {
        false
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use cocoa::{
        appkit::{NSColor, NSWindow, NSWindowStyleMask},
        base::{id, nil, NO},
    };
    use gpui::Window;
    use objc::{rc::StrongPtr, runtime::Sel, Message};
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

    pub(crate) fn configure(window: &Window, cx: &gpui::App) {
        let Some(native) = native_window(window) else {
            return;
        };
        // Changing the style synchronously resizes the NSView and calls back
        // into GPUI. Schedule it after the App/window update releases its
        // borrow, and retain the panel until the foreground task completes.
        let native = unsafe { StrongPtr::retain(native) };
        cx.foreground_executor()
            .spawn(async move { unsafe { configure_native(*native) } })
            .detach();
    }

    unsafe fn configure_native(native: id) {
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

    pub(crate) fn place(
        window: &Window,
        visible: bool,
        dimensions: gpui::Size<gpui::Pixels>,
        cx: &gpui::App,
    ) {
        let Some(native) = native_window(window) else {
            return;
        };
        let native_address = native as usize;
        let width = f64::from(f32::from(dimensions.width));
        cx.foreground_executor()
            .spawn(async move {
                unsafe {
                    let native = native_address as id;
                    let screen: id = (&*native)
                        .send_message(Sel::register("screen"), ())
                        .unwrap_or(nil);
                    if screen != nil {
                        if let Ok(frame) = (&*screen).send_message::<_, cocoa::foundation::NSRect>(
                            Sel::register("visibleFrame"),
                            (),
                        ) {
                            let origin = cocoa::foundation::NSPoint::new(
                                frame.origin.x + (frame.size.width - width) / 2.,
                                frame.origin.y
                                    + f64::from(super::BUBBLE_BOTTOM_GAP - super::SHADOW_MARGIN),
                            );
                            let _ = (&*native)
                                .send_message::<_, ()>(Sel::register("setFrameOrigin:"), (origin,));
                        }
                    }
                    if visible {
                        native.orderFrontRegardless();
                    } else {
                        native.orderOut_(nil);
                    }
                }
            })
            .detach();
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

    pub(crate) fn place(
        window: &mut Window,
        visible: bool,
        dimensions: gpui::Size<gpui::Pixels>,
        cx: &gpui::App,
    ) {
        let Ok(handle) = HasWindowHandle::window_handle(window) else {
            return;
        };
        let RawWindowHandle::Win32(win) = handle.as_raw() else {
            return;
        };
        let hwnd = win.hwnd.get();
        let width = f32::from(dimensions.width);
        let height = f32::from(dimensions.height);
        // Native visibility/position messages can synchronously ask GPUI to
        // draw. Dispatch them after the App/window update releases its borrow.
        cx.foreground_executor()
            .spawn(async move {
                unsafe {
                    place_native(hwnd as HWND, visible, width, height);
                }
            })
            .detach();
    }

    unsafe fn place_native(hwnd: HWND, visible: bool, width: f32, height: f32) {
        if windows_sys::Win32::UI::WindowsAndMessaging::IsWindow(hwnd) == 0 {
            return;
        }
        unsafe {
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE,
            };
            let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style | WS_EX_NOACTIVATE as isize);
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
                match bottom_center_on_nearest_monitor(hwnd, width, height) {
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

    fn bottom_center_on_nearest_monitor(
        hwnd: HWND,
        width: f32,
        height: f32,
    ) -> Option<(i32, i32, i32, i32)> {
        unsafe {
            // La fenetre demarre cachee avec une taille par defaut Windows
            // (CW_USEDEFAULT). On impose donc la taille reelle de la bulle
            // a chaque affichage, sinon GetWindowRect renvoie une grande
            // taille et la bulle finit vers le centre de l'ecran.
            let dpi = GetDpiForWindow(hwnd);
            let scale = if dpi == 0 { 1.0 } else { dpi as f32 / 96.0 };
            let win_w = (width * scale).round() as i32;
            let win_h = (height * scale).round() as i32;
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
    #[cfg(test)]
    mod tests {
        use super::*;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, GetForegroundWindow, GetWindowLongPtrW,
            IsWindowVisible, GWL_EXSTYLE, WS_EX_NOACTIVATE,
        };

        #[test]
        fn showing_overlay_preserves_foreground_and_sets_noactivate() {
            struct TestWindow(HWND);
            impl Drop for TestWindow {
                fn drop(&mut self) {
                    unsafe {
                        DestroyWindow(self.0);
                    }
                }
            }
            // SAFETY: this test creates, uses and destroys its own native
            // window on one thread without activating any other app.
            unsafe {
                let window = TestWindow(CreateWindowExW(
                    0,
                    windows_sys::w!("STATIC"),
                    windows_sys::w!("Plume feedback test"),
                    0,
                    0,
                    0,
                    120,
                    60,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null(),
                ));
                assert!(!window.0.is_null());
                let foreground = GetForegroundWindow();
                place_native(window.0, true, 120., 60.);
                assert_ne!(IsWindowVisible(window.0), 0);
                assert_eq!(GetForegroundWindow(), foreground);
                assert_ne!(
                    GetWindowLongPtrW(window.0, GWL_EXSTYLE) & WS_EX_NOACTIVATE as isize,
                    0
                );
                place_native(window.0, false, 120., 60.);
                assert_eq!(IsWindowVisible(window.0), 0);
            }
        }
    }
}

fn print_opened_line() {
    tracing::debug!(
        "plume-overlay: window opened title={WINDOW_TITLE} display={}",
        std::env::var("DISPLAY").unwrap_or_else(|_| "<unset>".into())
    );
}

#[cfg(target_os = "linux")]
mod linux {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use x11rb::{
        connection::Connection,
        protocol::xproto::{ConfigureWindowAux, ConnectionExt},
    };

    pub fn place(window: &gpui::Window, visible: bool, bounds: gpui::Bounds<gpui::Pixels>) {
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let handle = HasWindowHandle::window_handle(window)
                .map_err(|error| std::io::Error::other(error.to_string()))?;
            let id = match handle.as_raw() {
                RawWindowHandle::Xcb(handle) => handle.window.get(),
                RawWindowHandle::Xlib(handle) => u32::try_from(handle.window)?,
                _ => return Ok(()),
            };
            let (connection, _) = x11rb::connect(None)?;
            if visible {
                connection
                    .configure_window(
                        id,
                        &ConfigureWindowAux::new()
                            .x(f32::from(bounds.origin.x) as i32)
                            .y(f32::from(bounds.origin.y) as i32),
                    )?
                    .check()?;
                connection.map_window(id)?.check()?;
            } else {
                connection.unmap_window(id)?.check()?;
            }
            connection.flush()?;
            Ok(())
        })();
        if let Err(error) = result {
            tracing::warn!("plume-overlay: visibility: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{
        bar_attack, notice, shortcut_keys, smooth_level_tuned, terminal_motion, SHAKE_AMPLITUDE,
        SHAKE_DURATION,
    };
    use crate::feedback::{Phase, COPIED_DURATION, NO_SPEECH_DURATION, SUCCESS_DURATION};
    use crate::pill::{Lead, Tone};

    #[test]
    fn no_speech_shakes_then_settles_before_fading() {
        let at = |ms| {
            terminal_motion(
                Phase::NoSpeech,
                Some(NO_SPEECH_DURATION),
                Duration::from_millis(ms),
                false,
            )
        };
        assert_eq!(at(0).offset_x, 0.);
        assert!((1..SHAKE_DURATION.as_millis() as u64).any(|ms| at(ms).offset_x.abs() > 5.));
        assert!((0..2000).all(|ms| at(ms).offset_x.abs() <= SHAKE_AMPLITUDE));
        assert!(at(SHAKE_DURATION.as_millis() as u64).offset_x.abs() < 1e-4);
        assert_eq!(at(SHAKE_DURATION.as_millis() as u64).opacity, 1.);
        assert_eq!(at(NO_SPEECH_DURATION.as_millis() as u64).opacity, 0.);
    }

    #[test]
    fn success_holds_then_fades_without_moving() {
        for lifetime in [SUCCESS_DURATION, COPIED_DURATION] {
            let at = |ms| {
                terminal_motion(
                    Phase::Success,
                    Some(lifetime),
                    Duration::from_millis(ms),
                    false,
                )
            };
            assert_eq!(at(400).opacity, 1.);
            assert_eq!(at(400).offset_x, 0.);
            assert_eq!(at(lifetime.as_millis() as u64).opacity, 0.);
        }
        assert_eq!(
            terminal_motion(Phase::Success, None, Duration::from_secs(5), false).opacity,
            1.
        );
    }

    #[test]
    fn reduced_motion_keeps_the_pill_still() {
        let motion = terminal_motion(
            Phase::NoSpeech,
            Some(NO_SPEECH_DURATION),
            Duration::from_millis(100),
            true,
        );
        assert_eq!(motion.offset_x, 0.);
        assert_eq!(motion.opacity, 1.);
    }

    #[test]
    fn cancel_shortcut_becomes_key_caps() {
        assert_eq!(shortcut_keys("Esc"), vec!["Esc"]);
        assert_eq!(shortcut_keys("Ctrl + Shift+X"), vec!["Ctrl", "Shift", "X"]);
        assert!(shortcut_keys("").is_empty());
    }

    #[test]
    fn error_pill_keeps_messages_on_one_line() {
        let spec = notice(
            Lead::Alert,
            "Recording unavailable",
            "No device.
Check   the mic.",
        );
        assert_eq!(spec.lead, Lead::Alert);
        assert_eq!(spec.tone, Tone::Strong);
        assert_eq!(spec.detail, Some("No device. Check the mic.".into()));
        assert!(spec.dismiss);
        assert_eq!(notice(Lead::Alert, "Failed", "  ").detail, None);
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
