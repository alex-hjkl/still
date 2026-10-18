use crate::{
    api::{Album, Artist, Navidrome, Song, time_label},
    artwork::{app_icon, artwork},
    browser::{Browser, BrowserEntry, LibraryData, LibraryRequest, Page},
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
    settings::{SavedFolder, Section, Settings},
    slider::{CommitMode, Slider, SliderEvent},
    theme::{
        ACCENT, ACCENT_HOVER, ARTWORK_ICON, BG, BORDER, ERROR_BG, ERROR_TEXT, HOVER,
        KEYBOARD_FOCUS, LIKED, MUTED, ON_ACCENT, PANEL, SELECTED, TEXT,
    },
};
use gpui::{
    Context, Entity, FocusHandle, Focusable, IntoElement, KeyDownEvent, ScrollStrategy,
    SharedString, Subscription, UniformListScrollHandle, Window, WindowBounds, div, prelude::*, px,
    relative, rgb, svg, uniform_list,
};
use std::{sync::Arc, time::Duration};

#[path = "connection_screen.rs"]
mod connection_screen;
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
#[path = "sidebar.rs"]
mod sidebar;

// Single-line labels use truncate(), which also disables wrapping. In GPUI 0.2.2,
// text_ellipsis() alone can reuse shortened text runs during wrapped layout and
// split a UTF-8 character on a later measurement.

fn icon_sized(name: &'static str, color: u32, edge: f32) -> gpui::Svg {
    svg()
        .path(name)
        .size(px(edge))
        .flex_shrink_0()
        .text_color(rgb(color))
}

fn icon(name: &'static str, color: u32) -> gpui::Svg {
    icon_sized(name, color, 20.)
}

fn wordmark(size: f32) -> gpui::Div {
    div()
        .relative()
        // Center the visible letters against the icon, rather than the font's
        // line box, which reserves space below the baseline for descenders.
        .top(px(size * 0.1))
        .text_size(px(size))
        .line_height(px(size))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .child("Still")
}

fn button(
    id: impl Into<gpui::ElementId>,
    label: impl Into<SharedString>,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .gap_2()
        .px_4()
        .py_2()
        .rounded_md()
        .bg(rgb(HOVER))
        .text_color(rgb(TEXT))
        .cursor_pointer()
        .child(label.into())
}

fn toolbar_icon(
    id: &'static str,
    glyph: &'static str,
    hint: &'static str,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .size(px(32.))
        .flex_shrink_0()
        .rounded_md()
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .hover(|view| view.bg(rgb(HOVER)))
        .child(icon(glyph, MUTED))
        .tooltip(move |_, cx| cx.new(|_| Hint::new(hint)).into())
}

struct Hint(SharedString);

impl Hint {
    fn new(text: impl Into<SharedString>) -> Self {
        Self(text.into())
    }
}

impl Render for Hint {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py_2()
            .rounded_md()
            .bg(rgb(TEXT))
            .text_color(rgb(ON_ACCENT))
            .text_sm()
            .child(self.0.clone())
    }
}

fn track_row(id: usize, height: f32) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .w_full()
        .h(px(height))
        .flex()
        .items_center()
        .gap_4()
        .px_3()
        .border_b_1()
        .border_color(rgb(BORDER))
        .cursor_pointer()
        .hover(|s| s.bg(rgb(PANEL)))
}

#[derive(Clone)]
enum FailureTarget {
    Connection,
    Operation(Operation),
}

