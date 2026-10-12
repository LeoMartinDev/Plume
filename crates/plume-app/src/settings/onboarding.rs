use crate::settings::i18n::tr;
use gpui::{
    canvas, div, fill, prelude::*, px, relative, svg, Animation, AnimationExt, AnyElement, Context,
    FontWeight, SharedString, Window,
};
use plume_engine::Engine;
use plume_overlay::Feedback;
use plume_session::{PreviewEvent, SessionMode};
use plume_ui::{keycaps, Button, ButtonSize, KeySize, Tokens, FONT_MONO, FONT_SANS};

use super::view::{display_title, error_text, prose};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Duration;

use super::SettingsView;
use crate::catalog::ModelId;
use crate::permissions::{self, Permission, Permissions};
use crate::phase::{AppPhase, OnboardStatus};
use crate::prefs::{OnboardingStep, Prefs, ONBOARDING_VERSION};

pub(super) const CHOICES: [ModelId; 3] = [
    ModelId::WhisperBase,
    ModelId::Nemotron35Compact,
    ModelId::WhisperLargeV3Turbo,
];

pub(super) struct OnboardingState {
    pub engine: Option<Engine>,
    pub model_ready: bool,
    permissions: Permissions,
    microphone_prompt: Option<Receiver<()>>,
    microphone_ready: bool,
    microphone_checking: bool,
    pub session_starting: bool,
    pub session_error: Option<String>,
    feedback: Feedback,
    transcript: String,
    finished: bool,
    visible: bool,
    /// Onboarding was done before; macOS no longer grants what it needs.
    permissions_lost: bool,
}

impl OnboardingState {
    pub fn new(_prefs: &Prefs, permissions_lost: bool) -> Self {
        Self {
            engine: None,
            model_ready: false,
            permissions: permissions::snapshot(),
            microphone_prompt: None,
            microphone_ready: false,
            microphone_checking: false,
            session_starting: false,
            session_error: None,
            feedback: Feedback::Empty,
            transcript: String::new(),
            finished: false,
            visible: true,
            permissions_lost,
        }
    }
}

fn model_busy(phase: &AppPhase) -> bool {
    matches!(
        phase,
        AppPhase::Onboarding {
            status: OnboardStatus::Fetching { .. } | OnboardStatus::Activating,
            ..
        }
    )
}

impl SettingsView {
    pub(super) fn onboarding_model_loaded(
        &mut self,
        engine: Engine,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(setup) = &mut self.onboarding else {
            return false;
        };
        setup.engine = Some(engine.clone());
        setup.model_ready = true;
        setup.session_starting = false;
        setup.session_error = None;
        self.downloads.clear();
        if let Some(target) = &self.engine_target {
            target.set(engine);
            self.active_model = Some(self.phase.prefs().model);
        }
        self.phase = AppPhase::Onboarding {
            prefs: self.phase.prefs().clone(),
            status: OnboardStatus::Idle,
            warning: None,
        };
        cx.notify();
        true
    }

    pub(super) fn onboarding_model_failed(&mut self) {
        if let Some(setup) = &mut self.onboarding {
            setup.model_ready = false;
            setup.engine = None;
            if self.phase.prefs().onboarding_step != OnboardingStep::Languages {
                self.phase.prefs_mut().onboarding_step = OnboardingStep::Model;
            }
            self.save_prefs();
        }
    }

    fn select_model(&mut self, id: ModelId, cx: &mut Context<Self>) {
        if model_busy(&self.phase) || self.is_busy() || !id.supports(self.phase.prefs().language())
        {
            return;
        }
        if self.phase.prefs().model == id {
            return;
        }
        if let Some(setup) = &mut self.onboarding {
            setup.engine = None;
            setup.model_ready = false;
        }
        self.active_model = None;
        self.phase.prefs_mut().model = id;
        self.phase = AppPhase::Onboarding {
            prefs: self.phase.prefs().clone(),
            status: OnboardStatus::Idle,
            warning: None,
        };
        self.save_prefs();
        cx.notify();
    }

    fn download_model(&mut self, cx: &mut Context<Self>) {
        if model_busy(&self.phase)
            || self.is_busy()
            || self.settings_preview
            || !self
                .phase
                .prefs()
                .model
                .supports(self.phase.prefs().language())
        {
            return;
        }
        self.save_prefs();
        if self.save_error.is_some() {
            cx.notify();
            return;
        }
        let prefs = self.phase.prefs().clone();
        let request = self.begin_download(prefs.clone(), cx);
        cx.defer(move |cx| crate::begin_pack(cx, prefs, request));
    }

    fn is_busy(&self) -> bool {
        self.session_control
            .as_ref()
            .is_some_and(|control| control.is_busy())
    }

    fn can_finish(&self) -> bool {
        self.onboarding.as_ref().is_some_and(|setup| {
            setup.model_ready
                && setup.permissions.granted()
                && setup.microphone_ready
                && !setup.finished
                && self.session_control.is_some()
                && setup.session_error.is_none()
        }) && self
            .phase
            .prefs()
            .model
            .supports(self.phase.prefs().language())
            && !self.is_busy()
            && self.save_error.is_none()
    }

    fn change_step(&mut self, step: OnboardingStep, cx: &mut Context<Self>) {
        if self.is_busy() || model_busy(&self.phase) {
            return;
        }
        self.reset_for_phase();
        let previous_mode = self.session_control.as_ref().map(|control| control.mode());
        if let Some(control) = &self.session_control {
            if step == OnboardingStep::Try {
                let prefs = self.phase.prefs();
                let result = plume_session::Config::from_prefs(
                    prefs.hold(),
                    prefs.cancel(),
                    prefs.model.data_dir(),
                )
                .and_then(|config| config.with_toggle(prefs.toggle()))
                .map_err(|error| error.to_string())
                .and_then(|config| control.configure_shortcuts(config));
                if let Err(error) = result {
                    self.onboarding.as_mut().unwrap().session_error = Some(error);
                    cx.notify();
                    return;
                }
                self.onboarding.as_mut().unwrap().session_error = None;
            }
            let mode = if step == OnboardingStep::Try {
                SessionMode::Preview
            } else {
                SessionMode::Suspended
            };
            if let Err(error) = control.set_mode(mode) {
                self.onboarding.as_mut().unwrap().session_error = Some(error);
                cx.notify();
                return;
            }
        }
        let previous = self.phase.prefs().onboarding_step;
        self.phase.prefs_mut().onboarding_step = step;
        self.save_prefs();
        if self.save_error.is_some() {
            self.phase.prefs_mut().onboarding_step = previous;
            if let (Some(control), Some(mode)) = (&self.session_control, previous_mode) {
                if let Err(error) = control.set_mode(mode) {
                    self.onboarding.as_mut().unwrap().session_error = Some(error);
                }
            }
        }
        cx.notify();
    }

