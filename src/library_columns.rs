use super::columns::{Column, column_header, column_layout, column_message};
use super::*;
use crate::{
    browser::{PageId, PageState},
    models::{Album, Artist},
};

pub(super) struct LibraryRows {
    entry_count: usize,
    groups: Vec<SearchGroup>,
}

struct SearchGroup {
    label: &'static str,
    first_entry: usize,
    header_row: usize,
}

enum ListRow {
    Header(&'static str),
    Entry(usize),
}

impl LibraryRows {
    pub(super) fn new(state: &PageState) -> Self {
        let mut groups = Vec::new();
        if matches!(state.page, Page::Search(_)) {
            let mut first_entry = 0;
            for (count, label) in [
                (state.artists().len(), "Artists"),
                (state.albums().len(), "Albums"),
                (state.songs().len(), "Songs"),
            ] {
                if count > 0 {
                    groups.push(SearchGroup {
                        label,
                        first_entry,
                        header_row: first_entry + groups.len(),
                    });
                    first_entry += count;
                }
            }
        }
        Self {
            entry_count: state.entries().len(),
            groups,
        }
    }

    fn len(&self) -> usize {
        self.entry_count + self.groups.len()
    }

    fn row(&self, index: usize) -> ListRow {
        let mut header_count = 0;
        for group in &self.groups {
            if index == group.header_row {
                return ListRow::Header(group.label);
            }
            if index < group.header_row {
                break;
            }
            header_count += 1;
        }
        ListRow::Entry(index - header_count)
    }

    pub(super) fn entry_position(&self, index: usize) -> usize {
        index
            + self
                .groups
                .iter()
                .take_while(|group| group.first_entry <= index)
                .count()
    }
}

enum LibraryRow<'a> {
    Artist(&'a Artist),
    Album(&'a Album),
    Song(&'a Song),
}

impl<'a> LibraryRow<'a> {
    fn new(state: &'a PageState, entry: &BrowserEntry) -> Self {
        match entry {
            BrowserEntry::Artist(index) => Self::Artist(&state.artists()[*index]),
            BrowserEntry::Album(index) => Self::Album(&state.albums()[*index]),
            BrowserEntry::Song(index) => Self::Song(&state.songs()[*index]),
            BrowserEntry::Folder(_) => unreachable!("folders have their own row adapter"),
        }
    }

    fn title(&self) -> &str {
        match self {
            Self::Artist(artist) => &artist.name,
            Self::Album(album) => &album.name,
            Self::Song(song) => &song.title,
        }
    }

    fn detail(&self) -> String {
        match self {
            Self::Artist(artist) => count_label(artist.album_count, "album"),
            Self::Album(album) => [
                album.artist.clone(),
                album.year.map(|year| year.to_string()).unwrap_or_default(),
                count_label(album.song_count, "song"),
            ]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" · "),
            Self::Song(song) => song.artist.clone(),
        }
    }

    fn song(&self) -> Option<&'a Song> {
        if let Self::Song(song) = self {
            Some(song)
        } else {
            None
        }
    }

    fn glyph(&self) -> &'static str {
        match self {
            Self::Artist(_) => "artists",
            Self::Album(_) => "albums",
            Self::Song(_) => "songs",
        }
    }
}

impl PlayerView {
    fn prepare_library_scrolls(&mut self) {
        let browser = self.library.browser();
        let retained: std::collections::HashSet<_> = browser.retained_pages().collect();
        self.library_scrolls.retain(|id, _| retained.contains(id));
        for state in std::iter::once(browser.state())
            .chain(browser.parent())
            .chain(browser.preview())
        {
            self.library_scrolls.entry(state.page.id()).or_default();
        }
        if !matches!(browser.state().page, Page::Folder(_)) {
            self.list_scroll = self.library_scrolls[&browser.state().page.id()].clone();
        }
    }

    pub(super) fn update_columns(&mut self, cx: &mut Context<Self>) {
        if self.columns_update_pending {
            return;
        }
        self.columns_update_pending = true;
        let view = cx.weak_entity();
        // A navigation action may pass through a parent and then open its child.
        // Reconcile only its final state, before the next paint.
        cx.defer(move |cx| {
            let _ = view.update(cx, |this, cx| {
                this.columns_update_pending = false;
                let context = this.library_context(cx);
                this.library.sync_preview(context, cx);
                this.prepare_library_scrolls();
                this.reveal_cursor();
                this.remember_folder(cx);
                cx.notify();
            });
        });
    }

