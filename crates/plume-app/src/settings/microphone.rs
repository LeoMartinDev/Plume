use super::i18n::tr;
use super::view::{menu_option, menu_panel, row_label, select_trigger, settings_row};
use super::SettingsView;
use crate::prefs::Microphone;
use gpui::{anchored, deferred, div, prelude::*, px, AnyElement, Context, SharedString};
use plume_ui::Tokens;

const SELECT_WIDTH: f32 = 240.;

impl SettingsView {
    fn toggle_microphone_menu(&mut self, cx: &mut Context<Self>) {
        if self.microphone_menu.take().is_none() {
            self.language_menu = None;
            self.insertion_open = false;
            self.history_menu = None;
            // Listed on open so devices plugged in since the last look appear.
            self.microphone_menu = Some(plume_audio::input_devices().unwrap_or_else(|error| {
                tracing::warn!("microphone list unavailable: {error}");
                Vec::new()
            }));
        }
        cx.notify();
    }
}

/// The system default first, then connected devices, then the saved device
/// if it is disconnected so the current choice stays visible.
fn choices(
    selected: Option<&Microphone>,
    devices: &[plume_audio::InputDevice],
) -> Vec<(Option<Microphone>, bool)> {
    let mut choices = vec![(None, true)];
    choices.extend(devices.iter().map(|device| {
        (
            Some(Microphone {
                id: device.id.clone(),
                name: device.name.clone(),
            }),
            true,
        )
    }));
    if let Some(selected) = selected {
        if !devices.iter().any(|device| device.id == selected.id) {
            choices.push((Some(selected.clone()), false));
        }
    }
    choices
}

fn label(microphone: Option<&Microphone>, connected: bool) -> SharedString {
    match microphone {
        None => tr("ui.system_default").into(),
        Some(microphone) if connected => microphone.name.clone().into(),
        Some(microphone) => tr("settings.microphone.disconnected")
            .replace("{name}", &microphone.name)
            .into(),
    }
}

pub(super) fn microphone_row(
    view: &SettingsView,
    tokens: &Tokens,
    cx: &mut Context<SettingsView>,
) -> AnyElement {
    let selected = view.phase.prefs().microphone();
    let connected = view.microphone_menu.as_ref().is_none_or(|devices| {
        selected.is_none_or(|selected| devices.iter().any(|device| device.id == selected.id))
    });
    settings_row()
        .child(row_label(
            tokens,
            tr("ui.input_device"),
            Some(tr("settings.microphone.detail")),
        ))
        .child(
            div()
                .relative()
                .flex_shrink_0()
                .child(
                    select_trigger(
                        tokens,
                        "microphone-select",
                        label(selected, connected),
                        SELECT_WIDTH,
                        true,
                    )
                    .on_click(cx.listener(|view, _, _, cx| view.toggle_microphone_menu(cx))),
                )
                .children(view.microphone_menu.as_ref().map(|devices| {
                    div().absolute().top(px(38.)).left_0().child(
                        deferred(
                            anchored().snap_to_window().child(
                                menu_panel(tokens, SELECT_WIDTH)
                                    .id("microphone-menu")
                                    .on_mouse_down_out(cx.listener(|view, _, _, cx| {
                                        view.microphone_menu = None;
                                        cx.notify();
                                    }))
                                    .child(
                                        div()
                                            .id("microphone-options")
                                            .max_h(px(240.))
                                            .overflow_y_scroll()
                                            .flex()
                                            .flex_col()
                                            .children(
                                                choices(selected, devices)
                                                    .into_iter()
                                                    .enumerate()
                                                    .map(|(index, (choice, connected))| {
                                                        let active = choice.as_ref() == selected;
                                                        menu_option(
                                                            tokens,
                                                            SharedString::from(format!(
                                                                "microphone-{index}"
                                                            )),
                                                            label(choice.as_ref(), connected),
                                                            active,
                                                            false,
                                                        )
                                                        .on_click(cx.listener(
                                                            move |view, _, _, cx| {
                                                                view.commit_microphone(
                                                                    choice.clone(),
                                                                    cx,
                                                                )
                                                            },
                                                        ))
                                                    }),
                                            ),
                                    ),
                            ),
                        )
                        .with_priority(2),
                    )
                })),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(id: &str) -> plume_audio::InputDevice {
        plume_audio::InputDevice {
            id: id.into(),
            name: format!("Mic {id}"),
        }
    }

    fn microphone(id: &str) -> Microphone {
        Microphone {
            id: id.into(),
            name: format!("Mic {id}"),
        }
    }

    #[test]
    fn system_default_comes_first_then_connected_devices() {
        assert_eq!(
            choices(None, &[device("a"), device("b")]),
            vec![
                (None, true),
                (Some(microphone("a")), true),
                (Some(microphone("b")), true),
            ]
        );
    }

    #[test]
    fn a_disconnected_choice_stays_listed_last() {
        assert_eq!(
            choices(Some(&microphone("gone")), &[device("a")]),
            vec![
                (None, true),
                (Some(microphone("a")), true),
                (Some(microphone("gone")), false),
            ]
        );
        assert_eq!(choices(Some(&microphone("a")), &[device("a")]).len(), 2);
    }
}
