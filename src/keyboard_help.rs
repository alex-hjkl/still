use super::*;

impl PlayerView {
    pub(super) fn help_overlay(&self, window: &Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        let width =
            (f32::from(window.viewport_size().width) - self.sidebar_width() - 32.).min(640.);
        div()
            .id("help-backdrop")
            .absolute()
            .inset_0()
            .bg(gpui::rgba(0x25302928))
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
                    .bg(rgb(BG))
                    .rounded_lg()
                    .border_1()
                    .border_color(rgb(BORDER))
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
                                    .text_size(px(20.))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child("Keyboard shortcuts"),
                            )
                            .child(button("close-help", "Close · Esc").on_click(cx.listener(
                                |this, _, window, cx| {
                                    this.show_help = false;
                                    window.focus(&this.library_focus);
                                    cx.notify();
                                },
                            ))),
                    )
                    .child(self.keyboard_help(width - 32.)),
            )
            .into_any_element()
    }

    fn keyboard_help(&self, width: f32) -> gpui::AnyElement {
        let column_width = width - 8.;
        let groups = [
            (
                "Navigation & browsing",
                vec![
                    ("j / k", "Next / previous item"),
                    ("h / l", "Back / open or play"),
                    ("Enter", "Open or play selected item"),
                    ("Shift+Enter", "Play selected folder, artist or album"),
                    ("Backspace", "Go back"),
                    ("gg / G", "First / last item"),
                    ("Ctrl+D / U", "Half a page down / up"),
                    ("Ctrl+F / B", "Page down / up"),
                    ("/ · ⌘F", "Find in loaded list"),
                    ("Enter / Esc", "Return to list; Esc again clears"),
                ],
            ),
            (
                "Playback & app",
                vec![
                    ("Space", "Play / pause"),
                    ("s", "Toggle shuffle"),
                    ("[ / ]", "Previous / next song"),
                    ("← / → · H / L", "Seek back / forward 10s"),
                    ("− / +", "Volume down / up 5%"),
                    ("f / F", "Toggle like: selected / playing"),
                    ("⌘1–⌘5", "Switch sections; hold ⌘ for hints"),
                    ("Tab / Shift+Tab", "Next / previous section"),
                    ("r / e", "Refresh / load more"),
                    ("? · F1 · Esc", "Open / close keyboard help"),
                    ("⌘B", "Expand / collapse sidebar"),
                    ("⌘Q", "Quit Still"),
                ],
            ),
        ];
        div()
            .id("keyboard-help")
            .flex_1()
            .min_h_0()
            .min_w_0()
            .overflow_y_scroll()
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
                                    .text_sm()
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
                                            .text_sm()
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .text_color(rgb(ACCENT))
                                            .child(keys),
                                    )
                                    .child(
                                        div()
                                            .w(px(column_width - 140.))
                                            .flex_none()
                                            .min_w_0()
                                            .text_sm()
                                            .text_color(rgb(MUTED))
                                            .whitespace_normal()
                                            .child(text),
                                    )
                            }))
                    })),
            )
            .into_any_element()
    }
}
