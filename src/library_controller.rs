//! Coordinates library navigation, loading, cached pages and retry state.
use crate::{
    browser::{AlbumPage, Browser, BrowserEntry, LibraryRequest, Page},
    failure::Failure,
    library_loader::{LibraryLoader, LoadContext, LoadOutcome},
    models::Song,
    navigation::Section,
    session::Session,
    settings::SavedFolder,
};
use gpui::{App, AppContext, Entity};

pub struct LibraryContext {
    pub session: Session,
    pub like_revision: u64,
}

pub struct LibraryChange {
    pub restored_folder: bool,
}

enum PendingSelection {
    Folder(SavedFolder),
    Song(String),
}

pub struct LibraryController {
    browser: Browser,
    loader: Entity<LibraryLoader>,
    preview_loader: Entity<LibraryLoader>,
    preview_failure: Option<Failure<LibraryRequest>>,
    pending_selection: Option<PendingSelection>,
    failure: Option<Failure<LibraryRequest>>,
}

impl LibraryController {
    pub fn new(cx: &mut App) -> Self {
        Self {
            browser: Browser::default(),
            loader: cx.new(|_| LibraryLoader::default()),
            preview_loader: cx.new(|_| LibraryLoader::default()),
            preview_failure: None,
            pending_selection: None,
            failure: None,
        }
    }

    pub fn browser(&self) -> &Browser {
        &self.browser
    }
    pub fn loader(&self) -> &Entity<LibraryLoader> {
        &self.loader
    }
    pub fn preview_loader(&self) -> &Entity<LibraryLoader> {
        &self.preview_loader
    }

    pub fn preview_failure(&self) -> Option<&Failure<LibraryRequest>> {
        self.preview_failure.as_ref()
    }

    pub fn sync_preview(&mut self, context: LibraryContext, cx: &mut App) {
        if self.browser.sync_preview() {
            self.preview_loader.update(cx, |loader, _| loader.cancel());
            self.preview_failure = None;
        }
        let Some(preview) = self.browser.preview() else {
            return;
        };
        if preview.loaded()
            || self.preview_loader.read(cx).is_loading()
            || self.preview_failure.is_some()
            || !context.session.is_active()
        {
            return;
        }
        let load = LoadContext {
            request: LibraryRequest {
                page: preview.page.clone(),
                offset: 0,
                pages: 1,
            },
            like_revision: context.like_revision,
        };
        match context.session {
            Session::Demo => {
                let data = crate::demo::load(&load.request.page);
                self.browser.accept_preview(&load.request.page, data);
            }
            Session::Connected(client) => self.preview_loader.update(cx, |loader, cx| {
                loader.start_after(client, load, std::time::Duration::from_millis(150), cx)
            }),
            Session::Disconnected => {}
        }
    }

    pub fn accept_preview(&mut self, outcome: LoadOutcome) {
        if self.browser.preview().map(|state| state.page.id())
            != Some(outcome.context.request.page.id())
        {
            return;
        }
        match outcome.result {
            Ok(data) => {
                self.browser
                    .accept_preview(&outcome.context.request.page, data);
            }
            Err(error) => {
                self.preview_failure =
                    Some(Failure::new(format!("{error:#}"), outcome.context.request).log_error())
            }
        }
    }

    pub fn retry_preview(&mut self, context: LibraryContext, cx: &mut App) {
        self.preview_failure = None;
        self.sync_preview(context, cx);
    }

    pub fn failure(&self) -> Option<&Failure<LibraryRequest>> {
        self.failure.as_ref()
    }
    pub fn dismiss_failure(&mut self) {
        self.failure = None;
    }
    pub fn is_loading(&self, cx: &App) -> bool {
        self.loader.read(cx).is_loading()
    }
    pub fn request_id(&self, cx: &App) -> u64 {
        self.loader.read(cx).request_id()
    }
    pub fn saved_folder(&self, session: &Session) -> Option<SavedFolder> {
        self.browser.saved_folder(session.library_id()?)
    }
    pub fn set_cursor(&mut self, row: usize) {
        self.browser.set_cursor(row);
    }
    pub fn clear_history(&mut self) {
        self.browser.clear_history();
    }
    pub fn replace_songs(&mut self, songs: Vec<Song>) {
        self.browser.replace_songs(songs);
    }

    fn cancel_load(&mut self, cx: &mut App) {
        self.loader.update(cx, |loader, _| loader.cancel());
        self.preview_loader.update(cx, |loader, _| loader.cancel());
        self.preview_failure = None;
        self.failure = None;
        self.pending_selection = None;
    }

    pub fn clear(&mut self, cx: &mut App) {
        self.cancel_load(cx);
        self.browser.clear();
    }

    pub fn navigate(&mut self, page: Page, remember: bool, cx: &mut App) {
        if remember && self.preview_loader.read(cx).has_request_for(&page) {
            // Roles move with the entities; subscribers identify the role at delivery.
            self.loader.update(cx, |loader, _| loader.cancel());
            std::mem::swap(&mut self.loader, &mut self.preview_loader);
            self.failure = None;
            self.preview_failure = None;
            self.pending_selection = None;
        } else {
            self.cancel_load(cx);
        }
        self.browser.navigate(page, remember);
    }

