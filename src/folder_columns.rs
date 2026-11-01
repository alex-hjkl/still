use super::*;
use crate::folders::parent_path;

use super::columns::{Column, column_header, column_layout, column_message};

impl PlayerView {
    pub(super) fn reset_folder_columns(&mut self) {
        self.folder_parent_scroll = UniformListScrollHandle::new();
        self.folder_preview_scroll = UniformListScrollHandle::new();
        if let Page::Folder(path) = &self.library.browser().state().page
            && let Some(parent) = parent_path(path)
            && let Some(row) = self
                .library
                .browser()
                .folder_entries(parent)
                .iter()
                .position(|entry| matches!(entry, BrowserEntry::Folder(child) if child == path))
        {
            self.folder_parent_scroll
                .scroll_to_item(row, ScrollStrategy::Center);
        }
    }

    pub(super) fn folder_columns(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let colors = crate::theme::colors(cx);
        let Page::Folder(path) = &self.library.browser().state().page else {
            unreachable!();
        };
        if self.library.browser().folder_library().is_none() {
            return empty_state(colors, "Folders have not been loaded yet");
        }
        let parent = parent_path(path)
            .map(|parent| self.folder_column(parent, Column::Parent, Some(path), cx));
        let entries = self.library.browser().entries();
        let preview = match entries.get(self.library.browser().state().cursor) {
            Some(BrowserEntry::Folder(child)) => {
                self.folder_column(child, Column::Preview, None, cx)
            }
            Some(entry @ BrowserEntry::Song(_)) => self.song_preview(entry, cx),
            _ => div()
                .flex_1()
                .min_w_0()
                .child(column_message(
                    colors,
                    "Select a folder or song to preview it",
                ))
                .into_any_element(),
        };
        let parent = parent.unwrap_or_else(|| {
            div()
                .flex_1()
                .h_full()
                .flex()
                .flex_col()
                .child(column_message(colors, "Library root"))
                .into_any_element()
        });
        column_layout(
            colors,
            Some(parent),
            self.folder_column(path, Column::Current, None, cx),
            preview,
        )
        .into_any_element()
    }

    fn folder_column(
        &self,
        path: &str,
        column: Column,
        child: Option<&str>,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let colors = crate::theme::colors(cx);
        let entries = self.library.browser().folder_entries(path);
        let selected = if column == Column::Current {
            Some(self.library.browser().state().cursor)
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
            .child(column_header(
                colors,
                if path.is_empty() {
                    "/"
                } else {
                    file_name(path)
                }
                .to_owned(),
            ));
        if entries.is_empty() {
            return pane
                .child(column_message(colors, "No music in this folder"))
                .into_any_element();
        }
        let path = path.to_owned();
        let id: SharedString = format!("folder-column-{}-{path}", column as u8).into();
        let like_slot = match column {
            Column::Parent => "folder-parent-like",
            Column::Current => "folder-song-like",
            Column::Preview => "folder-preview-like",
        };
        pane = pane.child(
            uniform_list(
                id,
                entries.len(),
                cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                    let library = this
                        .library
                        .browser()
                        .folder_library()
                        .expect("loaded folders");
                    let folder = library.folder(&path).expect("existing folder");
                    range
                        .map(|row| {
                            let (name, detail, song) = match &entries[row] {
                                BrowserEntry::Folder(child) => {
                                    (file_name(child).to_owned(), String::new(), None)
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
                                        Some(song),
                                    )
                                }
                                _ => unreachable!(),
                            };
                            let folder_entry = song.is_none();
                            let active = song.is_some_and(|song| {
                                this.transport
                                    .queue()
                                    .current()
                                    .is_some_and(|current| current.id == song.id)
                            });
                            let color = if active { colors.accent } else { colors.text };
                            let path = path.clone();
                            let selected_entry = entries[row].clone();
                            track_row(colors, row, FOLDER_ROW_HEIGHT)
                                .gap_2()
                                .when(selected == Some(row), |view| view.bg(rgb(colors.selected)))
                                .when(column == Column::Current && this.cursor_on(row), |view| {
                                    view.border_1().border_color(rgb(colors.keyboard_focus))
                                })
                                .hover(move |style| style.bg(rgb(colors.hover)))
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
                                    if folder_entry { colors.accent } else { color },
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
                                                    .text_size(px(TEXT_CAPTION))
                                                    .text_color(rgb(colors.muted))
                                                    .truncate()
                                                    .child(detail),
                                            )
                                        }),
                                )
                                .when_some(song, |view, song| {
                                    view.child(this.like_button(song, like_slot, cx))
                                })
                                .when(folder_entry && column == Column::Current, |view| {
                                    let BrowserEntry::Folder(child) = &selected_entry else {
                                        unreachable!();
                                    };
                                    let child = child.clone();
                                    view.child(
                                        row_play_button(
                                            colors,
                                            format!("play-folder-{child}").into(),
                                            shortcuts::PLAY_SELECTION
                                                .hint("Play folder and subfolders"),
                                        )
                                        .on_click(
                                            cx.listener(move |this, _, window, cx| {
                                                cx.stop_propagation();
                                                window.focus(&this.library_focus);
                                                this.library.set_cursor(row);
                                                this.play_folder(&child, cx);
                                            }),
                                        ),
                                    )
                                })
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.remember_folder(cx);
                                    if this.library.focus_folder_entry(&path, &selected_entry, cx) {
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
}
