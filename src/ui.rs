use crate::{
    api::{Album, Artist, Navidrome, Song, time_label},
    artwork::artwork,
    browser::{Browser, BrowserEntry, LibraryData, LibraryRequest, Page},
    connection::{Activity, ConnectionState},
    credentials::{Connection, CredentialStore},
    failure::{Failure, RetryAction},
    folders::{FolderLibrary, file_name},
    input::{InputEvent, TextInput},
    keyboard::{Command, Keys, moved_cursor, section_hints},
    layout::{
        ALBUM_ROW_HEIGHT, ARTIST_ROW_HEIGHT, FOLDER_ROW_HEIGHT, PLAYER_HEIGHT,
        SIDEBAR_COMPACT_WIDTH, SIDEBAR_EXPANDED_WIDTH, SONG_ROW_HEIGHT, page_items,
    },
    likes::Likes,
    player::{Playback, PlaybackStatus, Player},
    queue::PlayQueue,
    settings::{SavedFolder, Section, Settings},
    slider::{CommitMode, Slider, SliderEvent},
    theme::{
        ACCENT, ACCENT_HOVER, ARTWORK_ICON, BG, BORDER, ERROR_BG, ERROR_TEXT, HOVER,
        KEYBOARD_FOCUS, LIKED, LIKED_BG, MUTED, ON_ACCENT, PANEL, SELECTED, TEXT,
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
#[path = "player_bar.rs"]
mod player_bar;

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

fn button(id: &'static str, label: impl Into<SharedString>) -> gpui::Stateful<gpui::Div> {
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

pub struct PlayerView {
    connection: ConnectionState,
    browser: Browser,
    server: Entity<TextInput>,
    username: Entity<TextInput>,
    password: Entity<TextInput>,
    filter: Entity<TextInput>,
    subscriptions: Vec<Subscription>,
    library_focus: FocusHandle,
    focus_library: bool,
    keyboard_active: bool,
    keys: Keys,
    list_scroll: UniformListScrollHandle,
    folder_parent_scroll: UniformListScrollHandle,
    folder_preview_scroll: UniformListScrollHandle,
    show_help: bool,
    seek_slider: Entity<Slider>,
    volume_slider: Entity<Slider>,
    unmuted_volume: f32,
    likes: Likes,
    media: crate::media::SystemMedia,
    error: Option<Failure>,
    player: Player,
    playback: Playback,
    queue: PlayQueue,
    generation: u64,
    settings: Settings,
    pending_folder: Option<SavedFolder>,
    #[cfg(feature = "ui-tests")]
    ui_probe: std::rc::Rc<std::cell::RefCell<ui_tests::RenderProbe>>,
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
        let mut subscriptions = Vec::new();
        subscriptions.push(cx.subscribe(&seek_slider, |this, _, event, cx| {
            if let SliderEvent::Change(ratio) = event
                && let Some(song) = this.queue.current()
            {
                this.player
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
        subscriptions.push(cx.on_app_quit(|this, _| {
            this.media.clear();
            this.remember_folder();
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
                move |this, _, event, window, cx| match event {
                    InputEvent::Submit => this.connect(cx),
                    InputEvent::Next(reverse) => {
                        let next = if *reverse {
                            (index + 2) % 3
                        } else {
                            (index + 1) % 3
                        };
                        window.focus(&fields[next].focus_handle(cx));
                    }
                    InputEvent::Change => this.connection.revision += 1,
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
                        this.remember_folder();
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
        let environment_connection = ["NAVIDROME_URL", "NAVIDROME_USERNAME", "NAVIDROME_PASSWORD"]
            .iter()
            .any(|name| std::env::var_os(name).is_some());
        let mut view = Self {
            connection: ConnectionState::new(environment_connection),
            browser: Browser::default(),
            server,
            username,
            password,
            filter,
            subscriptions,
            library_focus: cx.focus_handle(),
            focus_library: demo,
            keyboard_active: false,
            keys: Keys::default(),
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
            media: crate::media::SystemMedia::new(),
            error: None,
            player: Player::new(initial_volume),
            playback: Playback::default(),
            queue: PlayQueue::with_shuffle(initial_shuffle),
            generation: 0,
            settings,
            pending_folder: None,
            #[cfg(feature = "ui-tests")]
            ui_probe: Default::default(),
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

    fn reset_navigation_ui(&mut self, cx: &mut Context<Self>) {
        self.keys.reset();
        self.show_help = false;
        self.focus_library = true;
        self.list_scroll = UniformListScrollHandle::new();
        self.reset_folder_columns();
        let text = self.browser.state().filter_text.clone();
        self.filter
            .update(cx, |field, cx| field.set_value(text, cx));
        self.error = None;
    }

    fn navigate(&mut self, page: Page, remember: bool, cx: &mut Context<Self>) {
        self.remember_folder();
        self.pending_folder = None;
        self.browser.navigate(page, remember);
        self.navigation_changed(cx);
    }

    fn remember_folder(&mut self) {
        let library_id = if self.demo {
            "demo".to_owned()
        } else if let Some(client) = &self.connection.client {
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
        self.remember_folder();
        let library_id = if self.demo {
            "demo".to_owned()
        } else {
            self.connection
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
                self.remember_folder();
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
        self.remember_folder();
        self.reveal_cursor();
        cx.notify();
    }

    fn load(&mut self, request: LibraryRequest, cx: &mut Context<Self>) {
        let Some(client) = self.connection.client.clone() else {
            return;
        };
        let (request_id, cancellation) = self.browser.start_load();
        self.error = None;
        let like_revision = self.likes.revision();
        let playback_generation = self.generation;
        let operation = request.clone();
        let task = cx
            .background_executor()
            .spawn(async move { operation.run(client, &cancellation) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if !this.browser.finish_load(request_id) {
                    return;
                }
                match result {
                    Ok(mut data) => {
                        if matches!(this.browser.state().page, Page::Liked)
                            && let LibraryData::Songs(songs) = data
                        {
                            data =
                                LibraryData::Songs(this.likes.accept_server(songs, like_revision));
                        }
                        this.browser.apply(data, request.offset > 0);
                        if let Some(saved) = this.pending_folder.take() {
                            this.browser.restore_folder(&saved);
                            this.reset_navigation_ui(cx);
                        }
                        this.remember_folder();
                    }
                    Err(error) => {
                        this.error = Some(Failure::new(
                            format!("{error:#}"),
                            RetryAction::Library(request),
                        ))
                    }
                }
                if this.generation == playback_generation || !this.follow_current_track() {
                    this.reveal_cursor();
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn load_more(&mut self, cx: &mut Context<Self>) {
        if let Some(request) = self.browser.more_request() {
            self.load(request, cx);
        }
    }

    fn back(&mut self, cx: &mut Context<Self>) {
        self.remember_folder();
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

    fn retry(&mut self, cx: &mut Context<Self>) {
        if self.connection.is_busy() || self.browser.is_loading() {
            return;
        }
        let Some(action) = self.error.take().and_then(|failure| failure.retry) else {
            return;
        };
        match action {
            RetryAction::Library(request) => self.load(request, cx),
            RetryAction::Playback(generation) => {
                if self.generation == generation {
                    self.play_current(cx);
                }
            }
            RetryAction::Like { song, liked } => self.set_like(song, liked, cx),
            RetryAction::RestoreConnection => self.restore_connection(cx),
            RetryAction::SaveConnection(connection) => self.save_connection(connection, cx),
            RetryAction::ForgetConnection => self.forget_connection(cx),
        }
        cx.notify();
    }

    fn play(&mut self, queue: Arc<Vec<Song>>, index: usize, cx: &mut Context<Self>) {
        if self.demo {
            self.error = Some(
                "This demo previews the interface. Connect to Navidrome to play music.".into(),
            );
            cx.notify();
            return;
        }
        if self.connection.client.is_none() || !self.queue.start(queue, index) {
            return;
        }
        self.play_current(cx);
    }

    fn play_current(&mut self, cx: &mut Context<Self>) {
        self.seek_slider.update(cx, |slider, cx| slider.cancel(cx));
        let (Some(client), Some(song)) = (&self.connection.client, self.queue.current()) else {
            return;
        };
        if self
            .error
            .as_ref()
            .is_some_and(|failure| matches!(failure.retry.as_ref(), Some(RetryAction::Playback(_))))
        {
            self.error = None;
        }
        self.generation += 1;
        self.player
            .play(self.generation, client.stream_url(&song.id));
        self.playback = Playback {
            generation: self.generation,
            status: PlaybackStatus::Loading,
            ..Playback::default()
        };
        self.follow_current_track();
        self.sync_media();
        cx.notify();
    }

    fn follow_current_track(&mut self) -> bool {
        if self.show_help || self.browser.is_loading() {
            return false;
        }
        let Some(song) = self.queue.current() else {
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
        if !self.demo && self.connection.client.is_none() {
            return;
        }
        if let Some(liked) = self.likes.begin(&song) {
            self.send_like(song, liked, cx);
        }
    }

    fn set_like(&mut self, song: Song, liked: bool, cx: &mut Context<Self>) {
        if !self.demo && self.connection.client.is_none() {
            return;
        }
        let Some(liked) = self.likes.begin_set(&song, liked) else {
            return;
        };
        self.send_like(song, liked, cx);
    }

    fn send_like(&mut self, song: Song, liked: bool, cx: &mut Context<Self>) {
        if self.demo {
            self.finish_like(&song, true);
            cx.notify();
            return;
        }
        let client = self.connection.client.clone().expect("connected client");
        let session_id = self.connection.session_id;
        let id = song.id.clone();
        let task = cx
            .background_executor()
            .spawn(async move { client.set_liked(&id, liked) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.connection.session_id != session_id {
                    return;
                }
                this.finish_like(&song, result.is_ok());
                if let Err(error) = result {
                    this.error = Some(Failure::new(
                        format!("Could not save the like: {error:#}"),
                        RetryAction::Like { song, liked },
                    ));
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn finish_like(&mut self, song: &Song, success: bool) {
        self.likes.finish(song, success);
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
            .when(liked, |view| view.bg(rgb(LIKED_BG)))
            .when(pending, |view| view.opacity(0.5))
            .when(!pending, |view| view.cursor_pointer())
            .hover(|s| s.bg(rgb(LIKED_BG)))
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
        if self.queue.advance() {
            self.play_current(cx);
        }
    }

    fn previous(&mut self, cx: &mut Context<Self>) {
        if self.playback.position.as_secs() > 3
            && !self.playback.is_finished()
            && self.playback.error().is_none()
        {
            self.player.seek(0);
        } else if self.queue.previous() {
            self.play_current(cx);
        } else if self.queue.current().is_some()
            && (self.playback.is_finished() || self.playback.error().is_some())
        {
            self.queue.restart();
            self.play_current(cx);
        }
    }

    fn tick(&mut self, cx: &mut Context<Self>) {
        if self.settings.poll() {
            cx.notify();
        }
        for command in self.media.drain() {
            self.media_command(command, cx);
        }
        let snapshot = self.player.snapshot();
        if snapshot.generation != self.generation {
            return;
        }
        if let Some(error) = snapshot.error()
            && self.playback.error() != Some(error)
        {
            self.error = Some(Failure::new(error, RetryAction::Playback(self.generation)));
        }
        if snapshot.warning != self.playback.warning
            && let Some((_, message)) = &snapshot.warning
        {
            self.error = Some(message.clone().into());
        }
        let finished_now = snapshot.is_finished() && !self.playback.is_finished();
        let changed = self.playback.position.as_secs() != snapshot.position.as_secs()
            || self.playback.status != snapshot.status
            || self.playback.warning != snapshot.warning;
        self.playback = snapshot;
        if finished_now {
            self.next(cx);
        }
        if changed {
            self.sync_media();
            cx.notify();
        }
    }

    fn sync_media(&mut self) {
        self.media.sync(self.queue.current(), &self.playback);
    }

    fn media_command(&mut self, command: crate::media::MediaCommand, cx: &mut Context<Self>) {
        use crate::media::MediaCommand;
        let Some(song) = self.queue.current() else {
            return;
        };
        let duration = song.duration;
        crate::diagnostics::event(&format!("System media command: {command:?}"));
        match command {
            MediaCommand::Play => {
                if self.playback.needs_restart() {
                    self.play_current(cx);
                } else {
                    self.player.set_paused(false);
                }
            }
            MediaCommand::Pause => {
                if !self.playback.needs_restart() {
                    self.player.set_paused(true);
                }
            }
            MediaCommand::Toggle => self.toggle(cx),
            MediaCommand::Next => self.next(cx),
            MediaCommand::Previous => self.previous(cx),
            MediaCommand::Stop => {
                self.generation += 1;
                self.player.stop();
                self.playback = Playback {
                    generation: self.generation,
                    ..Playback::default()
                };
                self.sync_media();
                cx.notify();
            }
            MediaCommand::SeekTo { seconds } => {
                if let Some(position) = crate::media::seek_position(seconds, duration) {
                    self.player.seek(position);
                }
            }
            MediaCommand::Skip { seconds } => {
                if let Some(position) = crate::media::seek_position(
                    self.playback.position.as_secs_f64() + seconds,
                    duration,
                ) {
                    self.player.seek(position);
                }
            }
        }
    }

    fn toggle(&mut self, cx: &mut Context<Self>) {
        if self.playback.needs_restart() {
            self.play_current(cx);
        } else if self.queue.current().is_some() {
            self.player.toggle();
        }
    }

    fn toggle_shuffle(&mut self, cx: &mut Context<Self>) {
        self.queue.toggle_shuffle();
        let shuffled = self.queue.is_shuffled();
        self.settings.update(|value| value.shuffle = shuffled);
        cx.notify();
    }

    fn adjust_volume(&mut self, delta: f32, cx: &mut Context<Self>) {
        self.set_volume(self.settings.value().volume + delta, cx);
    }

    fn set_volume(&mut self, volume: f32, cx: &mut Context<Self>) {
        self.settings
            .update(|value| value.volume = volume.clamp(0., 1.));
        self.player.volume(self.settings.value().volume);
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
                    self.error = None;
                }
                window.focus(&self.library_focus);
            }
            Toggle => {
                if self.queue.current().is_some() {
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
                    self.queue.current().cloned()
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
            Seek(delta) => {
                if let Some(song) = self.queue.current() {
                    let duration = if song.duration > 0 {
                        song.duration
                    } else {
                        u64::MAX
                    };
                    self.player.seek(
                        self.playback
                            .position
                            .as_secs()
                            .saturating_add_signed(delta)
                            .min(duration),
                    );
                }
            }
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
        self.remember_folder();
        self.reveal_cursor();
        cx.notify();
    }

    fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.settings
            .update(|value| value.sidebar_compact = !value.sidebar_compact);
        cx.notify();
    }

    fn shortcut_badge(&self, _id: &'static str, label: impl Into<SharedString>) -> gpui::Div {
        let label = label.into();
        #[cfg(feature = "ui-tests")]
        {
            self.ui_probe
                .borrow_mut()
                .shortcut_hints
                .insert(_id.into(), label.to_string());
        }
        div()
            .w(px(32.))
            .h(px(24.))
            .flex_shrink_0()
            .rounded_sm()
            .bg(rgb(SELECTED))
            .border_1()
            .border_color(rgb(BORDER))
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(12.))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_color(rgb(ACCENT))
            .child(label)
    }

    fn sidebar_icon_slot(&self, _id: &'static str, content: impl IntoElement) -> gpui::Div {
        // The same 40px column anchors every icon in both sidebar modes.
        let slot = div()
            .relative()
            .w(px(40.))
            .h_full()
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .child(content);
        #[cfg(feature = "ui-tests")]
        {
            let probe = self.ui_probe.clone();
            slot.on_children_prepainted(move |children, _, _| {
                if let Some(bounds) = children.first() {
                    probe.borrow_mut().sidebar_icons.insert(
                        _id.into(),
                        [
                            f32::from(bounds.left() + bounds.size.width / 2.),
                            f32::from(bounds.top() + bounds.size.height / 2.),
                        ],
                    );
                }
            })
        }
        #[cfg(not(feature = "ui-tests"))]
        {
            slot
        }
    }

    fn sidebar_toggle(&self, window: &Window, cx: &mut Context<Self>) -> gpui::Stateful<gpui::Div> {
        let compact = self.settings.value().sidebar_compact;
        let hints = window.is_window_active() && section_hints(window.modifiers());
        div()
            .id("toggle-sidebar")
            .size(px(32.))
            .flex_shrink_0()
            .rounded_md()
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .hover(|view| view.bg(rgb(HOVER)))
            .child(if hints {
                self.shortcut_badge("toggle-sidebar", "⌘B")
                    .into_any_element()
            } else {
                icon(
                    if compact {
                        "sidebar-expand"
                    } else {
                        "sidebar-collapse"
                    },
                    MUTED,
                )
                .into_any_element()
            })
            .tooltip(move |_, cx| {
                cx.new(|_| {
                    Hint::new(if compact {
                        "Expand sidebar · ⌘B"
                    } else {
                        "Collapse sidebar · ⌘B"
                    })
                })
                .into()
            })
            .on_click(cx.listener(|this, _, _, cx| this.toggle_sidebar(cx)))
    }

    fn sidebar_width(&self) -> f32 {
        if self.settings.value().sidebar_compact {
            SIDEBAR_COMPACT_WIDTH
        } else {
            SIDEBAR_EXPANDED_WIDTH
        }
    }

    fn sidebar(&self, window: &Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        let compact = self.settings.value().sidebar_compact;
        let width = self.sidebar_width();
        let item_width = width - 24.;
        #[cfg(feature = "ui-tests")]
        {
            self.ui_probe.borrow_mut().sidebar_width = width;
        }
        let selected = self.section_index();
        let hints = window.is_window_active() && section_hints(window.modifiers());
        let server_label = if self.demo {
            "Demo · no playback".to_owned()
        } else {
            self.connection
                .client
                .as_ref()
                .map(Navidrome::server_label)
                .unwrap_or_default()
        };
        let disconnect_label = if self.connection.saved {
            "Disconnect & forget"
        } else {
            "Disconnect"
        };
        div()
            .w(px(width))
            .flex_shrink_0()
            .h_full()
            .bg(rgb(PANEL))
            .border_r_1()
            .border_color(rgb(BORDER))
            .flex()
            .flex_col()
            .items_start()
            .pl(px(12.))
            .py_4()
            .gap_2()
            .child(
                div()
                    .w(px(item_width))
                    .h(px(32.))
                    .flex_shrink_0()
                    .mb_5()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        self.sidebar_icon_slot(
                            "brand",
                            div()
                                .size(px(32.))
                                .flex_shrink_0()
                                .rounded_md()
                                .bg(rgb(ACCENT))
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(23.))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(rgb(ON_ACCENT))
                                .child("s"),
                        ),
                    )
                    .when(!compact, |view| {
                        view.child(
                            div()
                                .text_size(px(23.))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .child("Still"),
                        )
                    }),
            )
            .children(
                [
                    ("artists", "Artists", "Artists · ⌘1", Page::Artists),
                    ("albums", "Albums", "Albums · ⌘2", Page::Albums),
                    ("songs", "Songs", "Songs · ⌘3", Page::Songs),
                    (
                        "folders",
                        "Folders",
                        "Folders · ⌘4",
                        Page::Folder(String::new()),
                    ),
                    ("heart", "Liked songs", "Liked songs · ⌘5", Page::Liked),
                ]
                .into_iter()
                .enumerate()
                .map(|(index, (glyph, label, hint, page))| {
                    div()
                        .id(index)
                        .w(px(item_width))
                        .h(px(40.))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .gap_2()
                        .rounded_md()
                        .cursor_pointer()
                        .text_color(rgb(if selected == index { ACCENT } else { MUTED }))
                        .when(selected == index, |view| view.bg(rgb(SELECTED)))
                        .hover(|view| view.bg(rgb(HOVER)))
                        .child(self.sidebar_icon_slot(
                            glyph,
                            if hints {
                                self.shortcut_badge(glyph, format!("⌘{}", index + 1))
                                    .into_any_element()
                            } else {
                                icon(glyph, if selected == index { ACCENT } else { MUTED })
                                    .into_any_element()
                            },
                        ))
                        .when(!compact, |view| {
                            view.child(div().flex_1().min_w_0().truncate().child(label))
                        })
                        .tooltip(move |_, cx| cx.new(|_| Hint::new(hint)).into())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.browser.clear_history();
                            this.open_section(page.section(), cx);
                        }))
                }),
            )
            .child(div().flex_1())
            .child(
                div()
                    .id("connection-status")
                    .w(px(item_width))
                    .h(px(24.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_color(rgb(MUTED))
                    .text_xs()
                    .child(self.sidebar_icon_slot(
                        "connection-status",
                        div().text_color(rgb(ACCENT)).child("●"),
                    ))
                    .when(!compact, |view| {
                        view.child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .child(server_label.clone()),
                        )
                    })
                    .tooltip(move |_, cx| cx.new(|_| Hint::new(server_label.clone())).into()),
            )
            .child(
                div()
                    .id("keyboard-help")
                    .w(px(item_width))
                    .h(px(40.))
                    .flex_shrink_0()
                    .rounded_md()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_color(rgb(MUTED))
                    .cursor_pointer()
                    .when(self.show_help, |view| view.bg(rgb(SELECTED)))
                    .hover(|view| view.bg(rgb(HOVER)))
                    .child(
                        self.sidebar_icon_slot(
                            "keyboard-help",
                            div().text_size(px(20.)).child("?"),
                        ),
                    )
                    .when(!compact, |view| {
                        view.child(div().text_sm().child("Keyboard shortcuts"))
                    })
                    .tooltip(|_, cx| cx.new(|_| Hint::new("Keyboard shortcuts · ?")).into())
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.show_help = !this.show_help;
                        window.focus(&this.library_focus);
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .id("disconnect")
                    .w(px(item_width))
                    .h(px(32.))
                    .flex_shrink_0()
                    .rounded_md()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_color(rgb(MUTED))
                    .cursor_pointer()
                    .hover(|view| view.bg(rgb(HOVER)))
                    .child(self.sidebar_icon_slot("disconnect", icon("logout", MUTED)))
                    .when(!compact, |view| {
                        view.child(div().text_sm().child(disconnect_label))
                    })
                    .tooltip(move |_, cx| cx.new(|_| Hint::new(disconnect_label)).into())
                    .on_click(cx.listener(|this, _, window, cx| this.disconnect(window, cx))),
            )
            .into_any_element()
    }

    fn header(&self, window: &Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        let mut subtitle = match &self.browser.state().page {
            Page::Artists => count_label(self.browser.state().artists.len(), "artist"),
            Page::Albums => format!(
                "{}{}",
                count_label(self.browser.state().albums.len(), "album"),
                if self.browser.state().more {
                    " · more available"
                } else {
                    ""
                }
            ),
            Page::Songs => format!(
                "{}{}",
                count_label(self.browser.state().songs.len(), "song"),
                if self.browser.state().more {
                    " · more available"
                } else {
                    ""
                }
            ),
            Page::Artist(_) => count_label(self.browser.state().albums.len(), "album"),
            Page::Liked => count_label(self.browser.state().songs.len(), "liked song"),
            Page::Album(album) => format!(
                "{}{} · {}",
                album.artist,
                album.year.map(|y| format!(" · {y}")).unwrap_or_default(),
                count_label(
                    album.song_count.max(self.browser.state().songs.len()),
                    "song"
                )
            ),
            Page::Folder(path) => self
                .browser
                .folder_library()
                .and_then(|library| library.folder(path))
                .map(|folder| {
                    format!(
                        "{} · {}",
                        count_label(folder.children.len(), "folder"),
                        count_label(folder.songs.len(), "song")
                    )
                })
                .unwrap_or_default(),
        };
        if !self.browser.state().query.is_empty() {
            let state = self.browser.state();
            let (total, noun) = match &state.page {
                Page::Artists => (state.artists.len(), "artists"),
                Page::Albums | Page::Artist(_) => (state.albums.len(), "albums"),
                Page::Folder(path) => (
                    self.browser
                        .folder_library()
                        .and_then(|library| library.folder(path))
                        .map_or(0, |folder| folder.children.len() + folder.songs.len()),
                    "items",
                ),
                _ => (state.songs.len(), "songs"),
            };
            subtitle = format!(
                "{} of {total} loaded {noun}{}",
                self.visible_entries().len(),
                if state.more { " · more available" } else { "" }
            );
        }
        #[cfg(feature = "ui-tests")]
        {
            self.ui_probe.borrow_mut().find_summary = subtitle.clone();
        }
        let title = match &self.browser.state().page {
            Page::Folder(path) if path.is_empty() => "Folders".to_owned(),
            Page::Folder(path) => {
                let parts: Vec<_> = path
                    .rsplit('/')
                    .filter(|part| !part.is_empty())
                    .take(2)
                    .collect();
                format!(
                    "Folders / {}",
                    parts.into_iter().rev().collect::<Vec<_>>().join(" / ")
                )
            }
            _ => self.title(),
        };
        div()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .px_4()
            .py_3()
            .gap_2()
            .border_b_1()
            .border_color(rgb(BORDER))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(self.sidebar_toggle(window, cx))
                    .when(self.browser.has_history(), |view| {
                        view.child(
                            toolbar_icon("back", "back", "Go back · h")
                                .on_click(cx.listener(|this, _, _, cx| this.back(cx))),
                        )
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .text_size(px(19.))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .truncate()
                                    .child(title),
                            )
                            .child(div().text_xs().text_color(rgb(MUTED)).truncate().child(
                                if self.browser.is_loading() {
                                    "Loading…".into()
                                } else {
                                    subtitle
                                },
                            )),
                    )
                    .when(
                        matches!(self.browser.state().page, Page::Folder(_))
                            && !self.browser.is_loading(),
                        |view| {
                            view.child(
                                toolbar_icon("refresh-folders", "refresh", "Refresh folders · r")
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.refresh_folders(cx)),
                                    ),
                            )
                        },
                    )
                    .when(
                        !self.browser.is_loading()
                            && match &self.browser.state().page {
                                Page::Folder(path) => self
                                    .browser
                                    .folder_library()
                                    .and_then(|library| library.folder(path))
                                    .is_some_and(|folder| {
                                        !folder.songs.is_empty() || !folder.children.is_empty()
                                    }),
                                Page::Album(_) | Page::Liked => {
                                    !self.browser.state().songs.is_empty()
                                }
                                _ => false,
                            },
                        |view| {
                            view.child(
                                toolbar_icon(
                                    "play-page",
                                    "play",
                                    match self.browser.state().page {
                                        Page::Folder(_) => {
                                            "Play folder and subfolders · Shift+Enter"
                                        }
                                        Page::Liked => "Play liked songs",
                                        _ => "Play album",
                                    },
                                )
                                .bg(rgb(SELECTED))
                                .on_click(cx.listener(
                                    |this, _, _, cx| {
                                        if let Page::Folder(path) = &this.browser.state().page {
                                            let path = path.clone();
                                            this.play_folder(&path, cx);
                                        } else {
                                            this.play(this.browser.state().songs.clone(), 0, cx);
                                        }
                                    },
                                )),
                            )
                        },
                    )
                    .child(
                        div()
                            .id("find-in-list")
                            .w(px(220.))
                            .flex_shrink_0()
                            .tooltip(|_, cx| {
                                cx.new(|_| Hint::new("Find in loaded items · / or ⌘F"))
                                    .into()
                            })
                            .child(self.filter.clone()),
                    ),
            )
            .into_any_element()
    }

    fn artists_list(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let entries = self.visible_entries();
        if entries.is_empty() {
            return empty_state("No artists yet", &self.browser.state().query);
        }
        uniform_list(
            ("artists", self.browser.request_id() as usize),
            entries.len(),
            cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                range
                    .map(|row| {
                        let BrowserEntry::Artist(index) = &entries[row] else {
                            unreachable!()
                        };
                        let artist = this.browser.state().artists[*index].clone();
                        let initial = artist
                            .name
                            .chars()
                            .next()
                            .unwrap_or('♪')
                            .to_uppercase()
                            .to_string();
                        div()
                            .id(row)
                            .w_full()
                            .h(px(ARTIST_ROW_HEIGHT))
                            .flex()
                            .items_center()
                            .gap_4()
                            .px_3()
                            .border_b_1()
                            .border_color(rgb(BORDER))
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(PANEL)))
                            .when(this.cursor_on(row), |view| {
                                view.bg(rgb(SELECTED))
                                    .border_1()
                                    .border_color(rgb(KEYBOARD_FOCUS))
                            })
                            .child(
                                div()
                                    .size(px(34.))
                                    .rounded_full()
                                    .bg(rgb(SELECTED))
                                    .text_color(rgb(ACCENT))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_lg()
                                    .child(initial),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .child(artist.name.clone()),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(MUTED))
                                    .child(count_label(artist.album_count, "album")),
                            )
                            .child(svg().path("forward").size(px(16.)).text_color(rgb(MUTED)))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.browser.set_cursor(row);
                                this.navigate(Page::Artist(artist.clone()), true, cx)
                            }))
                    })
                    .collect()
            }),
        )
        .track_scroll(self.list_scroll.clone())
        .w_full()
        .flex_1()
        .min_h_0()
        .into_any_element()
    }

    fn albums_list(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let entries = self.visible_entries();
        if entries.is_empty() {
            return empty_state("No albums yet", &self.browser.state().query);
        }
        uniform_list(
            ("albums", self.browser.request_id() as usize),
            entries.len(),
            cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                range
                    .map(|row| {
                        let BrowserEntry::Album(index) = &entries[row] else {
                            unreachable!()
                        };
                        let album = this.browser.state().albums[*index].clone();
                        div()
                            .id(row)
                            .w_full()
                            .h(px(ALBUM_ROW_HEIGHT))
                            .flex()
                            .items_center()
                            .gap_4()
                            .px_3()
                            .border_b_1()
                            .border_color(rgb(BORDER))
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(PANEL)))
                            .when(this.cursor_on(row), |view| {
                                view.bg(rgb(SELECTED))
                                    .border_1()
                                    .border_color(rgb(KEYBOARD_FOCUS))
                            })
                            .child(artwork(
                                this.connection.client.as_ref(),
                                album.cover_art.as_deref(),
                                44.,
                            ))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(div().truncate().child(album.name.clone()))
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(rgb(MUTED))
                                            .truncate()
                                            .child(album.artist.clone()),
                                    ),
                            )
                            .when_some(album.year, |view, year| {
                                view.child(
                                    div()
                                        .w(px(48.))
                                        .text_sm()
                                        .text_color(rgb(MUTED))
                                        .child(year.to_string()),
                                )
                            })
                            .child(
                                div()
                                    .w(px(86.))
                                    .text_sm()
                                    .text_color(rgb(MUTED))
                                    .child(count_label(album.song_count, "song")),
                            )
                            .child(icon("forward", MUTED))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.browser.set_cursor(row);
                                this.navigate(Page::Album(album.clone()), true, cx)
                            }))
                    })
                    .collect()
            }),
        )
        .track_scroll(self.list_scroll.clone())
        .w_full()
        .flex_1()
        .min_h_0()
        .into_any_element()
    }

    fn songs_list(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let entries = self.visible_entries();
        if entries.is_empty() {
            return empty_state(
                if matches!(self.browser.state().page, Page::Liked) {
                    "Click ♡ next to a song to add it here"
                } else {
                    "No songs yet"
                },
                &self.browser.state().query,
            );
        }
        let list = uniform_list(
            ("songs", self.browser.request_id() as usize),
            entries.len(),
            cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                range
                    .map(|row| {
                        let song = this.entry_song(&entries[row]).expect("song entry");
                        let active = this
                            .queue
                            .current()
                            .is_some_and(|current| current.id == song.id);
                        track_row(row, SONG_ROW_HEIGHT)
                            .when(this.cursor_on(row), |view| {
                                view.bg(rgb(SELECTED))
                                    .border_1()
                                    .border_color(rgb(KEYBOARD_FOCUS))
                            })
                            .child(
                                div()
                                    .w(px(24.))
                                    .text_sm()
                                    .text_color(rgb(if active { ACCENT } else { MUTED }))
                                    .child(if active {
                                        icon_sized(
                                            if this.playback.is_playing() {
                                                "play"
                                            } else {
                                                "pause"
                                            },
                                            ACCENT,
                                            14.,
                                        )
                                        .into_any_element()
                                    } else {
                                        div().child(format!("{:02}", row + 1)).into_any_element()
                                    }),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_color(rgb(if active { ACCENT } else { TEXT }))
                                            .font_weight(if active {
                                                gpui::FontWeight::SEMIBOLD
                                            } else {
                                                gpui::FontWeight::NORMAL
                                            })
                                            .truncate()
                                            .child(song.title.clone()),
                                    )
                                    .child(
                                        div().flex().items_center().gap_2().child(
                                            div()
                                                .flex_1()
                                                .min_w_0()
                                                .text_sm()
                                                .text_color(rgb(MUTED))
                                                .truncate()
                                                .child(song.artist.clone()),
                                        ),
                                    ),
                            )
                            .when(
                                matches!(this.browser.state().page, Page::Songs | Page::Liked),
                                |view| {
                                    view.child(
                                        div()
                                            .w(relative(0.3))
                                            .text_sm()
                                            .text_color(rgb(MUTED))
                                            .truncate()
                                            .child(song.album.clone()),
                                    )
                                },
                            )
                            .child(this.like_button(song, "song-like", cx))
                            .child(
                                div()
                                    .w(px(60.))
                                    .text_sm()
                                    .text_color(rgb(MUTED))
                                    .child(time_label(song.duration)),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.browser.set_cursor(row);
                                this.play_row(row, cx)
                            }))
                    })
                    .collect()
            }),
        )
        .track_scroll(self.list_scroll.clone())
        .w_full()
        .flex_1()
        .min_h_0();
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(
                div()
                    .flex()
                    .gap_4()
                    .pl(px(20.))
                    .pr_3()
                    .pb_3()
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .child(div().w(px(24.)).child("#"))
                    .child(div().flex_1().child("Title"))
                    .when(
                        matches!(self.browser.state().page, Page::Songs | Page::Liked),
                        |view| view.child(div().w(relative(0.3)).child("Album")),
                    )
                    .child(div().w(px(32.)))
                    .child(div().w(px(60.)).child("Duration")),
            )
            .child(list)
            .into_any_element()
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
        let connected = self.connection.client.is_some() || self.demo;
        let mut root = div()
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
            .when_some(self.error.clone(), |view, error| {
                view.child(
                    div()
                        .mb_4()
                        .p_3()
                        .rounded_md()
                        .bg(rgb(ERROR_BG))
                        .text_color(rgb(ERROR_TEXT))
                        .flex()
                        .items_center()
                        .gap_3()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .whitespace_normal()
                                .child(error.message),
                        )
                        .when(error.retry.is_some(), |view| {
                            view.child(
                                button("retry", "Retry")
                                    .on_click(cx.listener(|this, _, _, cx| this.retry(cx))),
                            )
                        }),
                )
            })
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