    pub fn open_section(
        &mut self,
        section: Section,
        session: &Session,
        saved: Option<SavedFolder>,
        cx: &mut App,
    ) {
        let library_id = session.library_id();
        let saved = saved.filter(|saved| {
            section == Section::Folders && library_id.as_deref() == Some(saved.library_id.as_str())
        });
        let page = saved.as_ref().map_or_else(
            || Page::for_section(section),
            |saved| Page::Folder(saved.path.clone()),
        );
        self.navigate(page, false, cx);
        self.pending_selection = saved.map(PendingSelection::Folder);
        self.resolve_selection();
    }

    /// Returns whether revealing the song opened another page.
    pub fn reveal_song(&mut self, song: &Song, cx: &mut App) -> bool {
        if self.browser.select_song(&song.id) {
            return false;
        }
        let page = if let Some(id) = song.album_id.as_ref().filter(|id| !id.is_empty()) {
            Page::Album(AlbumPage {
                id: id.clone(),
                name: song.album.clone(),
                artist: song.artist.clone(),
                year: None,
                song_count: 0,
            })
        } else {
            Page::Search(song.title.clone())
        };
        self.navigate(page, true, cx);
        self.pending_selection = Some(PendingSelection::Song(song.id.clone()));
        self.resolve_selection();
        true
    }

    fn resolve_selection(&mut self) -> bool {
        if !self.browser.state().loaded() {
            return false;
        }
        match self.pending_selection.take() {
            Some(PendingSelection::Folder(saved)) => {
                self.browser.restore_folder(&saved);
                true
            }
            Some(PendingSelection::Song(id)) => {
                self.browser.select_song(&id);
                false
            }
            None => false,
        }
    }

    pub fn back(&mut self, cx: &mut App) -> bool {
        if !self.browser.back() {
            return false;
        }
        self.cancel_load(cx);
        true
    }

    pub fn refresh(&mut self, cx: &mut App) {
        self.cancel_load(cx);
        self.browser.refresh();
    }

    pub fn refresh_folders(&mut self, cx: &mut App) -> bool {
        if self.is_loading(cx) {
            return false;
        }
        self.browser.clear_folders();
        self.navigate(Page::Folder(String::new()), false, cx);
        true
    }

    pub fn focus_folder_entry(&mut self, path: &str, entry: &BrowserEntry, cx: &mut App) -> bool {
        if !self.browser.focus_folder_entry(path, entry) {
            return false;
        }
        self.cancel_load(cx);
        true
    }

    pub fn set_search(&mut self, text: String, cx: &mut App) -> bool {
        if !self.browser.set_search(text) {
            return false;
        }
        self.cancel_load(cx);
        true
    }

    pub fn load_current(&mut self, context: LibraryContext, cx: &mut App) -> Option<LoadOutcome> {
        if self.browser.state().loaded()
            || self
                .loader
                .read(cx)
                .has_request_for(&self.browser.state().page)
        {
            return None;
        }
        self.load(self.browser.request(), context, cx)
    }

    pub fn load_more(&mut self, context: LibraryContext, cx: &mut App) -> Option<LoadOutcome> {
        if self.is_loading(cx) {
            return None;
        }
        let request = self.browser.more_request()?;
        self.load(request, context, cx)
    }

    pub fn retry(&mut self, context: LibraryContext, cx: &mut App) -> Option<LoadOutcome> {
        if !context.session.is_active() || self.is_loading(cx) {
            return None;
        }
        let request = self.failure.take().and_then(|failure| failure.retry)?;
        self.load(request, context, cx)
    }

    pub fn load(
        &mut self,
        request: LibraryRequest,
        context: LibraryContext,
        cx: &mut App,
    ) -> Option<LoadOutcome> {
        if !context.session.is_active() {
            return None;
        }
        self.failure = None;
        let load = LoadContext {
            request,
            like_revision: context.like_revision,
        };
        match context.session {
            Session::Demo => {
                let result = Ok(crate::demo::load(&load.request.page).into());
                Some(LoadOutcome {
                    context: load,
                    result,
                })
            }
            Session::Connected(client) => {
                self.loader
                    .update(cx, |loader, cx| loader.start(client, load, cx));
                None
            }
            Session::Disconnected => None,
        }
    }

    pub fn accept(&mut self, outcome: LoadOutcome) -> LibraryChange {
        let LoadOutcome { context, result } = outcome;
        let mut restored_folder = false;
        match result {
            Ok(data) => {
                self.browser.apply(data, context.request.offset > 0);
                restored_folder = self.resolve_selection();
            }
            Err(error) => {
                self.failure = Some(Failure::new(format!("{error:#}"), context.request).log_error())
            }
        }
        LibraryChange { restored_folder }
    }
}
