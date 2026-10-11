use crate::settings::i18n::tr;
use gpui::{deferred, div, list, prelude::*, px, AnyElement, Context, SharedString};
use plume_ui::{Button, ButtonSize, IconButton, Tokens, FONT_MONO};

use super::view::{
    error_text, menu_option, menu_panel, page, row_label, section_title, select_trigger,
    settings_divider, settings_group, settings_row, settings_section, CONTENT_LEFT, CONTENT_RIGHT,
    CONTENT_TOP, PAGE_WIDTH,
};
use super::{HistoryLimitMenu, SettingsView};
use crate::history_policy::HistoryPolicy;

const LIMIT_WIDTH: f32 = 148.;

pub(super) fn history_page(
    view: &mut SettingsView,
    tokens: &Tokens,
    cx: &mut Context<SettingsView>,
) -> AnyElement {
    let item_count = view.display_history().len() + 1;
    if view.history_list.item_count() != item_count {
        view.history_list.reset(item_count);
    }
    let entity = cx.entity();
    let tokens = *tokens;
    list(view.history_list.clone(), move |index, _, cx| {
        entity.update(cx, |view, cx| {
            let _language =
                super::i18n::LanguageScope::new(view.phase.prefs().interface_language());
            let item = if index == 0 {
                div()
                    .pt(px(CONTENT_TOP))
                    .pb(px(8.))
                    .child(history_header(view, &tokens, cx))
                    .into_any_element()
            } else {
                div()
                    .max_w(px(PAGE_WIDTH))
                    .when(index == view.display_history().len(), |el| el.pb(px(28.)))
                    .child(history_entry(view, index - 1, &tokens, cx))
                    .into_any_element()
            };
            div()
                .w_full()
                .pl(px(CONTENT_LEFT))
                .pr(px(CONTENT_RIGHT))
                .child(item)
                .into_any_element()
        })
    })
    .size_full()
    .into_any_element()
}

fn history_header(
    view: &SettingsView,
    tokens: &Tokens,
    cx: &mut Context<SettingsView>,
) -> AnyElement {
    let entries = view.display_history();
    let body = div()
        .flex()
        .flex_col()
        .gap(px(22.))
        .child(settings_section(
            tokens,
            tr("ui.storage"),
            settings_group(tokens)
                .child(history_limit_row(
                    view,
                    tokens,
                    HistoryLimitMenu::Retention,
                    cx,
                ))
                .child(settings_divider(tokens))
                .child(history_limit_row(
                    view,
                    tokens,
                    HistoryLimitMenu::Entries,
                    cx,
                )),
        ))
        .children(
            view.history_error
                .clone()
                .or_else(|| view.save_error.clone())
                .map(|error| error_text(tokens, error)),
        )
        .when(!entries.is_empty(), |list| {
            list.child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(section_title(tokens, tr("ui.transcriptions")))
                    .child(
                        Button::new(*tokens, "history-clear", tr("ui.clear_all"))
                            .ghost()
                            .size(ButtonSize::Small)
                            .on_click(cx, |this, _, cx| this.clear_history(cx)),
                    ),
            )
        })
        .when(entries.is_empty(), |list| {
            list.child(settings_section(
                tokens,
                tr("ui.transcriptions"),
                settings_group(tokens)
                    .px(px(18.))
                    .py(px(20.))
                    .text_size(px(13.5))
                    .text_color(tokens.muted)
                    .child(tr("ui.no_transcriptions_yet")),
            ))
        });

    page(
        tokens,
        tr("ui.history"),
        Some((tr("settings.history.lead"), tr("settings.history.serif"))),
        body,
    )
}

/// Only an entry that did not land where it should says so.
fn failure_badge(entry: &crate::history::HistoryEntry) -> Option<&'static str> {
    if entry.transcription_error.is_some() {
        None
    } else if entry.error.is_some() {
        Some(if entry.copied_on_failure {
            tr("ui.not_inserted_copied")
        } else {
            tr("ui.not_inserted")
        })
    } else if entry.status == "Recovered" {
        Some(tr("ui.recovered"))
    } else {
        None
    }
}

