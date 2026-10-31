use crate::{
    folders::{FolderLibrary, parent_path},
    models::{Album, Artist, PAGE_SIZE, SearchResults, Song},
    navigation::Section,
    settings::{FolderSelection, SavedFolder},
};
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

#[derive(Clone)]
pub enum PageData {
    Artists(Arc<Vec<Artist>>),
    Albums(Arc<Vec<Album>>),
    Songs(Arc<Vec<Song>>),
    Search {
        artists: Arc<Vec<Artist>>,
        albums: Arc<Vec<Album>>,
        songs: Arc<Vec<Song>>,
    },
    Folder,
}

#[derive(Clone)]
pub struct PageState {
    pub page: Page,
    pub cursor: usize,
    data: Option<PageData>,
    pub more: bool,
    entries: Arc<Vec<BrowserEntry>>,
}

impl PageState {
    fn new(page: Page) -> Self {
        Self {
            page,
            cursor: 0,
            data: None,
            more: false,
            entries: Arc::default(),
        }
    }
    pub fn loaded(&self) -> bool {
        self.data.is_some()
    }
    pub fn artists(&self) -> &[Artist] {
        match &self.data {
            Some(PageData::Artists(artists) | PageData::Search { artists, .. }) => artists,
            _ => &[],
        }
    }
    pub fn albums(&self) -> &[Album] {
        match &self.data {
            Some(PageData::Albums(albums) | PageData::Search { albums, .. }) => albums,
            _ => &[],
        }
    }
    pub fn song_collection(&self) -> Option<&Arc<Vec<Song>>> {
        match &self.data {
            Some(PageData::Songs(songs) | PageData::Search { songs, .. }) => Some(songs),
            _ => None,
        }
    }
    pub fn songs(&self) -> &[Song] {
        self.song_collection().map_or(&[], |songs| songs.as_slice())
    }
}

pub struct Browser {
    state: PageState,
    history: Vec<PageState>,
    folder_library: Option<Arc<FolderLibrary>>,
    folder_states: HashMap<String, PageState>,
}