    fn finish_setup(&mut self, cx: &mut Context<Self>) {
        if !self.can_finish() {
            return;
        }
        let mut completed = self.phase.prefs().clone();
        completed.onboarding_version = ONBOARDING_VERSION;
        let result =
            self.session_control
                .as_ref()
                .unwrap()
                .set_mode_after(SessionMode::System, || {
                    crate::prefs::save_at(&self.prefs_path, &completed)
                        .map_err(|error| error.to_string())
                });
        if let Err(error) = result {
            self.save_prefs();
            self.onboarding.as_mut().unwrap().session_error = Some(error);
            cx.notify();
            return;
        }
        *self.phase.prefs_mut() = completed;
        self.phase = AppPhase::Live {
            prefs: self.phase.prefs().clone(),
        };
        self.onboarding.as_mut().unwrap().finished = true;
        cx.notify();
        cx.defer(super::window::finish_onboarding_window);
    }

    pub(super) fn onboarding_preview_event(&mut self, event: PreviewEvent, cx: &mut Context<Self>) {
        if let Some(setup) = &mut self.onboarding {
            match event {
                PreviewEvent::Transcript(text) => setup.transcript = text,
                PreviewEvent::Feedback(feedback) => {
                    if feedback == Feedback::Starting {
                        setup.transcript.clear();
                    }
                    setup.feedback = feedback;
                }
            }
            cx.notify();
        }
    }

    pub(super) fn onboarding_visibility(&mut self, visible: bool, cx: &mut Context<Self>) {
        let Some(setup) = &mut self.onboarding else {
            return;
        };
        setup.visible = visible;
        if !setup.finished && !self.is_busy() {
            if let Some(control) = &self.session_control {
                let mode = if visible && self.phase.prefs().onboarding_step == OnboardingStep::Try {
                    SessionMode::Preview
                } else {
                    SessionMode::Suspended
                };
                let _ = control.set_mode(mode);
            }
        }
        cx.notify();
    }

    pub(super) fn poll_onboarding(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(350))
                .await;
            let keep_polling = this
                .update(cx, |view, cx| view.refresh_onboarding(cx))
                .unwrap_or(false);
            if !keep_polling {
                break;
            }
        })
        .detach();
    }

    fn refresh_onboarding(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(setup) = &mut self.onboarding else {
            return false;
        };
        if setup.finished {
            return false;
        }
        if let Some(completion) = &setup.microphone_prompt {
            match completion.try_recv() {
                Ok(()) => {
                    setup.microphone_prompt = None;
                    // AVFoundation's callback runs off the UI thread. Restore
                    // focus once the prompt has closed, after this view update.
                    let view = cx.weak_entity();
                    cx.defer(move |cx| {
                        let visible = view.upgrade().is_some_and(|view| {
                            view.read(cx)
                                .onboarding
                                .as_ref()
                                .is_some_and(|setup| setup.visible && !setup.finished)
                        });
                        if visible {
                            super::window::show_settings(cx);
                        }
                    });
                }
                Err(TryRecvError::Disconnected) => setup.microphone_prompt = None,
                Err(TryRecvError::Empty) => {}
            }
        }
        setup.permissions = permissions::snapshot();
        if !setup.permissions.granted() {
            setup.microphone_ready = false;
        }
        let visible = setup.visible;
        if !self.is_busy() {
            if let Some(control) = &self.session_control {
                let setup = self.onboarding.as_ref().unwrap();
                let mode = if visible
                    && setup.permissions.granted()
                    && self
                        .phase
                        .prefs()
                        .model
                        .supports(self.phase.prefs().language())
                    && self.phase.prefs().onboarding_step == OnboardingStep::Try
                {
                    SessionMode::Preview
                } else {
                    SessionMode::Suspended
                };
                if control.mode() != mode {
                    let _ = control.set_mode(mode);
                }
            }
        }
        let setup = self.onboarding.as_mut().unwrap();
        if !self.settings_preview
            && visible
            && self.phase.prefs().onboarding_step == OnboardingStep::Try
            && setup.model_ready
            && self
                .phase
                .prefs()
                .model
                .supports(self.phase.prefs().language())
            && setup.permissions.granted()
        {
            if !setup.microphone_ready
                && !setup.microphone_checking
                && setup.session_error.is_none()
            {
                setup.microphone_checking = true;
                let device = self
                    .phase
                    .prefs()
                    .microphone()
                    .map(|microphone| microphone.id.clone());
                cx.spawn(async move |this, cx| {
                    let result = cx
                        .background_executor()
                        .spawn(async move {
                            let microphone = plume_audio::Mic::open(device.as_deref())
                                .map_err(|error| error.to_string())?;
                            drop(microphone);
                            Ok::<_, String>(())
                        })
                        .await;
                    let _ = this.update(cx, |view, cx| {
                        if let Some(setup) = &mut view.onboarding {
                            setup.microphone_checking = false;
                            match result {
                                Ok(()) => {
                                    setup.microphone_ready = true;
                                }
                                Err(error) => {
                                    setup.session_error =
                                        Some(format!("Microphone unavailable: {error}"))
                                }
                            }
                            cx.notify();
                        }
                    });
                })
                .detach();
            } else if setup.microphone_ready
                && !setup.session_starting
                && setup.session_error.is_none()
                && self.session_control.is_none()
            {
                if let Some(engine) = setup.engine.clone() {
                    setup.session_starting = true;
                    let prefs = self.phase.prefs().clone();
                    cx.defer(move |cx| {
                        crate::start_session(cx, prefs, engine, SessionMode::Preview)
                    });
                }
            }
        }
        cx.notify();
        true
    }
}

/// Only macOS asks for permissions; elsewhere they are always granted.
#[cfg(target_os = "macos")]
const STEPS: &[(OnboardingStep, &str)] = &[
    (OnboardingStep::Languages, "onboarding.step.languages"),
    (OnboardingStep::Model, "onboarding.step.model"),
    (OnboardingStep::Shortcuts, "onboarding.step.shortcuts"),
    (OnboardingStep::Permissions, "onboarding.step.permissions"),
    (OnboardingStep::Try, "onboarding.step.try"),
];
#[cfg(not(target_os = "macos"))]
const STEPS: &[(OnboardingStep, &str)] = &[
    (OnboardingStep::Languages, "onboarding.step.languages"),
    (OnboardingStep::Model, "onboarding.step.model"),
    (OnboardingStep::Shortcuts, "onboarding.step.shortcuts"),
    (OnboardingStep::Try, "onboarding.step.try"),
];

fn adjacent_step(step: OnboardingStep, offset: isize) -> OnboardingStep {
    let index = STEPS
        .iter()
        .position(|(item, _)| *item == step)
        .unwrap_or(0);
    let index = index.saturating_add_signed(offset).min(STEPS.len() - 1);
    STEPS[index].0
}

