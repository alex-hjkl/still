use crate::{
    artwork::{app_icon, artwork},
    browser::{BrowserEntry, LibraryData, Page, PageId},
    collection::Collection,
    collection_controller::{CollectionController, CollectionEvent},
    connection::{Activity, ConnectionController, ConnectionEvent},
    credentials::Connection,
    failure::{Failures, Operation, RetryAction},
    folders::file_name,
    input::{InputEvent, TextInput},
    keyboard::{Command, Keys, moved_cursor, section_hints},
    layout::{
        ALBUM_ROW_HEIGHT, ARTIST_ROW_HEIGHT, FOLDER_ROW_HEIGHT, PLAYER_HEIGHT,
        SIDEBAR_COMPACT_WIDTH, SIDEBAR_EXPANDED_WIDTH, SONG_ROW_HEIGHT, page_items,
    },
    library_controller::{LibraryContext, LibraryController},
    library_loader::LoadOutcome,
    likes_controller::{LikesController, LikesEvent},
    models::{Song, duration_label, time_label},
    navigation::Section,
    playback_controller::PlaybackController,
    settings::Settings,
    shortcuts,
    slider::{CommitMode, Slider, SliderEvent},
    theme::{Palette, TEXT_BODY, TEXT_CAPTION, TEXT_SECONDARY, ThemePreference},
};
use gpui::{
    Context, Entity, FocusHandle, Focusable, IntoElement, KeyDownEvent, ScrollStrategy,
    SharedString, Subscription, UniformListScrollHandle, Window, WindowBounds, div, prelude::*, px,
    relative, rgb, uniform_list,
};
use std::{sync::Arc, time::Duration};

use crate::ui_components::{
    Hint, button, count_label, empty_state, icon, icon_button, icon_sized, notification,
    row_play_button, toolbar_button, toolbar_icon, track_row, wordmark,
};
#[path = "connection_screen.rs"]
mod connection_screen;
#[path = "theme_picker.rs"]
mod theme_picker;
use theme_picker::ThemeMenu;
#[path = "columns.rs"]
mod columns;
#[path = "folder_columns.rs"]
mod folder_columns;
#[path = "keyboard_help.rs"]
mod keyboard_help;
#[path = "library_columns.rs"]
mod library_columns;
#[path = "track_details.rs"]
mod track_details;
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
    Preview,
    Connection,
    Operation(Operation),
}

enum CollectionFailurePolicy {
    Preserve,
    Dismiss,
}

