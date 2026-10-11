use crate::settings::i18n::tr;
use gpui::{deferred, div, prelude::*, px, AnyElement, Context, SharedString};
use plume_session::{Destination, InsertionMode};
use plume_ui::{
    keycaps, Button, ButtonSize, IconButton, KeySize, Segment, Segmented, Switch, Tokens,
};

use crate::prefs::Prefs;

use super::view::{
    error_text, menu_option, menu_panel, page, row_label, select_trigger, settings_divider,
    settings_group, settings_row, settings_section,
};
use super::SettingsView;

const HOLD_CAPTURE_ID: &str = "hold-capture";
const INSERTION_MODES: [(InsertionMode, &str); 3] = [
    (InsertionMode::Auto, "ui.automatic"),
    (InsertionMode::Clipboard, "ui.paste"),
    (InsertionMode::Typing, "ui.typing"),
];
const INSERTION_WIDTH: f32 = 156.;

pub(super) fn dictation_page(
    view: &SettingsView,
    tokens: &Tokens,
    prefs: &Prefs,
    cx: &mut Context<SettingsView>,
) -> AnyElement {
    let error = view
        .capture
        .reject()
        .map(str::to_string)
        .or_else(|| view.save_error.clone());
    let inserting = prefs.destination() == Destination::FocusedField;
    page(
        tokens,
        tr("ui.dictation"),
        Some((
            tr("settings.dictation.lead"),
            tr("settings.dictation.serif"),
        )),
        div()
            .flex()
            .flex_col()
            .gap(px(22.))
            .when(
                view.session_control.is_none() && !view.settings_preview,
                |el| {
                    el.child(
                        div()
                            .flex()
                            .flex_col()
                            .items_start()
                            .gap(px(10.))
                            .child(error_text(tokens, "dictation.model_unavailable".into()))
                            .child(
                                Button::new(*tokens, "dictation-open-models", tr("ui.open_models"))
                                    .size(ButtonSize::Small)
                                    .on_click(cx, |view, _, cx| {
                                        view.section = super::SettingsSection::Model;
                                        cx.notify();
                                    }),
                            ),
                    )
                },
            )
            .child(settings_section(
                tokens,
                tr("ui.shortcuts"),
                settings_group(tokens)
                    .child(shortcut_row(view, tokens, prefs, false, cx))
                    .child(settings_divider(tokens))
                    .child(shortcut_row(view, tokens, prefs, true, cx)),
            ))
            .child(settings_section(
                tokens,
                tr("ui.text_output"),
                settings_group(tokens)
                    .child(destination_row(tokens, prefs.destination(), cx))
                    .child(settings_divider(tokens))
                    .child(insertion_mode_row(
                        tokens,
                        prefs.insertion_mode(),
                        view.insertion_open,
                        inserting,
                        cx,
                    ))
                    .child(settings_divider(tokens))
                    .child(copy_on_failure_row(
                        tokens,
                        prefs.copy_on_failure(),
                        inserting,
                        cx,
                    )),
            ))
            .children(error.map(|text| error_text(tokens, text))),
    )
}

pub(super) fn shortcut_row(
    view: &SettingsView,
    tokens: &Tokens,
    prefs: &Prefs,
    toggle: bool,
    cx: &mut Context<SettingsView>,
) -> impl IntoElement {
    let listening = view.capture.is_listening() && view.capture_toggle == toggle;
    let shortcut = if toggle {
        prefs.toggle()
    } else {
        Some(prefs.hold())
    };
    let detail = if listening {
        tr("ui.press_a_shortcut_then_release_to_apply")
    } else if toggle {
        tr("settings.shortcuts.toggle_detail")
    } else {
        tr("settings.shortcuts.hold_detail")
    };
    let tokens_ = *tokens;
    let keys = if listening {
        div()
            .h(px(32.))
            .px(px(14.))
            .flex()
            .items_center()
            .rounded_full()
            .bg(tokens.highlight)
            .border_1()
            .border_color(tokens.accent_ring)
            .text_size(px(13.))
            .text_color(tokens.accent_text)
            .child(SharedString::from(
                view.capture
                    .preview()
                    .unwrap_or_else(|| tr("ui.press_keys_progress"))
                    .to_string(),
            ))
            .into_any_element()
    } else {
        match shortcut {
            Some(shortcut) => keycaps(tokens, shortcut, KeySize::Small),
            None => div()
                .text_size(px(13.))
                .text_color(tokens.subtle)
                .child(tr("ui.disabled"))
                .into_any_element(),
        }
    };
    settings_row()
        .id(if toggle {
            "toggle-capture"
        } else {
            HOLD_CAPTURE_ID
        })
        .when(listening, |el| {
            el.track_focus(&view.hold_focus)
                .on_key_down(cx.listener(SettingsView::on_hold_key))
                .on_key_up(cx.listener(SettingsView::on_hold_key_up))
                .on_modifiers_changed(cx.listener(SettingsView::on_hold_modifiers))
        })
        .child(row_label(
            tokens,
            if toggle {
                tr("ui.hands_free")
            } else {
                tr("ui.push_to_talk")
            },
            Some(detail),
        ))
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(4.))
                .child(
                    div()
                        .id(if toggle {
                            "toggle-change"
                        } else {
                            "hold-change"
                        })
                        .p(px(4.))
                        .rounded(px(12.))
                        .border_1()
                        .border_color(gpui::transparent_black())
                        .cursor_pointer()
                        .hover(move |style| style.border_color(tokens_.border_strong))
                        .on_click(cx.listener(move |this, _event, window, cx| {
                            if this.capture.is_listening() && this.capture_toggle != toggle {
                                this.capture.cancel();
                                this.shortcut_edit.take();
                            }
                            this.capture_toggle = toggle;
                            this.toggle_hold_capture(window, cx);
                        }))
                        .child(keys),
                )
                .child(
                    IconButton::new(
                        *tokens,
                        if toggle { "toggle-reset" } else { "hold-reset" },
                        "ui/reset.svg",
                    )
                    .on_click(cx, move |this, window, cx| {
                        this.capture_toggle = toggle;
                        this.reset_hold(window, cx);
                    }),
                ),
        )
}

