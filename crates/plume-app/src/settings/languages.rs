use super::i18n::tr;
use super::view::{
    menu_option, menu_panel, row_label, select_trigger, settings_divider, settings_group,
    settings_row,
};
use super::SettingsView;
use crate::prefs::{InterfaceLanguage, LanguagePref};
use gpui::{anchored, deferred, div, prelude::*, px, AnyElement, Context, SharedString, Window};
use plume_ui::Tokens;
use std::sync::OnceLock;

const SELECT_WIDTH: f32 = 168.;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum LanguageMenu {
    Interface,
    Dictation,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LanguageChoice {
    Interface(InterfaceLanguage),
    Dictation(LanguagePref),
}

impl LanguageChoice {
    fn label(self) -> &'static str {
        match self {
            Self::Interface(language) => language.label(),
            Self::Dictation(LanguagePref::Auto) => tr("ui.automatic"),
            Self::Dictation(language) => language.label(),
        }
    }
}

impl LanguageMenu {
    fn id(self) -> &'static str {
        match self {
            Self::Interface => "interface-language",
            Self::Dictation => "dictation-language",
        }
    }
    fn focus_index(self) -> usize {
        match self {
            Self::Interface => 0,
            Self::Dictation => 1,
        }
    }
    fn choices(self) -> &'static [LanguageChoice] {
        match self {
            Self::Interface => {
                static CHOICES: OnceLock<Vec<LanguageChoice>> = OnceLock::new();
                CHOICES.get_or_init(|| {
                    InterfaceLanguage::available()
                        .map(LanguageChoice::Interface)
                        .collect()
                })
            }
            Self::Dictation => &[
                LanguageChoice::Dictation(LanguagePref::Auto),
                LanguageChoice::Dictation(LanguagePref::French),
                LanguageChoice::Dictation(LanguagePref::English),
            ],
        }
    }
    fn selected(self, view: &SettingsView) -> LanguageChoice {
        match self {
            Self::Interface => LanguageChoice::Interface(view.phase.prefs().interface_language()),
            Self::Dictation => LanguageChoice::Dictation(view.phase.prefs().language()),
        }
    }
}

impl SettingsView {
    fn open_language_menu(&mut self, menu: LanguageMenu, cx: &mut Context<Self>) {
        self.language_menu = Some(menu);
        self.insertion_open = false;
        self.history_menu = None;
        self.language_menu_index = menu
            .choices()
            .iter()
            .position(|choice| *choice == menu.selected(self))
            .unwrap_or(0);
        self.language_scroll
            .scroll_to_item(self.language_menu_index);
        cx.notify();
    }
    fn choose_language(&mut self, choice: LanguageChoice, cx: &mut Context<Self>) {
        match choice {
            LanguageChoice::Interface(language) => self.commit_interface_language(language, cx),
            LanguageChoice::Dictation(language) => self.commit_language(language, cx),
        }
    }
    fn language_key(
        &mut self,
        menu: LanguageMenu,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match key {
            "escape" | "esc" | "tab" => self.language_menu = None,
            "enter" | "space" if self.language_menu == Some(menu) => {
                if let Some(choice) = menu.choices().get(self.language_menu_index).copied() {
                    self.choose_language(choice, cx);
                }
            }
            "enter" | "space" | "up" | "down" if self.language_menu != Some(menu) => {
                self.open_language_menu(menu, cx)
            }
            "up" => self.language_menu_index = self.language_menu_index.saturating_sub(1),
            "down" => {
                self.language_menu_index =
                    (self.language_menu_index + 1).min(menu.choices().len() - 1)
            }
            "home" => self.language_menu_index = 0,
            "end" => self.language_menu_index = menu.choices().len() - 1,
            _ => return,
        }
        if key != "tab" {
            cx.stop_propagation();
            window.prevent_default();
        }
        self.language_scroll
            .scroll_to_item(self.language_menu_index);
        cx.notify();
    }
}

pub(super) fn language_choices(
    view: &SettingsView,
    tokens: &Tokens,
    cx: &mut Context<SettingsView>,
) -> AnyElement {
    settings_group(tokens)
        .child(interface_language_row(view, tokens, cx))
        .child(settings_divider(tokens))
        .child(dictation_language_row(view, tokens, cx))
        .into_any_element()
}

