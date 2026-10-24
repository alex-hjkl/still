use super::*;

impl PlayerView {
    pub(super) fn search_changed(
        &mut self,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let searching = matches!(self.browser.state().page, Page::Search(_));
        if text.trim().is_empty() {
            if searching {
                self.back(cx);
                self.focus_library = false;
                window.focus(&self.search.focus_handle(cx));
            }
            return;
        }
        if searching {
            if !self.browser.set_search(text) {
                return;
            }
            self.reset_library_load(cx);
            self.list_scroll.scroll_to_item(0, ScrollStrategy::Top);
        } else {
            self.remember_folder(cx);
            self.pending_folder = None;
            self.browser.navigate(Page::Search(text), true);
            self.reset_navigation_ui(cx);
            self.focus_library = false;
            window.focus(&self.search.focus_handle(cx));
        }
        if self.demo {
            self.load_demo_page();
            return;
        }
        let request_id = self.browser.request_id();
        let timer = cx.background_executor().timer(Duration::from_millis(250));
        self.search_delay = Some(cx.spawn(async move |this, cx| {
            timer.await;
            let _ = this.update(cx, |this, cx| {
                if this.browser.request_id() == request_id {
                    this.submit_search(cx);
                }
            });
        }));
    }

    pub(super) fn submit_search(&mut self, cx: &mut Context<Self>) {
        if matches!(self.browser.state().page, Page::Search(_))
            && !self.browser.state().loaded
            && !self.browser.is_loading()
        {
            self.search_delay = None;
            self.load(self.browser.request(), cx);
        }
    }

    pub(super) fn search_results(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let colors = crate::theme::colors(cx);
        let entries = self.visible_entries();
        if entries.is_empty() {
            let message = if self
                .failures
                .iter()
                .any(|(operation, _)| *operation == Operation::Library)
            {
                "Could not search your library"
            } else if !self.browser.state().loaded {
                "Searching your library…"
            } else {
                "No matches in your library"
            };
            return empty_state(colors, message);
        }
        uniform_list(
            ("search-results", self.browser.request_id() as usize),
            entries.len(),
            cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                range
                    .map(|row| {
                        let entry = entries[row].clone();
                        let (glyph, kind, title, detail, cover) = match &entry {
                            BrowserEntry::Artist(index) => {
                                let artist = &this.browser.state().artists[*index];
                                (
                                    "artists",
                                    "Artist",
                                    artist.name.clone(),
                                    count_label(artist.album_count, "album"),
                                    None,
                                )
                            }
                            BrowserEntry::Album(index) => {
                                let album = &this.browser.state().albums[*index];
                                (
                                    "albums",
                                    "Album",
                                    album.name.clone(),
                                    album.artist.clone(),
                                    album.cover_art.as_deref(),
                                )
                            }
                            BrowserEntry::Song(index) => {
                                let song = &this.browser.state().songs[*index];
                                (
                                    "songs",
                                    "Song",
                                    song.title.clone(),
                                    [&song.artist, &song.album]
                                        .into_iter()
                                        .filter(|part| !part.is_empty())
                                        .cloned()
                                        .collect::<Vec<_>>()
                                        .join(" · "),
                                    song.cover_art.as_deref(),
                                )
                            }
                            BrowserEntry::Folder(_) => unreachable!(),
                        };
                        let active = this.entry_song(&entry).is_some_and(|song| {
                            this.transport
                                .queue()
                                .current()
                                .is_some_and(|current| current.id == song.id)
                        });
                        track_row(colors, row, SONG_ROW_HEIGHT)
                            .when(this.cursor_on(row), |view| {
                                view.bg(rgb(colors.selected))
                                    .border_1()
                                    .border_color(rgb(colors.keyboard_focus))
                            })
                            .child(if cover.is_some() {
                                artwork(
                                    colors,
                                    this.connection.read(cx).client.as_ref(),
                                    cover,
                                    34.,
                                )
                            } else {
                                div()
                                    .size(px(34.))
                                    .flex_shrink_0()
                                    .rounded_md()
                                    .bg(rgb(colors.selected))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(icon(glyph, colors.accent))
                                    .into_any_element()
                            })
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .child(
                                        div()
                                            .truncate()
                                            .when(active, |view| {
                                                view.text_color(rgb(colors.accent))
                                            })
                                            .child(title),
                                    )
                                    .child(
                                        div()
                                            .truncate()
                                            .text_size(px(TEXT_CAPTION))
                                            .text_color(rgb(colors.muted))
                                            .child(detail),
                                    ),
                            )
                            .child(
                                div()
                                    .text_size(px(TEXT_CAPTION))
                                    .text_color(rgb(colors.muted))
                                    .child(kind),
                            )
                            .child(
                                row_play_button(
                                    colors,
                                    format!("search-play-{row}").into(),
                                    "Play · Shift+Enter",
                                )
                                .on_click(cx.listener(
                                    move |this, _, window, cx| {
                                        cx.stop_propagation();
                                        this.browser.set_cursor(row);
                                        this.play_selection(cx);
                                        window.focus(&this.library_focus);
                                    },
                                )),
                            )
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.browser.set_cursor(row);
                                this.open_selected(cx);
                                window.focus(&this.library_focus);
                            }))
                    })
                    .collect()
            }),
        )
        .flex_1()
        .min_h_0()
        .track_scroll(self.list_scroll.clone())
        .into_any_element()
    }
}
