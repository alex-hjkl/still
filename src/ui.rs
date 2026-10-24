use crate::{
    api::{Album, Artist, Navidrome, Song, time_label},
    artwork::{app_icon, artwork},
    browser::{Browser, BrowserEntry, LibraryData, LibraryRequest, Page},
    collection::Collection,
    connection::{Activity, ConnectionController, ConnectionEvent},
    credentials::Connection,
    failure::{Failure, Failures, Operation, RetryAction},
    folders::{FolderLibrary, file_name},
    input::{InputEvent, TextInput},
    keyboard::{Command, Keys, moved_cursor, section_hints},
    layout::{
        ALBUM_ROW_HEIGHT, ARTIST_ROW_HEIGHT, FOLDER_ROW_HEIGHT, PLAYER_HEIGHT,
        SIDEBAR_COMPACT_WIDTH, SIDEBAR_EXPANDED_WIDTH, SONG_ROW_HEIGHT, page_items,
    },
    library_loader::{LibraryLoader, LoadContext, LoadOutcome},
    likes::Likes,
    playback_controller::PlaybackController,
    requests::Requests,
    settings::{SavedFolder, Section, Settings},
    slider::{CommitMode, Slider, SliderEvent},
    theme::{Palette, TEXT_BODY, TEXT_CAPTION, TEXT_SECONDARY, ThemePreference},
};
use gpui::{
    Context, Entity, FocusHandle, Focusable, IntoElement, KeyDownEvent, ScrollStrategy,
    SharedString, Subscription, UniformListScrollHandle, Window, WindowBounds, div, prelude::*, px,
    relative, rgb, uniform_list,
};
use std::{sync::Arc, time::Duration};

#[path = "ui_components.rs"]
mod components;
use components::{
    Hint, button, count_label, empty_state, icon, icon_sized, row_play_button, toolbar_button,
    toolbar_icon, track_row, wordmark,
};
#[path = "connection_screen.rs"]
mod connection_screen;
#[path = "theme_picker.rs"]
mod theme_picker;
use theme_picker::ThemeMenu;
#[path = "demo.rs"]
mod demo;
#[path = "folder_columns.rs"]
mod folder_columns;
#[path = "keyboard_help.rs"]
mod keyboard_help;
#[cfg(feature = "ui-tests")]
#[path = "ui_tests.rs"]
mod ui_tests;
#[cfg(feature = "ui-tests")]
pub(crate) use ui_tests::start_ui_test;
#[path = "library_view.rs"]
mod library_view;
#[path = "player_bar.rs"]
mod player_bar;
#[path = "search_view.rs"]
mod search_view;
#[path = "sidebar.rs"]
mod sidebar;
#[path = "status_bar.rs"]
mod status_bar;

// Single-line labels use truncate(), which also disables wrapping. In GPUI 0.2.2,
// text_ellipsis() alone can reuse shortened text runs during wrapped layout and
// split a UTF-8 character on a later measurement.

#[derive(Clone)]
enum FailureTarget {
    Connection,
    Operation(Operation),
}

pub struct PlayerView {
    connection: Entity<ConnectionController>,
    browser: Browser,
    library_loader: Entity<LibraryLoader>,
    collection_requests: Requests,
    pending_collection: Option<Collection>,
    server: Entity<TextInput>,
    username: Entity<TextInput>,
    password: Entity<TextInput>,
    search: Entity<TextInput>,
    subscriptions: Vec<Subscription>,
    library_focus: FocusHandle,
    focus_library: bool,
    keyboard_active: bool,
    keys: Keys,
    connection_scroll: gpui::ScrollHandle,
    list_scroll: UniformListScrollHandle,
    folder_parent_scroll: UniformListScrollHandle,
    folder_preview_scroll: UniformListScrollHandle,
    show_help: bool,
    theme_menu: Option<ThemeMenu>,
    search_delay: Option<gpui::Task<()>>,
    seek_slider: Entity<Slider>,
    volume_slider: Entity<Slider>,
    unmuted_volume: f32,
    likes: Likes,
    failures: Failures,
    transport: PlaybackController,
    settings: Settings,
    pending_folder: Option<SavedFolder>,
    #[cfg(feature = "ui-tests")]
    ui_probe: std::rc::Rc<std::cell::RefCell<ui_tests::RenderProbe>>,
    #[cfg(feature = "ui-tests")]
    demo_hint: Option<ui_tests::DemoHint>,
    demo: bool,
}

