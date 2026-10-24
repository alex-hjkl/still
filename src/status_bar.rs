use super::*;

impl PlayerView {
    pub(super) fn status_bar(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let colors = crate::theme::colors(cx);
        let mut hints = vec![("j k", "Move")];
        match self.browser.state().page {
            Page::Artists => hints.extend([("Enter", "Open"), ("⇧ Enter", "Play artist")]),
            Page::Albums | Page::Artist(_) => {
                hints.extend([("Enter", "Open"), ("⇧ Enter", "Play album")])
            }
            Page::Folder(_) => hints.extend([
                ("Enter", "Open / play"),
                ("h", "Parent"),
                ("⇧ Enter", "Play folder"),
            ]),
            Page::Album(_) => hints.extend([
                ("Enter", "Play song"),
                ("⇧ Enter", "Play album"),
                ("f", "Like"),
            ]),
            Page::Liked => hints.extend([
                ("Enter", "Play song"),
                ("⇧ Enter", "Play all"),
                ("f", "Unlike"),
            ]),
            Page::Songs => hints.extend([("Enter", "Play"), ("f", "Like")]),
            Page::Search(_) => hints.extend([("Enter", "Open / play"), ("Esc", "Back")]),
        }
        hints.push(("/", "Search"));
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
            .when_some(self.pending_collection.as_ref(), |view, collection| {
                view.child(
                    div()
                        .min_w_0()
                        .truncate()
                        .child(format!("Loading {}…", collection.name())),
                )
            })
            .when(self.pending_collection.is_none(), |view| {
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
                        .child(if self.browser.is_loading() {
                            "Loading…"
                        } else {
                            "Load more · e"
                        })
                        .on_click(cx.listener(|this, _, _, cx| this.load_more(cx))),
                )
            })
            .into_any_element()
    }
}
