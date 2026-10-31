use super::*;

fn remember_checkbox(colors: Palette, checked: bool) -> gpui::Div {
    div()
        .size(px(18.))
        .flex_shrink_0()
        .rounded_sm()
        .border_1()
        .border_color(rgb(if checked {
            colors.accent
        } else {
            colors.input_border
        }))
        .bg(rgb(if checked {
            colors.accent
        } else {
            colors.input_bg
        }))
        .flex()
        .items_center()
        .justify_center()
        .when(checked, |view| {
            view.child(icon_sized("check", colors.on_accent, 12.))
        })
}

impl PlayerView {
    pub(super) fn connection_screen(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let colors = crate::theme::colors(cx);
        let form = div()
            .w(px(380.))
            .max_w_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(app_icon(colors, 48.))
                            .child(wordmark(32.)),
                    )
                    .child(div().text_size(px(26.)).child("Your music. Nothing extra."))
                    .child(
                        div()
                            .text_size(px(TEXT_SECONDARY))
                            .text_color(rgb(colors.muted))
                            .line_height(px(20.))
                            .whitespace_normal()
                            .child("Connect to Navidrome to listen to your library."),
                    ),
            )
            .child(
                div().flex_shrink_0().flex().flex_col().gap_3().children(
                    [
                        ("Server URL", self.server.clone()),
                        ("Username", self.username.clone()),
                        ("Password", self.password.clone()),
                    ]
                    .into_iter()
                    .map(|(label, input)| {
                        div()
                            .flex_shrink_0()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .text_size(px(TEXT_SECONDARY))
                                    .text_color(rgb(colors.muted))
                                    .child(label),
                            )
                            .child(input)
                    }),
                ),
            )
            .child(
                div()
                    .id("remember-connection")
                    .h(px(28.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_size(px(TEXT_SECONDARY))
                    .when(!self.connection.read(cx).is_busy(), |view| {
                        view.cursor_pointer()
                    })
                    .when(self.connection.read(cx).is_busy(), |view| view.opacity(0.6))
                    .child(remember_checkbox(colors, self.connection.read(cx).remember))
                    .child("Remember connection")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.connection
                            .update(cx, |connection, cx| connection.toggle_remember(cx));
                    })),
            )
            .children(self.failure_banners(cx))
            .child(
                button(
                    colors,
                    "connect",
                    match self.connection.read(cx).activity {
                        Activity::Idle => "Connect",
                        Activity::Restoring => "Restoring connection…",
                        Activity::Connecting => "Connecting…",
                        Activity::Saving => "Saving connection…",
                        Activity::Forgetting => "Forgetting connection…",
                    },
                )
                .bg(rgb(colors.accent))
                .text_color(rgb(colors.on_accent))
                .h(px(44.))
                .flex_shrink_0()
                .hover(move |s| s.bg(rgb(colors.accent_hover)))
                .on_click(cx.listener(|this, _, _, cx| this.connect(cx))),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(px(TEXT_CAPTION))
                    .line_height(px(18.))
                    .text_color(rgb(colors.muted))
                    .whitespace_normal()
                    .child("Your saved connection is protected by the system credential store."),
            )
            .when(self.connection.read(cx).saved, |view| {
                view.child(
                    button(colors, "forget-connection", "Forget connection")
                        .flex_shrink_0()
                        .text_color(rgb(colors.muted))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.connection
                                .update(cx, |connection, cx| connection.forget(cx))
                        })),
                )
            });
        #[cfg(feature = "ui-tests")]
        let form = {
            let probe = self.ui_probe.clone();
            form.on_children_prepainted(move |children, window, _| {
                let viewport = window.viewport_size();
                probe.borrow_mut().connection_fits = Some(children.iter().all(|bounds| {
                    bounds.left() >= px(0.)
                        && bounds.top() >= px(0.)
                        && bounds.right() <= viewport.width
                        && bounds.bottom() <= viewport.height
                }));
            })
        };
        div()
            .id("connection-scroll")
            .size_full()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.connection_scroll)
            .flex()
            .flex_col()
            .child(
                div()
                    .w_full()
                    .min_h_full()
                    .flex_shrink_0()
                    .p_6()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(form),
            )
            .into_any_element()
    }
}
