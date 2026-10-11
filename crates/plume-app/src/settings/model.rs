use crate::settings::i18n::tr;
use gpui::{
    div, prelude::*, px, relative, svg, AnyElement, Context, Div, FontWeight, SharedString,
};
use plume_ui::{Button, ButtonSize, IconButton, Tokens, FONT_MONO};

use crate::catalog::{self, ModelEntry, ModelId};
use crate::phase::{AppPhase, OnboardStatus, Progress};
use crate::prefs::Prefs;

use super::view::{
    error_text, page, row_label, settings_divider, settings_group, settings_row, settings_section,
};
use super::SettingsView;

pub(super) fn model_page(
    view: &SettingsView,
    tokens: &Tokens,
    prefs: &Prefs,
    cx: &mut Context<SettingsView>,
) -> AnyElement {
    page(
        tokens,
        tr("ui.models"),
        Some((tr("settings.models.lead"), tr("settings.models.serif"))),
        div()
            .flex()
            .flex_col()
            .gap(px(22.))
            .child(settings_section(
                tokens,
                tr("ui.languages"),
                settings_group(tokens)
                    .child(super::languages::dictation_language_row(view, tokens, cx)),
            ))
            .children(super::languages::compatibility_warning(view, tokens))
            .child(settings_section(
                tokens,
                tr("ui.available_models"),
                model_catalog(view, tokens, prefs, cx),
            ))
            .children(model_error(&view.phase).map(|text| error_text(tokens, text)))
            .children(view.save_error.clone().map(|text| error_text(tokens, text))),
    )
}

fn model_catalog(
    view: &SettingsView,
    tokens: &Tokens,
    prefs: &Prefs,
    cx: &mut Context<SettingsView>,
) -> impl IntoElement {
    let count = catalog::entries().len();
    settings_group(tokens).children(catalog::entries().iter().enumerate().flat_map(
        |(index, entry)| {
            let mut rows =
                vec![catalog_model_row(entry, view, tokens, prefs, cx).into_any_element()];
            if index + 1 < count {
                rows.push(settings_divider(tokens).into_any_element());
            }
            rows
        },
    ))
}

fn catalog_model_row(
    entry: &ModelEntry,
    view: &SettingsView,
    tokens: &Tokens,
    prefs: &Prefs,
    cx: &mut Context<SettingsView>,
) -> impl IntoElement {
    let phase = &view.phase;
    let progress = match phase {
        AppPhase::Onboarding {
            prefs,
            status: OnboardStatus::Fetching { last },
            ..
        } if prefs.model == entry.id => Some(last),
        _ => None,
    };
    let downloading: Option<SharedString> =
        progress.and_then(download_status).map(SharedString::from);
    let supported = entry.id.supports(prefs.language());
    let installed = catalog::is_complete(entry.id, &entry.id.data_dir());
    let (label, enabled) = catalog_model_action(phase, view.active_model, entry.id, installed);
    // A pack that does not cover the pinned language cannot be selected.
    let (label, enabled) = if supported {
        (label, enabled && !view.settings_preview)
    } else {
        ("ui.unsupported".into(), false)
    };
    let can_delete = installed && !view.downloads.is_active_for(entry.id) && !view.settings_preview;
    let id = entry.id;
    settings_row()
        .id(SharedString::from(format!("model-{}", id.as_str())))
        .relative()
        .min_h(px(76.))
        // Pack hors langue épinglée : grisé, non cliquable.
        .when(!supported, |el| el.opacity(0.45))
        .child(
            row_label(tokens, entry.name, None).child(match downloading {
                Some(text) => div()
                    .mt(px(4.))
                    .font_family(FONT_MONO)
                    .text_size(px(11.5))
                    .text_color(tokens.accent_text)
                    .child(text)
                    .into_any_element(),
                None => div()
                    .mt(px(4.))
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(14.))
                    .child(
                        div()
                            .font_family(FONT_MONO)
                            .text_size(px(11.5))
                            .text_color(tokens.subtle)
                            .child(if supported {
                                entry.size
                            } else {
                                tr("ui.language_not_supported")
                            }),
                    )
                    .child(spec_meter(tr("ui.speed"), entry.speed, tokens, false))
                    .child(spec_meter(tr("ui.accuracy"), entry.accuracy, tokens, false))
                    .into_any_element(),
            }),
        )
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(4.))
                .children(can_delete.then(|| {
                    IconButton::new(
                        *tokens,
                        SharedString::from(format!("model-{}-delete", id.as_str())),
                        "ui/delete.svg",
                    )
                    .on_click(cx, move |this, _, cx| {
                        this.capture.cancel();
                        this.delete_model(id, cx);
                    })
                }))
                .child(catalog_model_button(tokens, id, label, enabled, cx)),
        )
        .children(progress.and_then(|progress| catalog_model_progress(progress, tokens)))
}