fn history_entry(
    view: &SettingsView,
    index: usize,
    tokens: &Tokens,
    cx: &mut Context<SettingsView>,
) -> AnyElement {
    let entries = view.display_history();
    let entry = &entries[index];
    let copy_text = entry.text.clone();
    let id = entry.id;
    let record_id = entry.record_id;
    let audio_available = record_id.is_some_and(|id| {
        view.recordings
            .as_ref()
            .is_some_and(|store| store.lock().unwrap().has_audio(id))
    });
    let copied = view.copied_history_id == Some(id);
    let first = index == 0;
    let last = index + 1 == entries.len();
    let group = SharedString::from(format!("history-entry-{id}"));
    let tokens_ = *tokens;
    // Consecutive items draw one card: the sides of each, the top of the first, the bottom of the last.
    div()
        .id(("history-entry", id))
        .w_full()
        .bg(tokens.surface)
        .border_color(tokens.border)
        .border_x_1()
        .px(px(4.))
        .when(first, |el| el.border_t_1().rounded_t(px(18.)).pt(px(4.)))
        .when(last, |el| el.border_b_1().rounded_b(px(18.)).pb(px(4.)))
        .child(
            div()
                .group(group.clone())
                .w_full()
                .px(px(14.))
                .py(px(12.))
                .flex()
                .flex_col()
                .gap(px(8.))
                .rounded(px(14.))
                .hover(move |style| style.bg(tokens_.hover))
                .children(
                    entry
                        .transcription_error
                        .clone()
                        .map(|error| error_text(tokens, error)),
                )
                .child(
                    div()
                        .flex()
                        .items_start()
                        .gap(px(8.))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_size(px(14.))
                                .line_height(px(21.))
                                .max_h(px(63.))
                                .overflow_hidden()
                                .child(entry.text.clone()),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_shrink_0()
                                .gap(px(2.))
                                .opacity(0.)
                                .group_hover(group, |style| style.opacity(1.))
                                .child(
                                    IconButton::new(*tokens, ("history-copy", id), "ui/copy.svg")
                                        .size(28.)
                                        .on_click(cx, move |this, _, cx| {
                                            this.copy_history(id, copy_text.clone(), cx);
                                        }),
                                )
                                .child(
                                    IconButton::new(
                                        *tokens,
                                        ("history-delete", id),
                                        "ui/delete.svg",
                                    )
                                    .size(28.)
                                    .on_click(
                                        cx,
                                        move |this, _, cx| {
                                            this.delete_history(id, cx);
                                        },
                                    ),
                                ),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(8.))
                        .text_size(px(12.))
                        .child(
                            div()
                                .px(px(9.))
                                .py(px(2.))
                                .rounded_full()
                                .bg(tokens.surface_2)
                                .text_color(tokens.muted)
                                .child(
                                    entry
                                        .application
                                        .clone()
                                        .unwrap_or_else(|| tr("ui.unknown_app").into()),
                                ),
                        )
                        .child(
                            div()
                                .font_family(FONT_MONO)
                                .text_size(px(11.5))
                                .text_color(if copied {
                                    tokens.accent_text
                                } else {
                                    tokens.subtle
                                })
                                .child(if copied {
                                    tr("ui.copied").to_string()
                                } else {
                                    super::i18n::age_label(entry.created_at)
                                }),
                        )
                        .children(failure_badge(entry).map(|label| {
                            div()
                                .px(px(9.))
                                .py(px(2.))
                                .rounded_full()
                                .bg(tokens.warn_bg)
                                .text_color(tokens.warn)
                                .child(label)
                        })),
                )
                .when(audio_available, |row| {
                    let record_id = record_id.unwrap();
                    row.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(px(12.))
                                    .text_color(tokens.subtle)
                                    .child(tr("ui.audio_available")),
                            )
                            .child(
                                Button::new(
                                    *tokens,
                                    SharedString::from(format!("retry-{id}")),
                                    tr("ui.retranscribe"),
                                )
                                .size(ButtonSize::Small)
                                .on_click(cx, move |this, _, cx| {
                                    this.retry_recording(record_id, cx)
                                }),
                            )
                            .child(
                                Button::new(
                                    *tokens,
                                    SharedString::from(format!("delete-audio-{id}")),
                                    tr("ui.delete_audio"),
                                )
                                .ghost()
                                .size(ButtonSize::Small)
                                .on_click(cx, move |this, _, cx| this.delete_audio(record_id, cx)),
                            ),
                    )
                }),
        )
        .into_any_element()
}

