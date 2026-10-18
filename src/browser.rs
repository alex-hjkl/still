use crate::{
    api::{Album, Artist, Navidrome, PAGE_SIZE, Song},
    folders::{FolderLibrary, file_name, parent_path},
    requests::{Cancellation, Requests},
    settings::{FolderSelection, SavedFolder, Section},
};
use anyhow::Result;
use std::{collections::HashMap, sync::Arc};

#[derive(Clone, Debug)]
pub enum Page {
    Artists,
    Albums,
    Songs,
    Liked,
    Artist(Artist),
    Album(Album),
    Folder(String),
}

impl Page {
    pub fn for_section(section: Section) -> Self {
        match section {
            Section::Artists => Self::Artists,
            Section::Albums => Self::Albums,
            Section::Songs => Self::Songs,
            Section::Folders => Self::Folder(String::new()),
            Section::Liked => Self::Liked,
        }
    }
    pub fn section(&self) -> Section {
        match self {
            Self::Artists | Self::Artist(_) => Section::Artists,
            Self::Albums | Self::Album(_) => Section::Albums,
            Self::Songs => Section::Songs,
            Self::Folder(_) => Section::Folders,
            Self::Liked => Section::Liked,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BrowserEntry {
    Artist(usize),
    Album(usize),
    Folder(String),
    Song(usize),
}

pub enum LibraryData {
    Artists(Vec<Artist>),
    Albums(Vec<Album>),
    Songs(Vec<Song>),
    Album(Album),
    Folders(FolderLibrary),
}

#[derive(Clone, Debug)]
pub struct LibraryRequest {
    pub page: Page,
    pub offset: usize,
}

impl LibraryRequest {
    pub fn run(&self, client: Navidrome, cancellation: &Cancellation) -> Result<LibraryData> {
        cancellation.check()?;
        let data = match &self.page {
            Page::Artists => LibraryData::Artists(client.artists()?),
            Page::Albums => LibraryData::Albums(client.albums(self.offset)?),
            Page::Songs => LibraryData::Songs(client.songs(self.offset)?),
            Page::Liked => LibraryData::Songs(client.liked_songs()?),
            Page::Artist(artist) => LibraryData::Albums(client.artist_albums(&artist.id)?),
            Page::Album(album) => LibraryData::Album(client.album(&album.id)?),
            Page::Folder(_) => LibraryData::Folders(client.folder_library(cancellation)?),
        };
        cancellation.check()?;
        Ok(data)
    }
}

#[derive(Clone)]
pub struct PageState {
    pub page: Page,
    pub cursor: usize,
    pub query: String,
    pub filter_text: String,
    pub artists: Arc<Vec<Artist>>,
    pub albums: Arc<Vec<Album>>,
    pub songs: Arc<Vec<Song>>,
    pub more: bool,
    pub loaded: bool,
    entries: Arc<Vec<BrowserEntry>>,
}

impl PageState {
    fn new(page: Page) -> Self {
        Self {
            page,
            cursor: 0,
            query: String::new(),
            filter_text: String::new(),
            artists: Arc::default(),
            albums: Arc::default(),
            songs: Arc::default(),
            more: false,
            loaded: false,
            entries: Arc::default(),
        }
    }
}

pub struct Browser {
    state: PageState,
    history: Vec<PageState>,
    folder_library: Option<Arc<FolderLibrary>>,
    folder_states: HashMap<String, PageState>,
    loading: bool,
    requests: Requests,
}

impl Default for Browser {
    fn default() -> Self {
        Self {
            state: PageState::new(Page::Artists),
            history: Vec::new(),
            folder_library: None,
            folder_states: HashMap::new(),
            loading: false,
            requests: Requests::default(),
        }
    }
}

impl Browser {
    pub fn state(&self) -> &PageState {
        &self.state
    }

    pub fn saved_folder(&self, library_id: String) -> Option<SavedFolder> {
        let Page::Folder(path) = &self.state.page else {
            return None;
        };
        if !self.state.loaded || self.folder_library.is_none() {
            return None;
        }
        let selected = self
            .state
            .entries
            .get(self.state.cursor)
            .and_then(|entry| match entry {
                BrowserEntry::Folder(path) => Some(FolderSelection::Folder(path.clone())),
                BrowserEntry::Song(_) => self
                    .entry_song(entry)
                    .map(|song| FolderSelection::Song(song.id.clone())),
                _ => None,
            });
        Some(SavedFolder {
            library_id,
            path: path.clone(),
            selected,
            filter: self.state.filter_text.clone(),
        })
    }

    pub fn restore_folder(&mut self, saved: &SavedFolder) {
        let Some(library) = &self.folder_library else {
            return;
        };
        let mut path = saved.path.as_str();
        while library.folder(path).is_none() {
            let Some(parent) = parent_path(path) else {
                break;
            };
            path = parent;
        }
        let same_folder = path == saved.path;
        self.navigate(Page::Folder(path.to_owned()), false);
        self.set_query(if same_folder {
            saved.filter.clone()
        } else {
            String::new()
        });
        let row = self
            .state
            .entries
            .iter()
            .position(|entry| match (&saved.selected, entry) {
                (Some(FolderSelection::Folder(selected)), BrowserEntry::Folder(path)) => {
                    selected == path
                }
                (Some(FolderSelection::Song(selected)), BrowserEntry::Song(_)) => self
                    .entry_song(entry)
                    .is_some_and(|song| &song.id == selected),
                _ => false,
            });
        self.set_cursor(row.unwrap_or(0));
    }
    pub fn folder_library(&self) -> Option<&Arc<FolderLibrary>> {
        self.folder_library.as_ref()
    }
    pub fn is_loading(&self) -> bool {
        self.loading
    }
    pub fn has_history(&self) -> bool {
        !self.history.is_empty()
            || matches!(&self.state.page, Page::Folder(path) if parent_path(path).is_some())
    }
    pub fn clear_history(&mut self) {
        self.history.clear();
    }
    pub fn request_id(&self) -> u64 {
        self.requests.id()
    }
    pub fn invalidate_requests(&mut self) {
        self.requests.invalidate();
        self.loading = false;
    }
    pub fn start_load(&mut self) -> (u64, Cancellation) {
        self.loading = true;
        self.requests.start()
    }
    pub fn finish_load(&mut self, id: u64) -> bool {
        if !self.requests.is_current(id) {
            return false;
        }
        self.loading = false;
        true
    }
    pub fn set_cursor(&mut self, row: usize) {
        self.state.cursor = row;
        self.clamp_cursor();
    }
    pub fn replace_songs(&mut self, songs: Vec<Song>) {
        self.apply(LibraryData::Songs(songs), false);
    }
    pub fn clear_folders(&mut self) {
        self.folder_library = None;
        self.folder_states.clear();
        self.history.clear();
        if matches!(self.state.page, Page::Folder(_)) {
            self.state.loaded = false;
            self.rebuild_entries();
        }
    }
    pub fn use_cached_folders(&mut self) -> bool {
        if self.folder_library.is_none() {
            return false;
        }
        self.state.loaded = true;
        self.rebuild_entries();
        true
    }

    pub fn clear(&mut self) {
        self.requests.invalidate();
        self.loading = false;
        self.state = PageState::new(Page::Artists);
        self.history.clear();
        self.folder_library = None;
        self.folder_states.clear();
    }

    pub fn navigate(&mut self, page: Page, remember: bool) {
        self.requests.invalidate();
        self.loading = false;
        if self.folder_library.is_some()
            && let Page::Folder(path) = &self.state.page
        {
            self.folder_states.insert(path.clone(), self.state.clone());
        }
        let next = match &page {
            Page::Folder(path) => self.folder_states.get(path).cloned(),
            _ => None,
        }
        .unwrap_or_else(|| PageState::new(page));
        let previous = std::mem::replace(&mut self.state, next);
        if remember
            && !matches!(
                (&previous.page, &self.state.page),
                (Page::Folder(_), Page::Folder(_))
            )
        {
            self.history.push(previous);
        }
        if matches!(self.state.page, Page::Folder(_)) && self.folder_library.is_some() {
            self.state.loaded = true;
            self.rebuild_entries();
        }
    }

    pub fn back(&mut self) -> bool {
        if let Page::Folder(path) = &self.state.page
            && let Some(parent) = parent_path(path)
        {
            let child = BrowserEntry::Folder(path.clone());
            self.navigate(Page::Folder(parent.to_owned()), false);
            // A restored filter may hide the child after a jump from a side column.
            if !self.state.entries.contains(&child) {
                self.set_query(String::new());
            }
            if let Some(row) = self.state.entries.iter().position(|entry| entry == &child) {
                self.set_cursor(row);
            }
            return true;
        }
        let Some(state) = self.history.pop() else {
            return false;
        };
        self.requests.invalidate();
        self.loading = false;
        self.state = state;
        self.rebuild_entries();
        true
    }

    pub fn request(&self) -> LibraryRequest {
        LibraryRequest {
            page: self.state.page.clone(),
            offset: 0,
        }
    }

    pub fn more_request(&self) -> Option<LibraryRequest> {
        if self.loading || !self.state.more {
            return None;
        }
        let offset = match self.state.page {
            Page::Albums => self.state.albums.len(),
            Page::Songs => self.state.songs.len(),
            _ => return None,
        };
        Some(LibraryRequest {
            page: self.state.page.clone(),
            offset,
        })
    }

    pub fn apply(&mut self, data: LibraryData, append: bool) {
        match data {
            LibraryData::Artists(artists) => self.state.artists = Arc::new(artists),
            LibraryData::Albums(albums) => {
                self.state.more =
                    matches!(self.state.page, Page::Albums) && albums.len() == PAGE_SIZE;
                if append {
                    Arc::make_mut(&mut self.state.albums).extend(albums);
                } else {
                    self.state.albums = Arc::new(albums);
                }
            }
            LibraryData::Songs(songs) => {
                self.state.more =
                    matches!(self.state.page, Page::Songs) && songs.len() == PAGE_SIZE;
                if append {
                    Arc::make_mut(&mut self.state.songs).extend(songs);
                } else {
                    self.state.songs = Arc::new(songs);
                }
            }
            LibraryData::Album(mut album) => {
                self.state.songs = Arc::new(std::mem::take(&mut album.song));
                self.state.page = Page::Album(album);
            }
            LibraryData::Folders(library) => self.folder_library = Some(Arc::new(library)),
        }
        self.state.loaded = true;
        self.rebuild_entries();
        self.clamp_cursor();
    }

    pub fn set_query(&mut self, text: String) -> bool {
        let query = text.trim().to_lowercase();
        self.state.filter_text = text;
        if self.state.query == query {
            return false;
        }
        self.state.query = query;
        self.state.cursor = 0;
        self.rebuild_entries();
        true
    }

    pub fn entries(&self) -> Arc<Vec<BrowserEntry>> {
        self.state.entries.clone()
    }

    pub fn folder_entries(&self, path: &str) -> Arc<Vec<BrowserEntry>> {
        if matches!(&self.state.page, Page::Folder(current) if current == path) {
            return self.entries();
        }
        Arc::new(
            self.folder_library
                .as_ref()
                .and_then(|library| library.folder(path))
                .map(|folder| {
                    folder
                        .children
                        .iter()
                        .cloned()
                        .map(BrowserEntry::Folder)
                        .chain((0..folder.songs.len()).map(BrowserEntry::Song))
                        .collect()
                })
                .unwrap_or_default(),
        )
    }

    pub fn focus_folder_entry(&mut self, path: &str, entry: &BrowserEntry) -> bool {
        if !matches!(&self.state.page, Page::Folder(current) if current == path) {
            self.navigate(Page::Folder(path.to_owned()), false);
        }
        if !self.state.entries.contains(entry) {
            self.set_query(String::new());
        }
        let Some(row) = self
            .state
            .entries
            .iter()
            .position(|candidate| candidate == entry)
        else {
            return false;
        };
        self.set_cursor(row);
        true
    }

    fn rebuild_entries(&mut self) {
        let query = &self.state.query;
        // Avoid formatting and lowercasing every record when the filter is empty.
        let matches =
            |parts: &[&str]| query.is_empty() || parts.join(" ").to_lowercase().contains(query);
        let entries = match &self.state.page {
            Page::Artists => self
                .state
                .artists
                .iter()
                .enumerate()
                .filter(|(_, artist)| matches(&[&artist.name]))
                .map(|(i, _)| BrowserEntry::Artist(i))
                .collect(),
            Page::Albums | Page::Artist(_) => self
                .state
                .albums
                .iter()
                .enumerate()
                .filter(|(_, album)| matches(&[&album.name, &album.artist]))
                .map(|(i, _)| BrowserEntry::Album(i))
                .collect(),
            Page::Songs | Page::Liked | Page::Album(_) => self
                .state
                .songs
                .iter()
                .enumerate()
                .filter(|(_, song)| matches(&[&song.title, &song.artist, &song.album]))
                .map(|(i, _)| BrowserEntry::Song(i))
                .collect(),
            Page::Folder(path) => {
                if let Some(folder) = self
                    .folder_library
                    .as_ref()
                    .and_then(|library| library.folder(path))
                {
                    folder
                        .children
                        .iter()
                        .filter(|path| matches(&[file_name(path)]))
                        .cloned()
                        .map(BrowserEntry::Folder)
                        .chain(
                            folder
                                .songs
                                .iter()
                                .enumerate()
                                .filter(|(_, song)| {
                                    matches(&[
                                        song.path.as_deref().map(file_name).unwrap_or_default(),
                                        &song.title,
                                        &song.artist,
                                        &song.album,
                                    ])
                                })
                                .map(|(i, _)| BrowserEntry::Song(i)),
                        )
                        .collect()
                } else {
                    Vec::new()
                }
            }
        };
        self.state.entries = Arc::new(entries);
    }

    pub fn entry_song(&self, entry: &BrowserEntry) -> Option<&Song> {
        let BrowserEntry::Song(index) = entry else {
            return None;
        };
        if let Page::Folder(path) = &self.state.page {
            self.folder_library
                .as_ref()?
                .folder(path)?
                .songs
                .get(*index)
        } else {
            self.state.songs.get(*index)
        }
    }

    fn clamp_cursor(&mut self) {
        self.state.cursor = self
            .state
            .cursor
            .min(self.state.entries.len().saturating_sub(1));
    }

    pub fn selected_queue(&self, row: usize) -> Option<(Arc<Vec<Song>>, usize)> {
        let entries = &self.state.entries;
        if !matches!(entries.get(row)?, BrowserEntry::Song(_)) {
            return None;
        }
        let index = entries[..row]
            .iter()
            .filter(|entry| matches!(entry, BrowserEntry::Song(_)))
            .count();
        let source = if let Page::Folder(path) = &self.state.page {
            &self.folder_library.as_ref()?.folder(path)?.songs
        } else {
            &self.state.songs
        };
        let songs = if self.state.query.is_empty() {
            source.clone()
        } else {
            Arc::new(
                entries
                    .iter()
                    .filter_map(|entry| self.entry_song(entry).cloned())
                    .collect(),
            )
        };
        Some((songs, index))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::failure::{Failure, RetryAction};
    use serde_json::json;

    fn album(index: usize) -> Album {
        serde_json::from_value(
            json!({"id": format!("album-{index}"), "name": format!("Альбом {index}")}),
        )
        .unwrap()
    }

    fn song(index: usize) -> Song {
        serde_json::from_value(json!({"id": format!("song-{index}"), "title": format!("Track {index}"), "artist": "Исполнитель"})).unwrap()
    }

    fn folder_browser() -> Browser {
        let songs = [
            ("a", "/music/A/01.wav"),
            ("b", "/music/B/01.wav"),
            ("cd1", "/music/B/CD 1/01.wav"),
            ("cd2", "/music/B/CD 2/01.wav"),
        ]
        .into_iter()
        .map(|(id, path)| {
            serde_json::from_value(json!({"id": id, "title": id, "path": path})).unwrap()
        })
        .collect();
        let mut browser = Browser::default();
        browser.navigate(Page::Folder("/music".into()), false);
        browser.apply(
            LibraryData::Folders(FolderLibrary::from_songs(songs).unwrap()),
            false,
        );
        browser
    }

    #[test]
    fn restoring_a_folder_uses_song_identity_after_the_file_order_changes() {
        let library = |reverse: bool| {
            FolderLibrary::from_songs(
                [
                    (
                        "chosen",
                        if reverse {
                            "/music/00.wav"
                        } else {
                            "/music/99.wav"
                        },
                    ),
                    (
                        "other",
                        if reverse {
                            "/music/99.wav"
                        } else {
                            "/music/00.wav"
                        },
                    ),
                ]
                .into_iter()
                .map(|(id, path)| {
                    serde_json::from_value(json!({"id":id,"title":"Theme","path":path})).unwrap()
                })
                .collect(),
            )
            .unwrap()
        };
        let mut first = Browser::default();
        first.navigate(Page::Folder("/music".into()), false);
        first.apply(LibraryData::Folders(library(false)), false);
        first.set_query("  THEME  ".into());
        first.set_cursor(1);
        let saved = first.saved_folder("library".into()).unwrap();
        let mut restarted = Browser::default();
        restarted.apply(LibraryData::Folders(library(true)), false);
        restarted.restore_folder(&saved);
        assert!(matches!(&restarted.state.page, Page::Folder(path) if path == "/music"));
        assert_eq!(restarted.state.filter_text, "  THEME  ");
        assert_eq!(restarted.state.cursor, 0);
        assert_eq!(restarted.selected_queue(0).unwrap().0[0].id, "chosen");
    }

    #[test]
    fn restoring_a_removed_folder_falls_back_to_an_existing_parent() {
        let mut browser = folder_browser();
        browser.restore_folder(&SavedFolder {
            library_id: "library".into(),
            path: "/music/B/missing/also-missing".into(),
            selected: Some(FolderSelection::Song("removed".into())),
            filter: "no match".into(),
        });
        assert!(matches!(&browser.state.page, Page::Folder(path) if path == "/music/B"));
        assert!(browser.state.query.is_empty());
        assert_eq!(browser.state.cursor, 0);
        assert!(browser.state.loaded);
    }

    #[test]
    fn folder_columns_restore_each_directory_selection_and_filter() {
        let mut browser = folder_browser();
        browser.set_query("  B  ".into());
        browser.navigate(Page::Folder("/music/B".into()), true);
        browser.set_cursor(1);
        browser.navigate(Page::Folder("/music/B/CD 2".into()), true);
        assert!(browser.back());
        assert!(matches!(&browser.state.page, Page::Folder(path) if path == "/music/B"));
        assert_eq!(browser.state.cursor, 1);
        browser.navigate(Page::Folder("/music/B/CD 1".into()), true);
        browser.navigate(Page::Folder("/music/B".into()), false);
        assert_eq!(browser.state.cursor, 1);
        assert!(browser.back());
        assert_eq!(browser.state.filter_text, "  B  ");
        assert_eq!(
            browser.entries()[browser.state.cursor],
            BrowserEntry::Folder("/music/B".into())
        );
        assert!(browser.state.loaded);
        assert!(browser.history.is_empty());
    }

    #[test]
    fn side_column_activation_selects_the_clicked_song_despite_a_saved_filter() {
        let mut browser = folder_browser();
        browser.navigate(Page::Folder("/music/B".into()), false);
        browser.set_query("CD".into());
        browser.navigate(Page::Folder("/music".into()), false);
        let preview = browser.folder_entries("/music/B");
        assert_eq!(preview.len(), 3);
        assert!(browser.focus_folder_entry("/music/B", &preview[2]));
        assert!(browser.state.query.is_empty());
        let (queue, index) = browser.selected_queue(browser.state.cursor).unwrap();
        assert_eq!(queue[index].id, "b");
        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn folder_back_follows_the_parent_after_a_side_column_jump() {
        let mut browser = folder_browser();
        browser.set_query("A".into());
        browser.navigate(Page::Folder("/music/B/CD 2".into()), false);
        assert!(browser.back());
        assert!(matches!(&browser.state.page, Page::Folder(path) if path == "/music/B"));
        assert_eq!(
            browser.entries()[browser.state.cursor],
            BrowserEntry::Folder("/music/B/CD 2".into())
        );
        assert!(browser.back());
        assert!(browser.state.query.is_empty());
        assert_eq!(
            browser.entries()[browser.state.cursor],
            BrowserEntry::Folder("/music/B".into())
        );
        assert!(browser.back());
        assert!(matches!(&browser.state.page, Page::Folder(path) if path.is_empty()));
        assert!(!browser.has_history());
        assert!(!browser.back());
    }

    #[test]
    fn refreshing_folders_discards_saved_directory_filters() {
        let mut browser = folder_browser();
        browser.set_query("B".into());
        browser.navigate(Page::Folder("/music/B".into()), false);
        browser.set_query("CD 2".into());
        browser.clear_folders();
        browser.navigate(Page::Folder(String::new()), false);
        assert!(browser.folder_states.is_empty());
        assert!(browser.state.query.is_empty());
        assert!(!browser.state.loaded);
    }

    #[test]
    fn replacing_songs_keeps_the_cached_filter_and_cursor_consistent() {
        let mut browser = Browser::default();
        browser.navigate(Page::Songs, false);
        browser.apply(LibraryData::Songs((0..3).map(song).collect()), false);
        browser.set_query("Track".into());
        browser.set_cursor(2);
        browser.replace_songs(vec![song(20)]);
        assert_eq!(browser.state().cursor, 0);
        assert_eq!(&*browser.entries(), &[BrowserEntry::Song(0)]);
        let (queue, selected) = browser.selected_queue(0).unwrap();
        assert_eq!(queue[selected].id, "song-20");
        browser.replace_songs(vec![]);
        assert!(browser.entries().is_empty());
        assert!(browser.selected_queue(0).is_none());
    }

    #[test]
    fn back_restores_all_album_pages_filter_and_selection_without_reloading() {
        let mut browser = Browser::default();
        browser.navigate(Page::Albums, false);
        browser.apply(LibraryData::Albums((0..200).map(album).collect()), false);
        browser.apply(LibraryData::Albums((200..400).map(album).collect()), true);
        browser.set_query("  АЛЬБОМ  ".into());
        browser.state.cursor = 250;
        let albums = browser.state.albums.clone();
        browser.navigate(Page::Album(album(250)), true);
        let (_, cancellation) = browser.requests.start();
        browser.loading = true;

        assert!(browser.back());
        assert!(cancellation.check().is_err());
        assert!(!browser.loading);
        assert!(browser.state.loaded);
        assert!(Arc::ptr_eq(&browser.state.albums, &albums));
        assert_eq!(browser.state.albums.len(), 400);
        assert_eq!(browser.state.filter_text, "  АЛЬБОМ  ");
        assert_eq!(browser.state.query, "альбом");
        assert_eq!(browser.state.cursor, 250);
        assert_eq!(browser.entries()[250], BrowserEntry::Album(250));
        assert_eq!(browser.more_request().unwrap().offset, 400);
    }

    #[test]
    fn queues_match_visible_rows_and_remain_stable_after_filtering_and_appending() {
        let mut browser = Browser::default();
        browser.navigate(Page::Songs, false);
        browser.apply(LibraryData::Songs((0..4).map(song).collect()), false);
        let (all, _) = browser.selected_queue(2).unwrap();
        assert!(Arc::ptr_eq(&all, &browser.state.songs));
        browser.set_query("TRACK 2".into());
        let entries = browser.entries();
        assert_eq!(&*entries, &[BrowserEntry::Song(2)]);
        let (filtered, selected) = browser.selected_queue(0).unwrap();
        assert_eq!(selected, 0);
        assert_eq!(filtered[0].id, "song-2");
        // Repeated renders and equivalent queries retain the cached selection.
        assert!(!browser.set_query(" track 2 ".into()));
        assert!(Arc::ptr_eq(&entries, &browser.entries()));
        browser.apply(LibraryData::Songs(vec![song(20)]), true);
        assert_eq!(browser.entries().len(), 2);
        assert_eq!(filtered.len(), 1);
        assert_eq!(all.len(), 4);
        let (queue, selected) = browser.selected_queue(1).unwrap();
        assert_eq!(selected, 1);
        assert_eq!(
            queue
                .iter()
                .map(|song| song.id.as_str())
                .collect::<Vec<_>>(),
            ["song-2", "song-20"]
        );
    }

    #[test]
    fn folder_selection_skips_directory_rows_and_uses_the_same_filtered_queue() {
        let mut direct = song(0);
        direct.path = Some("/music/01.flac".into());
        let mut second = song(1);
        second.path = Some("/music/02.flac".into());
        let mut nested = song(2);
        nested.path = Some("/music/Disc 2/01.flac".into());
        let library = FolderLibrary::from_songs(vec![direct, second, nested]).unwrap();
        let source = library.folder("/music").unwrap().songs.clone();
        let mut browser = Browser::default();
        browser.navigate(Page::Folder("/music".into()), false);
        browser.apply(LibraryData::Folders(library), false);
        assert!(browser.selected_queue(0).is_none());
        let (queue, selected) = browser.selected_queue(2).unwrap();
        assert!(Arc::ptr_eq(&queue, &source));
        assert_eq!(selected, 1);
        assert_eq!(queue[selected].id, "song-1");
        browser.set_query("02.FLAC".into());
        let (queue, selected) = browser.selected_queue(0).unwrap();
        assert_eq!(selected, 0);
        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0].id, "song-1");
        browser.set_query("исполнитель".into());
        assert_eq!(browser.entries().len(), 2);
    }

    #[test]
    fn back_to_an_unfinished_page_requires_a_new_load() {
        let mut browser = Browser::default();
        browser.navigate(Page::Albums, false);
        let (_, cancellation) = browser.requests.start();
        browser.loading = true;
        browser.navigate(Page::Songs, true);
        assert!(cancellation.check().is_err());
        assert!(browser.back());
        assert!(!browser.state.loaded);
        assert!(matches!(browser.request().page, Page::Albums));
    }

    #[test]
    fn retry_fetches_the_failed_page_over_http_and_keeps_existing_results() {
        use std::{
            io::{BufRead, BufReader, Write},
            net::TcpListener,
            time::Duration,
        };
        for page in [Page::Songs, Page::Albums] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let songs = matches!(page, Page::Songs);
            let server = std::thread::spawn(move || {
                for failed in [true, false] {
                    let (mut socket, _) = listener.accept().unwrap();
                    socket
                        .set_read_timeout(Some(Duration::from_secs(5)))
                        .unwrap();
                    let mut reader = BufReader::new(socket.try_clone().unwrap());
                    let mut first = String::new();
                    reader.read_line(&mut first).unwrap();
                    let url = reqwest::Url::parse(&format!(
                        "http://localhost{}",
                        first.split_whitespace().nth(1).unwrap()
                    ))
                    .unwrap();
                    let params: std::collections::HashMap<_, _> =
                        url.query_pairs().into_owned().collect();
                    assert_eq!(params[if songs { "songOffset" } else { "offset" }], "200");
                    assert_eq!(
                        url.path(),
                        if songs {
                            "/rest/search3.view"
                        } else {
                            "/rest/getAlbumList2.view"
                        }
                    );
                    loop {
                        let mut header = String::new();
                        reader.read_line(&mut header).unwrap();
                        if header.trim().is_empty() {
                            break;
                        }
                    }
                    let body = if failed {
                        json!({"status":"failed", "error":{"code":50}})
                    } else if songs {
                        json!({"status":"ok", "searchResult3":{"song":[{"id":"last", "title":"Track 200"}]}})
                    } else {
                        json!({"status":"ok", "albumList2":{"album":[{"id":"last", "name":"Альбом 200"}]}})
                    };
                    let payload = json!({"subsonic-response": body}).to_string();
                    write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", payload.len(), payload).unwrap();
                }
            });
            let client = Navidrome::new(&format!("http://{address}"), "user", "pass").unwrap();
            let mut browser = Browser::default();
            browser.navigate(page, false);
            let data = if songs {
                LibraryData::Songs((0..200).map(song).collect())
            } else {
                LibraryData::Albums((0..200).map(album).collect())
            };
            browser.apply(data, false);
            browser.set_query(if songs { "Track" } else { "Альбом" }.into());
            browser.state.cursor = 175;
            let request = browser.more_request().unwrap();
            let result = request.run(client.clone(), &Cancellation::default());
            let failure = Failure::new(
                result.err().unwrap().to_string(),
                RetryAction::Library(request),
            );
            let Some(RetryAction::Library(retry)) = failure.retry else {
                panic!("library retry");
            };
            let data = retry.run(client, &Cancellation::default()).unwrap();
            browser.apply(data, retry.offset > 0);
            assert_eq!(browser.entries().len(), 201);
            assert_eq!(browser.state.cursor, 175);
            assert_eq!(
                browser.state.filter_text,
                if songs { "Track" } else { "Альбом" }
            );
            assert!(!browser.state.more);
            if songs {
                assert_eq!(browser.state.songs[200].id, "last");
            } else {
                assert_eq!(browser.state.albums[200].id, "last");
            }
            server.join().unwrap();
        }
    }
}