/// Five short bars, ink on a track; on an inverted card they follow its colors.
/// `value` is 1..=5, more filled bars = faster / more accurate.
pub(super) fn spec_meter(
    label: &'static str,
    value: u8,
    tokens: &Tokens,
    inverted: bool,
) -> impl IntoElement {
    let value = value.clamp(1, 5);
    let (on, off, text) = if inverted {
        (tokens.invert_fg, tokens.invert_track, tokens.invert_muted)
    } else {
        (tokens.text, tokens.track, tokens.subtle)
    };
    div()
        .flex()
        .flex_row()
        .flex_shrink_0()
        .items_center()
        .justify_between()
        .gap(px(8.))
        .child(
            div()
                .flex_shrink_0()
                .text_size(px(12.))
                .whitespace_nowrap()
                .text_color(text)
                .child(label),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(3.))
                .children((0..5).map(move |i| {
                    div()
                        .w(px(12.))
                        .h(px(4.))
                        .rounded(px(2.))
                        .bg(if i < value { on } else { off })
                })),
        )
}

fn catalog_model_action(
    phase: &AppPhase,
    active_model: Option<ModelId>,
    id: ModelId,
    installed: bool,
) -> (SharedString, bool) {
    if active_model == Some(id) {
        return (
            if installed {
                "ui.in_use"
            } else {
                "ui.in_memory"
            }
            .into(),
            false,
        );
    }
    if phase.prefs().model == id {
        return match phase {
            AppPhase::Live { .. } => (
                if installed { "ui.use" } else { "ui.download" }.into(),
                true,
            ),
            AppPhase::Onboarding { status, .. } => match status {
                OnboardStatus::Idle => (
                    if installed { "ui.use" } else { "ui.download" }.into(),
                    true,
                ),
                OnboardStatus::Activating => ("ui.loading_progress".into(), false),
                OnboardStatus::Fetching { .. } => ("ui.downloading".into(), false),
                OnboardStatus::Failed { .. } => ("ui.retry".into(), true),
            },
            AppPhase::Refused { .. } => ("ui.retry".into(), true),
        };
    }
    let busy = matches!(
        phase,
        AppPhase::Onboarding {
            status: OnboardStatus::Activating | OnboardStatus::Fetching { .. },
            ..
        }
    );
    (
        if installed { "ui.use" } else { "ui.download" }.into(),
        !busy,
    )
}

fn catalog_model_button(
    tokens: &Tokens,
    id: ModelId,
    label: SharedString,
    enabled: bool,
    cx: &mut Context<SettingsView>,
) -> AnyElement {
    if matches!(label.as_ref(), "ui.in_use" | "ui.in_memory") {
        return div()
            .h(px(28.))
            .px(px(4.))
            .flex()
            .items_center()
            .gap(px(6.))
            .text_size(px(12.5))
            .font_weight(FontWeight::MEDIUM)
            .text_color(tokens.accent_text)
            .child(
                svg()
                    .path("ui/check.svg")
                    .size(px(12.))
                    .text_color(tokens.accent_text),
            )
            .child(tr(label.as_ref()).to_owned())
            .into_any_element();
    }
    Button::new(
        *tokens,
        SharedString::from(format!("model-{}-action", id.as_str())),
        tr(label.as_ref()).to_owned(),
    )
    .size(ButtonSize::Small)
    .enabled(enabled)
    .on_click(cx, move |this, _, cx| {
        if !id.supports(this.phase.prefs().language()) {
            return;
        }
        this.capture.cancel();
        let mut prefs = this.phase.prefs().clone();
        prefs.model = id;
        let request = this.begin_download(prefs.clone(), cx);
        crate::begin_pack(cx, prefs, request);
    })
    .into_any_element()
}

fn catalog_model_progress(progress: &Progress, tokens: &Tokens) -> Option<Div> {
    let total = progress.total.filter(|total| *total > 0)?;
    let fraction = (progress.bytes as f64 / total as f64).clamp(0.0, 1.0) as f32;
    Some(
        div()
            .absolute()
            .left(px(18.))
            .right(px(14.))
            .bottom(px(6.))
            .h(px(3.))
            .rounded_full()
            .bg(tokens.track)
            .child(
                div()
                    .h_full()
                    .w(relative(fraction))
                    .rounded_full()
                    .bg(tokens.accent),
            ),
    )
}

fn model_error(phase: &AppPhase) -> Option<String> {
    match phase {
        AppPhase::Live { .. } => None,
        AppPhase::Onboarding {
            status, warning, ..
        } => match status {
            OnboardStatus::Idle | OnboardStatus::Activating | OnboardStatus::Fetching { .. } => {
                warning.clone()
            }
            OnboardStatus::Failed { reason } => Some(reason.clone()),
        },
        AppPhase::Refused { reason, .. } => Some(reason.clone()),
    }
}