pub(super) fn render(
    view: &SettingsView,
    tokens: &Tokens,
    cx: &mut Context<SettingsView>,
) -> AnyElement {
    let setup = view.onboarding.as_ref().unwrap();
    let step = view.phase.prefs().onboarding_step;
    let shell = div()
        .id("onboarding-shell")
        .tab_group()
        .size_full()
        .flex()
        .flex_col()
        .overflow_hidden()
        .bg(tokens.bg)
        .font_family(FONT_SANS)
        .text_color(tokens.text)
        .when(!view.capture.is_listening(), |el| {
            el.track_focus(&view.hold_focus)
        })
        .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
            if !view.capture.is_listening() && event.keystroke.key == "tab" {
                cx.stop_propagation();
                window.prevent_default();
                if event.keystroke.modifiers.shift {
                    window.focus_prev();
                } else {
                    window.focus_next();
                }
            }
        }))
        .when(view.settings_preview, |el| {
            el.on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
                if !view.capture.is_listening() && event.keystroke.modifiers.control {
                    if event.keystroke.key == "5" {
                        cx.stop_propagation();
                        window.prevent_default();
                        cx.defer(super::window::finish_onboarding_window);
                        return;
                    }
                    let step = match event.keystroke.key.as_str() {
                        "0" => Some(OnboardingStep::Languages),
                        "1" => Some(OnboardingStep::Model),
                        "2" => Some(OnboardingStep::Shortcuts),
                        "3" => Some(OnboardingStep::Permissions),
                        "4" => Some(OnboardingStep::Try),
                        _ => None,
                    };
                    if let Some(step) = step {
                        cx.stop_propagation();
                        window.prevent_default();
                        view.phase.prefs_mut().onboarding_step = step;
                        cx.notify();
                    }
                }
            }))
        });
    #[cfg(target_os = "macos")]
    let shell = shell.child(super::view::macos_titlebar(tokens));
    let body = match step {
        OnboardingStep::Languages => languages_screen(view, tokens, cx),
        OnboardingStep::Model => model_screen(view, tokens, cx),
        OnboardingStep::Shortcuts => shortcuts_screen(view, tokens, cx),
        OnboardingStep::Permissions => permissions_screen(view, tokens, cx),
        OnboardingStep::Try => try_screen(view, tokens, cx),
    };
    let busy = model_busy(&view.phase);
    let next_enabled = match step {
        OnboardingStep::Languages => !busy,
        OnboardingStep::Model => {
            view.phase
                .prefs()
                .model
                .supports(view.phase.prefs().language())
                && (setup.model_ready || (!busy && !view.settings_preview))
        }
        OnboardingStep::Shortcuts => {
            !view.capture.is_listening() && view.capture.reject().is_none()
        }
        OnboardingStep::Permissions => setup.permissions.granted(),
        OnboardingStep::Try => view.can_finish(),
    } && !view.is_busy()
        && view.save_error.is_none();
    let downloads = step == OnboardingStep::Model && !setup.model_ready && !busy;
    let next_label = match step {
        OnboardingStep::Model if busy => tr("ui.preparing_progress"),
        OnboardingStep::Model if !setup.model_ready => {
            if crate::catalog::is_complete(
                view.phase.prefs().model,
                &view.phase.prefs().model.data_dir(),
            ) {
                tr("ui.load_model")
            } else {
                tr("ui.download")
            }
        }
        OnboardingStep::Try => tr("common.finish"),
        _ => tr("common.next"),
    };
    let next = Button::new(*tokens, "onboarding-next", next_label)
        .primary()
        .size(ButtonSize::Large)
        .enabled(next_enabled)
        .map(|button| {
            if downloads {
                button.icon("ui/download.svg")
            } else if step == OnboardingStep::Try {
                button
            } else {
                button.trailing_icon("ui/arrow-right.svg")
            }
        })
        .on_click(cx, move |view, _, cx| match step {
            OnboardingStep::Model if !view.onboarding.as_ref().unwrap().model_ready => {
                view.download_model(cx)
            }
            OnboardingStep::Try => view.finish_setup(cx),
            _ => view.change_step(adjacent_step(step, 1), cx),
        });
    let back = (step != OnboardingStep::Languages).then(|| {
        Button::new(*tokens, "onboarding-back", tr("common.back"))
            .ghost()
            .size(ButtonSize::Large)
            .icon("ui/arrow-left.svg")
            .enabled(!view.is_busy() && !setup.session_starting)
            .on_click(cx, move |view, _, cx| {
                view.change_step(adjacent_step(step, -1), cx)
            })
    });
    let header = div()
        .h(px(56.))
        .flex_shrink_0()
        .px(px(24.))
        .flex()
        .items_center()
        .child(
            div()
                .flex_1()
                .flex()
                .items_center()
                .gap(px(9.))
                .text_size(px(15.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(super::view::brand_mark(tokens, 24.))
                .child("Plume"),
        )
        .child(step_island(step, tokens))
        .child(div().flex_1());
    let content = div()
        .id("onboarding-content")
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .flex()
        .flex_col()
        .px(px(32.))
        .py(px(12.))
        .child(
            div()
                .flex_1()
                .flex_shrink_0()
                .flex()
                .flex_col()
                .justify_center()
                .child(body),
        )
        .children(view.capture.reject().map(|error| {
            div()
                .mt(px(12.))
                .child(error_text(tokens, error.to_string()))
        }))
        .children(view.save_error.clone().map(|error| {
            div()
                .mt(px(12.))
                .flex()
                .items_center()
                .gap(px(10.))
                .child(div().flex_1().child(error_text(tokens, error)))
                .child(
                    Button::new(*tokens, "onboarding-save-retry", tr("ui.retry_saving"))
                        .size(ButtonSize::Small)
                        .on_click(cx, |view, _, cx| {
                            view.save_prefs();
                            cx.notify();
                        }),
                )
        }));
    let footer = div()
        .h(px(72.))
        .flex_shrink_0()
        .px(px(24.))
        .flex()
        .items_center()
        .justify_between()
        .border_t_1()
        .border_color(tokens.border)
        .child(div().children(back))
        .child(next);
    shell
        .child(header)
        .child(content)
        .child(footer)
        .into_any_element()
}

/// The steps in a floating pill, like the website's navbar island.
fn step_island(step: OnboardingStep, tokens: &Tokens) -> AnyElement {
    let current = STEPS
        .iter()
        .position(|(item, _)| *item == step)
        .unwrap_or(0);
    div()
        .id("onboarding-progress")
        .flex()
        .items_center()
        .gap(px(2.))
        .p(px(4.))
        .rounded_full()
        .border_1()
        .border_color(tokens.border_strong)
        .bg(tokens.island)
        .shadow(tokens.float())
        .text_size(px(12.5))
        .children(STEPS.iter().enumerate().map(|(index, (_, name))| {
            let item = div()
                .h(px(28.))
                .px(px(12.))
                .flex()
                .items_center()
                .gap(px(7.))
                .rounded_full();
            if index < current {
                item.text_color(tokens.muted)
                    .child(
                        svg()
                            .path("ui/check.svg")
                            .size(px(11.))
                            .text_color(tokens.accent_text),
                    )
                    .child(tr(name))
            } else {
                let active = index == current;
                // One line of text with two runs, so the number and the name share a baseline.
                let number = format!("{:02} ", index + 1);
                let name = tr(name);
                let mut name_font = gpui::font(FONT_SANS);
                if active {
                    name_font.weight = FontWeight::MEDIUM;
                }
                let run = |len, font, color: gpui::Rgba| gpui::TextRun {
                    len,
                    font,
                    color: color.into(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let runs = vec![
                    run(
                        number.len(),
                        gpui::font(FONT_MONO),
                        if active {
                            tokens.accent_text
                        } else {
                            tokens.faint
                        },
                    ),
                    run(
                        name.len(),
                        name_font,
                        if active { tokens.text } else { tokens.faint },
                    ),
                ];
                item.when(active, |el| el.bg(tokens.hover))
                    .child(gpui::StyledText::new(format!("{number}{name}")).with_runs(runs))
            }
        }))
        .into_any_element()
}

/// Two-line title on the left, a short lead on the right.
fn step_heading(
    tokens: &Tokens,
    headline: &str,
    serif: &str,
    lead: Option<&str>,
) -> impl IntoElement {
    div()
        .flex()
        .items_end()
        .justify_between()
        .gap(px(24.))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(10.))
                .child(display_title(tokens, headline, serif, 28.)),
        )
        .children(lead.map(|lead| {
            div()
                .max_w(px(240.))
                .text_size(px(13.))
                .line_height(px(19.5))
                .text_color(tokens.muted)
                .child(prose(lead))
        }))
}

fn languages_screen(
    view: &SettingsView,
    tokens: &Tokens,
    cx: &mut Context<SettingsView>,
) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(px(32.))
        .px(px(8.))
        .child(
            div()
                .w(px(270.))
                .flex_shrink_0()
                .flex()
                .flex_col()
                .gap(px(12.))
                .child(display_title(
                    tokens,
                    tr("onboarding.languages.headline"),
                    tr("onboarding.languages.serif"),
                    30.,
                ))
                .child(
                    div()
                        .mt(px(4.))
                        .text_size(px(14.))
                        .line_height(px(21.))
                        .text_color(tokens.muted)
                        .child(prose(tr("onboarding.languages.description"))),
                ),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(super::languages::language_choices(view, tokens, cx)),
        )
        .into_any_element()
}

