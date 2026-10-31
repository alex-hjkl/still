use super::*;

impl PlayerView {
    pub(super) fn scroll_help(&self, delta: f32, cx: &mut Context<Self>) {
        let mut offset = self.help_scroll.offset();
        offset.y = (offset.y + px(delta)).clamp(-self.help_scroll.max_offset().height, px(0.));
        self.help_scroll.set_offset(offset);
        cx.notify();
    }

    pub(super) fn help_overlay(&self, window: &Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        let colors = crate::theme::colors(cx);
        let width =
            (f32::from(window.viewport_size().width) - self.sidebar_width() - 32.).min(640.);
        div()
            .id("help-backdrop")
            .absolute()
            .inset_0()
            .bg(gpui::rgba(colors.backdrop))
            .occlude()
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.show_help = false;
                    window.focus(&this.library_focus);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .id("help-panel")
                    .absolute()
                    .right(px(16.))
                    .top(px(16.))
                    .bottom(px(16.))
                    .w(px(width))
                    .p_4()
                    .bg(rgb(colors.bg))
                    .rounded_lg()
                    .border_1()
                    .border_color(rgb(colors.border))
                    .shadow_lg()
                    .occlude()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .flex()
                            .flex_shrink_0()
                            .items_center()
                            .gap_3()
                            .child(
                                div()
                                    .flex_1()
                                    .child(
                                        div()
                                            .text_size(px(22.))
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .child("Keyboard shortcuts"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(TEXT_CAPTION))
                                            .text_color(rgb(colors.muted))
                                            .child("j / k · ↑ / ↓ Scroll"),
                                    ),
                            )
                            .child(
                                button(colors, "close-help", shortcuts::ESCAPE.hint("Close"))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.show_help = false;
                                        window.focus(&this.library_focus);
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(self.keyboard_help(colors, width - 32.)),
            )
            .into_any_element()
    }

    fn keyboard_help(&self, colors: Palette, width: f32) -> gpui::AnyElement {
        let column_width = width - 8.;
        let groups = shortcuts::help_groups();
        div()
            .id("keyboard-help")
            .flex_1()
            .min_h_0()
            .min_w_0()
            .overflow_y_scroll()
            .track_scroll(&self.help_scroll)
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_start()
                    .flex_col()
                    .gap_6()
                    .pr_2()
                    .children(groups.into_iter().map(|(title, rows)| {
                        div()
                            .w(px(column_width))
                            .min_w_0()
                            .flex_shrink_0()
                            .flex()
                            .flex_col()
                            .gap_3()
                            .child(
                                div()
                                    .text_size(px(TEXT_SECONDARY))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child(title),
                            )
                            .children(rows.into_iter().map(|(keys, text)| {
                                div()
                                    .flex()
                                    .w_full()
                                    .min_w_0()
                                    .flex_shrink_0()
                                    .items_start()
                                    .gap_3()
                                    .py_1()
                                    .child(
                                        div()
                                            .w(px(128.))
                                            .flex_none()
                                            .text_size(px(TEXT_SECONDARY))
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .text_color(rgb(colors.accent))
                                            .child(keys),
                                    )
                                    .child(
                                        div()
                                            .w(px(column_width - 140.))
                                            .flex_none()
                                            .min_w_0()
                                            .text_size(px(TEXT_SECONDARY))
                                            .text_color(rgb(colors.muted))
                                            .whitespace_normal()
                                            .child(text),
                                    )
                            }))
                    })),
            )
            .into_any_element()
    }
}
