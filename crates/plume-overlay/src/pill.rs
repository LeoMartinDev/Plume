//! The dictation pill: one capsule whose content follows the capture phase.
//! `PillSpec` drives both measurement and drawing, so the native window is
//! always sized to exactly what is painted.

use gpui::{
    canvas, div, hsla, point, prelude::*, px, rgb, App, BoxShadow, ClickEvent, Div, FontWeight,
    PathBuilder, Rgba, SharedString, TextRun, Window,
};
use plume_ui::Palette;

pub(crate) const PILL_HEIGHT: f32 = 46.;
pub(crate) const NOTICE_HEIGHT: f32 = 58.;
const PAD_LEFT: f32 = 15.;
const PAD_RIGHT: f32 = 16.;
const PAD_RIGHT_CONTROL: f32 = 8.;
const BORDER: f32 = 1.;
const GAP: f32 = 12.;
const LABEL_SIZE: f32 = 13.;
const DETAIL_SIZE: f32 = 12.;
const DETAIL_MAX: f32 = 300.;
const KEY_SIZE: f32 = 11.;
const KEY_PAD_X: f32 = 9.;
const KEY_GAP: f32 = 4.;
const ACTION_SIZE: f32 = 12.;
const ACTION_PAD_X: f32 = 12.;
const CONTROL_GAP: f32 = 4.;
const DISMISS_SIZE: f32 = 28.;
const CROSS_SIZE: f32 = 9.;

const DOT_SIZE: f32 = 9.;
const PULSE_SIZE: f32 = 18.;
const PULSE_PERIOD: f32 = 1.6;
const THINKING_DOT: f32 = 6.;
const THINKING_GAP: f32 = 4.;
const BADGE_SIZE: f32 = 20.;

const ACCENT: u32 = 0x3b6dff;
const ERROR: u32 = 0xe5484d;

/// Mirrored voice bars: symmetric envelope (small-tall-small) so the pill
/// reads as a waveform. Bar heights are a pure function of the audio level:
/// no time-driven wave, so bars hold still on a steady note and sit flat in
/// silence. During transients the per-bar lag in the overlay spreads them.
pub(crate) const WAVE_BARS: usize = 12;
const WAVE_SHAPE: [f32; WAVE_BARS] = [
    0.3, 0.45, 0.6, 0.75, 0.9, 1.0, 1.0, 0.9, 0.75, 0.6, 0.45, 0.3,
];
const BAR_WIDTH: f32 = 3.;
const BAR_GAP: f32 = 3.;
const BAR_MIN: f32 = 4.;
const BAR_MAX: f32 = 22.;
const WAVE_WIDTH: f32 = WAVE_BARS as f32 * BAR_WIDTH + (WAVE_BARS - 1) as f32 * BAR_GAP;

pub(crate) struct Colors {
    fill: Rgba,
    border: Rgba,
    fg: Rgba,
    muted: Rgba,
    key: Rgba,
    key_fg: Rgba,
    dot: Rgba,
    warning: Rgba,
    action: Rgba,
    action_hover: Rgba,
    action_fg: Rgba,
    shadow: f32,
}