pub struct PlayerView {
    connection: Entity<ConnectionController>,
    library: LibraryController,
    collections: Entity<CollectionController>,
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
    library_scrolls: std::collections::HashMap<PageId, UniformListScrollHandle>,
    columns_update_pending: bool,
    folder_parent_scroll: UniformListScrollHandle,
    folder_preview_scroll: UniformListScrollHandle,
    show_help: bool,
    help_scroll: gpui::ScrollHandle,
    theme_menu: Option<ThemeMenu>,
    search_delay: Option<gpui::Task<()>>,
    seek_slider: Entity<Slider>,
    volume_slider: Entity<Slider>,
    unmuted_volume: f32,
    likes: Entity<LikesController>,
    failures: Failures,
    transport: PlaybackController,
    settings: Settings,
    #[cfg(feature = "ui-tests")]
    ui_probe: std::rc::Rc<std::cell::RefCell<ui_tests::RenderProbe>>,
    #[cfg(feature = "ui-tests")]
    demo_hint: Option<ui_tests::DemoHint>,
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
        let library = LibraryController::new(cx);
        let collections = cx.new(|_| CollectionController::default());
        let likes = cx.new(|_| LikesController::default());
        let environment_connection = ["NAVIDROME_URL", "NAVIDROME_USERNAME", "NAVIDROME_PASSWORD"]
            .iter()
            .any(|name| std::env::var_os(name).is_some());
        let demo = std::env::args().any(|arg| arg == "--demo");
        let connection = cx.new(|_| {
            if demo {
                ConnectionController::demo()
            } else {
                ConnectionController::new(environment_connection)
            }
        });
        let mut view = Self {
            connection,
            library,
            collections,
            server,
            username,
            password,
            search,
            subscriptions: Vec::new(),
            library_focus: cx.focus_handle(),
            focus_library: demo,
            keyboard_active: false,
            keys: Keys::default(),
            connection_scroll: gpui::ScrollHandle::new(),
            list_scroll: UniformListScrollHandle::new(),
            library_scrolls: Default::default(),
            columns_update_pending: false,
            folder_parent_scroll: UniformListScrollHandle::new(),
            folder_preview_scroll: UniformListScrollHandle::new(),
            show_help: false,
            help_scroll: gpui::ScrollHandle::new(),
            theme_menu: None,
            search_delay: None,
            seek_slider,
            volume_slider,
            unmuted_volume: if initial_volume > 0. {
                initial_volume
            } else {
                0.7
            },
            likes,
            failures: Failures::default(),
            transport: PlaybackController::new(initial_volume, initial_shuffle),
            settings,
            #[cfg(feature = "ui-tests")]
            ui_probe: Default::default(),
            #[cfg(feature = "ui-tests")]
            demo_hint: None,
        };
        view.subscribe_events(window, cx);
        view.initialize(window, environment_connection, cx);
        view
    }

    fn subscribe_events(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.subscriptions
            .push(cx.observe_window_appearance(window, |_, window, cx| {
                // Wayland holds its platform state while delivering this event.
                // Read the current appearance after that callback has returned.
                cx.on_next_frame(window, |this, _, cx| {
                    this.settings
                        .value()
                        .theme
                        .follow_appearance(cx.window_appearance(), cx);
                });
            }));
        self.subscriptions
            .push(cx.subscribe(&self.connection, |this, _, event, cx| {
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
        for loader in [
            self.library.loader().clone(),
            self.library.preview_loader().clone(),
        ] {
            self.subscriptions
                .push(cx.subscribe(&loader, |this, loader, _, cx| {
                    if let Some(outcome) = loader.update(cx, |loader, _| loader.take_outcome()) {
                        if loader == *this.library.loader() {
                            this.accept_library(outcome, cx);
                        } else {
                            this.library.accept_preview(outcome);
                            this.update_columns(cx);
                            cx.notify();
                        }
                    }
                }));
        }
        self.subscriptions.push(
            cx.subscribe(&self.collections, |this, controller, event, cx| {
                if let CollectionEvent::Ready = event
                    && let Some(songs) =
                        controller.update(cx, |controller, _| controller.take_ready())
                {
                    this.play(songs, 0, cx);
                }
                cx.notify();
            }),
        );
        self.subscriptions
            .push(cx.subscribe(&self.likes, |this, _, event, cx| {
                if matches!(event, LikesEvent::Saved)
                    && matches!(this.library.browser().state().page, Page::Liked)
                {
                    this.merge_liked(cx);
                    this.update_columns(cx);
                }
                cx.notify();
            }));
        self.subscriptions
            .push(cx.subscribe(&self.seek_slider, |this, _, event, cx| {
                if let SliderEvent::Change(ratio) = event
                    && this.transport.queue().current().is_some()
                {
                    this.transport
                        .seek((this.transport.duration() as f32 * ratio).round() as u64);
                }
                cx.notify();
            }));
        self.subscriptions
            .push(cx.subscribe(&self.volume_slider, |this, _, event, cx| {
                if let SliderEvent::Change(value) = event {
                    this.set_volume(*value, cx);
                }
            }));
        self.subscriptions
            .push(cx.observe_window_activation(window, |this, window, cx| {
                cx.on_next_frame(window, |this, _, cx| {
                    this.settings
                        .value()
                        .theme
                        .follow_appearance(cx.window_appearance(), cx);
                });
                this.seek_slider.update(cx, |slider, cx| slider.cancel(cx));
                this.volume_slider
                    .update(cx, |slider, cx| slider.cancel(cx));
                cx.notify();
            }));
        self.subscriptions
            .push(cx.observe_window_bounds(window, |this, window, _| {
                if matches!(window.window_bounds(), WindowBounds::Windowed(_)) {
                    let size = window.viewport_size();
                    this.settings.update(|value| {
                        value.window_size = [f32::from(size.width), f32::from(size.height)];
                    });
                }
            }));
        self.subscriptions.push(cx.on_app_quit(|this, cx| {
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
        let fields = [
            self.server.clone(),
            self.username.clone(),
            self.password.clone(),
        ];
        for (index, field) in fields.iter().enumerate() {
            let fields = fields.clone();
            self.subscriptions.push(cx.subscribe_in(
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
        self.subscriptions.push(cx.subscribe_in(
            &self.search,
            window,
            |this, input, event, window, cx| {
                match event {
                    InputEvent::Change => {
                        this.search_changed(input.read(cx).value(), window, cx);
                        this.remember_folder(cx);
                    }
                    InputEvent::Submit | InputEvent::Next(_) => {
                        this.submit_search(cx);
                        window.focus(&this.library_focus);
                        this.keyboard_active = true;
                        this.update_columns(cx);
                    }
                }
                cx.notify();
            },
        ));
    }

    fn initialize(
        &mut self,
        window: &mut Window,
        environment_connection: bool,
        cx: &mut Context<Self>,
    ) {
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
        self.sync_seek_slider(cx);
        self.sync_volume_slider(cx);
        if self.connection.read(cx).session().is_demo() {
            self.open_section(self.settings.value().last_section, cx);
        } else if !self.server.read(cx).value().is_empty()
            && !self.username.read(cx).value().is_empty()
            && !self.password.read(cx).value().is_empty()
        {
            self.connect(cx);
        } else {
            window.focus(&self.server.focus_handle(cx));
            if !environment_connection {
                self.restore_connection(cx);
            }
        }
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
        self.library.clear(cx);
        self.library_scrolls.clear();
        self.cancel_collection(cx);
        self.reset_navigation_ui(cx);
        self.transport.disconnect();
        self.sync_seek_slider(cx);
        self.connection
            .update(cx, |connection, cx| connection.disconnect(cx));
        self.failures = Failures::default();
        self.likes.update(cx, |likes, _| likes.reset());
        self.password.update(cx, |field, cx| field.clear(cx));
        window.focus(&self.password.focus_handle(cx));
        cx.notify();
    }

    fn failure_banners(&self, cx: &mut Context<Self>) -> Vec<gpui::AnyElement> {
        let mut failures: Vec<_> = self
            .failures
            .iter()
            .chain(self.likes.read(cx).failures().iter())
            .map(|(operation, failure)| {
                (
                    FailureTarget::Operation(operation.clone()),
                    failure.message.clone(),
                    failure.retry.is_some(),
                )
            })
            .collect();
        if let Some(failure) = self.library.failure() {
            failures.push((
                FailureTarget::Operation(Operation::Library),
                failure.message.clone(),
                failure.retry.is_some(),
            ));
        }
        if let Some(failure) = self.collections.read(cx).failure() {
            failures.push((
                FailureTarget::Operation(Operation::Collection),
                failure.message.clone(),
                failure.retry.is_some(),
            ));
        }
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
            FailureTarget::Preview => "retry-preview".into(),
            FailureTarget::Connection => "retry-connection".into(),
            FailureTarget::Operation(Operation::Library) => "retry-library".into(),
            FailureTarget::Operation(Operation::Collection) => "retry-collection".into(),
            FailureTarget::Operation(Operation::Playback) => "retry-playback".into(),
            FailureTarget::Operation(Operation::Like(id)) => format!("retry-like-{id}").into(),
        };
        #[cfg(feature = "ui-tests")]
        self.ui_probe.borrow_mut().failures.push(id.to_string());
        notification(colors, message)
            .when(retry, |view| {
                view.child(
                    button(colors, id, "Retry").on_click(
                        cx.listener(move |this, _, _, cx| this.retry(target.clone(), cx)),
                    ),
                )
            })
            .into_any_element()
    }

    fn reset_navigation_ui(&mut self, cx: &mut Context<Self>) {
        self.search_delay = None;
        self.keys.reset();
        self.show_help = false;
        self.focus_library = true;
        if matches!(self.library.browser().state().page, Page::Folder(_)) {
            self.list_scroll = UniformListScrollHandle::new();
        }
        self.update_columns(cx);
        self.reset_folder_columns();
        let text = match &self.library.browser().state().page {
            Page::Search(query) => query.clone(),
            _ => String::new(),
        };
        self.search
            .update(cx, |field, cx| field.set_value(text, cx));
    }

    fn navigate(&mut self, page: Page, remember: bool, cx: &mut Context<Self>) {
        self.remember_folder(cx);
        self.library.navigate(page, remember, cx);
        self.navigation_changed(cx);
    }

    fn remember_folder(&mut self, cx: &gpui::App) {
        if let Some(saved) = self
            .library
            .saved_folder(self.connection.read(cx).session())
        {
            self.settings
                .update(|value| value.last_folder = Some(saved));
        }
    }

    fn open_section(&mut self, section: Section, cx: &mut Context<Self>) {
        self.remember_folder(cx);
        let session = self.connection.read(cx).session().clone();
        self.library.open_section(
            section,
            &session,
            self.settings.value().last_folder.clone(),
            cx,
        );
        self.navigation_changed(cx);
    }

    fn navigation_changed(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.library.browser().state().page, Page::Search(_)) {
            self.settings
                .update(|value| value.last_section = self.library.browser().section());
        }
        self.reset_navigation_ui(cx);
        let context = self.library_context(cx);
        if let Some(outcome) = self.library.load_current(context, cx) {
            self.accept_library(outcome, cx);
        } else if matches!(self.library.browser().state().page, Page::Liked)
            && self.library.browser().state().loaded()
        {
            self.merge_liked(cx);
        }
        self.remember_folder(cx);
        self.update_columns(cx);
        cx.notify();
    }

    fn library_context(&self, cx: &gpui::App) -> LibraryContext {
        LibraryContext {
            session: self.connection.read(cx).session().clone(),
            like_revision: self.likes.read(cx).state().revision(),
            playback_generation: self.transport.generation(),
        }
    }

    fn accept_library(&mut self, mut outcome: LoadOutcome, cx: &mut Context<Self>) {
        if matches!(outcome.context.request.page, Page::Liked)
            && let Ok(crate::browser::LibraryPage {
                data: LibraryData::Songs(songs),
                ..
            }) = &mut outcome.result
        {
            let songs = std::mem::take(songs);
            outcome.result = Ok(LibraryData::Songs(
                if self.connection.read(cx).session().is_demo() {
                    self.likes.read(cx).state().merge_liked(songs)
                } else {
                    self.likes.update(cx, |likes, _| {
                        likes.accept_server(songs, outcome.context.like_revision)
                    })
                },
            )
            .into());
        }
        let change = self.library.accept(outcome);
        if change.restored_folder {
            self.reset_navigation_ui(cx);
        }
        self.remember_folder(cx);
        if self.transport.generation() != change.playback_generation {
            self.follow_current_track(cx);
        }
        self.update_columns(cx);
        cx.notify();
    }

    fn load_more(&mut self, cx: &mut Context<Self>) {
        let context = self.library_context(cx);
        if let Some(outcome) = self.library.load_more(context, cx) {
            self.accept_library(outcome, cx);
        }
        cx.notify();
    }

    fn back(&mut self, cx: &mut Context<Self>) {
        self.remember_folder(cx);
        if self.library.back(cx) {
            self.navigation_changed(cx);
        }
    }

    fn refresh_folders(&mut self, cx: &mut Context<Self>) {
        self.remember_folder(cx);
        if self.library.refresh_folders(cx) {
            self.navigation_changed(cx);
        }
    }

    fn retry(&mut self, target: FailureTarget, cx: &mut Context<Self>) {
        match target {
            FailureTarget::Preview => {
                let context = self.library_context(cx);
                self.library.retry_preview(context, cx);
                self.update_columns(cx);
            }
            FailureTarget::Connection => {
                self.connection
                    .update(cx, |connection, cx| connection.retry(cx));
            }
            FailureTarget::Operation(Operation::Collection) => {
                if let Some(client) = self.connection.read(cx).client().cloned() {
                    self.collections
                        .update(cx, |collections, cx| collections.retry(client, cx));
                }
            }
            FailureTarget::Operation(Operation::Like(id)) => {
                let session = self.connection.read(cx).session().clone();
                self.likes
                    .update(cx, |likes, cx| likes.retry(&session, &id, cx));
            }
            FailureTarget::Operation(Operation::Library) => {
                let context = self.library_context(cx);
                if let Some(outcome) = self.library.retry(context, cx) {
                    self.accept_library(outcome, cx);
                }
            }
            FailureTarget::Operation(operation) => {
                let action = self
                    .failures
                    .take(&operation)
                    .and_then(|failure| failure.retry);
                match action {
                    Some(RetryAction::Playback(generation))
                        if self.transport.generation() == generation =>
                    {
                        self.play_current(cx)
                    }
                    _ => {}
                }
            }
        }
        cx.notify();
    }

    fn play(&mut self, queue: Arc<Vec<Song>>, index: usize, cx: &mut Context<Self>) {
        self.cancel_collection(cx);
        if self.connection.read(cx).session().is_demo() {
            self.failures.set(
                Operation::Playback,
                "This demo previews the interface. Connect to Navidrome to play music.".into(),
            );
            cx.notify();
            return;
        }
        if let Some(client) = self.connection.read(cx).client()
            && self.transport.play(client, queue, index)
        {
            self.playback_changed(CollectionFailurePolicy::Dismiss, cx);
        }
    }
    fn play_current(&mut self, cx: &mut Context<Self>) {
        if let Some(client) = self.connection.read(cx).client()
            && self.transport.restart(client)
        {
            self.playback_changed(CollectionFailurePolicy::Dismiss, cx);
        }
    }
    fn playback_changed(
        &mut self,
        collection_failure: CollectionFailurePolicy,
        cx: &mut Context<Self>,
    ) {
        self.collections
            .update(cx, |collections, _| match collection_failure {
                CollectionFailurePolicy::Preserve => collections.cancel_pending(),
                CollectionFailurePolicy::Dismiss => collections.cancel(),
            });
        self.seek_slider.update(cx, |slider, cx| slider.cancel(cx));
        self.sync_seek_slider(cx);
        self.failures.clear(&Operation::Playback);
        self.follow_current_track(cx);
        self.update_columns(cx);
        cx.notify();
    }

    fn sync_seek_slider(&self, cx: &mut Context<Self>) {
        let duration = self.transport.duration();
        let enabled = self.transport.queue().current().is_some() && duration > 0;
        let progress = if enabled {
            self.transport.state().position.as_secs_f32() / duration as f32
        } else {
            0.
        };
        self.seek_slider.update(cx, |slider, cx| {
            slider.configure(progress, enabled, Some(duration), cx)
        });
    }

    fn follow_current_track(&mut self, cx: &gpui::App) -> bool {
        if self.show_help || self.library.is_loading(cx) {
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
        self.library.set_cursor(row);
        let scroll = if matches!(self.library.browser().state().page, Page::Folder(_)) {
            &self.list_scroll
        } else {
            self.library_scrolls
                .entry(self.library.browser().state().page.id())
                .or_default()
        };
        scroll.scroll_to_item(row, ScrollStrategy::Center);
        true
    }

    fn play_row(&mut self, row: usize, cx: &mut Context<Self>) {
        if self.library.is_loading(cx) {
            return;
        }
        if let Some((songs, index)) = self.library.browser().selected_queue(row) {
            self.library.set_cursor(row);
            self.play(songs, index, cx);
        }
    }

    fn play_selection(&mut self, cx: &mut Context<Self>) {
        if self.library.is_loading(cx) {
            return;
        }
        let entries = self.visible_entries();
        match entries.get(self.library.browser().state().cursor) {
            Some(BrowserEntry::Artist(index)) => self.play_collection(
                Collection::Artist(self.library.browser().state().artists()[*index].clone()),
                cx,
            ),
            Some(BrowserEntry::Album(index)) => self.play_collection(
                Collection::Album(self.library.browser().state().albums()[*index].clone()),
                cx,
            ),
            Some(BrowserEntry::Folder(path)) => self.play_folder(path, cx),
            _ => match &self.library.browser().state().page {
                Page::Folder(path) => self.play_folder(&path.clone(), cx),
                Page::Album(_) | Page::Liked => self.play(
                    self.library
                        .browser()
                        .state()
                        .song_collection()
                        .cloned()
                        .unwrap_or_default(),
                    0,
                    cx,
                ),
                _ => self.play_row(self.library.browser().state().cursor, cx),
            },
        }
    }

    fn cancel_collection(&mut self, cx: &mut Context<Self>) {
        self.collections
            .update(cx, |collections, _| collections.cancel());
    }

    fn play_collection(&mut self, collection: Collection, cx: &mut Context<Self>) {
        if self.connection.read(cx).session().is_demo() {
            self.play(Arc::default(), 0, cx);
            return;
        }
        if let Some(client) = self.connection.read(cx).client().cloned() {
            self.collections.update(cx, |collections, cx| {
                collections.play(client, collection, cx)
            });
        }
    }

    fn play_folder(&mut self, path: &str, cx: &mut Context<Self>) {
        if self.library.is_loading(cx) {
            return;
        }
        if let Some(library) = self.library.browser().folder_library() {
            self.play(Arc::new(library.songs_recursive(path)), 0, cx);
        }
    }

    fn toggle_like(&mut self, song: Song, cx: &mut Context<Self>) {
        let session = self.connection.read(cx).session().clone();
        self.likes
            .update(cx, |likes, cx| likes.toggle(&session, song, cx));
    }

    fn merge_liked(&mut self, cx: &gpui::App) {
        let songs = self
            .likes
            .read(cx)
            .state()
            .merge_liked(self.library.browser().state().songs().to_vec());
        self.library.replace_songs(songs);
    }

    fn like_button(
        &self,
        song: &Song,
        slot: &'static str,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let colors = crate::theme::colors(cx);
        let liked = self.likes.read(cx).state().is_liked(song);
        let pending = self.likes.read(cx).state().is_pending(&song.id);
        let song = song.clone();
        let hint_song = song.clone();
        let hint_likes = self.likes.clone();
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
            .tooltip(move |_, cx| {
                let song = hint_song.clone();
                cx.new(|cx| {
                    Hint::live(hint_likes.clone(), cx, move |likes| {
                        let state = likes.state();
                        if state.is_pending(&song.id) {
                            "Saving…"
                        } else if state.is_liked(&song) {
                            "Remove from liked songs"
                        } else {
                            "Add to liked songs"
                        }
                        .into()
                    })
                })
                .into()
            })
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                window.focus(&this.library_focus);
                this.toggle_like(song.clone(), cx);
            }))
            .into_any_element()
    }

    fn next(&mut self, cx: &mut Context<Self>) {
        if let Some(client) = self.connection.read(cx).client()
            && self.transport.next(client)
        {
            self.playback_changed(CollectionFailurePolicy::Dismiss, cx);
        }
    }
    fn previous(&mut self, cx: &mut Context<Self>) {
        if let Some(client) = self.connection.read(cx).client()
            && self.transport.previous(client)
        {
            self.playback_changed(CollectionFailurePolicy::Dismiss, cx);
        }
    }
    fn tick(&mut self, cx: &mut Context<Self>) {
        if self.settings.poll() {
            cx.notify();
        }
        let update = self.transport.poll(self.connection.read(cx).client());
        if update.track_changed {
            self.playback_changed(CollectionFailurePolicy::Preserve, cx);
        } else if update.changed {
            self.sync_seek_slider(cx);
        }
        if let Some(failure) = update.failure {
            self.failures.set(Operation::Playback, failure);
        }
        if update.changed {
            cx.notify();
        }
    }
    fn toggle(&mut self, cx: &mut Context<Self>) {
        if let Some(client) = self.connection.read(cx).client()
            && self.transport.toggle(client)
        {
            self.playback_changed(CollectionFailurePolicy::Dismiss, cx);
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
        self.sync_volume_slider(cx);
        cx.notify();
    }

    fn sync_volume_slider(&self, cx: &mut Context<Self>) {
        self.volume_slider.update(cx, |slider, cx| {
            slider.configure(self.settings.value().volume, true, None, cx)
        });
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
        self.library.browser().state().page.title()
    }

    fn visible_entries(&self) -> Arc<Vec<BrowserEntry>> {
        self.library.browser().entries()
    }

    fn entry_song(&self, entry: &BrowserEntry) -> Option<&Song> {
        self.library.browser().entry_song(entry)
    }

    fn reveal_cursor(&self) {
        self.list_scroll
            .scroll_to_item(self.library.browser().state().cursor, ScrollStrategy::Top);
        self.folder_preview_scroll
            .scroll_to_item(0, ScrollStrategy::Top);
    }

    fn cursor_on(&self, row: usize) -> bool {
        self.keyboard_active && self.library.browser().state().cursor == row
    }

    fn section_index(&self) -> usize {
        self.library.browser().section().index()
    }

    fn switch_section(&mut self, index: usize, cx: &mut Context<Self>) {
        self.library.clear_history();
        self.open_section(Section::from_index(index), cx);
    }

    fn open_selected(&mut self, cx: &mut Context<Self>) {
        if self.library.is_loading(cx) {
            return;
        }
        let entries = self.visible_entries();
        let Some(entry) = entries.get(self.library.browser().state().cursor) else {
            return;
        };
        match entry {
            BrowserEntry::Artist(i) => self.navigate(
                Page::Artist(self.library.browser().state().artists()[*i].clone()),
                true,
                cx,
            ),
            BrowserEntry::Album(i) => self.navigate(
                Page::Album(self.library.browser().state().albums()[*i].clone()),
                true,
                cx,
            ),
            BrowserEntry::Folder(path) => self.navigate(Page::Folder(path.clone()), true, cx),
            BrowserEntry::Song(_) => self.play_row(self.library.browser().state().cursor, cx),
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
        if self.show_help && !command.available_with_help() {
            self.keys.reset();
            return;
        }
        if self.show_help && matches!(command, Command::Down | Command::Up) {
            self.scroll_help(if command == Command::Down { -40. } else { 40. }, cx);
            return;
        }
        if command == Command::ToggleSidebar {
            self.toggle_sidebar(cx);
            return;
        }
        if editing && command == Command::Escape {
            window.focus(&self.library_focus);
            self.keyboard_active = true;
            self.update_columns(cx);
            cx.notify();
            return;
        }
        use Command::*;
        let count = self.visible_entries().len();
        let viewport_height =
            f32::from(self.list_scroll.0.borrow().base_handle.bounds().size.height);
        let row_height = match self.library.browser().state().page {
            Page::Artists => ARTIST_ROW_HEIGHT,
            Page::Albums | Page::Artist(_) => ALBUM_ROW_HEIGHT,
            Page::Folder(_) => FOLDER_ROW_HEIGHT,
            _ => SONG_ROW_HEIGHT,
        };
        let page = page_items(viewport_height, row_height);
        self.keyboard_active = true;
        match command {
            Down => self.library.set_cursor(moved_cursor(
                self.library.browser().state().cursor,
                1,
                count,
            )),
            Up => self.library.set_cursor(moved_cursor(
                self.library.browser().state().cursor,
                -1,
                count,
            )),
            Back => self.back(cx),
            Open => self.open_selected(cx),
            PlaySelection => self.play_selection(cx),
            First => self.library.set_cursor(0),
            Last => self.library.set_cursor(count.saturating_sub(1)),
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
                self.library.set_cursor(moved_cursor(
                    self.library.browser().state().cursor,
                    delta,
                    count,
                ));
            }
            FocusSearch => {
                self.show_help = false;
                window.focus(&self.search.focus_handle(cx));
            }
            Escape => {
                if self.show_help {
                    self.show_help = false;
                } else if matches!(self.library.browser().state().page, Page::Search(_)) {
                    self.back(cx);
                } else {
                    self.failures = Failures::default();
                    self.library.dismiss_failure();
                    self.collections
                        .update(cx, |collections, _| collections.dismiss_failure());
                    self.likes.update(cx, |likes, _| likes.dismiss_failures());
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
                let song = self.transport.queue().current().cloned().or_else(|| {
                    if command == Like && !self.show_help && !self.library.is_loading(cx) {
                        self.visible_entries()
                            .get(self.library.browser().state().cursor)
                            .and_then(|entry| self.entry_song(entry))
                            .cloned()
                    } else {
                        None
                    }
                });
                if let Some(song) = song {
                    self.toggle_like(song, cx);
                }
            }
            Seek(delta) => self.transport.seek_relative(delta),
            Volume(delta) => self.adjust_volume(delta, cx),
            Section(index) => self.switch_section(index, cx),
            Cycle(delta) => self.switch_section(
                (self.section_index() as i32 + delta)
                    .rem_euclid(crate::navigation::Section::ALL.len() as i32)
                    as usize,
                cx,
            ),
            Help => self.show_help = !self.show_help,
            Refresh => {
                if !self.library.is_loading(cx) {
                    if matches!(self.library.browser().state().page, Page::Folder(_)) {
                        self.refresh_folders(cx);
                    } else {
                        self.library.refresh(cx);
                        self.navigation_changed(cx);
                    }
                }
            }
            More => self.load_more(cx),
            Prefix | ToggleSidebar => {}
        }
        if matches!(
            command,
            Down | Up | First | Last | HalfDown | HalfUp | PageDown | PageUp
        ) {
            self.update_columns(cx);
        }
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
        let connected = self.connection.read(cx).session().is_active();
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
        let content = if matches!(self.library.browser().state().page, Page::Folder(_)) {
            if self.library.is_loading(cx) {
                empty_state(colors, "Loading your library…")
            } else {
                self.folder_columns(cx)
            }
        } else {
            self.library_columns(cx)
        };
        let main = div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .child(self.header(window, cx))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .child(content),
            )
            .children(self.failure_banners(cx))
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