impl PlayerView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>, settings: Settings) -> Self {
        let initial_volume = settings.value().volume;
        let initial_shuffle = settings.value().shuffle;
        let server = cx.new(|cx| {
            TextInput::new(
                "https://music.example.com",
                std::env::var("NAVIDROME_URL").unwrap_or_default(),
                false,
                cx,
            )
        });
        let username = cx.new(|cx| {
            TextInput::new(
                "Navidrome username",
                std::env::var("NAVIDROME_USERNAME").unwrap_or_default(),
                false,
                cx,
            )
        });
        let password = cx.new(|cx| {
            TextInput::new(
                "Password",
                std::env::var("NAVIDROME_PASSWORD").unwrap_or_default(),
                true,
                cx,
            )
        });
        let search =
            cx.new(|cx| TextInput::new("Search library", String::new(), false, cx).clearable());
        let seek_slider = cx.new(|_| Slider::new("seek", CommitMode::OnRelease));
        let volume_slider = cx.new(|_| Slider::new("volume-slider", CommitMode::Continuous));
        let library_loader = cx.new(|_| LibraryLoader::default());
        let environment_connection = ["NAVIDROME_URL", "NAVIDROME_USERNAME", "NAVIDROME_PASSWORD"]
            .iter()
            .any(|name| std::env::var_os(name).is_some());
        let connection = cx.new(|_| ConnectionController::new(environment_connection));
        let mut subscriptions = Vec::new();
        subscriptions.push(cx.observe_window_appearance(window, |this, _, cx| {
            this.settings
                .value()
                .theme
                .follow_appearance(cx.window_appearance(), cx);
        }));
        subscriptions.push(cx.subscribe(&connection, |this, _, event, cx| {
            match event {
                ConnectionEvent::Restored(connection) => {
                    this.server.update(cx, |field, cx| {
                        field.set_value(connection.server.clone(), cx)
                    });
                    this.username.update(cx, |field, cx| {
                        field.set_value(connection.username.clone(), cx)
                    });
                    this.password.update(cx, |field, cx| {
                        field.set_value(connection.password.clone(), cx)
                    });
                }
                ConnectionEvent::Connected => {
                    this.password.update(cx, |field, cx| field.clear(cx));
                    this.open_section(this.settings.value().last_section, cx);
                }
                ConnectionEvent::Changed => {}
            }
            cx.notify();
        }));
        subscriptions.push(cx.subscribe(&library_loader, |this, loader, _, cx| {
            let outcome = loader.update(cx, |loader, _| loader.take_outcome());
            if let Some(outcome) = outcome {
                this.accept_library(outcome, cx);
            }
        }));
        subscriptions.push(cx.subscribe(&seek_slider, |this, _, event, cx| {
            if let SliderEvent::Change(ratio) = event
                && let Some(song) = this.transport.queue().current()
            {
                this.transport
                    .seek((song.duration as f32 * ratio).round() as u64);
            }
            cx.notify();
        }));
        subscriptions.push(cx.subscribe(&volume_slider, |this, _, event, cx| {
            if let SliderEvent::Change(value) = event {
                this.set_volume(*value, cx);
            }
        }));
        subscriptions.push(cx.observe_window_activation(window, |this, _, cx| {
            this.settings
                .value()
                .theme
                .follow_appearance(cx.window_appearance(), cx);
            this.seek_slider.update(cx, |slider, cx| slider.cancel(cx));
            this.volume_slider
                .update(cx, |slider, cx| slider.cancel(cx));
            cx.notify();
        }));
        subscriptions.push(cx.observe_window_bounds(window, |this, window, _| {
            if matches!(window.window_bounds(), WindowBounds::Windowed(_)) {
                let size = window.viewport_size();
                this.settings.update(|value| {
                    value.window_size = [f32::from(size.width), f32::from(size.height)];
                });
            }
        }));
        subscriptions.push(cx.on_app_quit(|this, cx| {
            this.transport.clear_media();
            this.remember_folder(cx);
            let pending = this.settings.flush();
            async move {
                match pending.await {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => crate::diagnostics::event(&format!(
                        "Could not save preferences on exit: {error:#}"
                    )),
                    Err(_) => crate::diagnostics::event(
                        "Preferences worker stopped before the final save",
                    ),
                }
            }
        }));
        let fields = [server.clone(), username.clone(), password.clone()];
        for (index, field) in fields.iter().enumerate() {
            let fields = fields.clone();
            subscriptions.push(cx.subscribe_in(
                field,
                window,
                move |this, _, event, window, cx| {
                    match event {
                        InputEvent::Submit => this.connect(cx),
                        InputEvent::Next(reverse) => {
                            let next = if *reverse {
                                (index + 2) % 3
                            } else {
                                (index + 1) % 3
                            };
                            window.focus(&fields[next].focus_handle(cx));
                        }
                        InputEvent::Change => this
                            .connection
                            .update(cx, |connection, _| connection.edited()),
                    }
                },
            ));
        }
        subscriptions.push(
            cx.subscribe_in(&search, window, |this, input, event, window, cx| {
                match event {
                    InputEvent::Change => {
                        this.search_changed(input.read(cx).value(), window, cx);
                        this.remember_folder(cx);
                    }
                    InputEvent::Submit | InputEvent::Next(_) => {
                        this.submit_search(cx);
                        window.focus(&this.library_focus);
                        this.keyboard_active = true;
                        this.reveal_cursor();
                    }
                }
                cx.notify();
            }),
        );
        let timer = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            loop {
                timer.timer(Duration::from_millis(200)).await;
                if this.update(cx, |this, cx| this.tick(cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
        let demo = std::env::args().any(|arg| arg == "--demo");
        let mut view = Self {
            connection,
            browser: Browser::default(),
            library_loader,
            collection_requests: Requests::default(),
            pending_collection: None,
            server,
            username,
            password,
            search,
            subscriptions,
            library_focus: cx.focus_handle(),
            focus_library: demo,
            keyboard_active: false,
            keys: Keys::default(),
            connection_scroll: gpui::ScrollHandle::new(),
            list_scroll: UniformListScrollHandle::new(),
            folder_parent_scroll: UniformListScrollHandle::new(),
            folder_preview_scroll: UniformListScrollHandle::new(),
            show_help: false,
            theme_menu: None,
            search_delay: None,
            seek_slider,
            volume_slider,
            unmuted_volume: if initial_volume > 0. {
                initial_volume
            } else {
                0.7
            },
            likes: Likes::default(),
            failures: Failures::default(),
            transport: PlaybackController::new(initial_volume, initial_shuffle),
            settings,
            pending_folder: None,
            #[cfg(feature = "ui-tests")]
            ui_probe: Default::default(),
            #[cfg(feature = "ui-tests")]
            demo_hint: None,
            demo,
        };
        if demo {
            view.open_section(view.settings.value().last_section, cx);
        } else if !view.server.read(cx).value().is_empty()
            && !view.username.read(cx).value().is_empty()
            && !view.password.read(cx).value().is_empty()
        {
            view.connect(cx);
        } else {
            window.focus(&view.server.focus_handle(cx));
            if !environment_connection {
                view.restore_connection(cx);
            }
        }
        view
    }

    fn restore_connection(&mut self, cx: &mut Context<Self>) {
        self.connection
            .update(cx, |connection, cx| connection.restore(cx));
    }

    fn connect(&mut self, cx: &mut Context<Self>) {
        let credentials = Arc::new(Connection {
            server: self.server.read(cx).value(),
            username: self.username.read(cx).value(),
            password: self.password.read(cx).value(),
        });
        self.connection.update(cx, |connection, cx| {
            connection.connect(credentials, true, cx)
        });
    }

    fn disconnect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.connection.read(cx).is_busy() {
            return;
        }
        self.remember_folder(cx);
        self.pending_folder = None;
        self.browser.clear();
        self.cancel_collection();
        self.reset_navigation_ui(cx);
        self.transport.disconnect();
        self.connection
            .update(cx, |connection, cx| connection.disconnect(cx));
        self.demo = false;
        self.failures = Failures::default();
        self.likes = Likes::default();
        self.password.update(cx, |field, cx| field.clear(cx));
        window.focus(&self.password.focus_handle(cx));
        cx.notify();
    }

    fn failure_banners(&self, cx: &mut Context<Self>) -> Vec<gpui::AnyElement> {
        let mut failures: Vec<_> = self
            .failures
            .iter()
            .map(|(operation, failure)| {
                (
                    FailureTarget::Operation(operation.clone()),
                    failure.message.clone(),
                    failure.retry.is_some(),
                )
            })
            .collect();
        if let Some(failure) = self.connection.read(cx).failure() {
            failures.insert(
                0,
                (
                    FailureTarget::Connection,
                    failure.message.clone(),
                    failure.retry.is_some(),
                ),
            );
        }
        failures
            .into_iter()
            .map(|(target, message, retry)| self.failure_banner(target, message, retry, cx))
            .collect()
    }

    fn failure_banner(
        &self,
        target: FailureTarget,
        message: String,
        retry: bool,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let colors = crate::theme::colors(cx);
        let id: SharedString = match &target {
            FailureTarget::Connection => "retry-connection".into(),
            FailureTarget::Operation(Operation::Library) => "retry-library".into(),
            FailureTarget::Operation(Operation::Collection) => "retry-collection".into(),
            FailureTarget::Operation(Operation::Playback) => "retry-playback".into(),
            FailureTarget::Operation(Operation::Like(id)) => format!("retry-like-{id}").into(),
        };
        #[cfg(feature = "ui-tests")]
        self.ui_probe.borrow_mut().failures.push(id.to_string());
        div()
            .flex_shrink_0()
            .p_3()
            .rounded_md()
            .bg(rgb(colors.error_bg))
            .text_color(rgb(colors.error_text))
            .flex()
            .items_center()
            .gap_3()
            .child(div().flex_1().min_w_0().whitespace_normal().child(message))
            .when(retry, |view| {
                view.child(
                    button(colors, id, "Retry").on_click(
                        cx.listener(move |this, _, _, cx| this.retry(target.clone(), cx)),
                    ),
                )
            })
            .into_any_element()
    }

    fn reset_library_load(&mut self, cx: &mut Context<Self>) {
        self.search_delay = None;
        self.library_loader.update(cx, |loader, _| loader.cancel());
        self.failures.clear(&Operation::Library);
    }

    fn reset_navigation_ui(&mut self, cx: &mut Context<Self>) {
        self.reset_library_load(cx);
        self.keys.reset();
        self.show_help = false;
        self.focus_library = true;
        self.list_scroll = UniformListScrollHandle::new();
        self.reset_folder_columns();
        let text = match &self.browser.state().page {
            Page::Search(query) => query.clone(),
            _ => String::new(),
        };
        self.search
            .update(cx, |field, cx| field.set_value(text, cx));
    }

    fn navigate(&mut self, page: Page, remember: bool, cx: &mut Context<Self>) {
        self.remember_folder(cx);
        self.pending_folder = None;
        self.browser.navigate(page, remember);
        self.navigation_changed(cx);
    }

    fn remember_folder(&mut self, cx: &gpui::App) {
        let library_id = if self.demo {
            "demo".to_owned()
        } else if let Some(client) = &self.connection.read(cx).client {
            client.library_id()
        } else {
            return;
        };
        if let Some(folder) = self.browser.saved_folder(library_id) {
            self.settings
                .update(|value| value.last_folder = Some(folder));
        }
    }

    fn open_section(&mut self, section: Section, cx: &mut Context<Self>) {
        self.remember_folder(cx);
        let library_id = if self.demo {
            "demo".to_owned()
        } else {
            self.connection
                .read(cx)
                .client
                .as_ref()
                .map(Navidrome::library_id)
                .unwrap_or_default()
        };
        let saved = self
            .settings
            .value()
            .last_folder
            .clone()
            .filter(|saved| section == Section::Folders && saved.library_id == library_id);
        let page = saved.as_ref().map_or_else(
            || Page::for_section(section),
            |saved| Page::Folder(saved.path.clone()),
        );
        self.navigate(page, false, cx);
        if let Some(saved) = saved {
            if self.browser.state().loaded {
                self.browser.restore_folder(&saved);
                self.reset_navigation_ui(cx);
                self.remember_folder(cx);
            } else {
                self.pending_folder = Some(saved);
            }
        }
    }

    fn navigation_changed(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.browser.state().page, Page::Search(_)) {
            self.settings
                .update(|value| value.last_section = self.browser.state().page.section());
        }
        self.reset_navigation_ui(cx);
        if !self.browser.state().loaded {
            if self.demo {
                self.load_demo_page();
            } else {
                self.load(self.browser.request(), cx);
            }
        } else if matches!(self.browser.state().page, Page::Liked) {
            self.merge_liked();
        }
        self.remember_folder(cx);
        self.reveal_cursor();
        cx.notify();
    }