/// Dark pill in dark mode, light pill in light mode.
pub(crate) fn colors(palette: Palette) -> Colors {
    match palette {
        Palette::Dark => Colors {
            fill: rgb(0x141414),
            border: rgb(0x3a3a40),
            fg: rgb(0xffffff),
            muted: rgb(0xc9c9ce),
            key: rgb(0x2a2a2e),
            key_fg: rgb(0xe4e4e8),
            dot: rgb(0x6e6e76),
            warning: rgb(0xf0c47b),
            action: rgb(0xf5f5f7),
            action_hover: rgb(0xdadade),
            action_fg: rgb(0x141414),
            shadow: 0.45,
        },
        Palette::Light => Colors {
            fill: rgb(0xf2f2f0),
            border: rgb(0xcfcfca),
            // Near-black reads softer than pure black on the warm grey fill.
            fg: rgb(0x1d1d1f),
            muted: rgb(0x515157),
            key: rgb(0xe2e2df),
            key_fg: rgb(0x2a2a2e),
            dot: rgb(0x8a8a90),
            warning: rgb(0x8f5600),
            action: rgb(0x141414),
            action_hover: rgb(0x2e2e32),
            action_fg: rgb(0xffffff),
            shadow: 0.18,
        },
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Lead {
    /// Grey dot: ready, starting, cancelling, no speech.
    Idle,
    /// Pulsing accent dot plus the live waveform.
    Listening,
    /// Three accent dots blinking in turn.
    Thinking,
    /// Accent badge with a check.
    Done,
    /// Amber badge: the text is safe but needs the user.
    Caution,
    /// Red badge: something failed.
    Alert,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tone {
    Muted,
    Strong,
    Warning,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PillSpec {
    pub lead: Lead,
    pub label: SharedString,
    pub tone: Tone,
    /// Second line; switches the pill to its taller notice layout.
    pub detail: Option<SharedString>,
    pub keys: Vec<SharedString>,
    pub action: Option<SharedString>,
    pub dismiss: bool,
}

impl PillSpec {
    pub fn new(lead: Lead, label: impl Into<SharedString>) -> Self {
        Self {
            lead,
            label: label.into(),
            tone: Tone::Muted,
            detail: None,
            keys: Vec::new(),
            action: None,
            dismiss: false,
        }
    }

    fn label_weight(&self) -> FontWeight {
        if self.tone == Tone::Strong {
            FontWeight::MEDIUM
        } else {
            FontWeight::NORMAL
        }
    }

    fn has_control(&self) -> bool {
        !self.keys.is_empty() || self.action.is_some() || self.dismiss
    }

    /// Outer size of the pill, border included.
    pub fn measure(&self, window: &Window) -> Measure {
        let label = text_width(window, &self.label, LABEL_SIZE, self.label_weight());
        let detail = self
            .detail
            .as_ref()
            .map(|detail| fit(window, detail, DETAIL_SIZE, DETAIL_MAX));
        let text = match &detail {
            Some(detail) => label.max(text_width(window, detail, DETAIL_SIZE, FontWeight::NORMAL)),
            None => label,
        };
        let mut width = PAD_LEFT + lead_width(self.lead) + GAP + text;
        if !self.keys.is_empty() {
            let keys: f32 = self
                .keys
                .iter()
                .map(|key| text_width(window, key, KEY_SIZE, FontWeight::NORMAL) + KEY_PAD_X * 2.)
                .sum();
            width += GAP + keys + KEY_GAP * (self.keys.len() - 1) as f32;
        }
        if self.action.is_some() || self.dismiss {
            let action = self.action.as_ref().map_or(0., |action| {
                text_width(window, action, ACTION_SIZE, FontWeight::MEDIUM) + ACTION_PAD_X * 2.
            });
            let both = if self.action.is_some() && self.dismiss {
                CONTROL_GAP
            } else {
                0.
            };
            let dismiss = if self.dismiss { DISMISS_SIZE } else { 0. };
            width += GAP + action + both + dismiss;
        }
        width += if self.has_control() {
            PAD_RIGHT_CONTROL
        } else {
            PAD_RIGHT
        };
        // Border plus a little room for sub-pixel differences between
        // shaping here and layout at paint time.
        width += BORDER * 2. + 2.;
        let height = if self.detail.is_some() {
            NOTICE_HEIGHT
        } else {
            PILL_HEIGHT
        };
        Measure {
            width: width.ceil(),
            height,
            detail,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Measure {
    pub width: f32,
    pub height: f32,
    /// The detail line, cut with an ellipsis to fit `DETAIL_MAX`.
    pub detail: Option<SharedString>,
}

fn lead_width(lead: Lead) -> f32 {
    match lead {
        Lead::Idle => DOT_SIZE,
        Lead::Listening => PULSE_SIZE + GAP + WAVE_WIDTH,
        Lead::Thinking => THINKING_DOT * 3. + THINKING_GAP * 2.,
        Lead::Done | Lead::Caution | Lead::Alert => BADGE_SIZE,
    }
}

fn text_width(window: &Window, text: &str, size: f32, weight: FontWeight) -> f32 {
    if text.is_empty() {
        return 0.;
    }
    let mut font = window.text_style().font();
    font.weight = weight;
    let run = TextRun {
        len: text.len(),
        font,
        color: hsla(0., 0., 0., 1.),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let line = window.text_system().shape_line(
        SharedString::from(text.to_owned()),
        px(size),
        &[run],
        None,
    );
    f32::from(line.width).ceil()
}

/// Longest prefix of `text` that fits in `max` once an ellipsis is added.
/// GPUI's own truncation reuses the untruncated layout of a `nowrap` text
/// measured earlier, so the pill cuts the line itself.
fn fit(window: &Window, text: &str, size: f32, max: f32) -> SharedString {
    if text_width(window, text, size, FontWeight::NORMAL) <= max {
        return SharedString::from(text.to_owned());
    }
    let ends: Vec<usize> = text
        .char_indices()
        .map(|(index, _)| index)
        .chain([text.len()])
        .collect();
    let cut = |chars: usize| format!("{}…", text[..ends[chars]].trim_end());
    let (mut low, mut high) = (0, ends.len() - 2);
    while low < high {
        let middle = (low + high).div_ceil(2);
        if text_width(window, &cut(middle), size, FontWeight::NORMAL) <= max {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    cut(low).into()
}

/// Per-frame values the overlay animates around the spec.
pub(crate) struct Frame<'a> {
    pub width: f32,
    pub height: f32,
    pub offset_x: f32,
    pub opacity: f32,
    /// Seconds since the phase started; frozen when motion is reduced.
    pub time: f32,
    pub animate: bool,
    pub bars: &'a [f32; WAVE_BARS],
    /// The fitted detail line from `PillSpec::measure`.
    pub detail: Option<SharedString>,
}

type Handler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

pub(crate) fn render(
    spec: &PillSpec,
    frame: Frame,
    colors: &Colors,
    on_action: Handler,
    on_dismiss: Handler,
) -> Div {
    let label_color = match spec.tone {
        Tone::Muted => colors.muted,
        Tone::Strong => colors.fg,
        Tone::Warning => colors.warning,
    };
    let label = div()
        .flex_shrink_0()
        .whitespace_nowrap()
        .text_size(px(LABEL_SIZE))
        .line_height(px(17.))
        .font_weight(spec.label_weight())
        .text_color(label_color)
        .child(spec.label.clone());
    let text = match frame.detail.clone() {
        Some(detail) => div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .gap(px(1.))
            .child(label)
            .child(
                div()
                    .whitespace_nowrap()
                    .text_size(px(DETAIL_SIZE))
                    .line_height(px(16.))
                    .text_color(colors.muted)
                    .child(detail),
            ),
        None => label,
    };
    let has_control = spec.has_control();
    div()
        .relative()
        .left(px(frame.offset_x))
        .opacity(frame.opacity)
        .w(px(frame.width))
        .h(px(frame.height))
        .flex_shrink_0()
        .overflow_hidden()
        .rounded(px(frame.height / 2.))
        .bg(colors.fill)
        .border(px(BORDER))
        .border_color(colors.border)
        .shadow(vec![
            BoxShadow {
                color: hsla(0., 0., 0., colors.shadow),
                offset: point(px(0.), px(6.)),
                blur_radius: px(16.),
                spread_radius: px(-2.),
            },
            BoxShadow {
                color: hsla(0., 0., 0., colors.shadow * 0.5),
                offset: point(px(0.), px(1.)),
                blur_radius: px(3.),
                spread_radius: px(0.),
            },
        ])
        .flex()
        .items_center()
        .gap(px(GAP))
        .pl(px(PAD_LEFT))
        .pr(px(if has_control {
            PAD_RIGHT_CONTROL
        } else {
            PAD_RIGHT
        }))
        .child(lead(spec.lead, &frame, colors))
        .child(text)
        .when(!spec.keys.is_empty(), |el| {
            el.child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .gap(px(KEY_GAP))
                    .children(spec.keys.iter().map(|key| {
                        div()
                            .px(px(KEY_PAD_X))
                            .py(px(5.))
                            .rounded_full()
                            .bg(colors.key)
                            .text_color(colors.key_fg)
                            .text_size(px(KEY_SIZE))
                            .line_height(px(14.))
                            .whitespace_nowrap()
                            .child(key.clone())
                    })),
            )
        })
        .when(spec.action.is_some() || spec.dismiss, |el| {
            el.child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(CONTROL_GAP))
                    .when_some(spec.action.clone(), |el, action| {
                        el.child(
                            div()
                                .id("pill-action")
                                .cursor_pointer()
                                .px(px(ACTION_PAD_X))
                                .h(px(DISMISS_SIZE))
                                .flex()
                                .items_center()
                                .rounded_full()
                                .bg(colors.action)
                                .hover(|el| el.bg(colors.action_hover))
                                .text_color(colors.action_fg)
                                .text_size(px(ACTION_SIZE))
                                .font_weight(FontWeight::MEDIUM)
                                .whitespace_nowrap()
                                .on_click(on_action)
                                .child(action),
                        )
                    })
                    .when(spec.dismiss, |el| {
                        el.child(
                            div()
                                .id("pill-dismiss")
                                .cursor_pointer()
                                .size(px(DISMISS_SIZE))
                                .rounded_full()
                                .hover(|el| el.bg(colors.key))
                                .on_click(on_dismiss)
                                .child(cross(colors.muted)),
                        )
                    }),
            )
        })
}

fn lead(lead: Lead, frame: &Frame, colors: &Colors) -> Div {
    match lead {
        Lead::Idle => div()
            .flex_shrink_0()
            .size(px(DOT_SIZE))
            .rounded_full()
            .bg(colors.dot),
        Lead::Listening => div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(GAP))
            .child(pulse(frame))
            .child(wave(frame.bars, colors.fg)),
        Lead::Thinking => div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(THINKING_GAP))
            .children((0..3).map(|index| {
                let opacity = if frame.animate {
                    blink(frame.time - index as f32 * 0.2)
                } else {
                    1.
                };
                div()
                    .size(px(THINKING_DOT))
                    .rounded_full()
                    .bg(rgb(ACCENT))
                    .opacity(opacity)
            })),
        Lead::Done => badge(rgb(ACCENT), "✓"),
        Lead::Caution => badge(colors.warning, "!"),
        Lead::Alert => badge(rgb(ERROR), "!"),
    }
}

/// Triangle wave in [0, 1] over one period.
fn triangle(time: f32, period: f32) -> f32 {
    let phase = (time / period).rem_euclid(1.);
    1. - (phase * 2. - 1.).abs()
}

fn blink(time: f32) -> f32 {
    1. - 0.75 * triangle(time, 1.)
}

fn pulse(frame: &Frame) -> Div {
    let core = DOT_SIZE;
    let center = PULSE_SIZE / 2.;
    let mut root = div().relative().flex_shrink_0().size(px(PULSE_SIZE));
    if frame.animate {
        let wave = triangle(frame.time, PULSE_PERIOD);
        let ring = core + (PULSE_SIZE - core) * wave;
        root = root.child(
            div()
                .absolute()
                .left(px(center - ring / 2.))
                .top(px(center - ring / 2.))
                .size(px(ring))
                .rounded_full()
                .bg(rgb(ACCENT))
                .opacity(0.45 * (1. - wave)),
        );
    }
    root.child(
        div()
            .absolute()
            .left(px(center - core / 2.))
            .top(px(center - core / 2.))
            .size(px(core))
            .rounded_full()
            .bg(rgb(ACCENT)),
    )
}

fn wave(bars: &[f32; WAVE_BARS], color: Rgba) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(BAR_GAP))
        .h(px(BAR_MAX))
        .children((0..WAVE_BARS).map(|index| {
            div()
                .w(px(BAR_WIDTH))
                .h(px(bar_height(index, bars[index])))
                .rounded(px(BAR_WIDTH / 2.))
                .bg(color)
        }))
}

/// The dismiss cross, drawn from the button's own center: a "×" glyph sits
/// on the font's baseline and lands off-center in the round hover state.
fn cross(color: Rgba) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let center = bounds.center();
            let arm = px(CROSS_SIZE / 2.);
            let mut builder = PathBuilder::stroke(px(1.5));
            builder.move_to(point(center.x - arm, center.y - arm));
            builder.line_to(point(center.x + arm, center.y + arm));
            builder.move_to(point(center.x + arm, center.y - arm));
            builder.line_to(point(center.x - arm, center.y + arm));
            if let Ok(path) = builder.build() {
                window.paint_path(path, color);
            }
        },
    )
    .size_full()
}

