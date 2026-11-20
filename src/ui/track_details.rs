use super::*;

impl PlayerView {
    pub(super) fn song_preview(
        &self,
        entry: &BrowserEntry,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let song = self.entry_song(entry).expect("selected song");
        let row = self.library.browser().state().cursor;
        let colors = crate::theme::colors(cx);
        let duration = if self
            .transport
            .queue()
            .current()
            .is_some_and(|current| current.id == song.id)
        {
            self.transport.duration()
        } else {
            song.duration
        };
        let duration_text = duration_label(duration);
        #[cfg(feature = "ui-tests")]
        {
            self.ui_probe.borrow_mut().preview_song = Some(song.id.clone());
        }
        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .id("track-preview")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .p_3()
                    .flex()
                    .flex_col()
                    .items_start()
                    .gap_4()
                    .child(artwork(
                        colors,
                        self.connection.read(cx).client(),
                        song.artwork_id(),
                        96.,
                    ))
                    .child(
                        div()
                            .w_full()
                            .flex_shrink_0()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .text_size(px(18.))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .whitespace_normal()
                                    .child(song.title.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(TEXT_SECONDARY))
                                    .text_color(rgb(colors.muted))
                                    .whitespace_normal()
                                    .child(song.artist.clone()),
                            ),
                    )
                    .child(
                        div()
                            .w_full()
                            .flex_shrink_0()
                            .text_size(px(TEXT_CAPTION))
                            .text_color(rgb(colors.muted))
                            .whitespace_normal()
                            .child(if song.album.is_empty() {
                                duration_text
                            } else {
                                format!("{} · {}", song.album, duration_text)
                            }),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                button(colors, "preview-play", "Play · Enter")
                                    .bg(rgb(colors.accent))
                                    .text_color(rgb(colors.on_accent))
                                    .hover(move |view| view.bg(rgb(colors.accent_hover)))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.play_row(row, cx);
                                    })),
                            )
                            .child(self.like_button(song, "preview-like", cx)),
                    ),
            )
            .into_any_element()
    }
}