/// Insert into the focused field, or only copy to the clipboard.
fn destination_row(
    tokens: &Tokens,
    selected: Destination,
    cx: &mut Context<SettingsView>,
) -> impl IntoElement {
    settings_row()
        .child(row_label(
            tokens,
            tr("ui.text_destination"),
            Some(tr("settings.output.destination_detail")),
        ))
        .child(Segmented::new(
            *tokens,
            selected,
            [
                Segment::new(
                    Destination::FocusedField,
                    "destination-insert",
                    tr("ui.insert"),
                )
                .on_click(cx, |this: &mut SettingsView, cx| {
                    this.commit_destination(Destination::FocusedField, cx);
                }),
                Segment::new(Destination::Clipboard, "destination-copy", tr("ui.copy")).on_click(
                    cx,
                    |this: &mut SettingsView, cx| {
                        this.commit_destination(Destination::Clipboard, cx);
                    },
                ),
            ],
        ))
}

/// Insertion settings only apply when text goes into the focused field.
fn inserting(view: &SettingsView) -> bool {
    view.phase.prefs().destination() == Destination::FocusedField
}

fn insertion_mode_row(
    tokens: &Tokens,
    selected: InsertionMode,
    open: bool,
    enabled: bool,
    cx: &mut Context<SettingsView>,
) -> impl IntoElement {
    let open = open && enabled;
    settings_row()
        .relative()
        .when(!enabled, |row| row.opacity(0.45))
        .child(row_label(
            tokens,
            tr("ui.text_insertion"),
            Some(tr("settings.output.insertion_detail")),
        ))
        .child(
            select_trigger(
                tokens,
                "insertion-select",
                tr(insertion_mode_label(selected)),
                INSERTION_WIDTH,
                enabled,
            )
            .when(enabled, |select| {
                select.on_click(cx.listener(|this, _event, _window, cx| {
                    this.insertion_open = !this.insertion_open;
                    cx.notify();
                }))
            }),
        )
        .when(open, |row| {
            row.child(
                div()
                    .id("insertion-menu-boundary")
                    .absolute()
                    .bottom(px(13.))
                    .right(px(14.))
                    .w(px(INSERTION_WIDTH))
                    .h(px(146.))
                    .on_mouse_down_out(cx.listener(|this, _event, _window, cx| {
                        this.insertion_open = false;
                        cx.notify();
                    })),
            )
            .child(
                deferred(
                    // This group sits near the bottom of the settings window: open upward.
                    menu_panel(tokens, INSERTION_WIDTH)
                        .absolute()
                        .bottom(px(51.))
                        .right(px(14.))
                        .children(INSERTION_MODES.into_iter().map(|(mode, label)| {
                            let active = mode == selected;
                            menu_option(
                                tokens,
                                SharedString::from(format!("insertion-option-{}", mode.as_str())),
                                tr(label),
                                active,
                                false,
                            )
                            .when(!active, |option| {
                                option.on_click(cx.listener(move |this, _event, _window, cx| {
                                    this.commit_insertion_mode(mode, cx);
                                }))
                            })
                        })),
                )
                .with_priority(1),
            )
        })
}

fn insertion_mode_label(mode: InsertionMode) -> &'static str {
    INSERTION_MODES
        .into_iter()
        .find_map(|(candidate, label)| (candidate == mode).then_some(label))
        .expect("every insertion mode has a label")
}

fn copy_on_failure_row(
    tokens: &Tokens,
    enabled: bool,
    available: bool,
    cx: &mut Context<SettingsView>,
) -> impl IntoElement {
    settings_row()
        .when(!available, |row| row.opacity(0.45))
        .child(row_label(
            tokens,
            tr("ui.copy_on_insertion_failure"),
            Some(tr("settings.output.copy_detail")),
        ))
        .child(Switch::new(*tokens, "copy-failure", enabled).on_click(
            cx,
            |this: &mut SettingsView, cx| {
                if inserting(this) {
                    this.toggle_copy_on_failure(cx);
                }
            },
        ))
}