    fn load(&mut self, request: LibraryRequest, cx: &mut Context<Self>) {
        let Some(client) = self.connection.read(cx).client.clone() else {
            return;
        };
        let (request_id, cancellation) = self.browser.start_load();
        self.failures.clear(&Operation::Library);
        let context = LoadContext {
            request_id,
            request,
            like_revision: self.likes.revision(),
            playback_generation: self.transport.generation(),
        };
        self.library_loader.update(cx, |loader, cx| {
            loader.start(client, context, cancellation, cx)
        });
        cx.notify();
    }

    fn accept_library(&mut self, outcome: LoadOutcome, cx: &mut Context<Self>) {
        let LoadOutcome { context, result } = outcome;
        let LoadContext {
            request_id,
            request,
            like_revision,
            playback_generation,
        } = context;
        if !self.browser.finish_load(request_id) {
            return;
        }
        match result {
            Ok(mut data) => {
                if matches!(self.browser.state().page, Page::Liked)
                    && let LibraryData::Songs(songs) = data
                {
                    data = LibraryData::Songs(self.likes.accept_server(songs, like_revision));
                }
                self.browser.apply(data, request.offset > 0);
                if let Some(saved) = self.pending_folder.take() {
                    self.browser.restore_folder(&saved);
                    self.reset_navigation_ui(cx);
                }
                self.remember_folder(cx);
            }
            Err(error) => self.failures.set(
                Operation::Library,
                Failure::new(format!("{error:#}"), RetryAction::Library(request)),
            ),
        }
        if self.transport.generation() == playback_generation || !self.follow_current_track() {
            self.reveal_cursor();
        }
        cx.notify();
    }