fn model_screen(
    view: &SettingsView,
    tokens: &Tokens,
    cx: &mut Context<SettingsView>,
) -> AnyElement {
    let selected = view.phase.prefs().model;
    let busy = model_busy(&view.phase);
    let mut body = div()
        .flex()
        .flex_col()
        .gap(px(18.))
        .child(step_heading(
            tokens,
            tr("onboarding.model.headline"),
            tr("onboarding.model.serif"),
            None,
        ))
        .child(
            div().flex().gap(px(12.)).children(
                CHOICES
                    .into_iter()
                    .enumerate()
                    .map(|(index, id)| model_card(view, tokens, index, id, selected, busy, cx)),
            ),
        );
    if !CHOICES.contains(&selected) {
        body = body.child(
            div()
                .text_size(px(12.5))
                .text_color(tokens.subtle)
                .child(format!(
                    "{} {}",
                    tr("ui.current_model"),
                    selected.entry().name
                )),
        );
    }
    // A model chosen in Settings (e.g. Whisper Small) has no card in this grid.
    if busy && !CHOICES.contains(&selected) {
        body = body.child(model_download_progress(&view.phase, tokens, false));
    }
    if let AppPhase::Onboarding {
        status: OnboardStatus::Failed { reason },
        ..
    } = &view.phase
    {
        body = body.child(error_text(tokens, reason.clone()));
    }
    if let AppPhase::Onboarding {
        warning: Some(warning),
        ..
    } = &view.phase
    {
        body = body.child(error_text(tokens, warning.clone()));
    }
    body.children(super::languages::compatibility_warning(view, tokens))
        .into_any_element()
}

