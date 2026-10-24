use crate::{
    api::{Album, Artist, Navidrome, PAGE_SIZE, SearchResults, Song},
    folders::{FolderLibrary, parent_path},
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
    // Empty input returns to the source page before entering search.
    Search(String),
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
            Self::Search(_) => Section::Songs,
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
    Search(SearchResults),
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
            Page::Search(query) => LibraryData::Search(client.search(query, self.offset)?),
        };
        cancellation.check()?;
        Ok(data)
    }
}

#[derive(Clone)]
pub struct PageState {
    pub page: Page,
    pub cursor: usize,
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
        self.navigate(Page::Folder(path.to_owned()), false);
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
            Page::Search(_) => self
                .state
                .artists
                .len()
                .max(self.state.albums.len())
                .max(self.state.songs.len()),
            _ => return None,
        };
        Some(LibraryRequest {
            page: self.state.page.clone(),
            offset,
        })
    }

    pub fn apply(&mut self, data: LibraryData, append: bool) {
        let selected = (append && matches!(self.state.page, Page::Search(_)))
            .then(|| self.state.entries.get(self.state.cursor).cloned())
            .flatten();
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
            LibraryData::Search(results) => {
                self.state.more = results.artists.len() == PAGE_SIZE
                    || results.albums.len() == PAGE_SIZE
                    || results.songs.len() == PAGE_SIZE;
                if append {
                    Arc::make_mut(&mut self.state.artists).extend(results.artists);
                    Arc::make_mut(&mut self.state.albums).extend(results.albums);
                    Arc::make_mut(&mut self.state.songs).extend(results.songs);
                } else {
                    self.state.artists = Arc::new(results.artists);
                    self.state.albums = Arc::new(results.albums);
                    self.state.songs = Arc::new(results.songs);
                }
            }
        }
        self.state.loaded = true;
        self.rebuild_entries();
        if let Some(selected) = selected
            && let Some(row) = self
                .state
                .entries
                .iter()
                .position(|entry| *entry == selected)
        {
            self.state.cursor = row;
        }
        self.clamp_cursor();
    }

    pub fn set_search(&mut self, text: String) -> bool {
        if text.trim().is_empty() {
            return false;
        }
        let Page::Search(query) = &mut self.state.page else {
            return false;
        };
        if query.trim() == text.trim() {
            *query = text;
            return false;
        }
        *query = text;
        self.invalidate_requests();
        self.state.artists = Arc::default();
        self.state.albums = Arc::default();
        self.state.songs = Arc::default();
        self.state.more = false;
        self.state.loaded = false;
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
        let entries = match &self.state.page {
            Page::Search(_) => (0..self.state.artists.len())
                .map(BrowserEntry::Artist)
                .chain((0..self.state.albums.len()).map(BrowserEntry::Album))
                .chain((0..self.state.songs.len()).map(BrowserEntry::Song))
                .collect(),
            Page::Artists => (0..self.state.artists.len())
                .map(BrowserEntry::Artist)
                .collect(),
            Page::Albums | Page::Artist(_) => (0..self.state.albums.len())
                .map(BrowserEntry::Album)
                .collect(),
            Page::Songs | Page::Liked | Page::Album(_) => (0..self.state.songs.len())
                .map(BrowserEntry::Song)
                .collect(),
            Page::Folder(path) => self
                .folder_library
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
        let BrowserEntry::Song(index) = self.state.entries.get(row)? else {
            return None;
        };
        let songs = if let Page::Folder(path) = &self.state.page {
            &self.folder_library.as_ref()?.folder(path)?.songs
        } else {
            &self.state.songs
        };
        Some((songs.clone(), *index))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::failure::{Failure, RetryAction};
    use serde_json::json;

    #[test]
    fn search_pages_keep_all_categories_and_queue_only_songs() {
        let mut browser = Browser::default();
        browser.navigate(Page::Search("piano".into()), true);
        browser.apply(
            LibraryData::Search(SearchResults {
                artists: vec![
                    serde_json::from_value(json!({"id":"artist", "name":"Piano"})).unwrap(),
                ],
                albums: (0..PAGE_SIZE).map(album).collect(),
                songs: vec![song(0), song(1)],
            }),
            false,
        );
        let song_row = 1 + PAGE_SIZE;
        browser.set_cursor(song_row);
        let (queue, selected) = browser.selected_queue(song_row + 1).unwrap();
        assert_eq!(selected, 1);
        assert_eq!(queue.len(), 2);
        assert_eq!(queue[0].id, "song-0");
        assert!(browser.selected_queue(0).is_none());
        assert_eq!(browser.more_request().unwrap().offset, PAGE_SIZE);
        browser.apply(
            LibraryData::Search(SearchResults {
                albums: vec![album(PAGE_SIZE)],
                ..Default::default()
            }),
            true,
        );
        assert_eq!(browser.state.cursor, song_row + 1);
        assert_eq!(
            browser.entries()[browser.state.cursor],
            BrowserEntry::Song(0)
        );
        assert_eq!(browser.state.albums.len(), PAGE_SIZE + 1);
        assert!(browser.more_request().is_none());
    }

    #[test]
    fn changing_search_rejects_old_requests_and_ignores_empty_updates() {
        let mut browser = Browser::default();
        browser.navigate(Page::Search("old".into()), true);
        let (old_id, old_cancel) = browser.start_load();
        assert!(browser.set_search("Музыка 🎵".into()));
        assert!(old_cancel.check().is_err());
        assert!(!browser.finish_load(old_id));
        browser.apply(
            LibraryData::Search(SearchResults {
                songs: vec![song(0)],
                ..Default::default()
            }),
            false,
        );
        assert!(!browser.set_search("  Музыка 🎵  ".into()));
        assert_eq!(browser.entries().len(), 1);
        assert!(!browser.set_search("  ".into()));
        assert!(matches!(&browser.state.page, Page::Search(query) if query.trim() == "Музыка 🎵"));
        assert_eq!(browser.entries().len(), 1);
    }

    #[test]
    fn back_restores_search_results_query_and_selection() {
        let mut browser = Browser::default();
        browser.navigate(Page::Search("piano".into()), true);
        browser.apply(
            LibraryData::Search(SearchResults {
                albums: vec![album(0)],
                songs: vec![song(0)],
                ..Default::default()
            }),
            false,
        );
        browser.set_cursor(1);
        browser.navigate(Page::Album(album(0)), true);
        assert!(browser.back());
        assert!(matches!(&browser.state.page, Page::Search(query) if query == "piano"));
        assert!(browser.state.loaded);
        assert_eq!(browser.state.cursor, 1);
        assert_eq!(browser.entries().len(), 2);
    }

    #[test]
    fn leaving_search_restores_source_selection_and_rejects_inflight_results() {
        let mut browser = folder_browser();
        browser.set_cursor(1);
        browser.navigate(Page::Search("piano".into()), true);
        let (request_id, cancellation) = browser.start_load();
        assert!(browser.back());
        assert!(cancellation.check().is_err());
        assert!(!browser.finish_load(request_id));
        assert!(matches!(&browser.state.page, Page::Folder(path) if path == "/music"));
        assert!(browser.state.loaded);
        assert_eq!(
            browser.entries()[browser.state.cursor],
            BrowserEntry::Folder("/music/B".into())
        );
    }

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
        first.set_cursor(1);
        let saved = first.saved_folder("library".into()).unwrap();
        let mut restarted = Browser::default();
        restarted.apply(LibraryData::Folders(library(true)), false);
        restarted.restore_folder(&saved);
        assert!(matches!(&restarted.state.page, Page::Folder(path) if path == "/music"));
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
        });
        assert!(matches!(&browser.state.page, Page::Folder(path) if path == "/music/B"));
        assert_eq!(browser.state.cursor, 0);
        assert!(browser.state.loaded);
    }

    #[test]
    fn folder_columns_restore_each_directory_selection() {
        let mut browser = folder_browser();
        browser.set_cursor(1);
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
        assert_eq!(
            browser.entries()[browser.state.cursor],
            BrowserEntry::Folder("/music/B".into())
        );
        assert!(browser.state.loaded);
        assert!(browser.history.is_empty());
    }

    #[test]
    fn side_column_activation_selects_the_clicked_song() {
        let mut browser = folder_browser();
        browser.navigate(Page::Folder("/music/B".into()), false);
        browser.navigate(Page::Folder("/music".into()), false);
        let preview = browser.folder_entries("/music/B");
        assert_eq!(preview.len(), 3);
        assert!(browser.focus_folder_entry("/music/B", &preview[2]));
        let (queue, index) = browser.selected_queue(browser.state.cursor).unwrap();
        assert_eq!(queue[index].id, "b");
        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn folder_back_follows_the_parent_after_a_side_column_jump() {
        let mut browser = folder_browser();
        browser.navigate(Page::Folder("/music/B/CD 2".into()), false);
        assert!(browser.back());
        assert!(matches!(&browser.state.page, Page::Folder(path) if path == "/music/B"));
        assert_eq!(
            browser.entries()[browser.state.cursor],
            BrowserEntry::Folder("/music/B/CD 2".into())
        );
        assert!(browser.back());
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
    fn refreshing_folders_discards_cached_directory_selections() {
        let mut browser = folder_browser();
        browser.navigate(Page::Folder("/music/B".into()), false);
        browser.set_cursor(1);
        browser.clear_folders();
        browser.navigate(Page::Folder(String::new()), false);
        assert!(browser.folder_states.is_empty());
        assert!(browser.folder_library.is_none());
        assert!(browser.entries().is_empty());
        assert!(!browser.state.loaded);
    }

    #[test]
    fn replacing_songs_clamps_selection_and_rebuilds_entries() {
        let mut browser = Browser::default();
        browser.navigate(Page::Songs, false);
        browser.apply(LibraryData::Songs((0..3).map(song).collect()), false);
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
    fn back_restores_all_album_pages_and_selection_without_reloading() {
        let mut browser = Browser::default();
        browser.navigate(Page::Albums, false);
        browser.apply(LibraryData::Albums((0..200).map(album).collect()), false);
        browser.apply(LibraryData::Albums((200..400).map(album).collect()), true);
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
        assert_eq!(browser.state.cursor, 250);
        assert_eq!(browser.entries()[250], BrowserEntry::Album(250));
        assert_eq!(browser.more_request().unwrap().offset, 400);
    }

    #[test]
    fn queues_use_source_indices_and_remain_stable_after_appending() {
        let mut browser = Browser::default();
        browser.navigate(Page::Songs, false);
        browser.apply(LibraryData::Songs((0..4).map(song).collect()), false);
        let (all, selected) = browser.selected_queue(2).unwrap();
        assert!(Arc::ptr_eq(&all, &browser.state.songs));
        assert_eq!(selected, 2);
        assert_eq!(all[selected].id, "song-2");
        browser.apply(LibraryData::Songs(vec![song(20)]), true);
        assert_eq!(browser.entries().len(), 5);
        assert_eq!(all.len(), 4);
        let (queue, selected) = browser.selected_queue(4).unwrap();
        assert_eq!(selected, 4);
        assert!(Arc::ptr_eq(&queue, &browser.state.songs));
        assert_eq!(queue[selected].id, "song-20");
        assert_eq!(queue[2].id, "song-2");
    }

    #[test]
    fn folder_selection_skips_directory_rows_and_uses_the_source_queue() {
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
        assert_eq!(queue.len(), 2);
        browser.navigate(Page::Folder("/music/Disc 2".into()), true);
        let (nested, selected) = browser.selected_queue(0).unwrap();
        assert_eq!(nested[selected].id, "song-2");
        assert_eq!(queue.len(), 2);
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
