use super::*;

impl PlayerView {
    pub(super) fn header(&self, window: &Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        let mut subtitle = match &self.browser.state().page {
            Page::Artists => count_label(self.browser.state().artists.len(), "artist"),
            Page::Albums => format!(
                "{}{}",
                count_label(self.browser.state().albums.len(), "album"),
                if self.browser.state().more {
                    " · more available"
                } else {
                    ""
                }
            ),
            Page::Songs => format!(
                "{}{}",
                count_label(self.browser.state().songs.len(), "song"),
                if self.browser.state().more {
                    " · more available"
                } else {
                    ""
                }
            ),
            Page::Artist(_) => count_label(self.browser.state().albums.len(), "album"),
            Page::Liked => count_label(self.browser.state().songs.len(), "liked song"),
            Page::Album(album) => format!(
                "{}{} · {}",
                album.artist,
                album.year.map(|y| format!(" · {y}")).unwrap_or_default(),
                count_label(
                    album.song_count.max(self.browser.state().songs.len()),
                    "song"
                )
            ),
            Page::Folder(path) => self
                .browser
                .folder_library()
                .and_then(|library| library.folder(path))
                .map(|folder| {
                    format!(
                        "{} · {}",
                        count_label(folder.children.len(), "folder"),
                        count_label(folder.songs.len(), "song")
                    )
                })
                .unwrap_or_default(),
        };
        if !self.browser.state().query.is_empty() {
            let state = self.browser.state();
            let (total, noun) = match &state.page {
                Page::Artists => (state.artists.len(), "artists"),
                Page::Albums | Page::Artist(_) => (state.albums.len(), "albums"),
                Page::Folder(path) => (
                    self.browser
                        .folder_library()
                        .and_then(|library| library.folder(path))
                        .map_or(0, |folder| folder.children.len() + folder.songs.len()),
                    "items",
                ),
                _ => (state.songs.len(), "songs"),
            };
            subtitle = format!(
                "{} of {total} loaded {noun}{}",
                self.visible_entries().len(),
                if state.more { " · more available" } else { "" }
            );
        }
        #[cfg(feature = "ui-tests")]
        {
            self.ui_probe.borrow_mut().find_summary = subtitle.clone();
        }
        let title = match &self.browser.state().page {
            Page::Folder(path) if path.is_empty() => "Folders".to_owned(),
            Page::Folder(path) => {
                let parts: Vec<_> = path
                    .rsplit('/')
                    .filter(|part| !part.is_empty())
                    .take(2)
                    .collect();
                format!(
                    "Folders / {}",
                    parts.into_iter().rev().collect::<Vec<_>>().join(" / ")
                )
            }
            _ => self.title(),
        };
        div()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .px_4()
            .py_3()
            .gap_2()
            .border_b_1()
            .border_color(rgb(BORDER))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(self.sidebar_toggle(window, cx))
                    .when(self.browser.has_history(), |view| {
                        view.child(
                            toolbar_icon("back", "back", "Go back · h")
                                .on_click(cx.listener(|this, _, _, cx| this.back(cx))),
                        )
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .text_size(px(19.))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .truncate()
                                    .child(title),
                            )
                            .child(div().text_xs().text_color(rgb(MUTED)).truncate().child(
                                if self.browser.is_loading() {
                                    "Loading…".into()
                                } else {
                                    subtitle
                                },
                            )),
                    )
                    .when(
                        matches!(self.browser.state().page, Page::Folder(_))
                            && !self.browser.is_loading(),
                        |view| {
                            view.child(
                                toolbar_icon("refresh-folders", "refresh", "Refresh folders · r")
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.refresh_folders(cx)),
                                    ),
                            )
                        },
                    )
                    .when(
                        !self.browser.is_loading()
                            && match &self.browser.state().page {
                                Page::Folder(path) => self
                                    .browser
                                    .folder_library()
                                    .and_then(|library| library.folder(path))
                                    .is_some_and(|folder| {
                                        !folder.songs.is_empty() || !folder.children.is_empty()
                                    }),
                                Page::Artist(_) => !self.browser.state().albums.is_empty(),
                                Page::Album(_) | Page::Liked => {
                                    !self.browser.state().songs.is_empty()
                                }
                                _ => false,
                            },
                        |view| {
                            view.child(
                                toolbar_icon(
                                    "play-page",
                                    "play",
                                    match self.browser.state().page {
                                        Page::Folder(_) => "Play this folder and subfolders",
                                        Page::Liked => "Play liked songs · Shift+Enter",
                                        Page::Artist(_) => "Play all by this artist",
                                        _ => "Play album · Shift+Enter",
                                    },
                                )
                                .bg(rgb(SELECTED))
                                .on_click(cx.listener(
                                    |this, _, _, cx| match &this.browser.state().page {
                                        Page::Folder(path) => this.play_folder(&path.clone(), cx),
                                        Page::Artist(artist) => this.play_collection(
                                            Collection::Artist(artist.clone()),
                                            cx,
                                        ),
                                        _ => this.play(this.browser.state().songs.clone(), 0, cx),
                                    },
                                )),
                            )
                        },
                    )
                    .child(
                        div()
                            .id("find-in-list")
                            .w(px(220.))
                            .flex_shrink_0()
                            .tooltip(|_, cx| {
                                cx.new(|_| Hint::new("Find in loaded items · / or ⌘F"))
                                    .into()
                            })
                            .child(self.filter.clone()),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn artists_list(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let entries = self.visible_entries();
        if entries.is_empty() {
            return empty_state("No artists yet", &self.browser.state().query);
        }
        uniform_list(
            ("artists", self.browser.request_id() as usize),
            entries.len(),
            cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                range
                    .map(|row| {
                        let BrowserEntry::Artist(index) = &entries[row] else {
                            unreachable!()
                        };
                        let artist = this.browser.state().artists[*index].clone();
                        let initial = artist
                            .name
                            .chars()
                            .next()
                            .unwrap_or('♪')
                            .to_uppercase()
                            .to_string();
                        div()
                            .id(row)
                            .w_full()
                            .h(px(ARTIST_ROW_HEIGHT))
                            .flex()
                            .items_center()
                            .gap_4()
                            .px_3()
                            .border_b_1()
                            .border_color(rgb(BORDER))
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(PANEL)))
                            .when(this.cursor_on(row), |view| {
                                view.bg(rgb(SELECTED))
                                    .border_1()
                                    .border_color(rgb(KEYBOARD_FOCUS))
                            })
                            .child(
                                div()
                                    .size(px(34.))
                                    .flex_shrink_0()
                                    .rounded_full()
                                    .bg(rgb(SELECTED))
                                    .text_color(rgb(ACCENT))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_lg()
                                    .child(initial),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .child(artist.name.clone()),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(MUTED))
                                    .child(count_label(artist.album_count, "album")),
                            )
                            .child({
                                let artist = artist.clone();
                                row_play_button(
                                    format!("play-artist-{}", artist.id).into(),
                                    "Play artist · Shift+Enter",
                                )
                                .on_click(cx.listener(
                                    move |this, _, window, cx| {
                                        cx.stop_propagation();
                                        window.focus(&this.library_focus);
                                        this.browser.set_cursor(row);
                                        this.play_collection(
                                            Collection::Artist(artist.clone()),
                                            cx,
                                        );
                                    },
                                ))
                            })
                            .child(icon_sized("forward", MUTED, 16.))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.browser.set_cursor(row);
                                this.navigate(Page::Artist(artist.clone()), true, cx)
                            }))
                    })
                    .collect()
            }),
        )
        .track_scroll(self.list_scroll.clone())
        .w_full()
        .flex_1()
        .min_h_0()
        .into_any_element()
    }

    pub(super) fn albums_list(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let entries = self.visible_entries();
        if entries.is_empty() {
            return empty_state("No albums yet", &self.browser.state().query);
        }
        uniform_list(
            ("albums", self.browser.request_id() as usize),
            entries.len(),
            cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                range
                    .map(|row| {
                        let BrowserEntry::Album(index) = &entries[row] else {
                            unreachable!()
                        };
                        let album = this.browser.state().albums[*index].clone();
                        div()
                            .id(row)
                            .w_full()
                            .h(px(ALBUM_ROW_HEIGHT))
                            .flex()
                            .items_center()
                            .gap_4()
                            .px_3()
                            .border_b_1()
                            .border_color(rgb(BORDER))
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(PANEL)))
                            .when(this.cursor_on(row), |view| {
                                view.bg(rgb(SELECTED))
                                    .border_1()
                                    .border_color(rgb(KEYBOARD_FOCUS))
                            })
                            .child(artwork(
                                this.connection.read(cx).client.as_ref(),
                                album.cover_art.as_deref(),
                                44.,
                            ))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(div().truncate().child(album.name.clone()))
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(rgb(MUTED))
                                            .truncate()
                                            .child(album.artist.clone()),
                                    ),
                            )
                            .when_some(album.year, |view, year| {
                                view.child(
                                    div()
                                        .w(px(48.))
                                        .text_sm()
                                        .text_color(rgb(MUTED))
                                        .child(year.to_string()),
                                )
                            })
                            .child(
                                div()
                                    .w(px(86.))
                                    .text_sm()
                                    .text_color(rgb(MUTED))
                                    .child(count_label(album.song_count, "song")),
                            )
                            .child({
                                let album = album.clone();
                                row_play_button(
                                    format!("play-album-{}", album.id).into(),
                                    "Play album · Shift+Enter",
                                )
                                .on_click(cx.listener(
                                    move |this, _, window, cx| {
                                        cx.stop_propagation();
                                        window.focus(&this.library_focus);
                                        this.browser.set_cursor(row);
                                        this.play_collection(Collection::Album(album.clone()), cx);
                                    },
                                ))
                            })
                            .child(icon_sized("forward", MUTED, 16.))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.browser.set_cursor(row);
                                this.navigate(Page::Album(album.clone()), true, cx)
                            }))
                    })
                    .collect()
            }),
        )
        .track_scroll(self.list_scroll.clone())
        .w_full()
        .flex_1()
        .min_h_0()
        .into_any_element()
    }

    pub(super) fn songs_list(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let entries = self.visible_entries();
        if entries.is_empty() {
            return empty_state(
                if matches!(self.browser.state().page, Page::Liked) {
                    "Click ♡ next to a song to add it here"
                } else {
                    "No songs yet"
                },
                &self.browser.state().query,
            );
        }
        let list = uniform_list(
            ("songs", self.browser.request_id() as usize),
            entries.len(),
            cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                range
                    .map(|row| {
                        let song = this.entry_song(&entries[row]).expect("song entry");
                        let active = this
                            .transport
                            .queue()
                            .current()
                            .is_some_and(|current| current.id == song.id);
                        track_row(row, SONG_ROW_HEIGHT)
                            .when(this.cursor_on(row), |view| {
                                view.bg(rgb(SELECTED))
                                    .border_1()
                                    .border_color(rgb(KEYBOARD_FOCUS))
                            })
                            .child(
                                div()
                                    .w(px(24.))
                                    .text_sm()
                                    .text_color(rgb(if active { ACCENT } else { MUTED }))
                                    .child(if active {
                                        icon_sized(
                                            if this.transport.state().is_playing() {
                                                "play"
                                            } else {
                                                "pause"
                                            },
                                            ACCENT,
                                            14.,
                                        )
                                        .into_any_element()
                                    } else {
                                        div().child(format!("{:02}", row + 1)).into_any_element()
                                    }),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_color(rgb(if active { ACCENT } else { TEXT }))
                                            .font_weight(if active {
                                                gpui::FontWeight::SEMIBOLD
                                            } else {
                                                gpui::FontWeight::NORMAL
                                            })
                                            .truncate()
                                            .child(song.title.clone()),
                                    )
                                    .child(
                                        div().flex().items_center().gap_2().child(
                                            div()
                                                .flex_1()
                                                .min_w_0()
                                                .text_sm()
                                                .text_color(rgb(MUTED))
                                                .truncate()
                                                .child(song.artist.clone()),
                                        ),
                                    ),
                            )
                            .when(
                                matches!(this.browser.state().page, Page::Songs | Page::Liked),
                                |view| {
                                    view.child(
                                        div()
                                            .w(relative(0.3))
                                            .text_sm()
                                            .text_color(rgb(MUTED))
                                            .truncate()
                                            .child(song.album.clone()),
                                    )
                                },
                            )
                            .child(this.like_button(song, "song-like", cx))
                            .child(
                                div()
                                    .w(px(60.))
                                    .text_sm()
                                    .text_color(rgb(MUTED))
                                    .child(time_label(song.duration)),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.browser.set_cursor(row);
                                this.play_row(row, cx)
                            }))
                    })
                    .collect()
            }),
        )
        .track_scroll(self.list_scroll.clone())
        .w_full()
        .flex_1()
        .min_h_0();
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(
                div()
                    .h(px(36.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap_4()
                    .px_3()
                    .mt_2()
                    .mb_1()
                    .rounded_md()
                    .bg(rgb(PANEL))
                    .text_xs()
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(MUTED))
                    .child(div().w(px(24.)).child("#"))
                    .child(div().flex_1().min_w_0().child("Title"))
                    .when(
                        matches!(self.browser.state().page, Page::Songs | Page::Liked),
                        |view| view.child(div().w(relative(0.3)).child("Album")),
                    )
                    .child(div().w(px(32.)))
                    .child(div().w(px(60.)).child("Duration")),
            )
            .child(list)
            .into_any_element()
    }
}