    pub(super) fn library_columns(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let colors = crate::theme::colors(cx);
        let browser = self.library.browser();
        let parent = browser
            .parent()
            .map(|state| self.library_column(state.clone(), Column::Parent, cx));
        let current = self.library_column(browser.state().clone(), Column::Current, cx);
        let preview = if let Some(state) = browser.preview() {
            self.library_column(state.clone(), Column::Preview, cx)
        } else if let Some(entry @ BrowserEntry::Song(_)) = browser.entry_at(browser.state().cursor)
        {
            self.song_preview(&entry, cx)
        } else {
            div()
                .flex_1()
                .min_w_0()
                .child(column_message(
                    colors,
                    "Select an artist, album or song to preview it",
                ))
                .into_any_element()
        };
        column_layout(colors, parent, current, preview).into_any_element()
    }

    fn library_column(
        &self,
        state: PageState,
        column: Column,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let colors = crate::theme::colors(cx);
        #[cfg(feature = "ui-tests")]
        self.ui_probe.borrow_mut().columns.insert(
            match column {
                Column::Parent => "parent",
                Column::Current => "current",
                Column::Preview => "preview",
            }
            .into(),
            format!("{:?}", state.page.id()),
        );
        let mut pane = div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .child(column_header(colors, state.page.title()));
        if column == Column::Preview
            && let Some(failure) = self.library.preview_failure()
        {
            return pane
                .child(column_message(colors, failure.message.clone()))
                .child(
                    div()
                        .px_3()
                        .child(
                            button(colors, "retry-preview", "Retry").on_click(cx.listener(
                                |this, _, _, cx| {
                                    this.retry(FailureTarget::Preview, cx);
                                },
                            )),
                        ),
                )
                .into_any_element();
        }
        if !state.has_data() {
            return pane
                .child(column_message(
                    colors,
                    if column == Column::Preview {
                        "Loading preview…"
                    } else if self.library.failure().is_some() {
                        "Could not load this section"
                    } else {
                        "Loading your library…"
                    },
                ))
                .into_any_element();
        }
        let entries = state.entries();
        if entries.is_empty() {
            let message = match state.page {
                Page::Search(_) => "No matches in your library",
                Page::Liked => "Click ♡ next to a song to add it here",
                Page::Artists => "No artists yet",
                Page::Albums | Page::Artist(_) => "No albums yet",
                _ => "No songs yet",
            };
            return pane
                .child(column_message(colors, message))
                .into_any_element();
        }
        let scroll = self
            .library_scrolls
            .get(&state.page.id())
            .cloned()
            .unwrap_or_default();
        let id: SharedString = format!("library-{column:?}-{:?}", state.page.id()).into();
        let rows = LibraryRows::new(&state);
        #[cfg(feature = "ui-tests")]
        if column == Column::Current {
            self.ui_probe.borrow_mut().search_groups = rows
                .groups
                .iter()
                .map(|group| group.label.to_owned())
                .collect();
        }
        pane = pane.child(
            uniform_list(
                id,
                rows.len(),
                cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                    range
                        .map(|index| match rows.row(index) {
                            ListRow::Header(label) => column_header(colors, label)
                                .h(px(SONG_ROW_HEIGHT))
                                .into_any_element(),
                            ListRow::Entry(row) => this
                                .library_row(&state, &entries[row], column, row, cx)
                                .into_any_element(),
                        })
                        .collect()
                }),
            )
            .track_scroll(scroll)
            .w_full()
            .flex_1()
            .min_h_0(),
        );
        pane.into_any_element()
    }

    fn library_row(
        &self,
        state: &PageState,
        entry: &BrowserEntry,
        column: Column,
        row: usize,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let colors = crate::theme::colors(cx);
        let content = LibraryRow::new(state, entry);
        let song = content.song();
        let active = song.is_some_and(|song| {
            self.transport
                .queue()
                .current()
                .is_some_and(|current| current.id == song.id)
        });
        let row_height = match state.page {
            Page::Artists => ARTIST_ROW_HEIGHT,
            Page::Albums | Page::Artist(_) => ALBUM_ROW_HEIGHT,
            _ => SONG_ROW_HEIGHT,
        };
        let selected = state.cursor == row;
        let page_id = state.page.id();
        track_row(colors, row, row_height)
            .gap_2()
            .hover(move |style| style.bg(rgb(colors.panel)))
            .when(selected, |view| view.bg(rgb(colors.selected)))
            .when(column == Column::Current && self.cursor_on(row), |view| {
                view.border_1().border_color(rgb(colors.keyboard_focus))
            })
            .child(if let LibraryRow::Album(album) = &content {
                artwork(
                    colors,
                    self.connection.read(cx).client(),
                    album.cover_art.as_deref(),
                    if column == Column::Parent { 28. } else { 36. },
                )
            } else {
                icon_sized(
                    if active {
                        if self.transport.state().is_playing() {
                            "play"
                        } else {
                            "pause"
                        }
                    } else {
                        content.glyph()
                    },
                    if active { colors.accent } else { colors.muted },
                    18.,
                )
                .into_any_element()
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
                            .truncate()
                            .when(active, |view| view.text_color(rgb(colors.accent)))
                            .child(content.title().to_owned()),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(TEXT_CAPTION))
                            .text_color(rgb(colors.muted))
                            .child(content.detail()),
                    ),
            )
            .when(column == Column::Current, |view| {
                view.when_some(song, |view, song| {
                    view.child(self.like_button(song, "song-like", cx))
                })
            })
            .when(column != Column::Parent, |view| {
                if let Some(song) = song {
                    view.child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(TEXT_CAPTION))
                            .text_color(rgb(colors.muted))
                            .child(duration_label(song.duration)),
                    )
                } else {
                    let play_state = state.clone();
                    view.child(
                        row_play_button(
                            colors,
                            format!("column-play-{column:?}-{row}").into(),
                            "Play · Shift+Enter",
                        )
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                cx.stop_propagation();
                                this.play_column_row(&play_state, row, cx);
                                window.focus(&this.library_focus);
                            },
                        )),
                    )
                }
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                this.activate_library_row(column, &page_id, row, cx);
                window.focus(&this.library_focus);
            }))
    }

    pub(super) fn activate_library_row(
        &mut self,
        column: Column,
        page: &PageId,
        row: usize,
        cx: &mut Context<Self>,
    ) {
        let state = match column {
            Column::Current => Some(self.library.browser().state()),
            Column::Parent => self.library.browser().parent(),
            Column::Preview => self.library.browser().preview(),
        };
        let Some(state) = state.filter(|state| state.page.id() == *page && state.loaded()) else {
            return;
        };
        if row >= state.entries().len() {
            return;
        }
        let target = state.page.clone();
        match column {
            Column::Parent => self.back(cx),
            Column::Preview => self.navigate(target, true, cx),
            Column::Current => {}
        }
        self.library.set_cursor(row);
        self.open_selected(cx);
        self.update_columns(cx);
        cx.notify();
    }

    fn play_column_row(&mut self, state: &PageState, row: usize, cx: &mut Context<Self>) {
        match state.entries().get(row) {
            Some(BrowserEntry::Artist(index)) => {
                self.play_collection(Collection::Artist(state.artists()[*index].clone()), cx)
            }
            Some(BrowserEntry::Album(index)) => {
                self.play_collection(Collection::Album(state.albums()[*index].clone()), cx)
            }
            Some(BrowserEntry::Song(index)) => {
                if let Some(songs) = state.song_collection() {
                    self.play(songs.clone(), *index, cx);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grouped_rows_keep_selection_and_rendering_aligned_after_append() {
        use crate::browser::{Browser, LibraryData};
        use crate::models::SearchResults;

        let mut browser = Browser::default();
        browser.navigate(Page::Search("mix".into()), true);
        let results = |value| serde_json::from_value::<SearchResults>(value).unwrap();
        browser.apply(
            LibraryData::Search(results(serde_json::json!({
                "album": [{"id": "album", "name": "Album"}],
                "song": [{"id": "song", "title": "Song"}]
            }))),
            false,
        );
        browser.set_cursor(1);
        assert_eq!(
            LibraryRows::new(browser.state()).entry_position(browser.state().cursor),
            3
        );

        browser.apply(
            LibraryData::Search(results(serde_json::json!({
                "artist": [{"id": "artist", "name": "Artist"}],
                "album": [{"id": "second-album", "name": "Second album"}]
            }))),
            true,
        );

        let rows = LibraryRows::new(browser.state());
        assert_eq!(rows.entry_position(browser.state().cursor), 6);
        assert_eq!(rows.len(), 7);
        assert!(matches!(rows.row(0), ListRow::Header("Artists")));
        assert!(matches!(rows.row(2), ListRow::Header("Albums")));
        assert!(matches!(rows.row(5), ListRow::Header("Songs")));
        for (entry, position) in [1, 3, 4, 6].into_iter().enumerate() {
            assert_eq!(rows.entry_position(entry), position);
            assert!(matches!(rows.row(position), ListRow::Entry(row) if row == entry));
        }
    }
}