    fn load_more(&mut self, cx: &mut Context<Self>) {
        if let Some(request) = self.browser.more_request() {
            self.load(request, cx);
        }
    }

    fn back(&mut self, cx: &mut Context<Self>) {
        self.remember_folder(cx);
        self.pending_folder = None;
        if self.browser.back() {
            self.navigation_changed(cx);
        }
    }

    fn refresh_folders(&mut self, cx: &mut Context<Self>) {
        if self.browser.is_loading() {
            return;
        }
        self.browser.clear_folders();
        self.navigate(Page::Folder(String::new()), false, cx);
    }

    fn retry(&mut self, target: FailureTarget, cx: &mut Context<Self>) {
        let FailureTarget::Operation(operation) = target else {
            self.connection
                .update(cx, |connection, cx| connection.retry(cx));
            return;
        };
        if operation == Operation::Library && self.browser.is_loading() {
            return;
        }
        let Some(action) = self
            .failures
            .take(&operation)
            .and_then(|failure| failure.retry)
        else {
            return;
        };
        match action {
            RetryAction::Library(request) => self.load(request, cx),
            RetryAction::Collection(collection) => self.play_collection(collection, cx),
            RetryAction::Playback(generation) => {
                if self.transport.generation() == generation {
                    self.play_current(cx);
                }
            }
            RetryAction::Like { song, liked } => self.set_like(song, liked, cx),
        }
        cx.notify();
    }