pub struct PlayerView {
    connection: Entity<ConnectionController>,
    browser: Browser,
    library_loader: Entity<LibraryLoader>,
    server: Entity<TextInput>,
    username: Entity<TextInput>,
    password: Entity<TextInput>,
    filter: Entity<TextInput>,
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
        let filter =
            cx.new(|cx| TextInput::new("Find in this list", String::new(), false, cx).clearable());
        let seek_slider = cx.new(|_| Slider::new("seek", CommitMode::OnRelease));
        let volume_slider = cx.new(|_| Slider::new("volume-slider", CommitMode::Continuous));
        let library_loader = cx.new(|_| LibraryLoader::default());
        let environment_connection = ["NAVIDROME_URL", "NAVIDROME_USERNAME", "NAVIDROME_PASSWORD"]
            .iter()
            .any(|name| std::env::var_os(name).is_some());
        let connection = cx.new(|_| ConnectionController::new(environment_connection));
        let mut subscriptions = Vec::new();
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
            cx.subscribe_in(&filter, window, |this, input, event, window, cx| {
                match event {
                    InputEvent::Change => {
                        if this.browser.set_query(input.read(cx).value()) {
                            this.list_scroll.scroll_to_item(0, ScrollStrategy::Top);
                        }
                        this.remember_folder(cx);
                    }
                    InputEvent::Submit | InputEvent::Next(_) => {
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
            server,
            username,
            password,
            filter,
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
        let id: SharedString = match &target {
            FailureTarget::Connection => "retry-connection".into(),
            FailureTarget::Operation(Operation::Library) => "retry-library".into(),
            FailureTarget::Operation(Operation::Playback) => "retry-playback".into(),
            FailureTarget::Operation(Operation::Like(id)) => format!("retry-like-{id}").into(),
        };
        #[cfg(feature = "ui-tests")]
        self.ui_probe.borrow_mut().failures.push(id.to_string());
        div()
            .flex_shrink_0()
            .p_3()
            .rounded_md()
            .bg(rgb(ERROR_BG))
            .text_color(rgb(ERROR_TEXT))
            .flex()
            .items_center()
            .gap_3()
            .child(div().flex_1().min_w_0().whitespace_normal().child(message))
            .when(retry, |view| {
                view.child(
                    button(id, "Retry").on_click(
                        cx.listener(move |this, _, _, cx| this.retry(target.clone(), cx)),
                    ),
                )
            })
            .into_any_element()
    }

    fn reset_navigation_ui(&mut self, cx: &mut Context<Self>) {
        self.library_loader.update(cx, |loader, _| loader.cancel());
        self.keys.reset();
        self.show_help = false;
        self.focus_library = true;
        self.list_scroll = UniformListScrollHandle::new();
        self.reset_folder_columns();
        let text = self.browser.state().filter_text.clone();
        self.filter
            .update(cx, |field, cx| field.set_value(text, cx));
        self.failures.clear(&Operation::Library);
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
        self.settings
            .update(|value| value.last_section = self.browser.state().page.section());
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

    fn play_selected_folder(&mut self, cx: &mut Context<Self>) {
        if self.browser.is_loading() {
            return;
        }
        let entries = self.visible_entries();
        let path = match entries.get(self.browser.state().cursor) {
            Some(BrowserEntry::Folder(path)) => Some(path.clone()),
            _ => match &self.browser.state().page {
                Page::Folder(path) => Some(path.clone()),
                _ => None,
            },
        };
        if let Some(path) = path {
            self.play_folder(&path, cx);
        }
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
            .hover(|s| s.bg(rgb(HOVER)))
            .child(icon(
                if liked { "heart-filled" } else { "heart" },
                if liked { LIKED } else { MUTED },
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
        let editing = self.filter.focus_handle(cx).is_focused(window);
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
                    | Command::Find
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
            PlayFolder => self.play_selected_folder(cx),
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
            Find => {
                self.show_help = false;
                window.focus(&self.filter.focus_handle(cx));
            }
            Escape => {
                if self.show_help {
                    self.show_help = false;
                } else if !self.browser.state().query.is_empty() {
                    self.filter.update(cx, |input, cx| input.clear(cx));
                    self.browser.set_query(String::new());
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

    fn load_demo_page(&mut self) {
        self.browser.invalidate_requests();
        let mut data = match self.browser.state().page.clone() {
            Page::Artists => LibraryData::Artists(demo_artists()),
            Page::Albums => LibraryData::Albums(demo_albums()),
            Page::Artist(artist) => LibraryData::Albums(
                demo_albums()
                    .into_iter()
                    .filter(|album| album.artist == artist.name)
                    .collect(),
            ),
            Page::Songs => LibraryData::Songs(
                demo_albums()
                    .into_iter()
                    .flat_map(|album| album.song)
                    .collect(),
            ),
            Page::Liked => LibraryData::Songs(
                self.likes.merge_liked(
                    demo_albums()
                        .into_iter()
                        .flat_map(|album| album.song)
                        .filter(|song| song.starred.is_some())
                        .collect(),
                ),
            ),
            Page::Album(album) => LibraryData::Album(
                demo_albums()
                    .into_iter()
                    .find(|candidate| candidate.id == album.id)
                    .unwrap_or(album),
            ),
            Page::Folder(_) => {
                if self.browser.use_cached_folders() {
                    return;
                }
                let songs = demo_albums()
                    .into_iter()
                    .flat_map(|album| {
                        album.song.into_iter().map(move |mut song| {
                            song.path = Some(format!(
                                "/music/{}/{} - {}.flac",
                                album.name, song.track, song.title
                            ));
                            song
                        })
                    })
                    .collect();
                LibraryData::Folders(FolderLibrary::from_songs(songs).expect("valid demo paths"))
            }
        };
        if let LibraryData::Albums(albums) = &mut data {
            crate::api::sort_albums(albums);
        }
        self.browser.apply(data, false);
    }
}

impl Render for PlayerView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(feature = "ui-tests")]
        {
            *self.ui_probe.borrow_mut() = ui_tests::RenderProbe::default();
        }
        // Retain subscriptions for the lifetime of the view.
        let _ = &self.subscriptions;
        let connected = self.connection.read(cx).client.is_some() || self.demo;
        let mut root = div()
            .relative()
            .size_full()
            .bg(rgb(BG))
            .text_color(rgb(TEXT))
            .text_size(px(14.))
            .font_family("Inter")
            .flex()
            .flex_col();
        if let Some(warning) = self.settings.warning() {
            root = root.child(
                div()
                    .flex_shrink_0()
                    .px_4()
                    .py_2()
                    .bg(rgb(ERROR_BG))
                    .text_color(rgb(ERROR_TEXT))
                    .text_sm()
                    .child(warning.to_owned()),
            );
        }
        if !connected {
            return root.child(self.connection_screen(cx));
        }
        if self.focus_library {
            window.focus(&self.library_focus);
            self.focus_library = false;
        }
        root = root
            .track_focus(&self.library_focus)
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
        let content = if self.browser.is_loading() && !self.browser.state().more {
            empty_state("Loading your library…", "")
        } else {
            match self.browser.state().page {
                Page::Artists => self.artists_list(cx),
                Page::Albums | Page::Artist(_) => self.albums_list(cx),
                Page::Songs | Page::Liked | Page::Album(_) => self.songs_list(cx),
                Page::Folder(_) => self.folder_columns(cx),
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
            .child(
                div()
                    .h(px(28.))
                    .flex_shrink_0()
                    .px_4()
                    .border_t_1()
                    .border_color(rgb(BORDER))
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .gap_4()
                    .child(div().flex_1().child(
                        if matches!(self.browser.state().page, Page::Folder(_)) {
                            "h  Parent     l  Open     j k  Move     /  Find"
                        } else {
                            "j k  Move     Enter  Open / play     /  Find"
                        },
                    ))
                    .when(self.browser.state().more, |view| {
                        view.child(
                            div()
                                .id("load-more")
                                .cursor_pointer()
                                .text_color(rgb(ACCENT))
                                .child(if self.browser.is_loading() {
                                    "Loading…"
                                } else {
                                    "Load more · e"
                                })
                                .on_click(cx.listener(|this, _, _, cx| this.load_more(cx))),
                        )
                    })
                    .child(
                        div()
                            .id("status-help")
                            .cursor_pointer()
                            .child("?  Shortcuts")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.show_help = true;
                                window.focus(&this.library_focus);
                                cx.notify();
                            })),
                    ),
            );
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
            .child(self.player_bar(cx));
        #[cfg(feature = "ui-tests")]
        {
            root = root.when_some(
                ui_tests::demo_overlay(self.demo_hint.as_ref()),
                |root, overlay| root.child(overlay),
            );
        }
        root
    }
}

fn count_label(count: usize, noun: &str) -> String {
    format!("{count} {noun}{}", if count == 1 { "" } else { "s" })
}

fn empty_state(message: &str, query: &str) -> gpui::AnyElement {
    div()
        .flex_1()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_3()
        .text_color(rgb(MUTED))
        .child(
            svg()
                .path("songs")
                .size(px(32.))
                .text_color(rgb(ARTWORK_ICON)),
        )
        .child(if query.is_empty() {
            message.to_owned()
        } else {
            "No matches in the loaded list".into()
        })
        .into_any_element()
}

fn demo_artists() -> Vec<Artist> {
    [
        ("1", "Nils Frahm", 3),
        ("2", "Ólafur Arnalds", 2),
        ("3", "Hania Rani", 2),
        ("4", "Ryuichi Sakamoto", 2),
        ("5", "Max Richter", 1),
    ]
    .into_iter()
    .map(|(id, name, album_count)| Artist {
        id: id.into(),
        name: name.into(),
        album_count,
    })
    .collect()
}

fn demo_albums() -> Vec<Album> {
    [
        ("Spaces", "Nils Frahm", 2013),
        ("All Melody", "Nils Frahm", 2018),
        ("Music for Animals", "Nils Frahm", 2022),
        ("re:member", "Ólafur Arnalds", 2018),
        ("some kind of peace", "Ólafur Arnalds", 2020),
        ("Esja", "Hania Rani", 2019),
        ("Ghosts", "Hania Rani", 2023),
        ("async", "Ryuichi Sakamoto", 2017),
        ("12", "Ryuichi Sakamoto", 2023),
        ("The Blue Notebooks", "Max Richter", 2004),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (name, artist, year))| {
        let songs = [
            "Opening",
            "A quiet morning",
            "Drift",
            "Somewhere",
            "Still here",
        ]
        .into_iter()
        .enumerate()
        .map(|(track, title)| Song {
            id: format!("demo-{index}-{track}"),
            title: title.into(),
            artist: artist.into(),
            album: name.into(),
            duration: 180 + track as u64 * 23,
            track: track as u32 + 1,
            disc_number: 1,
            cover_art: None,
            path: None,
            starred: (index == 0 && track < 2).then(|| "2026-10-15T12:00:00Z".into()),
        })
        .collect();
        Album {
            id: format!("demo-{index}"),
            name: name.into(),
            artist: artist.into(),
            year: Some(year),
            song_count: 5,
            cover_art: None,
            song: songs,
        }
    })
    .collect()
}