/// One model. The chosen card inverts, like the website's highlighted feature card.
fn model_card(
    view: &SettingsView,
    tokens: &Tokens,
    index: usize,
    id: ModelId,
    selected: ModelId,
    busy: bool,
    cx: &mut Context<SettingsView>,
) -> impl IntoElement {
    let entry = id.entry();
    let supported = id.supports(view.phase.prefs().language());
    let chosen = selected == id;
    let nvidia = id == ModelId::Nemotron35Compact;
    let (fg, soft) = if chosen {
        (tokens.invert_fg, tokens.invert_muted)
    } else {
        (tokens.text, tokens.subtle)
    };
    let tokens_ = *tokens;
    div()
        .id(SharedString::from(format!(
            "onboarding-model-{}",
            id.as_str()
        )))
        .flex_1()
        .min_w_0()
        .h(px(228.))
        .pt(px(16.))
        .px(px(16.))
        .pb(px(18.))
        .flex()
        .flex_col()
        .rounded(px(20.))
        .border_1()
        .border_color(if chosen {
            tokens.invert_bg
        } else {
            tokens.border
        })
        .bg(if chosen {
            tokens.invert_bg
        } else {
            tokens.surface
        })
        .text_color(fg)
        .when((busy && !chosen) || !supported, |el| el.opacity(0.45))
        .when(!busy && supported, |el| {
            el.cursor_pointer()
                .tab_index(0)
                .when(!chosen, |el| {
                    el.hover(move |style| style.border_color(tokens_.faint))
                })
                .on_click(cx.listener(move |view, _, _, cx| view.select_model(id, cx)))
                .on_key_down(
                    cx.listener(move |view, event: &gpui::KeyDownEvent, window, cx| {
                        if matches!(event.keystroke.key.as_str(), "enter" | "space")
                            && !event.is_held
                        {
                            cx.stop_propagation();
                            window.prevent_default();
                            view.select_model(id, cx);
                        }
                    }),
                )
        })
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .px(px(10.))
                        .py(px(4.))
                        .rounded_full()
                        .bg(if chosen {
                            tokens.invert_chip
                        } else {
                            tokens.surface_2
                        })
                        .text_size(px(12.))
                        .text_color(if chosen {
                            tokens.invert_fg
                        } else {
                            tokens.muted
                        })
                        .child(if nvidia {
                            tr("ui.recommended")
                        } else {
                            [tr("ui.small"), tr("ui.medium"), tr("ui.large")][index]
                        }),
                )
                .child(
                    div()
                        .size(px(18.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .when(chosen, |el| {
                            el.bg(tokens.invert_fg).child(
                                svg()
                                    .path("ui/check.svg")
                                    .size(px(10.))
                                    .text_color(tokens.invert_bg),
                            )
                        })
                        .when(!chosen, |el| {
                            el.border_1().border_color(tokens.border_strong)
                        }),
                ),
        )
        .child(
            div()
                .mt(px(18.))
                .font_family(FONT_MONO)
                .text_size(px(11.))
                .text_color(soft)
                .child(if nvidia { "NVIDIA" } else { "OPENAI" }),
        )
        .child(
            div()
                .mt(px(4.))
                .min_h(px(42.))
                .text_size(px(18.))
                .line_height(px(21.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(entry.name),
        )
        .child(
            div()
                .mt(px(6.))
                .font_family(FONT_MONO)
                .text_size(px(12.))
                .text_color(soft)
                .child(entry.size),
        )
        .child(div().flex_1())
        .child(if busy && chosen {
            model_download_progress(&view.phase, tokens, true)
        } else if !supported {
            div()
                .text_size(px(12.))
                .text_color(soft)
                .child(tr("ui.language_not_supported"))
                .into_any_element()
        } else {
            div()
                .flex()
                .flex_col()
                .gap(px(8.))
                .child(super::model::spec_meter(
                    tr("ui.speed"),
                    entry.speed,
                    tokens,
                    chosen,
                ))
                .child(super::model::spec_meter(
                    tr("ui.accuracy"),
                    entry.accuracy,
                    tokens,
                    chosen,
                ))
                .into_any_element()
        })
}

fn model_download_progress(phase: &AppPhase, tokens: &Tokens, inverted: bool) -> AnyElement {
    let progress = match phase {
        AppPhase::Onboarding {
            status: OnboardStatus::Fetching { last },
            ..
        } => Some(last),
        _ => None,
    };
    let total = progress.and_then(|last| last.total.filter(|total| *total > 0));
    let fraction = progress
        .zip(total)
        .map(|(last, total)| (last.bytes as f32 / total as f32).clamp(0., 1.));
    let label = if progress.is_none() {
        tr("ui.loading_model")
    } else if progress.is_some_and(|last| last.bytes == 0) && total.is_none() {
        tr("ui.connecting_progress")
    } else {
        tr("ui.downloading")
    };
    let detail = match progress {
        Some(last) => match total {
            Some(total) => format!(
                "{:.0} / {:.0} MB",
                last.bytes as f64 / 1_048_576.,
                total as f64 / 1_048_576.,
            ),
            None if last.bytes > 0 => {
                format!(
                    "{} {}",
                    super::model::format_bytes(last.bytes),
                    tr("ui.received")
                )
            }
            None => tr("ui.waiting_for_the_download").into(),
        },
        None => tr("ui.getting_ready_to_transcribe").into(),
    };
    let (bar, track, soft) = if inverted {
        (tokens.invert_fg, tokens.invert_track, tokens.invert_muted)
    } else {
        (tokens.accent, tokens.track, tokens.subtle)
    };
    let fill = div().h_full().rounded_full().bg(bar);
    let fill = if let Some(fraction) = fraction {
        fill.w(relative(fraction)).into_any_element()
    } else {
        fill.w(relative(0.35))
            .with_animation(
                "onboarding-download-pending",
                Animation::new(Duration::from_secs(2)).repeat(),
                |el, delta| el.opacity(0.4 + 0.6 * (delta * std::f32::consts::PI).sin()),
            )
            .into_any_element()
    };
    div()
        .id("onboarding-model-download")
        .w_full()
        .flex()
        .flex_col()
        .gap(px(8.))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .text_size(px(12.))
                .child(label)
                .children(fraction.map(|fraction| {
                    div()
                        .font_family(FONT_MONO)
                        .child(format!("{:.0} %", fraction * 100.))
                })),
        )
        .child(
            div()
                .h(px(4.))
                .flex_shrink_0()
                .rounded_full()
                .overflow_hidden()
                .bg(track)
                .child(fill),
        )
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(8.))
                .font_family(FONT_MONO)
                .text_size(px(11.))
                .text_color(soft)
                .child(div().child(detail))
                .children(
                    progress
                        .and_then(|last| super::model::progress_speed(last.bytes_per_second))
                        .map(|speed| div().flex_shrink_0().whitespace_nowrap().child(speed)),
                ),
        )
        .into_any_element()
}

fn shortcuts_screen(
    view: &SettingsView,
    tokens: &Tokens,
    cx: &mut Context<SettingsView>,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(20.))
        .child(step_heading(
            tokens,
            tr("onboarding.shortcuts.headline"),
            tr("onboarding.shortcuts.serif"),
            Some(tr("onboarding.shortcuts.lead")),
        ))
        .child(
            div()
                .flex()
                .gap(px(12.))
                .child(shortcut_card(view, tokens, false, cx))
                .child(shortcut_card(view, tokens, true, cx)),
        )
        .children(
            view.onboarding
                .as_ref()
                .and_then(|setup| setup.session_error.clone())
                .map(|error| error_text(tokens, error)),
        )
        .into_any_element()
}

