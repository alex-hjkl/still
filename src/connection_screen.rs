use super::*;

impl PlayerView {
    pub(super) fn connection_screen(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .w(px(380.))
                    .flex()
                    .flex_col()
                    .gap_5()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(
                                div()
                                    .size(px(44.))
                                    .bg(rgb(ACCENT))
                                    .rounded_xl()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(icon("songs", ON_ACCENT)),
                            )
                            .child(
                                div()
                                    .text_size(px(32.))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child("Still"),
                            ),
                    )
                    .child(
                        div()
                            .mt_3()
                            .text_size(px(26.))
                            .child("Your music. Nothing extra."),
                    )
                    .child(
                        div()
                            .text_color(rgb(MUTED))
                            .line_height(px(22.))
                            .child("Connect to Navidrome to listen to your library."),
                    )
                    .child(
                        div()
                            .mt_4()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(div().text_sm().text_color(rgb(MUTED)).child("Server URL"))
                            .child(self.server.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(div().text_sm().text_color(rgb(MUTED)).child("Username"))
                            .child(self.username.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(div().text_sm().text_color(rgb(MUTED)).child("Password"))
                            .child(self.password.clone()),
                    )
                    .child(
                        div()
                            .id("remember-connection")
                            .flex()
                            .items_center()
                            .gap_2()
                            .cursor_pointer()
                            .text_sm()
                            .child(div().w(px(18.)).text_color(rgb(ACCENT)).child(
                                if self.connection.read(cx).remember {
                                    "☑"
                                } else {
                                    "☐"
                                },
                            ))
                            .child("Remember connection")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.connection
                                    .update(cx, |connection, cx| connection.toggle_remember(cx));
                            })),
                    )
                    .children(self.failure_banners(cx))
                    .child(
                        button(
                            "connect",
                            match self.connection.read(cx).activity {
                                Activity::Idle => "Connect",
                                Activity::Restoring => "Restoring connection…",
                                Activity::Connecting => "Connecting…",
                                Activity::Saving => "Saving connection…",
                                Activity::Forgetting => "Forgetting connection…",
                            },
                        )
                        .bg(rgb(ACCENT))
                        .text_color(rgb(ON_ACCENT))
                        .h(px(44.))
                        .hover(|s| s.bg(rgb(ACCENT_HOVER)))
                        .on_click(cx.listener(|this, _, _, cx| this.connect(cx))),
                    )
                    .child(div().text_xs().text_color(rgb(MUTED)).child(
                        "Your saved connection is protected by the system credential store.",
                    ))
                    .when(self.connection.read(cx).saved, |view| {
                        view.child(
                            button("forget-connection", "Forget connection")
                                .text_color(rgb(MUTED))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.connection
                                        .update(cx, |connection, cx| connection.forget(cx))
                                })),
                        )
                    }),
            )
            .into_any_element()
    }
}
