use crate::settings::i18n::tr;
use gpui::{
    deferred, div, list, prelude::*, px, svg, AnyElement, Context, ElementId, FontWeight,
    SharedString,
};
use plume_ui::Tokens;

use super::view::{
    error_text, page, section_title, settings_divider, settings_group, settings_row,
    settings_section,
};
use super::{HistoryLimitMenu, SettingsView};
use crate::history_policy::HistoryPolicy;

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
                    .pt(px(24.))
                    .pb(px(10.))
                    .child(history_header(view, &tokens, cx))
                    .into_any_element()
            } else {
                div()
                    .when(index == view.display_history().len(), |el| el.pb(px(24.)))
                    .child(history_entry(view, index - 1, &tokens, cx))
                    .into_any_element()
            };
            div().w_full().px(px(28.)).child(item).into_any_element()
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
        .gap(px(20.))
        .child(settings_section(
            tokens,
            tr("ui.storage"),
            settings_group(tokens)
                .flex()
                .flex_col()
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
                    .child(action_button(
                        tokens,
                        "history-clear",
                        tr("ui.clear_all"),
                        cx,
                        |this, cx| this.clear_history(cx),
                    )),
            )
        })
        .when(entries.is_empty(), |list| {
            list.child(settings_section(
                tokens,
                tr("ui.transcriptions"),
                settings_group(tokens)
                    .px(px(14.))
                    .py(px(18.))
                    .text_sm()
                    .text_color(tokens.muted)
                    .child(tr("ui.no_transcriptions_yet")),
            ))
        });

    page(tr("ui.history"), body)
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
    div()
        .id(("history-entry", id))
        .max_w(px(480.))
        .bg(tokens.group)
        .border_color(tokens.hairline)
        .border_x_1()
        .border_b_1()
        .when(index == 0, |el| el.border_t_1().rounded_t(px(10.)))
        .when(index + 1 == view.display_history().len(), |el| {
            el.rounded_b(px(10.))
        })
        .w_full()
        .px(px(14.))
        .py(px(12.))
        .flex()
        .flex_col()
        .gap(px(7.))
        .when(audio_available, |row| {
            let record_id = record_id.unwrap();
            row.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div()
                            .text_xs()
                            .text_color(tokens.muted)
                            .child(tr("ui.audio_available")),
                    )
                    .child(action_button(
                        tokens,
                        SharedString::from(format!("retry-{id}")),
                        tr("ui.retranscribe"),
                        cx,
                        move |this, cx| this.retry_recording(record_id, cx),
                    ))
                    .child(action_button(
                        tokens,
                        SharedString::from(format!("delete-audio-{id}")),
                        tr("ui.delete_audio"),
                        cx,
                        move |this, cx| this.delete_audio(record_id, cx),
                    )),
            )
        })
        .children(
            entry
                .transcription_error
                .clone()
                .map(|error| error_text(tokens, error)),
        )
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(7.))
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::MEDIUM)
                                .child(entry.status.clone()),
                        )
                        .child(
                            div().text_xs().text_color(tokens.muted).child(
                                entry
                                    .application
                                    .clone()
                                    .unwrap_or_else(|| tr("ui.unknown_app").into()),
                            ),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(
                            div()
                                .text_xs()
                                .text_color(if copied { tokens.accent } else { tokens.muted })
                                .child(if copied {
                                    tr("ui.copied").to_string()
                                } else {
                                    super::i18n::age_label(entry.created_at)
                                }),
                        )
                        .child(icon_button(
                            tokens,
                            ("history-copy", id),
                            "fluent/copy.svg",
                            cx,
                            move |this, cx| {
                                this.copy_history(id, copy_text.clone(), cx);
                            },
                        ))
                        .child(icon_button(
                            tokens,
                            ("history-delete", id),
                            "fluent/delete.svg",
                            cx,
                            move |this, cx| {
                                this.delete_history(id, cx);
                            },
                        )),
                ),
        )
        .child(
            div()
                .text_sm()
                .max_h(px(64.))
                .overflow_hidden()
                .child(entry.text.clone()),
        )
        .into_any_element()
}