fn shortcut_card(
    view: &SettingsView,
    tokens: &Tokens,
    toggle: bool,
    cx: &mut Context<SettingsView>,
) -> AnyElement {
    let listening = view.capture.is_listening() && view.capture_toggle == toggle;
    let shortcut = if toggle {
        view.phase.prefs().toggle()
    } else {
        Some(view.phase.prefs().hold())
    };
    let keys = if listening {
        div()
            .h(px(44.))
            .px(px(16.))
            .flex()
            .items_center()
            .gap(px(10.))
            .rounded_full()
            .bg(tokens.highlight)
            .border_1()
            .border_color(tokens.accent_ring)
            .text_size(px(13.5))
            .text_color(tokens.accent_text)
            .child(
                div()
                    .size(px(8.))
                    .rounded_full()
                    .bg(tokens.accent)
                    .with_animation(
                        "onboarding-capture-pulse",
                        Animation::new(Duration::from_millis(1600)).repeat(),
                        |el, delta| el.opacity(1. - 0.6 * (delta * std::f32::consts::PI).sin()),
                    ),
            )
            .child(SharedString::from(
                view.capture
                    .preview()
                    .unwrap_or(tr("ui.press_keys_progress"))
                    .to_string(),
            ))
            .into_any_element()
    } else {
        match shortcut {
            Some(shortcut) => keycaps(tokens, shortcut, KeySize::Large),
            None => div()
                .text_size(px(14.))
                .text_color(tokens.subtle)
                .child(tr("ui.disabled"))
                .into_any_element(),
        }
    };
    div()
        .id(if toggle {
            "onboarding-toggle-card"
        } else {
            "onboarding-hold-card"
        })
        .flex_1()
        .min_w_0()
        .h(px(236.))
        .pt(px(22.))
        .px(px(20.))
        .pb(px(16.))
        .flex()
        .flex_col()
        .rounded(px(20.))
        .border_1()
        .border_color(if listening {
            tokens.accent
        } else {
            tokens.border
        })
        .bg(tokens.surface)
        .when(listening, |el| {
            el.shadow(tokens.ring())
                .track_focus(&view.hold_focus)
                .on_key_down(cx.listener(SettingsView::on_hold_key))
                .on_key_up(cx.listener(SettingsView::on_hold_key_up))
                .on_modifiers_changed(cx.listener(SettingsView::on_hold_modifiers))
        })
        .child(
            div()
                .h(px(64.))
                .flex_shrink_0()
                .flex()
                .items_center()
                .child(keys),
        )
        .child(
            div()
                .mt(px(16.))
                .text_size(px(19.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(if toggle {
                    tr("ui.hands_free")
                } else {
                    tr("ui.push_to_talk")
                }),
        )
        .child(
            div()
                .mt(px(5.))
                .min_h(px(39.))
                .text_size(px(13.))
                .line_height(px(19.5))
                .text_color(tokens.muted)
                .child(prose(if toggle {
                    tr("onboarding.shortcuts.toggle_text")
                } else {
                    tr("onboarding.shortcuts.hold_text")
                })),
        )
        .child(div().flex_1())
        .child(
            div()
                .mt(px(16.))
                .flex()
                .items_center()
                .gap(px(4.))
                .child(
                    Button::new(
                        *tokens,
                        if toggle {
                            "onboarding-toggle-change"
                        } else {
                            "onboarding-hold-change"
                        },
                        if listening {
                            tr("common.cancel")
                        } else {
                            tr("ui.change")
                        },
                    )
                    .size(ButtonSize::Small)
                    .on_click(cx, move |view, window, cx| {
                        view.edit_onboarding_shortcut(toggle, window, cx);
                    }),
                )
                .child(
                    Button::new(
                        *tokens,
                        if toggle {
                            "onboarding-toggle-reset"
                        } else {
                            "onboarding-hold-reset"
                        },
                        tr("ui.reset"),
                    )
                    .ghost()
                    .size(ButtonSize::Small)
                    .on_click(cx, move |view, window, cx| {
                        view.capture_toggle = toggle;
                        view.reset_hold(window, cx);
                    }),
                ),
        )
        .into_any_element()
}

impl SettingsView {
    fn edit_onboarding_shortcut(
        &mut self,
        toggle: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.capture.is_listening() && self.capture_toggle != toggle {
            self.capture.cancel();
            self.shortcut_edit.take();
        }
        self.capture_toggle = toggle;
        self.toggle_hold_capture(window, cx);
    }
}

/// What the bubble on the test stage shows.
#[derive(Clone, Copy, PartialEq)]
enum StagePill {
    Idle,
    Listening,
    Thinking,
    Done,
}

fn try_screen(view: &SettingsView, tokens: &Tokens, cx: &mut Context<SettingsView>) -> AnyElement {
    let setup = view.onboarding.as_ref().unwrap();
    let ready = view.can_finish();
    let microphone = if setup.permissions.microphone == Permission::Granted {
        permission_allowed(
            tokens,
            "ui/mic.svg",
            tr("ui.microphone"),
            if setup.microphone_checking {
                tr("ui.checking_progress")
            } else {
                tr("ui.allowed")
            },
        )
    } else {
        let label = if setup.microphone_checking {
            tr("ui.checking_progress")
        } else if setup.permissions.microphone == Permission::Denied {
            tr("ui.settings")
        } else {
            tr("ui.allow")
        };
        Button::new(
            *tokens,
            "onboarding-microphone",
            format!("{} · {}", tr("ui.microphone"), label),
        )
        .size(ButtonSize::Small)
        .icon("ui/mic.svg")
        .enabled(view.can_request_microphone())
        .on_click(cx, |view, _, cx| view.request_microphone(cx))
        .into_any_element()
    };
    let accessibility = cfg!(target_os = "macos").then(|| {
        if setup.permissions.accessibility == Permission::Granted {
            permission_allowed(
                tokens,
                "ui/check.svg",
                tr("ui.accessibility"),
                tr("ui.allowed"),
            )
        } else {
            Button::new(
                *tokens,
                "onboarding-accessibility",
                format!("{} · {}", tr("ui.accessibility"), tr("ui.settings")),
            )
            .size(ButtonSize::Small)
            .enabled(!view.settings_preview)
            .on_click(cx, |_, _, cx| {
                cx.open_url(permissions::accessibility_settings_url())
            })
            .into_any_element()
        }
    });
    let status = match &setup.feedback {
        Feedback::Starting => tr("ui.opening_microphone_progress").to_string(),
        Feedback::RecordingNotice { .. } => tr("ui.listening_progress").into(),
        Feedback::Transcribing => tr("ui.transcribing_progress").into(),
        Feedback::Cancelling => tr("ui.cancelling_progress").into(),
        Feedback::NoSpeech => tr("ui.no_speech_detected_try_again").into(),
        Feedback::Success => tr("ui.done").into(),
        Feedback::Error { title, advice } => format!("{}: {}", tr(title), tr(advice)),
        _ if setup.permissions.microphone != Permission::Granted
            && setup.permissions.accessibility != Permission::Granted =>
        {
            tr("ui.allow_microphone_and_accessibility_to_continue").into()
        }
        _ if setup.permissions.microphone != Permission::Granted => {
            tr("ui.allow_microphone_to_continue").into()
        }
        _ if setup.permissions.accessibility != Permission::Granted => {
            tr("ui.enable_plume_in_accessibility_settings_to_continue").into()
        }
        _ if setup.session_error.is_some() => tr("ui.resolve_the_error_above_to_continue").into(),
        _ if setup.microphone_checking => tr("ui.checking_microphone_progress").into(),
        _ if !setup.model_ready => tr("ui.loading_model_progress").into(),
        _ if setup.session_starting => tr("ui.starting_shortcut_service_progress").into(),
        _ if ready => format!(
            "{} {}",
            tr("onboarding.try.hold"),
            view.phase
                .prefs()
                .hold()
                .split('+')
                .map(|key| plume_ui::key_label(key.trim()))
                .collect::<Vec<_>>()
                .join(" + ")
        ),
        _ if view.save_error.is_some() => tr("ui.save_settings_to_continue").into(),
        _ => tr("ui.preparing_dictation_progress").into(),
    };
    let pill = match setup.feedback {
        Feedback::Starting | Feedback::RecordingNotice { .. } => StagePill::Listening,
        Feedback::Transcribing | Feedback::Cancelling => StagePill::Thinking,
        Feedback::Success => StagePill::Done,
        _ => StagePill::Idle,
    };
    let mut body = div()
        .flex()
        .flex_col()
        .gap(px(18.))
        .child(
            div()
                .flex()
                .items_end()
                .justify_between()
                .gap(px(24.))
                .child(div().flex().flex_col().gap(px(10.)).child(display_title(
                    tokens,
                    tr("onboarding.try.headline"),
                    tr("onboarding.try.serif"),
                    28.,
                )))
                // Granted on the previous step; shown again only if revoked since.
                .when(!setup.permissions.granted(), |el| {
                    el.child(
                        div()
                            .flex()
                            .flex_col()
                            .items_end()
                            .gap(px(8.))
                            .child(microphone)
                            .children(accessibility),
                    )
                }),
        )
        .child(test_stage(
            tokens,
            pill,
            ready,
            view.phase.prefs().hold(),
            &setup.transcript,
            status,
        ));
    if cfg!(target_os = "macos") && setup.permissions.accessibility != Permission::Granted {
        body = body.child(
            div()
                .text_size(px(12.))
                .text_color(tokens.subtle)
                .child(prose(tr("onboarding.permissions.old_build"))),
        );
    }
    if let Some(error) = &setup.session_error {
        body = body
            .child(error_text(tokens, error.clone()))
            .when(cfg!(target_os = "macos"), |el| {
                el.child(div().text_size(px(12.)).text_color(tokens.subtle).child(tr(
                    "Check Privacy & Security, then relaunch Plume if needed.",
                )))
            })
            .child(
                div().flex().child(
                    Button::new(*tokens, "onboarding-service-retry", tr("ui.check_again"))
                        .size(ButtonSize::Small)
                        .enabled(!view.is_busy())
                        .on_click(cx, |view, _, cx| {
                            let setup = view.onboarding.as_mut().unwrap();
                            setup.session_error = None;
                            setup.microphone_ready = false;
                            cx.notify();
                        }),
                ),
            );
    }
    body.into_any_element()
}

impl SettingsView {
    fn can_request_microphone(&self) -> bool {
        self.onboarding
            .as_ref()
            .is_some_and(|setup| !setup.microphone_checking && setup.microphone_prompt.is_none())
            && !self.is_busy()
            && !self.settings_preview
    }

    /// The native prompt the first time, System Settings once refused.
    fn request_microphone(&mut self, cx: &mut Context<Self>) {
        let setup = self.onboarding.as_mut().unwrap();
        if setup.permissions.microphone == Permission::NotRequested {
            setup.microphone_prompt = Some(permissions::request_microphone());
        } else if setup.permissions.microphone == Permission::Denied {
            cx.open_url(permissions::microphone_settings_url());
        }
        setup.session_error = None;
        setup.microphone_ready = false;
        cx.notify();
    }
}

/// macOS only: each permission gets a card saying why, before the test needs it.
fn permissions_screen(
    view: &SettingsView,
    tokens: &Tokens,
    cx: &mut Context<SettingsView>,
) -> AnyElement {
    let setup = view.onboarding.as_ref().unwrap();
    let microphone = setup.permissions.microphone;
    let accessibility = setup.permissions.accessibility;
    let microphone_action = Button::new(
        *tokens,
        "onboarding-microphone",
        if microphone == Permission::Denied {
            tr("ui.open_settings")
        } else {
            tr("ui.allow")
        },
    )
    .primary()
    .size(ButtonSize::Small)
    .enabled(view.can_request_microphone())
    .on_click(cx, |view, _, cx| view.request_microphone(cx));
    // The primary action moves to Accessibility once the microphone is allowed.
    let accessibility_action =
        Button::new(*tokens, "onboarding-accessibility", tr("ui.open_settings"))
            .when(microphone == Permission::Granted, |button| button.primary())
            .size(ButtonSize::Small)
            .enabled(!view.settings_preview)
            .on_click(cx, |_, _, cx| {
                cx.open_url(permissions::accessibility_settings_url())
            });
    div()
        .flex()
        .flex_col()
        .gap(px(20.))
        .child(step_heading(
            tokens,
            tr("onboarding.permissions.headline"),
            tr("onboarding.permissions.serif"),
            None,
        ))
        .child(
            div()
                .flex()
                .gap(px(12.))
                .child(permission_card(
                    tokens,
                    "onboarding-microphone-card",
                    1,
                    "ui/mic.svg",
                    tr("ui.microphone"),
                    tr("onboarding.permissions.microphone_text"),
                    microphone == Permission::Granted,
                    microphone_action,
                ))
                .child(permission_card(
                    tokens,
                    "onboarding-accessibility-card",
                    2,
                    "ui/keyboard.svg",
                    tr("ui.accessibility"),
                    tr("onboarding.permissions.accessibility_text"),
                    accessibility == Permission::Granted,
                    accessibility_action,
                )),
        )
        .when(
            setup.permissions_lost && accessibility != Permission::Granted,
            |el| {
                el.child(
                    div()
                        .text_size(px(12.))
                        .text_color(tokens.subtle)
                        .child(tr("onboarding.permissions.stale_entry")),
                )
            },
        )
        .into_any_element()
}

#[allow(clippy::too_many_arguments)]
fn permission_card(
    tokens: &Tokens,
    id: &'static str,
    number: usize,
    icon: &'static str,
    title: &'static str,
    text: &'static str,
    granted: bool,
    action: Button,
) -> AnyElement {
    div()
        .id(id)
        .flex_1()
        .min_w_0()
        .h(px(236.))
        .pt(px(22.))
        .px(px(20.))
        .pb(px(16.))
        .flex()
        .flex_col()
        .rounded(px(20.))
        .border_1()
        .border_color(tokens.border)
        .bg(tokens.surface)
        .child(
            div()
                .flex()
                .items_start()
                .justify_between()
                .child(
                    div()
                        .size(px(44.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(12.))
                        .bg(tokens.surface_2)
                        .child(svg().path(icon).size(px(20.)).text_color(tokens.text)),
                )
                .child(if granted {
                    div()
                        .size(px(22.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .bg(tokens.accent)
                        .child(
                            svg()
                                .path("ui/check.svg")
                                .size(px(12.))
                                .text_color(tokens.on_accent),
                        )
                        .into_any_element()
                } else {
                    div()
                        .font_family(FONT_MONO)
                        .text_size(px(12.))
                        .text_color(tokens.faint)
                        .child(format!("{number:02}"))
                        .into_any_element()
                }),
        )
        .child(
            div()
                .mt(px(20.))
                .text_size(px(19.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(title),
        )
        .child(
            div()
                .mt(px(5.))
                .min_h(px(39.))
                .text_size(px(13.))
                .line_height(px(19.5))
                .text_color(tokens.muted)
                .child(prose(text)),
        )
        .child(div().flex_1())
        .child(div().mt(px(16.)).flex().items_center().map(|row| {
            if granted {
                row.h(px(28.))
                    .gap(px(6.))
                    .text_size(px(13.))
                    .text_color(tokens.accent_text)
                    .child(
                        svg()
                            .path("ui/check.svg")
                            .size(px(12.))
                            .text_color(tokens.accent_text),
                    )
                    .child(tr("ui.allowed"))
            } else {
                row.child(action)
            }
        }))
        .into_any_element()
}

fn permission_allowed(
    tokens: &Tokens,
    icon: &'static str,
    title: &'static str,
    state: &'static str,
) -> AnyElement {
    div()
        .h(px(28.))
        .pl(px(10.))
        .pr(px(12.))
        .flex()
        .items_center()
        .gap(px(8.))
        .rounded_full()
        .border_1()
        .border_color(tokens.border)
        .bg(tokens.surface)
        .text_size(px(12.5))
        .text_color(tokens.muted)
        .child(svg().path(icon).size(px(14.)).text_color(tokens.muted))
        .child(title)
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(4.))
                .text_color(tokens.accent_text)
                .child(
                    svg()
                        .path("ui/check.svg")
                        .size(px(11.))
                        .text_color(tokens.accent_text),
                )
                .child(state),
        )
        .into_any_element()
}

/// The website's hero in miniature: a note on a dark dotted stage, the bubble over it.
fn test_stage(
    tokens: &Tokens,
    pill: StagePill,
    ready: bool,
    hold: &str,
    transcript: &str,
    status: String,
) -> impl IntoElement {
    let dots = tokens.stage_dots;
    div()
        .relative()
        .h(px(236.))
        .flex_shrink_0()
        .pt(px(22.))
        .px(px(44.))
        .rounded(px(20.))
        .overflow_hidden()
        .border_1()
        .border_color(tokens.stage_border)
        .bg(tokens.stage)
        .child(
            canvas(
                |_, _, _| (),
                move |area, _, window, _| {
                    let step = 22.;
                    let width: f32 = area.size.width.into();
                    let height: f32 = area.size.height.into();
                    let mut y = step / 2.;
                    while y < height {
                        let mut x = step / 2.;
                        while x < width {
                            window.paint_quad(
                                fill(
                                    gpui::bounds(
                                        area.origin + gpui::point(px(x), px(y)),
                                        gpui::size(px(1.5), px(1.5)),
                                    ),
                                    dots,
                                )
                                .corner_radii(px(0.75)),
                            );
                            x += step;
                        }
                        y += step;
                    }
                },
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        )
        .child(
            div()
                .relative()
                .size_full()
                .flex()
                .flex_col()
                .rounded_t(px(12.))
                .overflow_hidden()
                .border_1()
                .border_b_0()
                .border_color(tokens.sheet_border)
                .bg(tokens.sheet)
                .child(
                    div()
                        .h(px(32.))
                        .flex_shrink_0()
                        .px(px(14.))
                        .flex()
                        .items_center()
                        .gap(px(7.))
                        .border_b_1()
                        .border_color(tokens.sheet_border)
                        .children(
                            (0..3)
                                .map(|_| div().size(px(9.)).rounded_full().bg(tokens.sheet_chrome)),
                        )
                        .child(
                            div()
                                .ml(px(10.))
                                .text_size(px(12.))
                                .text_color(tokens.faint)
                                .child(tr("onboarding.try.sheet_title")),
                        ),
                )
                .child(
                    div()
                        .id("onboarding-test-transcript")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .px(px(26.))
                        .pt(px(16.))
                        .pb(px(72.))
                        .text_size(px(14.))
                        .line_height(px(22.))
                        .map(|el| {
                            if transcript.is_empty() {
                                el.text_color(tokens.faint).child(status)
                            } else {
                                el.text_color(tokens.sheet_title)
                                    .child(transcript.to_string())
                            }
                        }),
                )
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .bottom(px(18.))
                        .flex()
                        .justify_center()
                        .child(stage_pill(tokens, pill, ready, hold)),
                ),
        )
}

/// The dictation bubble, drawn as on the website.
fn stage_pill(tokens: &Tokens, state: StagePill, ready: bool, hold: &str) -> impl IntoElement {
    let pill = div()
        .h(px(46.))
        .pl(px(15.))
        .pr(px(
            if state == StagePill::Listening || state == StagePill::Idle {
                8.
            } else {
                16.
            },
        ))
        .flex()
        .items_center()
        .gap(px(12.))
        .rounded_full()
        .bg(tokens.pill_bg)
        .text_color(tokens.pill_fg)
        .text_size(px(13.))
        .shadow(vec![gpui::BoxShadow {
            color: gpui::hsla(0., 0., 0., 0.35),
            offset: gpui::point(px(0.), px(12.)),
            blur_radius: px(40.),
            spread_radius: px(0.),
        }])
        .when(state == StagePill::Idle && !ready, |el| el.opacity(0.6));
    let chip = |label: String| {
        div()
            .h(px(26.))
            .px(px(9.))
            .flex()
            .items_center()
            .rounded_full()
            .bg(tokens.pill_chip)
            .text_color(tokens.pill_chip_fg)
            .text_size(px(11.))
            .child(label)
    };
    let label = |text: &'static str| div().text_color(tokens.pill_muted).child(text);
    match state {
        StagePill::Idle => pill
            .child(div().size(px(9.)).rounded_full().bg(tokens.pill_idle))
            .child(label(tr("ui.ready")))
            .child(
                div().flex().gap(px(4.)).children(
                    hold.split('+')
                        .map(|key| chip(plume_ui::key_label(key.trim()))),
                ),
            ),
        StagePill::Listening => pill
            .child(div().size(px(9.)).rounded_full().bg(tokens.accent))
            .child(div().h(px(22.)).flex().items_center().gap(px(3.)).children(
                WAVE.iter().enumerate().map(|(index, height)| {
                    let height = *height;
                    div()
                        .w(px(3.))
                        .h(px(height))
                        .rounded(px(1.5))
                        .bg(tokens.pill_fg)
                        .with_animation(
                            SharedString::from(format!("onboarding-wave-{index}")),
                            Animation::new(Duration::from_millis(800 + (index as u64 % 5) * 100))
                                .repeat(),
                            move |el, delta| {
                                let wave = (delta * std::f32::consts::TAU + index as f32 * 0.8)
                                    .sin()
                                    * 0.5
                                    + 0.5;
                                el.h(px((height * (0.4 + 0.7 * wave)).max(3.)))
                            },
                        )
                }),
            ))
            .child(label(tr("ui.listening"))),
        StagePill::Thinking => pill
            .child(div().flex().gap(px(4.)).children((0..3).map(|index| {
                div()
                    .size(px(6.))
                    .rounded_full()
                    .bg(tokens.accent)
                    .with_animation(
                        SharedString::from(format!("onboarding-thinking-{index}")),
                        Animation::new(Duration::from_millis(1000)).repeat(),
                        move |el, delta| {
                            let phase = (delta - index as f32 * 0.2).rem_euclid(1.);
                            el.opacity(1. - 0.75 * (phase * std::f32::consts::PI).sin())
                        },
                    )
            })))
            .child(label(tr("ui.transcribing_progress"))),
        StagePill::Done => pill
            .child(
                div()
                    .size(px(20.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .bg(tokens.accent)
                    .child(
                        svg()
                            .path("ui/check.svg")
                            .size(px(12.))
                            .text_color(tokens.on_accent),
                    ),
            )
            .child(label(tr("ui.inserted"))),
    }
}

const WAVE: [f32; 12] = [6., 16., 20., 12., 18., 8., 20., 14., 7., 17., 11., 6.];