fn history_limit_row(
    view: &SettingsView,
    tokens: &Tokens,
    menu: HistoryLimitMenu,
    cx: &mut Context<SettingsView>,
) -> impl IntoElement {
    let policy = view.phase.prefs().history();
    let (id, title, selected, options) = match menu {
        HistoryLimitMenu::Retention => (
            "history-retention",
            tr("ui.keep_transcriptions_for"),
            policy.retention_days().map(|days| days as usize),
            vec![Some(7), Some(30), Some(90), None],
        ),
        HistoryLimitMenu::Entries => (
            "history-entries",
            tr("ui.maximum_transcriptions"),
            policy.max_entries(),
            vec![Some(100), Some(500), Some(5_000), None],
        ),
    };
    let label = move |value: Option<usize>| match (menu, value) {
        (_, None) => tr("ui.unlimited").to_string(),
        (HistoryLimitMenu::Retention, Some(days)) => {
            tr("history.retention_days").replace("{count}", &days.to_string())
        }
        (HistoryLimitMenu::Entries, Some(entries)) => {
            tr("history.max_entries").replace("{count}", &entries.to_string())
        }
    };
    let open = view.history_menu == Some(menu);
    let menu_height = 34. + 4. + 8. + 32. * options.len() as f32;
    settings_row()
        .relative()
        .child(row_label(tokens, title, None))
        .child(
            select_trigger(tokens, id, label(selected), LIMIT_WIDTH, true).on_click(cx.listener(
                move |this, _event, _window, cx| {
                    this.history_menu = if this.history_menu == Some(menu) {
                        None
                    } else {
                        Some(menu)
                    };
                    cx.notify();
                },
            )),
        )
        .when(open, |row| {
            row.child(
                div()
                    .id(SharedString::from(format!("{id}-boundary")))
                    .absolute()
                    .top(px(13.))
                    .right(px(14.))
                    .w(px(LIMIT_WIDTH))
                    .h(px(menu_height))
                    .on_mouse_down_out(cx.listener(|this, _event, _window, cx| {
                        this.history_menu = None;
                        cx.notify();
                    })),
            )
            .child(
                deferred(
                    menu_panel(tokens, LIMIT_WIDTH)
                        .id(SharedString::from(format!("{id}-menu")))
                        .absolute()
                        .top(px(51.))
                        .right(px(14.))
                        .children(options.into_iter().map(|value| {
                            menu_option(
                                tokens,
                                SharedString::from(format!("{id}-{value:?}")),
                                label(value),
                                value == selected,
                                false,
                            )
                            .on_click(cx.listener(
                                move |this, _event, _window, cx| {
                                    this.history_menu = None;
                                    let current = this.phase.prefs().history();
                                    let next = match menu {
                                        HistoryLimitMenu::Retention => HistoryPolicy::new(
                                            value.map(|days| days as u32),
                                            current.max_entries(),
                                        ),
                                        HistoryLimitMenu::Entries => {
                                            HistoryPolicy::new(current.retention_days(), value)
                                        }
                                    }
                                    .expect("validated history option");
                                    this.commit_history_policy(next, cx);
                                },
                            ))
                        })),
                )
                .with_priority(1),
            )
        })
}

// Preserve the visible position while discarding heights cached before a data/width change.
pub(super) fn invalidate_list(state: &gpui::ListState, entry_count: usize) {
    let mut top = state.logical_scroll_top();
    let item_count = entry_count + 1; // The settings header is the first virtual item.
    if top.item_ix >= item_count {
        top.item_ix = item_count - 1;
        top.offset_in_item = px(0.);
    }
    state.reset(item_count);
    state.scroll_to(top);
}

#[cfg(test)]
mod virtual_history_tests {
    use super::*;
    use gpui::{ListAlignment, ListOffset, ListState};

    #[test]
    fn invalidation_preserves_visible_position_and_clamps_pruned_entries() {
        let state = ListState::new(501, ListAlignment::Top, px(200.)).measure_all();
        state.scroll_to(ListOffset {
            item_ix: 100,
            offset_in_item: px(17.),
        });
        invalidate_list(&state, 499);
        assert_eq!(state.item_count(), 500);
        assert_eq!(state.logical_scroll_top().item_ix, 100);
        assert_eq!(state.logical_scroll_top().offset_in_item, px(17.));
        invalidate_list(&state, 3);
        assert_eq!(state.item_count(), 4);
        assert_eq!(state.logical_scroll_top().item_ix, 3);
        assert_eq!(state.logical_scroll_top().offset_in_item, px(0.));
        invalidate_list(&state, 0);
        assert_eq!(state.item_count(), 1);
        assert_eq!(state.logical_scroll_top().item_ix, 0);
    }
}
