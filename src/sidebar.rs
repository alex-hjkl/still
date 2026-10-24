use super::*;

impl PlayerView {
    pub(super) fn shortcut_badge(
        &self,
        colors: Palette,
        _id: &'static str,
        label: impl Into<SharedString>,
    ) -> gpui::Div {
        let label = label.into();
        #[cfg(feature = "ui-tests")]
        {
            self.ui_probe
                .borrow_mut()
                .shortcut_hints
                .insert(_id.into(), label.to_string());
        }
        div()
            .w(px(32.))
            .h(px(24.))
            .flex_shrink_0()
            .rounded_sm()
            .bg(rgb(colors.selected))
            .border_1()
            .border_color(rgb(colors.border))
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(TEXT_CAPTION))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_color(rgb(colors.accent))
            .child(label)
    }

    pub(super) fn sidebar_icon_slot(
        &self,
        _id: &'static str,
        content: impl IntoElement,
    ) -> gpui::Div {
        // The same 40px column anchors every icon in both sidebar modes.
        let slot = div()
            .relative()
            .w(px(40.))
            .h_full()
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .child(content);
        #[cfg(feature = "ui-tests")]
        {
            let probe = self.ui_probe.clone();
            slot.on_children_prepainted(move |children, _, _| {
                if let Some(bounds) = children.first() {
                    probe.borrow_mut().sidebar_icons.insert(
                        _id.into(),
                        [
                            f32::from(bounds.left() + bounds.size.width / 2.),
                            f32::from(bounds.top() + bounds.size.height / 2.),
                        ],
                    );
                }
            })
        }
        #[cfg(not(feature = "ui-tests"))]
        {
            slot
        }
    }

    pub(super) fn sidebar_toggle(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let colors = crate::theme::colors(cx);
        let compact = self.settings.value().sidebar_compact;
        let hints = window.is_window_active() && section_hints(window.modifiers());
        div()
            .id("toggle-sidebar")
            .size(px(32.))
            .flex_shrink_0()
            .rounded_md()
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .hover(move |view| view.bg(rgb(colors.hover)))
            .child(if hints {
                self.shortcut_badge(colors, "toggle-sidebar", "⌘B")
                    .into_any_element()
            } else {
                icon(
                    if compact {
                        "sidebar-expand"
                    } else {
                        "sidebar-collapse"
                    },
                    colors.muted,
                )
                .into_any_element()
            })
            .tooltip(move |_, cx| {
                cx.new(|_| {
                    Hint::new(if compact {
                        "Expand sidebar · ⌘B"
                    } else {
                        "Collapse sidebar · ⌘B"
                    })
                })
                .into()
            })
            .on_click(cx.listener(|this, _, _, cx| this.toggle_sidebar(cx)))
    }

    pub(super) fn sidebar_width(&self) -> f32 {
        if self.settings.value().sidebar_compact {
            SIDEBAR_COMPACT_WIDTH
        } else {
            SIDEBAR_EXPANDED_WIDTH
        }
    }

    pub(super) fn sidebar(&self, window: &Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        let colors = crate::theme::colors(cx);
        let compact = self.settings.value().sidebar_compact;
        let width = self.sidebar_width();
        let item_width = width - 24.;
        #[cfg(feature = "ui-tests")]
        {
            self.ui_probe.borrow_mut().sidebar_width = width;
        }
        let selected = if matches!(self.browser.state().page, Page::Search(_)) {
            usize::MAX
        } else {
            self.section_index()
        };
        let hints = window.is_window_active() && section_hints(window.modifiers());
        let server_label = if self.demo {
            "Demo · no playback".to_owned()
        } else {
            self.connection
                .read(cx)
                .client
                .as_ref()
                .map(Navidrome::server_label)
                .unwrap_or_default()
        };
        #[cfg(feature = "ui-tests")]
        let server_label = if std::env::var_os("STILL_DEMO_CAPTURE").is_some() {
            "Demo library".to_owned()
        } else {
            server_label
        };
        let disconnect_label = if self.connection.read(cx).saved {
            "Disconnect & forget"
        } else {
            "Disconnect"
        };
        div()
            .w(px(width))
            .flex_shrink_0()
            .h_full()
            .bg(rgb(colors.panel))
            .border_r_1()
            .border_color(rgb(colors.border))
            .flex()
            .flex_col()
            .items_start()
            .pl(px(12.))
            .py_4()
            .gap_2()
            .child(
                div()
                    .w(px(item_width))
                    .h(px(32.))
                    .flex_shrink_0()
                    .mb_5()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(self.sidebar_icon_slot("brand", app_icon(colors, 32.)))
                    .when(!compact, |view| view.child(wordmark(23.))),
            )
            .children(
                [
                    ("artists", "Artists", "Artists · ⌘1", Page::Artists),
                    ("albums", "Albums", "Albums · ⌘2", Page::Albums),
                    ("songs", "Songs", "Songs · ⌘3", Page::Songs),
                    (
                        "folders",
                        "Folders",
                        "Folders · ⌘4",
                        Page::Folder(String::new()),
                    ),
                    ("heart", "Liked songs", "Liked songs · ⌘5", Page::Liked),
                ]
                .into_iter()
                .enumerate()
                .map(|(index, (glyph, label, hint, page))| {
                    div()
                        .id(index)
                        .w(px(item_width))
                        .h(px(40.))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .gap_2()
                        .rounded_md()
                        .cursor_pointer()
                        .text_color(rgb(if selected == index {
                            colors.accent
                        } else {
                            colors.muted
                        }))
                        .when(selected == index, |view| view.bg(rgb(colors.selected)))
                        .hover(move |view| view.bg(rgb(colors.hover)))
                        .child(self.sidebar_icon_slot(
                            glyph,
                            if hints {
                                self.shortcut_badge(colors, glyph, format!("⌘{}", index + 1))
                                    .into_any_element()
                            } else {
                                icon(
                                    glyph,
                                    if selected == index {
                                        colors.accent
                                    } else {
                                        colors.muted
                                    },
                                )
                                .into_any_element()
                            },
                        ))
                        .when(!compact, |view| {
                            view.child(div().flex_1().min_w_0().truncate().child(label))
                        })
                        .tooltip(move |_, cx| cx.new(|_| Hint::new(hint)).into())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.browser.clear_history();
                            this.open_section(page.section(), cx);
                        }))
                }),
            )
            .child(div().flex_1())
            .child(
                div()
                    .id("connection-status")
                    .w(px(item_width))
                    .h(px(24.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_color(rgb(colors.muted))
                    .text_size(px(TEXT_CAPTION))
                    .child(self.sidebar_icon_slot(
                        "connection-status",
                        div().text_color(rgb(colors.accent)).child("●"),
                    ))
                    .when(!compact, |view| {
                        view.child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .child(server_label.clone()),
                        )
                    })
                    .tooltip(move |_, cx| cx.new(|_| Hint::new(server_label.clone())).into()),
            )
            .child(
                div()
                    .id("keyboard-help")
                    .w(px(item_width))
                    .h(px(40.))
                    .flex_shrink_0()
                    .rounded_md()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_color(rgb(colors.muted))
                    .cursor_pointer()
                    .when(self.show_help, |view| view.bg(rgb(colors.selected)))
                    .hover(move |view| view.bg(rgb(colors.hover)))
                    .child(
                        self.sidebar_icon_slot(
                            "keyboard-help",
                            div().text_size(px(20.)).child("?"),
                        ),
                    )
                    .when(!compact, |view| {
                        view.child(
                            div()
                                .text_size(px(TEXT_SECONDARY))
                                .child("Keyboard shortcuts"),
                        )
                    })
                    .tooltip(|_, cx| cx.new(|_| Hint::new("Keyboard shortcuts · ?")).into())
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.show_help = !this.show_help;
                        window.focus(&this.library_focus);
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .id("disconnect")
                    .w(px(item_width))
                    .h(px(32.))
                    .flex_shrink_0()
                    .rounded_md()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_color(rgb(colors.muted))
                    .cursor_pointer()
                    .hover(move |view| view.bg(rgb(colors.hover)))
                    .child(self.sidebar_icon_slot("disconnect", icon("logout", colors.muted)))
                    .when(!compact, |view| {
                        view.child(div().text_size(px(TEXT_SECONDARY)).child(disconnect_label))
                    })
                    .tooltip(move |_, cx| cx.new(|_| Hint::new(disconnect_label)).into())
                    .on_click(cx.listener(|this, _, window, cx| this.disconnect(window, cx))),
            )
            .into_any_element()
    }
}