    fn play(&mut self, queue: Arc<Vec<Song>>, index: usize, cx: &mut Context<Self>) {
        self.cancel_collection();
        if self.demo {
            self.failures.set(
                Operation::Playback,
                "This demo previews the interface. Connect to Navidrome to play music.".into(),
            );
            cx.notify();
            return;
        }
        if let Some(client) = &self.connection.read(cx).client
            && self.transport.play(client, queue, index)
        {
            self.playback_changed(cx);
        }
    }
    fn play_current(&mut self, cx: &mut Context<Self>) {
        if let Some(client) = &self.connection.read(cx).client
            && self.transport.restart(client)
        {
            self.playback_changed(cx);
        }
    }
    fn playback_changed(&mut self, cx: &mut Context<Self>) {
        self.cancel_collection();
        self.seek_slider.update(cx, |slider, cx| slider.cancel(cx));
        self.failures.clear(&Operation::Playback);
        self.follow_current_track();
        cx.notify();
    }

    fn follow_current_track(&mut self) -> bool {
        if self.show_help || self.browser.is_loading() {
            return false;
        }
        let Some(song) = self.transport.queue().current() else {
            return false;
        };
        let entries = self.visible_entries();
        let Some(row) = entries.iter().position(|entry| {
            self.entry_song(entry)
                .is_some_and(|candidate| candidate.id == song.id)
        }) else {
            return false;
        };
        self.browser.set_cursor(row);
        self.list_scroll.scroll_to_item(row, ScrollStrategy::Center);
        true
    }

    fn play_row(&mut self, row: usize, cx: &mut Context<Self>) {
        if self.browser.is_loading() {
            return;
        }
        if let Some((songs, index)) = self.browser.selected_queue(row) {
            self.browser.set_cursor(row);
            self.play(songs, index, cx);
        }
    }

    fn play_selection(&mut self, cx: &mut Context<Self>) {
        if self.browser.is_loading() {
            return;
        }
        let entries = self.visible_entries();
        match entries.get(self.browser.state().cursor) {
            Some(BrowserEntry::Artist(index)) => self.play_collection(
                Collection::Artist(self.browser.state().artists[*index].clone()),
                cx,
            ),
            Some(BrowserEntry::Album(index)) => self.play_collection(
                Collection::Album(self.browser.state().albums[*index].clone()),
                cx,
            ),
            Some(BrowserEntry::Folder(path)) => self.play_folder(path, cx),
            _ => match &self.browser.state().page {
                Page::Folder(path) => self.play_folder(&path.clone(), cx),
                Page::Album(_) | Page::Liked => {
                    self.play(self.browser.state().songs.clone(), 0, cx)
                }
                _ => self.play_row(self.browser.state().cursor, cx),
            },
        }
    }

    fn cancel_collection(&mut self) {
        self.collection_requests.invalidate();
        self.pending_collection = None;
        self.failures.clear(&Operation::Collection);
    }

