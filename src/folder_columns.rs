use super::*;
use crate::folders::parent_path;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Column {
    Parent,
    Current,
    Preview,
}

fn column_header(label: &'static str, path: Option<&str>) -> gpui::Div {
    div()
        .h(px(56.))
        .flex_shrink_0()
        .px_3()
        .flex()
        .flex_col()
        .justify_center()
        .gap_1()
        .bg(rgb(PANEL))
        .border_b_1()
        .border_color(rgb(BORDER))
        .child(
            div()
                .when(path.is_some(), |view| view.text_xs().text_color(rgb(MUTED)))
                .when(path.is_none(), |view| {
                    view.text_sm().font_weight(gpui::FontWeight::MEDIUM)
                })
                .child(label),
        )
        .when_some(path, |view, path| {
            view.child(
                div()
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .truncate()
                    .child(
                        if path.is_empty() {
                            "/"
                        } else {
                            file_name(path)
                        }
                        .to_owned(),
                    ),
            )
        })
}

fn column_message(message: &'static str) -> gpui::Div {
    div()
        .p_3()
        .text_sm()
        .text_color(rgb(MUTED))
        .whitespace_normal()
        .child(message)
}

impl PlayerView {
    pub(super) fn reset_folder_columns(&mut self) {
        self.folder_parent_scroll = UniformListScrollHandle::new();
        self.folder_preview_scroll = UniformListScrollHandle::new();
        if let Page::Folder(path) = &self.browser.state().page
            && let Some(parent) = parent_path(path)
            && let Some(row) = self
                .browser
                .folder_entries(parent)
                .iter()
                .position(|entry| matches!(entry, BrowserEntry::Folder(child) if child == path))
        {
            self.folder_parent_scroll
                .scroll_to_item(row, ScrollStrategy::Center);
        }
        self.reveal_cursor();
    }

    pub(super) fn folder_columns(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let Page::Folder(path) = &self.browser.state().page else {
            unreachable!();
        };
        if self.browser.folder_library().is_none() {
            return empty_state("Folders have not been loaded yet", "");
        }
        let parent = parent_path(path)
            .map(|parent| self.folder_column(parent, Column::Parent, Some(path), cx));
        let entries = self.browser.entries();
        let preview = match entries.get(self.browser.state().cursor) {
            Some(BrowserEntry::Folder(child)) => {
                self.folder_column(child, Column::Preview, None, cx)
            }
            Some(entry @ BrowserEntry::Song(_)) => self.song_preview(entry, cx),
            _ => div()
                .flex_1()
                .min_w_0()
                .child(column_message("Select a folder or song to preview it"))
                .into_any_element(),
        };
        div()
            .flex()
            .flex_1()
            .min_h_0()
            .w_full()
            .overflow_hidden()
            .child(
                div()
                    .w(relative(0.23))
                    .h_full()
                    .min_w_0()
                    .flex_shrink_0()
                    .flex()
                    .border_r_1()
                    .border_color(rgb(BORDER))
                    .children(parent)
                    .when(path.is_empty(), |view| {
                        view.child(
                            div()
                                .flex_1()
                                .h_full()
                                .flex()
                                .flex_col()
                                .child(column_header("Parent folder", None))
                                .child(column_message("Library root")),
                        )
                    }),
            )
            .child(
                div()
                    .w(relative(0.42))
                    .h_full()
                    .min_w_0()
                    .flex_shrink_0()
                    .flex()
                    .border_r_1()
                    .border_color(rgb(BORDER))
                    .child(self.folder_column(path, Column::Current, None, cx)),
            )
            .child(preview)
            .into_any_element()
    }