fn action_button(
    tokens: &Tokens,
    id: impl Into<ElementId>,
    label: &'static str,
    cx: &mut Context<SettingsView>,
    on_click: impl Fn(&mut SettingsView, &mut Context<SettingsView>) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .h(px(28.))
        .px(px(9.))
        .flex()
        .items_center()
        .rounded(px(6.))
        .border_1()
        .border_color(tokens.hairline)
        .bg(tokens.fill)
        .text_xs()
        .cursor_pointer()
        .hover(|style| style.bg(tokens.fill_hover))
        .on_click(cx.listener(move |this, _event, _window, cx| on_click(this, cx)))
        .child(label)
}

fn icon_button(
    tokens: &Tokens,
    id: impl Into<ElementId>,
    icon: &'static str,
    cx: &mut Context<SettingsView>,
    on_click: impl Fn(&mut SettingsView, &mut Context<SettingsView>) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .size(px(24.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(5.))
        .text_color(tokens.muted)
        .cursor_pointer()
        .hover(|style| style.bg(tokens.fill_hover).text_color(tokens.text))
        .on_click(cx.listener(move |this, _event, _window, cx| on_click(this, cx)))
        .child(svg().path(icon).size(px(15.)).text_color(tokens.muted))
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
    let menu_height = 46. + 30. * options.len() as f32;
    settings_row()
        .relative()
        .child(div().flex_1().min_w_0().text_sm().child(title))
        .child(
            div()
                .id(id)
                .w(px(156.))
                .flex_shrink_0()
                .h(px(32.))
                .px(px(10.))
                .flex()
                .items_center()
                .justify_between()
                .rounded(px(6.))
                .border_1()
                .border_color(tokens.hairline)
                .bg(tokens.fill)
                .text_sm()
                .cursor_pointer()
                .hover(|style| style.bg(tokens.fill_hover))
                .on_click(cx.listener(move |this, _event, _window, cx| {
                    this.history_menu = if this.history_menu == Some(menu) {
                        None
                    } else {
                        Some(menu)
                    };
                    cx.notify();
                }))
                .child(label(selected))
                .child(
                    svg()
                        .path("fluent/chevron-down.svg")
                        .size(px(14.))
                        .text_color(tokens.muted),
                ),
        )
        .when(open, |row| {
            row.child(
                div()
                    .id(SharedString::from(format!("{id}-boundary")))
                    .absolute()
                    .top(px(10.))
                    .right(px(14.))
                    .w(px(156.))
                    .h(px(menu_height))
                    .on_mouse_down_out(cx.listener(|this, _event, _window, cx| {
                        this.history_menu = None;
                        cx.notify();
                    })),
            )
            .child(
                deferred(
                    div()
                        .id(SharedString::from(format!("{id}-menu")))
                        .absolute()
                        .top(px(46.))
                        .right(px(14.))
                        .w(px(156.))
                        .p(px(4.))
                        .flex()
                        .flex_col()
                        .rounded(px(7.))
                        .border_1()
                        .border_color(tokens.hairline)
                        .bg(tokens.elevated)
                        .shadow_md()
                        .children(options.into_iter().map(|value| {
                            div()
                                .id(SharedString::from(format!("{id}-{value:?}")))
                                .h(px(30.))
                                .px(px(8.))
                                .flex()
                                .items_center()
                                .rounded(px(5.))
                                .text_sm()
                                .when(value == selected, |option| {
                                    option.bg(tokens.fill).font_weight(FontWeight::MEDIUM)
                                })
                                .cursor_pointer()
                                .hover(|style| style.bg(tokens.fill_hover))
                                .on_click(cx.listener(move |this, _event, _window, cx| {
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
                                }))
                                .child(label(value))
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