    fn play_collection(&mut self, collection: Collection, cx: &mut Context<Self>) {
        if self.demo {
            self.play(Arc::default(), 0, cx);
            return;
        }
        let Some(client) = self.connection.read(cx).client.clone() else {
            return;
        };
        let (request_id, cancellation) = self.collection_requests.start();
        let session_id = self.connection.read(cx).session_id;
        let generation = self.transport.generation();
        self.pending_collection = Some(collection.clone());
        self.failures.clear(&Operation::Collection);
        let worker_collection = collection.clone();
        let task = cx
            .background_executor()
            .spawn(async move { worker_collection.songs(&client, &cancellation) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if !this.collection_requests.is_current(request_id)
                    || this.connection.read(cx).session_id != session_id
                {
                    return;
                }
                this.pending_collection = None;
                if this.transport.generation() == generation {
                    match result {
                        Ok(songs) if !songs.is_empty() => this.play(Arc::new(songs), 0, cx),
                        Ok(_) => this.failures.set(
                            Operation::Collection,
                            format!("No songs in {}", collection.name()).into(),
                        ),
                        Err(error) => this.failures.set(
                            Operation::Collection,
                            Failure::new(
                                format!("Could not play {}: {error:#}", collection.name()),
                                RetryAction::Collection(collection),
                            ),
                        ),
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn play_folder(&mut self, path: &str, cx: &mut Context<Self>) {
        if self.browser.is_loading() {
            return;
        }
        if let Some(library) = self.browser.folder_library() {
            self.play(Arc::new(library.songs_recursive(path)), 0, cx);
        }
    }

    fn toggle_like(&mut self, song: Song, cx: &mut Context<Self>) {
        if !self.demo && self.connection.read(cx).client.is_none() {
            return;
        }
        if let Some(liked) = self.likes.begin(&song) {
            self.send_like(song, liked, cx);
        }
    }

    fn set_like(&mut self, song: Song, liked: bool, cx: &mut Context<Self>) {
        if !self.demo && self.connection.read(cx).client.is_none() {
            return;
        }
        let Some(liked) = self.likes.begin_set(&song, liked) else {
            return;
        };
        self.send_like(song, liked, cx);
    }

    fn send_like(&mut self, song: Song, liked: bool, cx: &mut Context<Self>) {
        self.failures.clear(&Operation::Like(song.id.clone()));
        if self.demo {
            self.finish_like(&song, true);
            cx.notify();
            return;
        }
        let client = self
            .connection
            .read(cx)
            .client
            .clone()
            .expect("connected client");
        let session_id = self.connection.read(cx).session_id;
        let id = song.id.clone();
        let task = cx
            .background_executor()
            .spawn(async move { client.set_liked(&id, liked) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.connection.read(cx).session_id != session_id {
                    return;
                }
                this.finish_like(&song, result.is_ok());
                if let Err(error) = result {
                    this.failures.set(
                        Operation::Like(song.id.clone()),
                        Failure::new(
                            format!("Could not save the like for “{}”: {error:#}", song.title),
                            RetryAction::Like { song, liked },
                        ),
                    );
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn finish_like(&mut self, song: &Song, success: bool) {
        self.likes.finish(song, success);
        if success {
            self.failures.clear(&Operation::Like(song.id.clone()));
        }
        if success && matches!(self.browser.state().page, Page::Liked) {
            self.merge_liked();
        }
    }

    fn merge_liked(&mut self) {
        let songs = self
            .likes
            .merge_liked(self.browser.state().songs.as_ref().clone());
        self.browser.replace_songs(songs);
    }

    fn like_button(
        &self,
        song: &Song,
        slot: &'static str,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let colors = crate::theme::colors(cx);
        let liked = self.likes.is_liked(song);
        let pending = self.likes.is_pending(&song.id);
        let song = song.clone();
        let label = if pending {
            "Saving…"
        } else if liked {
            "Remove from liked songs"
        } else {
            "Add to liked songs"
        };
        div()
            .id(SharedString::from(format!("{slot}-{}", song.id)))
            .size(px(32.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .when(pending, |view| view.opacity(0.5))
            .when(!pending, |view| view.cursor_pointer())
            .hover(move |s| s.bg(rgb(colors.hover)))
            .child(icon(
                if liked { "heart-filled" } else { "heart" },
                if liked { colors.liked } else { colors.muted },
            ))
            .tooltip(move |_, cx| cx.new(|_| Hint::new(label)).into())
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                window.focus(&this.library_focus);
                this.toggle_like(song.clone(), cx);
            }))
            .into_any_element()
    }

    fn next(&mut self, cx: &mut Context<Self>) {
        if let Some(client) = &self.connection.read(cx).client
            && self.transport.next(client)
        {
            self.playback_changed(cx);
        }
    }
    fn previous(&mut self, cx: &mut Context<Self>) {
        if let Some(client) = &self.connection.read(cx).client
            && self.transport.previous(client)
        {
            self.playback_changed(cx);
        }
    }
    fn tick(&mut self, cx: &mut Context<Self>) {
        if self.settings.poll() {
            cx.notify();
        }
        let update = self
            .transport
            .poll(self.connection.read(cx).client.as_ref());
        if update.track_changed {
            self.failures.clear(&Operation::Playback);
            self.seek_slider.update(cx, |slider, cx| slider.cancel(cx));
            self.follow_current_track();
        }
        if let Some(failure) = update.failure {
            self.failures.set(Operation::Playback, failure);
        }
        if update.changed {
            cx.notify();
        }
    }
    fn toggle(&mut self, cx: &mut Context<Self>) {
        if let Some(client) = &self.connection.read(cx).client
            && self.transport.toggle(client)
        {
            self.playback_changed(cx);
        }
    }
    fn toggle_shuffle(&mut self, cx: &mut Context<Self>) {
        let shuffled = self.transport.toggle_shuffle();
        self.settings.update(|value| value.shuffle = shuffled);
        cx.notify();
    }

    fn adjust_volume(&mut self, delta: f32, cx: &mut Context<Self>) {
        self.set_volume(self.settings.value().volume + delta, cx);
    }

    fn set_volume(&mut self, volume: f32, cx: &mut Context<Self>) {
        self.settings
            .update(|value| value.volume = volume.clamp(0., 1.));
        self.transport.volume(self.settings.value().volume);
        if self.settings.value().volume > 0. {
            self.unmuted_volume = self.settings.value().volume;
        }
        cx.notify();
    }

    fn toggle_mute(&mut self, cx: &mut Context<Self>) {
        self.set_volume(
            if self.settings.value().volume > 0. {
                0.
            } else {
                self.unmuted_volume
            },
            cx,
        );
    }

    fn title(&self) -> String {
        match &self.browser.state().page {
            Page::Artists => "Artists".into(),
            Page::Albums => "Albums".into(),
            Page::Songs => "Songs".into(),
            Page::Liked => "Liked songs".into(),
            Page::Search(_) => "Search results".into(),
            Page::Artist(artist) => artist.name.clone(),
            Page::Album(album) => album.name.clone(),
            Page::Folder(path) => {
                if path.is_empty() {
                    "Folders".into()
                } else {
                    file_name(path).into()
                }
            }
        }
    }

    fn visible_entries(&self) -> Arc<Vec<BrowserEntry>> {
        self.browser.entries()
    }

    fn entry_song(&self, entry: &BrowserEntry) -> Option<&Song> {
        self.browser.entry_song(entry)
    }

    fn reveal_cursor(&self) {
        self.list_scroll
            .scroll_to_item(self.browser.state().cursor, ScrollStrategy::Top);
        self.folder_preview_scroll
            .scroll_to_item(0, ScrollStrategy::Top);
    }

    fn cursor_on(&self, row: usize) -> bool {
        self.keyboard_active && self.browser.state().cursor == row
    }

    fn section_index(&self) -> usize {
        self.browser.state().page.section().index()
    }

    fn switch_section(&mut self, index: usize, cx: &mut Context<Self>) {
        self.browser.clear_history();
        self.open_section(Section::from_index(index), cx);
    }

    fn open_selected(&mut self, cx: &mut Context<Self>) {
        if self.browser.is_loading() {
            return;
        }
        let entries = self.visible_entries();
        let Some(entry) = entries.get(self.browser.state().cursor) else {
            return;
        };
        match entry {
            BrowserEntry::Artist(i) => self.navigate(
                Page::Artist(self.browser.state().artists[*i].clone()),
                true,
                cx,
            ),
            BrowserEntry::Album(i) => self.navigate(
                Page::Album(self.browser.state().albums[*i].clone()),
                true,
                cx,
            ),
            BrowserEntry::Folder(path) => self.navigate(Page::Folder(path.clone()), true, cx),
            BrowserEntry::Song(_) => self.play_row(self.browser.state().cursor, cx),
        }
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.theme_menu.is_some() {
            return;
        }
        let editing = self.search.focus_handle(cx).is_focused(window);
        let Some(command) = self.keys.command_in_context(&event.keystroke, editing) else {
            return;
        };
        cx.stop_propagation();
        window.prevent_default();
        if self.show_help
            && !matches!(
                command,
                Command::Escape
                    | Command::Help
                    | Command::Section(_)
                    | Command::Cycle(_)
                    | Command::ToggleSidebar
                    | Command::FocusSearch
                    | Command::Toggle
                    | Command::Shuffle
                    | Command::Next
                    | Command::Previous
                    | Command::LikeCurrent
                    | Command::Seek(_)
                    | Command::Volume(_)
            )
        {
            self.keys.reset();
            return;
        }
        if command == Command::ToggleSidebar {
            self.toggle_sidebar(cx);
            return;
        }
        if editing && command == Command::Escape {
            window.focus(&self.library_focus);
            self.keyboard_active = true;
            self.reveal_cursor();
            cx.notify();
            return;
        }
        use Command::*;
        let count = self.visible_entries().len();
        let viewport_height =
            f32::from(self.list_scroll.0.borrow().base_handle.bounds().size.height);
        let row_height = match self.browser.state().page {
            Page::Artists => ARTIST_ROW_HEIGHT,
            Page::Albums | Page::Artist(_) => ALBUM_ROW_HEIGHT,
            Page::Folder(_) => FOLDER_ROW_HEIGHT,
            _ => SONG_ROW_HEIGHT,
        };
        let page = page_items(viewport_height, row_height);
        self.keyboard_active = true;
        match command {
            Down => self
                .browser
                .set_cursor(moved_cursor(self.browser.state().cursor, 1, count)),
            Up => self
                .browser
                .set_cursor(moved_cursor(self.browser.state().cursor, -1, count)),
            Back => self.back(cx),
            Open => self.open_selected(cx),
            PlaySelection => self.play_selection(cx),
            First => self.browser.set_cursor(0),
            Last => self.browser.set_cursor(count.saturating_sub(1)),
            HalfDown | HalfUp | PageDown | PageUp => {
                let step = if matches!(command, HalfDown | HalfUp) {
                    (page / 2).max(1)
                } else {
                    page
                };
                let delta = if matches!(command, HalfUp | PageUp) {
                    -(step as isize)
                } else {
                    step as isize
                };
                self.browser
                    .set_cursor(moved_cursor(self.browser.state().cursor, delta, count));
            }
            FocusSearch => {
                self.show_help = false;
                window.focus(&self.search.focus_handle(cx));
            }
            Escape => {
                if self.show_help {
                    self.show_help = false;
                } else if matches!(self.browser.state().page, Page::Search(_)) {
                    self.back(cx);
                } else {
                    self.failures = Failures::default();
                    self.connection
                        .update(cx, |connection, _| connection.dismiss_failure());
                }
                window.focus(&self.library_focus);
            }
            Toggle => {
                if self.transport.queue().current().is_some() {
                    self.toggle(cx);
                } else if !self.show_help {
                    self.open_selected(cx);
                }
            }
            Shuffle => self.toggle_shuffle(cx),
            Next => self.next(cx),
            Previous => self.previous(cx),
            Like | LikeCurrent => {
                let song = if command == LikeCurrent {
                    self.transport.queue().current().cloned()
                } else {
                    self.visible_entries()
                        .get(self.browser.state().cursor)
                        .and_then(|entry| self.entry_song(entry))
                        .cloned()
                };
                if let Some(song) = song {
                    self.toggle_like(song, cx);
                }
            }
            Seek(delta) => self.transport.seek_relative(delta),
            Volume(delta) => self.adjust_volume(delta, cx),
            Section(index) => self.switch_section(index, cx),
            Cycle(delta) => self.switch_section(
                (self.section_index() as i32 + delta).rem_euclid(5) as usize,
                cx,
            ),
            Help => self.show_help = !self.show_help,
            Refresh => {
                if !self.browser.is_loading() {
                    if matches!(self.browser.state().page, Page::Folder(_)) {
                        self.refresh_folders(cx);
                    } else {
                        self.navigate(self.browser.state().page.clone(), false, cx);
                    }
                }
            }
            More => self.load_more(cx),
            Prefix | ToggleSidebar => {}
        }
        self.remember_folder(cx);
        self.reveal_cursor();
        cx.notify();
    }

    fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.settings
            .update(|value| value.sidebar_compact = !value.sidebar_compact);
        cx.notify();
    }
}

impl Render for PlayerView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = crate::theme::colors(cx);
        #[cfg(feature = "ui-tests")]
        {
            *self.ui_probe.borrow_mut() = ui_tests::RenderProbe {
                theme_bg: colors.bg,
                ..Default::default()
            };
        }
        // Retain subscriptions for the lifetime of the view.
        let _ = &self.subscriptions;
        let connected = self.connection.read(cx).client.is_some() || self.demo;
        let mut root = div()
            .relative()
            .size_full()
            .bg(rgb(colors.bg))
            .text_color(rgb(colors.text))
            .text_size(px(TEXT_BODY))
            .font_family("Source Sans 3")
            .flex()
            .flex_col()
            .track_focus(&self.library_focus)
            .on_key_down(cx.listener(Self::theme_menu_key_down));
        if let Some(warning) = self.settings.warning() {
            root = root.child(
                div()
                    .flex_shrink_0()
                    .px_4()
                    .py_2()
                    .bg(rgb(colors.error_bg))
                    .text_color(rgb(colors.error_text))
                    .text_size(px(TEXT_SECONDARY))
                    .child(warning.to_owned()),
            );
        }
        if !connected {
            return root
                .child(self.connection_screen(cx))
                .child(
                    div()
                        .absolute()
                        .top(px(16.))
                        .right(px(16.))
                        .child(self.theme_toggle(cx)),
                )
                .when(self.theme_menu.is_some(), |root| {
                    root.child(self.theme_overlay(false, cx))
                });
        }
        if self.focus_library {
            window.focus(&self.library_focus);
            self.focus_library = false;
        }
        root = root
            .on_key_down(cx.listener(Self::key_down))
            .on_modifiers_changed(cx.listener(|_, _, _, cx| cx.notify()))
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.keyboard_active = false;
                    this.keys.reset();
                    cx.notify();
                }),
            );
        let content = if matches!(self.browser.state().page, Page::Search(_)) {
            self.search_results(cx)
        } else if self.browser.is_loading() && !self.browser.state().more {
            empty_state(colors, "Loading your library…")
        } else {
            match self.browser.state().page {
                Page::Artists => self.artists_list(cx),
                Page::Albums | Page::Artist(_) => self.albums_list(cx),
                Page::Songs | Page::Liked | Page::Album(_) => self.songs_list(cx),
                Page::Folder(_) => self.folder_columns(cx),
                Page::Search(_) => unreachable!(),
            }
        };
        let main = div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .child(self.header(window, cx))
            .children(self.failure_banners(cx))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .when(
                        !matches!(self.browser.state().page, Page::Folder(_)),
                        |view| view.px_4(),
                    )
                    .child(content),
            )
            .child(self.status_bar(cx));
        root = root
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .relative()
                    .flex()
                    .child(self.sidebar(window, cx))
                    .child(main)
                    .when(self.show_help, |view| {
                        view.child(self.help_overlay(window, cx))
                    }),
            )
            .child(self.player_bar(cx))
            .when(self.theme_menu.is_some(), |root| {
                root.child(self.theme_overlay(true, cx))
            });
        #[cfg(feature = "ui-tests")]
        {
            root = root.when_some(
                ui_tests::demo_overlay(colors, self.demo_hint.as_ref()),
                |root, overlay| root.child(overlay),
            );
        }
        root
    }
}
