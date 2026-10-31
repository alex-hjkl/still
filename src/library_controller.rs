//! Coordinates library navigation, loading, cached pages and retry state.
use crate::{
    browser::{Browser, BrowserEntry, LibraryRequest, Page},
    failure::{Failure, RetryAction},
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
    pub playback_generation: u64,
}

pub struct LibraryChange {
    pub restored_folder: bool,
    pub playback_generation: u64,
}

pub struct LibraryController {
    browser: Browser,
    loader: Entity<LibraryLoader>,
    pending_folder: Option<SavedFolder>,
    failure: Option<Failure>,
}

impl LibraryController {
    pub fn new(cx: &mut App) -> Self {
        Self {
            browser: Browser::default(),
            loader: cx.new(|_| LibraryLoader::default()),
            pending_folder: None,
            failure: None,
        }
    }

    pub fn browser(&self) -> &Browser {
        &self.browser
    }
    pub fn loader(&self) -> &Entity<LibraryLoader> {
        &self.loader
    }
    pub fn failure(&self) -> Option<&Failure> {
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
        self.failure = None;
    }

    pub fn clear(&mut self, cx: &mut App) {
        self.cancel_load(cx);
        self.pending_folder = None;
        self.browser.clear();
    }

    pub fn navigate(&mut self, page: Page, remember: bool, cx: &mut App) {
        self.cancel_load(cx);
        self.pending_folder = None;
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
        if let Some(saved) = saved {
            if self.browser.state().loaded() {
                self.browser.restore_folder(&saved);
            } else {
                self.pending_folder = Some(saved);
            }
        }
    }

    pub fn back(&mut self, cx: &mut App) -> bool {
        if !self.browser.back() {
            return false;
        }
        self.cancel_load(cx);
        self.pending_folder = None;
        true
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
        self.pending_folder = None;
        true
    }

    pub fn set_search(&mut self, text: String, cx: &mut App) -> bool {
        if !self.browser.set_search(text) {
            return false;
        }
        self.cancel_load(cx);
        self.pending_folder = None;
        true
    }

    pub fn load_current(&mut self, context: LibraryContext, cx: &mut App) -> Option<LoadOutcome> {
        if self.browser.state().loaded() || self.is_loading(cx) {
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
        let Some(RetryAction::Library(request)) =
            self.failure.take().and_then(|failure| failure.retry)
        else {
            return None;
        };
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
            playback_generation: context.playback_generation,
        };
        match context.session {
            Session::Demo => {
                let result = Ok(crate::demo::load(&load.request.page));
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
                if let Some(saved) = self.pending_folder.take() {
                    self.browser.restore_folder(&saved);
                    restored_folder = true;
                }
            }
            Err(error) => {
                self.failure = Some(Failure::new(
                    format!("{error:#}"),
                    RetryAction::Library(context.request),
                ))
            }
        }
        LibraryChange {
            restored_folder,
            playback_generation: context.playback_generation,
        }
    }
}