pub(super) fn interface_language_row(
    view: &SettingsView,
    tokens: &Tokens,
    cx: &mut Context<SettingsView>,
) -> AnyElement {
    language_row(view, tokens, LanguageMenu::Interface, true, cx)
}

pub(super) fn dictation_language_row(
    view: &SettingsView,
    tokens: &Tokens,
    cx: &mut Context<SettingsView>,
) -> AnyElement {
    let busy = view
        .session_control
        .as_ref()
        .is_some_and(|control| control.is_busy())
        || matches!(
            view.phase,
            crate::phase::AppPhase::Onboarding {
                status: crate::phase::OnboardStatus::Fetching { .. }
                    | crate::phase::OnboardStatus::Activating,
                ..
            }
        );
    language_row(view, tokens, LanguageMenu::Dictation, !busy, cx)
}

fn language_row(
    view: &SettingsView,
    tokens: &Tokens,
    menu: LanguageMenu,
    enabled: bool,
    cx: &mut Context<SettingsView>,
) -> AnyElement {
    let selected = menu.selected(view);
    let open = enabled && view.language_menu == Some(menu);
    let focus_index = menu.focus_index();
    let (title, detail) = match menu {
        LanguageMenu::Interface => ("language.interface", "settings.language.interface_detail"),
        LanguageMenu::Dictation => ("language.dictation", "settings.language.dictation_detail"),
    };
    settings_row()
        .child(row_label(tokens, tr(title), Some(tr(detail))))
        .child(
            div()
                .id(menu.id())
                .relative()
                .flex_shrink_0()
                .child(
                    select_trigger(
                        tokens,
                        SharedString::from(format!("{}-select", menu.id())),
                        selected.label(),
                        SELECT_WIDTH,
                        enabled,
                    )
                    .track_focus(&view.language_focus[focus_index])
                    .when(enabled, |el| {
                        el.tab_index(0)
                            .on_click(cx.listener(move |view, _, window, cx| {
                                window.focus(&view.language_focus[focus_index]);
                                if open {
                                    view.language_menu = None;
                                    cx.notify();
                                } else {
                                    view.open_language_menu(menu, cx);
                                }
                            }))
                            .on_key_down(cx.listener(
                                move |view, event: &gpui::KeyDownEvent, window, cx| {
                                    view.language_key(
                                        menu,
                                        event.keystroke.key.as_str(),
                                        window,
                                        cx,
                                    );
                                },
                            ))
                    }),
                )
                .when(open, |el| {
                    el.child(
                        div().absolute().top(px(38.)).left_0().child(
                            deferred(
                                anchored().snap_to_window().child(
                                    menu_panel(tokens, SELECT_WIDTH)
                                        .id(SharedString::from(format!("{}-menu", menu.id())))
                                        .on_mouse_down_out(cx.listener(move |view, _, _, cx| {
                                            if view.language_menu == Some(menu) {
                                                view.language_menu = None;
                                                cx.notify();
                                            }
                                        }))
                                        .child(
                                            div()
                                                .id(SharedString::from(format!(
                                                    "{}-options",
                                                    menu.id()
                                                )))
                                                .max_h(px(240.))
                                                .overflow_y_scroll()
                                                .track_scroll(&view.language_scroll)
                                                .flex()
                                                .flex_col()
                                                .children(
                                                    menu.choices().iter().copied().enumerate().map(
                                                        |(index, choice)| {
                                                            menu_option(
                                                                tokens,
                                                                SharedString::from(format!(
                                                                    "{}-{index}",
                                                                    menu.id()
                                                                )),
                                                                choice.label(),
                                                                choice == selected,
                                                                index == view.language_menu_index,
                                                            )
                                                            .on_click(cx.listener(
                                                                move |view, _, _, cx| {
                                                                    view.choose_language(choice, cx)
                                                                },
                                                            ))
                                                        },
                                                    ),
                                                ),
                                        ),
                                ),
                            )
                            .with_priority(2),
                        ),
                    )
                }),
        )
        .into_any_element()
}

pub(super) fn compatibility_warning(view: &SettingsView, tokens: &Tokens) -> Option<AnyElement> {
    let prefs = view.phase.prefs();
    (!prefs.model.supports(prefs.language())).then(|| {
        super::view::error_text(tokens, "models.language_incompatible".into()).into_any_element()
    })
}
