use super::*;
#[cfg(feature = "ui-tests")]
use crate::theme::Theme;

pub(super) struct ThemeMenu {
    cursor: usize,
    previous_focus: Option<FocusHandle>,
}

impl PlayerView {
    pub(super) fn open_theme_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.theme_menu = Some(ThemeMenu {
            cursor: ThemePreference::ALL
                .iter()
                .position(|value| *value == self.settings.value().theme)
                .unwrap_or(0),
            previous_focus: window.focused(cx),
        });
        window.focus(&self.library_focus);
        cx.notify();
    }

    pub(super) fn set_theme(&mut self, preference: ThemePreference, cx: &mut Context<Self>) {
        self.settings.update(|value| value.theme = preference);
        preference.apply(cx);
        cx.notify();
    }

    #[cfg(feature = "ui-tests")]
    pub(super) fn toggle_theme(&mut self, cx: &mut Context<Self>) {
        let preference = match cx.global::<Theme>() {
            Theme::Latte => ThemePreference::Macchiato,
            Theme::Macchiato => ThemePreference::Latte,
        };
        self.set_theme(preference, cx);
    }

    pub(super) fn close_theme_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(menu) = self.theme_menu.take() {
            if let Some(focus) = menu.previous_focus {
                window.focus(&focus);
            }
            cx.notify();
        }
    }

    pub(super) fn theme_menu_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(menu) = self.theme_menu.as_mut() else {
            return;
        };
        if event.keystroke.modifiers.platform {
            return;
        }
        cx.stop_propagation();
        window.prevent_default();
        match event.keystroke.key.as_str() {
            "escape" => self.close_theme_menu(window, cx),
            "up" | "k" => menu.cursor = (menu.cursor + 2) % 3,
            "down" | "j" => menu.cursor = (menu.cursor + 1) % 3,
            "tab" => {
                menu.cursor = (menu.cursor
                    + if event.keystroke.modifiers.shift {
                        2
                    } else {
                        1
                    })
                    % 3
            }
            "enter" | "space" => {
                let preference = ThemePreference::ALL[menu.cursor];
                self.close_theme_menu(window, cx);
                self.set_theme(preference, cx);
            }
            _ => {}
        }
        cx.notify();
    }

    pub(super) fn theme_toggle(&self, cx: &mut Context<Self>) -> gpui::Stateful<gpui::Div> {
        let colors = crate::theme::colors(cx);
        let glyph = match self.settings.value().theme {
            ThemePreference::System => "monitor",
            ThemePreference::Latte => "sun",
            ThemePreference::Macchiato => "moon",
        };
        toolbar_button(colors, "toggle-theme", glyph)
            .when(self.theme_menu.is_none(), |view| {
                view.tooltip(|_, cx| cx.new(|_| Hint::new("Appearance")).into())
            })
            .when(self.theme_menu.is_some(), |view| {
                view.bg(rgb(colors.selected))
            })
            .on_click(cx.listener(|this, _, window, cx| {
                if this.theme_menu.is_some() {
                    this.close_theme_menu(window, cx);
                } else {
                    this.open_theme_menu(window, cx);
                }
            }))
    }

    pub(super) fn theme_overlay(
        &self,
        connected: bool,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let colors = crate::theme::colors(cx);
        let cursor = self.theme_menu.as_ref().map_or(0, |menu| menu.cursor);
        #[cfg(feature = "ui-tests")]
        {
            self.ui_probe.borrow_mut().theme_menu_fits = Some(true);
        }
        #[cfg(feature = "ui-tests")]
        let panel_left = if connected {
            self.sidebar_width() + 60.
        } else {
            0.
        };
        div()
            .absolute()
            .inset_0()
            .occlude()
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.close_theme_menu(window, cx);
                    cx.stop_propagation();
                }),
            )
            .child(
                div()
                    .absolute()
                    .top(px(if connected { 64. } else { 56. }))
                    .when(connected, |view| view.left(px(self.sidebar_width() + 60.)))
                    .when(!connected, |view| view.right(px(16.)))
                    .w(px(232.))
                    .p_2()
                    .rounded_lg()
                    .bg(rgb(colors.panel))
                    .border_1()
                    .border_color(rgb(colors.border))
                    .shadow_lg()
                    .occlude()
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .children(ThemePreference::ALL.into_iter().enumerate().map(
                        |(index, preference)| {
                            let (glyph, title, subtitle) = match preference {
                                ThemePreference::System => ("monitor", "System", "Follow system"),
                                ThemePreference::Latte => ("sun", "Latte", "Light"),
                                ThemePreference::Macchiato => ("moon", "Macchiato", "Dark"),
                            };
                            let row = div()
                                .flex()
                                .items_center()
                                .gap_3()
                                .p_2()
                                .rounded_md()
                                .cursor_pointer()
                                .when(index == cursor, |view| view.bg(rgb(colors.hover)))
                                .hover(move |view| view.bg(rgb(colors.hover)))
                                .child(icon(glyph, colors.muted))
                                .child(
                                    div().flex_1().min_w_0().child(title).child(
                                        div()
                                            .text_size(px(TEXT_CAPTION))
                                            .text_color(rgb(colors.muted))
                                            .truncate()
                                            .child(subtitle),
                                    ),
                                )
                                .when(self.settings.value().theme == preference, |view| {
                                    view.child(icon("check", colors.accent))
                                });
                            #[cfg(feature = "ui-tests")]
                            let row = {
                                let probe = self.ui_probe.clone();
                                row.on_children_prepainted(move |children, window, _| {
                                    let left = if connected {
                                        panel_left
                                    } else {
                                        f32::from(window.viewport_size().width) - 248.
                                    };
                                    if !children.iter().all(|bounds| {
                                        bounds.left() >= px(left)
                                            && bounds.right() <= px(left + 232.)
                                    }) {
                                        probe.borrow_mut().theme_menu_fits = Some(false);
                                    }
                                })
                            };
                            row.id(("theme-choice", index)).on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.close_theme_menu(window, cx);
                                    this.set_theme(preference, cx);
                                },
                            ))
                        },
                    )),
            )
            .into_any_element()
    }
}
