use super::*;
use crate::theme::{INPUT_BG, INPUT_BORDER};

fn remember_checkbox(checked: bool) -> gpui::Div {
    div()
        .size(px(18.))
        .flex_shrink_0()
        .rounded_sm()
        .border_1()
        .border_color(rgb(if checked { ACCENT } else { INPUT_BORDER }))
        .bg(rgb(if checked { ACCENT } else { INPUT_BG }))
        .flex()
        .items_center()
        .justify_center()
        .when(checked, |view| {
            view.child(icon_sized("check", ON_ACCENT, 12.))
        })
}

impl PlayerView {
    pub(super) fn connection_screen(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
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
                            .child(app_icon(48.))
                            .child(wordmark(32.)),
                    )
                    .child(div().text_size(px(24.)).child("Your music. Nothing extra."))
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(MUTED))
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
                            .child(div().text_sm().text_color(rgb(MUTED)).child(label))
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
                    .text_sm()
                    .when(!self.connection.read(cx).is_busy(), |view| {
                        view.cursor_pointer()
                    })
                    .when(self.connection.read(cx).is_busy(), |view| view.opacity(0.6))
                    .child(remember_checkbox(self.connection.read(cx).remember))
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
                .flex_shrink_0()
                .hover(|s| s.bg(rgb(ACCENT_HOVER)))
                .on_click(cx.listener(|this, _, _, cx| this.connect(cx))),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .line_height(px(18.))
                    .text_color(rgb(MUTED))
                    .whitespace_normal()
                    .child("Your saved connection is protected by the system credential store."),
            )
            .when(self.connection.read(cx).saved, |view| {
                view.child(
                    button("forget-connection", "Forget connection")
                        .flex_shrink_0()
                        .text_color(rgb(MUTED))
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
