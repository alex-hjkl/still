use super::*;

impl PlayerView {
    pub(super) fn status_bar(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let colors = crate::theme::colors(cx);
        let hints = shortcuts::footer_hints(&self.browser.state().page);
        #[cfg(feature = "ui-tests")]
        {
            self.ui_probe.borrow_mut().footer_hints = hints
                .iter()
                .map(|(key, label)| format!("{key} {label}"))
                .collect();
        }
        let hints = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_x_4()
            .gap_y_1()
            .when_some(self.collections.read(cx).pending(), |view, collection| {
                view.child(
                    div()
                        .min_w_0()
                        .truncate()
                        .child(format!("Loading {}…", collection.name())),
                )
            })
            .when(self.collections.read(cx).pending().is_none(), |view| {
                view.children(hints.into_iter().map(|(key, label)| {
                    div()
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(
                            div()
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(rgb(colors.text))
                                .child(key),
                        )
                        .child(label)
                }))
            });
        #[cfg(feature = "ui-tests")]
        let hints = {
            let probe = self.ui_probe.clone();
            hints.on_children_prepainted(move |children, window, _| {
                let viewport = window.viewport_size();
                probe.borrow_mut().footer_fits = children
                    .iter()
                    .all(|bounds| bounds.left() >= px(0.) && bounds.right() <= viewport.width);
            })
        };
        div()
            .flex_shrink_0()
            .min_h(px(28.))
            .px_4()
            .py_1()
            .border_t_1()
            .border_color(rgb(colors.border))
            .flex()
            .items_center()
            .gap_4()
            .text_size(px(TEXT_CAPTION))
            .text_color(rgb(colors.muted))
            .child(hints)
            .when(self.browser.state().more, |view| {
                view.child(
                    div()
                        .id("load-more")
                        .flex_shrink_0()
                        .cursor_pointer()
                        .text_color(rgb(colors.accent))
                        .child(if self.library_loader.read(cx).is_loading() {
                            "Loading…".to_owned()
                        } else {
                            shortcuts::MORE.hint(shortcuts::MORE.label)
                        })
                        .on_click(cx.listener(|this, _, _, cx| this.load_more(cx))),
                )
            })
            .child(
                div()
                    .id("status-help")
                    .flex_shrink_0()
                    .cursor_pointer()
                    .child(format!("{} Shortcuts", shortcuts::HELP.footer))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.show_help = true;
                        window.focus(&this.library_focus);
                        cx.notify();
                    })),
            )
            .into_any_element()
    }
}