impl Default for Browser {
    fn default() -> Self {
        Self {
            state: PageState::new(Page::Artists),
            history: Vec::new(),
            folder_library: None,
            folder_states: HashMap::new(),
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
        if !self.state.loaded() || self.folder_library.is_none() {
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
    pub fn has_history(&self) -> bool {
        !self.history.is_empty()
            || matches!(&self.state.page, Page::Folder(path) if parent_path(path).is_some())
    }
    pub fn clear_history(&mut self) {
        self.history.clear();
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
            self.state.data = None;
            self.rebuild_entries();
        }
    }

    pub fn clear(&mut self) {
        self.state = PageState::new(Page::Artists);
        self.history.clear();
        self.folder_library = None;
        self.folder_states.clear();
    }

    pub fn navigate(&mut self, page: Page, remember: bool) {
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
            self.state.data = Some(PageData::Folder);
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
        if !self.state.more {
            return None;
        }
        let offset = match self.state.page {
            Page::Albums => self.state.albums().len(),
            Page::Songs => self.state.songs().len(),
            Page::Search(_) => self
                .state
                .artists()
                .len()
                .max(self.state.albums().len())
                .max(self.state.songs().len()),
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
            LibraryData::Artists(artists) => {
                self.state.data = Some(PageData::Artists(Arc::new(artists)))
            }
            LibraryData::Albums(albums) => {
                self.state.more =
                    matches!(self.state.page, Page::Albums) && albums.len() == PAGE_SIZE;
                if append && let Some(PageData::Albums(current)) = &mut self.state.data {
                    Arc::make_mut(current).extend(albums);
                } else {
                    self.state.data = Some(PageData::Albums(Arc::new(albums)));
                }
            }
            LibraryData::Songs(songs) => {
                self.state.more =
                    matches!(self.state.page, Page::Songs) && songs.len() == PAGE_SIZE;
                if let Some(PageData::Songs(current) | PageData::Search { songs: current, .. }) =
                    &mut self.state.data
                {
                    if append {
                        Arc::make_mut(current).extend(songs);
                    } else {
                        *current = Arc::new(songs);
                    }
                } else {
                    self.state.data = Some(PageData::Songs(Arc::new(songs)));
                }
            }
            LibraryData::Album(mut album) => {
                self.state.data = Some(PageData::Songs(Arc::new(std::mem::take(&mut album.song))));
                self.state.page = Page::Album(album);
            }
            LibraryData::Folders(library) => {
                self.folder_library = Some(Arc::new(library));
                self.state.data = Some(PageData::Folder);
            }
            LibraryData::Search(results) => {
                self.state.more = results.artists.len() == PAGE_SIZE
                    || results.albums.len() == PAGE_SIZE
                    || results.songs.len() == PAGE_SIZE;
                if append
                    && let Some(PageData::Search {
                        artists,
                        albums,
                        songs,
                    }) = &mut self.state.data
                {
                    Arc::make_mut(artists).extend(results.artists);
                    Arc::make_mut(albums).extend(results.albums);
                    Arc::make_mut(songs).extend(results.songs);
                } else {
                    self.state.data = Some(PageData::Search {
                        artists: Arc::new(results.artists),
                        albums: Arc::new(results.albums),
                        songs: Arc::new(results.songs),
                    });
                }
            }
        }
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
        self.state.more = false;
        self.state.data = None;
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
            Page::Search(_) => (0..self.state.artists().len())
                .map(BrowserEntry::Artist)
                .chain((0..self.state.albums().len()).map(BrowserEntry::Album))
                .chain((0..self.state.songs().len()).map(BrowserEntry::Song))
                .collect(),
            Page::Artists => (0..self.state.artists().len())
                .map(BrowserEntry::Artist)
                .collect(),
            Page::Albums | Page::Artist(_) => (0..self.state.albums().len())
                .map(BrowserEntry::Album)
                .collect(),
            Page::Songs | Page::Liked | Page::Album(_) => (0..self.state.songs().len())
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
            self.state.songs().get(*index)
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
            self.state.song_collection()?
        };
        Some((songs.clone(), *index))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_empty_loaded_page_is_distinct_from_an_unloaded_page_and_survives_history() {
        let mut browser = Browser::default();
        browser.navigate(Page::Songs, false);
        assert!(!browser.state().loaded());
        assert!(browser.state.data.is_none());
        browser.apply(LibraryData::Songs(Vec::new()), false);
        assert!(browser.state().loaded());
        assert!(matches!(browser.state.data, Some(PageData::Songs(_))));
        assert!(browser.entries().is_empty());
        browser.navigate(Page::Albums, true);
        assert!(!browser.state().loaded());
        assert!(browser.back());
        assert!(browser.state().loaded());
        assert!(matches!(browser.state.data, Some(PageData::Songs(_))));
    }

    #[test]
    fn changing_page_categories_keeps_only_the_data_for_the_current_page() {
        let mut browser = Browser::default();
        browser.apply(
            LibraryData::Artists(vec![
                serde_json::from_value(json!({"id":"artist", "name":"Artist"})).unwrap(),
            ]),
            false,
        );
        browser.navigate(Page::Songs, false);
        browser.apply(LibraryData::Songs(vec![song(0)]), false);
        assert!(browser.state().artists().is_empty());
        assert!(browser.state().albums().is_empty());
        assert_eq!(browser.state().songs()[0].id, "song-0");
        assert!(matches!(browser.state.data, Some(PageData::Songs(_))));
    }

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
        assert_eq!(browser.state.albums().len(), PAGE_SIZE + 1);
        assert!(browser.more_request().is_none());
    }

    #[test]
    fn changing_search_clears_results_and_ignores_empty_updates() {
        let mut browser = Browser::default();
        browser.navigate(Page::Search("old".into()), true);
        browser.apply(
            LibraryData::Search(SearchResults {
                songs: vec![song(0)],
                albums: vec![album(0)],
                ..Default::default()
            }),
            false,
        );
        assert!(browser.set_search("Музыка 🎵".into()));
        assert!(!browser.state().loaded());
        assert!(browser.entries().is_empty());
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
        assert!(browser.state.loaded());
        assert_eq!(browser.state.cursor, 1);
        assert_eq!(browser.entries().len(), 2);
    }

    #[test]
    fn leaving_search_restores_source_selection() {
        let mut browser = folder_browser();
        browser.set_cursor(1);
        browser.navigate(Page::Search("piano".into()), true);
        assert!(browser.back());
        assert!(matches!(&browser.state.page, Page::Folder(path) if path == "/music"));
        assert!(browser.state.loaded());
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
        assert!(browser.state.loaded());
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
        assert!(browser.state.loaded());
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
        assert!(!browser.state.loaded());
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
        let Some(PageData::Albums(albums)) = &browser.state.data else {
            panic!("albums")
        };
        let albums = albums.clone();
        browser.navigate(Page::Album(album(250)), true);

        assert!(browser.back());
        assert!(browser.state.loaded());
        assert!(std::ptr::eq(
            browser.state.albums().as_ptr(),
            albums.as_ptr()
        ));
        assert_eq!(browser.state.albums().len(), 400);
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
        assert!(Arc::ptr_eq(&all, browser.state.song_collection().unwrap()));
        assert_eq!(selected, 2);
        assert_eq!(all[selected].id, "song-2");
        browser.apply(LibraryData::Songs(vec![song(20)]), true);
        assert_eq!(browser.entries().len(), 5);
        assert_eq!(all.len(), 4);
        let (queue, selected) = browser.selected_queue(4).unwrap();
        assert_eq!(selected, 4);
        assert!(Arc::ptr_eq(
            &queue,
            browser.state.song_collection().unwrap()
        ));
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
        browser.navigate(Page::Songs, true);
        assert!(browser.back());
        assert!(!browser.state.loaded());
        assert!(matches!(browser.request().page, Page::Albums));
    }
}