pub(super) fn download_status(progress: &Progress) -> Option<String> {
    let mut parts = Vec::with_capacity(2);
    if let Some(total) = progress.total.filter(|total| *total > 0) {
        let percent = (progress.bytes.saturating_mul(100) / total).min(100);
        parts.push(format!("{percent}%"));
    }
    if let Some(speed) = progress_speed(progress.bytes_per_second) {
        parts.push(speed);
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}

pub(super) fn progress_speed(bytes_per_second: Option<u64>) -> Option<String> {
    bytes_per_second.map(|bytes| format!("{}/s", format_bytes(bytes)))
}

pub(super) fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    let bytes = bytes as f64;
    if bytes >= MB {
        format!("{:.1} MB", bytes / MB)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes / KB)
    } else {
        format!("{} B", bytes as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::{catalog_model_action, download_status, format_bytes};
    use crate::catalog::ModelId;
    use crate::phase::{AppPhase, OnboardStatus, Progress};
    use crate::prefs::Prefs;

    fn onboarding(model: ModelId, status: OnboardStatus) -> AppPhase {
        let mut prefs = Prefs::default_fresh();
        prefs.model = model;
        AppPhase::Onboarding {
            prefs,
            status,
            warning: None,
        }
    }

    #[test]
    fn default_preference_is_not_an_active_model() {
        let phase = onboarding(ModelId::Nemotron35Compact, OnboardStatus::Idle);
        let (label, enabled) =
            catalog_model_action(&phase, None, ModelId::Nemotron35Compact, false);
        assert_eq!(label.as_ref(), "ui.download");
        assert!(enabled);
        let (label, enabled) =
            catalog_model_action(&phase, None, ModelId::WhisperLargeV3Turbo, true);
        assert_eq!(label.as_ref(), "ui.use");
        assert!(enabled);
    }

    #[test]
    fn installed_model_is_loading_until_the_session_is_connected() {
        let phase = onboarding(ModelId::WhisperLargeV3Turbo, OnboardStatus::Activating);
        let (label, enabled) =
            catalog_model_action(&phase, None, ModelId::WhisperLargeV3Turbo, true);
        assert_eq!(label.as_ref(), "ui.loading_progress");
        assert!(!enabled);
        let (label, enabled) =
            catalog_model_action(&phase, None, ModelId::Nemotron35Compact, false);
        assert_eq!(label.as_ref(), "ui.download");
        assert!(!enabled);
    }

    #[test]
    fn switching_models_keeps_the_running_model_in_use() {
        let phase = onboarding(
            ModelId::Nemotron35Compact,
            OnboardStatus::Fetching {
                last: Progress {
                    file: "encoder.onnx".into(),
                    bytes: 0,
                    total: None,
                    bytes_per_second: None,
                },
            },
        );
        let active = Some(ModelId::WhisperLargeV3Turbo);
        let (label, enabled) =
            catalog_model_action(&phase, active, ModelId::WhisperLargeV3Turbo, true);
        assert_eq!(label.as_ref(), "ui.in_use");
        assert!(!enabled);
        let (label, enabled) =
            catalog_model_action(&phase, active, ModelId::Nemotron35Compact, false);
        assert_eq!(label.as_ref(), "ui.downloading");
        assert!(!enabled);
    }

    #[test]
    fn deleting_a_loaded_pack_does_not_unload_the_running_engine() {
        let phase = onboarding(ModelId::WhisperLargeV3Turbo, OnboardStatus::Idle);
        let (label, enabled) = catalog_model_action(
            &phase,
            Some(ModelId::WhisperLargeV3Turbo),
            ModelId::WhisperLargeV3Turbo,
            false,
        );
        assert_eq!(label.as_ref(), "ui.in_memory");
        assert!(!enabled);
    }

    #[test]
    fn progress_is_compact_and_bounded() {
        assert_eq!(
            download_status(&Progress {
                file: "starting".into(),
                bytes: 0,
                total: None,
                bytes_per_second: None,
            }),
            None
        );
        assert_eq!(
            download_status(&Progress {
                file: "encoder.onnx".into(),
                bytes: 25,
                total: Some(100),
                bytes_per_second: None,
            }),
            Some("25%".to_string())
        );
        assert_eq!(
            download_status(&Progress {
                file: "encoder.onnx".into(),
                bytes: 150,
                total: Some(100),
                bytes_per_second: Some(10 * 1024 * 1024),
            }),
            Some("100% · 10.0 MB/s".to_string())
        );
        assert_eq!(format_bytes(1536), "1.5 KB");
    }
}