    fn folder_column(
        &self,
        path: &str,
        column: Column,
        child: Option<&str>,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let entries = self.browser.folder_entries(path);
        let selected = if column == Column::Current {
            Some(self.browser.state().cursor)
        } else {
            child.and_then(|child| {
                entries
                    .iter()
                    .position(|entry| matches!(entry, BrowserEntry::Folder(path) if path == child))
            })
        };
        let scroll = match column {
            Column::Parent => self.folder_parent_scroll.clone(),
            Column::Current => self.list_scroll.clone(),
            Column::Preview => self.folder_preview_scroll.clone(),
        };
        let label = match column {
            Column::Parent => "Parent folder",
            Column::Current => "Current folder",
            Column::Preview => "Folder preview",
        };
        #[cfg(feature = "ui-tests")]
        self.ui_probe.borrow_mut().columns.insert(
            match column {
                Column::Parent => "parent",
                Column::Current => "current",
                Column::Preview => "preview",
            }
            .into(),
            path.to_owned(),
        );
        let mut pane = div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .child(column_header(label, Some(path)));
        if entries.is_empty() {
            return pane
                .child(column_message(
                    if column == Column::Current && !self.browser.state().query.is_empty() {
                        "No matching items"
                    } else {
                        "No music in this folder"
                    },
                ))
                .into_any_element();
        }
        let path = path.to_owned();
        let id: SharedString = format!("folder-column-{}-{path}", column as u8).into();
        pane = pane.child(
            uniform_list(
                id,
                entries.len(),
                cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                    let library = this.browser.folder_library().expect("loaded folders");
                    let folder = library.folder(&path).expect("existing folder");
                    range
                        .map(|row| {
                            let (name, detail, folder_entry, active) = match &entries[row] {
                                BrowserEntry::Folder(child) => {
                                    (file_name(child).to_owned(), String::new(), true, false)
                                }
                                BrowserEntry::Song(index) => {
                                    let song = &folder.songs[*index];
                                    (
                                        song.path
                                            .as_deref()
                                            .map(file_name)
                                            .unwrap_or(&song.title)
                                            .to_owned(),
                                        song.artist.clone(),
                                        false,
                                        this.transport
                                            .queue()
                                            .current()
                                            .is_some_and(|current| current.id == song.id),
                                    )
                                }
                                _ => unreachable!(),
                            };
                            let color = if active { ACCENT } else { TEXT };
                            let path = path.clone();
                            let selected_entry = entries[row].clone();
                            div()
                                .id(row)
                                .h(px(FOLDER_ROW_HEIGHT))
                                .w_full()
                                .flex()
                                .items_center()
                                .gap_2()
                                .px_3()
                                .border_b_1()
                                .border_color(rgb(BORDER))
                                .cursor_pointer()
                                .when(selected == Some(row), |view| view.bg(rgb(SELECTED)))
                                .when(column == Column::Current && this.cursor_on(row), |view| {
                                    view.border_1().border_color(rgb(KEYBOARD_FOCUS))
                                })
                                .hover(|style| style.bg(rgb(HOVER)))
                                .child(icon(
                                    if folder_entry {
                                        "folders"
                                    } else if active {
                                        if this.transport.state().is_playing() {
                                            "play"
                                        } else {
                                            "pause"
                                        }
                                    } else {
                                        "songs"
                                    },
                                    if folder_entry { ACCENT } else { color },
                                ))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .flex()
                                        .flex_col()
                                        .child(div().text_color(rgb(color)).truncate().child(name))
                                        .when(!detail.is_empty(), |view| {
                                            view.child(
                                                div()
                                                    .text_xs()
                                                    .text_color(rgb(MUTED))
                                                    .truncate()
                                                    .child(detail),
                                            )
                                        }),
                                )
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.remember_folder(cx);
                                    if this.browser.focus_folder_entry(&path, &selected_entry) {
                                        this.reset_navigation_ui(cx);
                                        this.open_selected(cx);
                                        this.remember_folder(cx);
                                    }
                                }))
                        })
                        .collect()
                }),
            )
            .track_scroll(scroll)
            .flex_1()
            .min_h_0()
            .w_full(),
        );
        pane.into_any_element()
    }

    fn song_preview(&self, entry: &BrowserEntry, cx: &mut Context<Self>) -> gpui::AnyElement {
        let song = self.entry_song(entry).expect("selected song");
        #[cfg(feature = "ui-tests")]
        {
            self.ui_probe.borrow_mut().preview_song = Some(song.id.clone());
        }
        let row = self.browser.state().cursor;
        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .child(column_header("Song preview", None))
            .child(
                div()
                    .id("folder-track-preview")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .p_3()
                    .flex()
                    .flex_col()
                    .items_start()
                    .gap_4()
                    .child(artwork(
                        self.connection.read(cx).client.as_ref(),
                        song.cover_art.as_deref(),
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
                                    .text_size(px(16.))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .whitespace_normal()
                                    .child(song.title.clone()),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(MUTED))
                                    .whitespace_normal()
                                    .child(song.artist.clone()),
                            ),
                    )
                    .child(
                        div()
                            .w_full()
                            .flex_shrink_0()
                            .text_xs()
                            .text_color(rgb(MUTED))
                            .whitespace_normal()
                            .child(if song.album.is_empty() {
                                time_label(song.duration)
                            } else {
                                format!("{} · {}", song.album, time_label(song.duration))
                            }),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                button("folder-preview-play", "Play · Enter")
                                    .bg(rgb(ACCENT))
                                    .text_color(rgb(ON_ACCENT))
                                    .hover(|view| view.bg(rgb(ACCENT_HOVER)))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.play_row(row, cx);
                                    })),
                            )
                            .child(self.like_button(song, "folder-preview-like", cx)),
                    ),
            )
            .into_any_element()
    }
}