fn badge(color: Rgba, glyph: &'static str) -> Div {
    div()
        .flex_shrink_0()
        .size(px(BADGE_SIZE))
        .rounded_full()
        .bg(color)
        .flex()
        .items_center()
        .justify_center()
        .text_color(rgb(0xffffff))
        .text_size(px(12.))
        .font_weight(FontWeight::BOLD)
        .child(glyph)
}

fn visual_level(level: f32) -> f32 {
    ((level.clamp(0.0, 1.0) - 0.02) / 0.48)
        .clamp(0.0, 1.0)
        .sqrt()
}

fn bar_height(index: usize, level: f32) -> f32 {
    (BAR_MIN + WAVE_SHAPE[index] * visual_level(level) * (BAR_MAX - BAR_MIN)).clamp(2.0, BAR_MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_mode_gets_its_own_pill() {
        let dark = colors(Palette::Dark);
        let light = colors(Palette::Light);
        assert_eq!(dark.fill, rgb(0x141414));
        assert_eq!(dark.fg, rgb(0xffffff));
        assert_eq!(light.fill, rgb(0xf2f2f0));
        assert_eq!(light.fg, rgb(0x1d1d1f));
    }

    #[test]
    fn voice_bars_form_a_symmetric_waveform() {
        for i in 0..WAVE_BARS / 2 {
            assert_eq!(WAVE_SHAPE[i], WAVE_SHAPE[WAVE_BARS - 1 - i]);
        }
        const { assert!(WAVE_SHAPE[WAVE_BARS / 2] > WAVE_SHAPE[0]) };
    }

    #[test]
    fn voice_bars_are_flat_in_silence() {
        assert!((0..WAVE_BARS).all(|bar| bar_height(bar, 0.0) == BAR_MIN));
    }

    #[test]
    fn voice_bars_hold_still_on_a_steady_note() {
        // Pure function of level: repeated renders are identical, no drift.
        for bar in 0..WAVE_BARS {
            assert_eq!(bar_height(bar, 0.4), bar_height(bar, 0.4));
        }
        assert!(bar_height(5, 0.5) > bar_height(0, 0.5));
        assert_eq!(bar_height(5, 0.5), bar_height(6, 0.5));
    }

    #[test]
    fn voice_bars_stay_within_the_pill() {
        for bar in 0..WAVE_BARS {
            for level in [0.0, 0.5, 1.0] {
                assert!((2.0..=BAR_MAX).contains(&bar_height(bar, level)));
            }
        }
    }

    #[test]
    fn visual_level_amplifies_normal_speech_without_animating_noise() {
        assert_eq!(visual_level(0.0), 0.0);
        assert_eq!(visual_level(0.02), 0.0);
        assert!(visual_level(0.1) > 0.4);
        assert_eq!(visual_level(0.5), 1.0);
        assert_eq!(visual_level(1.0), 1.0);
    }

    #[test]
    fn indicators_loop_smoothly() {
        assert_eq!(triangle(0., 1.6), 0.);
        assert!((triangle(0.8, 1.6) - 1.).abs() < 1e-6);
        assert!(triangle(1.6, 1.6) < 1e-6);
        assert!((blink(0.) - 1.).abs() < 1e-6);
        assert!((blink(0.5) - 0.25).abs() < 1e-6);
    }
}
