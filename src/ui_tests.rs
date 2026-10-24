//! Native UI regression driver, compiled only with --features ui-tests.
//! Keystrokes use GPUI's dispatch path; assertions run after a rendered frame.
use super::*;
use crate::{player::PlaybackStatus, theme::Theme};
use anyhow::{Context as _, Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    collections::{BTreeMap, VecDeque},
    fs,
    path::PathBuf,
    rc::Rc,
    time::Instant,
};

#[derive(Default, Clone, Serialize)]
pub(super) struct RenderProbe {
    pub theme_bg: u32,
    pub header_stacked: bool,
    pub theme_menu_fits: Option<bool>,
    pub connection_fits: Option<bool>,
    pub columns: BTreeMap<String, String>,
    pub failures: Vec<String>,
    pub find_summary: String,
    pub preview_song: Option<String>,
    pub sidebar_width: f32,
    pub sidebar_icons: BTreeMap<String, [f32; 2]>,
    pub shortcut_hints: BTreeMap<String, String>,
    pub footer_hints: Vec<String>,
    pub footer_fits: bool,
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum Step {
    ToggleTheme,
    SetTheme {
        preference: ThemePreference,
    },
    SystemAppearance {
        dark: bool,
    },
    ThemeMenu,

    Wait {
        milliseconds: u64,
    },
    DemoHint {
        keys: String,
        label: String,
    },
    Recording {
        start: bool,
    },
    Key {
        key: String,
    },
    KeyBurst {
        keys: Vec<String>,
    },
    Text {
        text: String,
        field: Option<String>,
    },
    Resize {
        size: [f32; 2],
    },
    ScrollConnection {
        delta: f32,
    },
    Media {
        #[serde(flatten)]
        command: crate::media::MediaCommand,
    },
    MediaBurst {
        commands: Vec<crate::media::MediaCommand>,
    },
    Disconnect,
    StopAudioWorker,
    FailConnectionSave,
    CredentialStoreAvailable,
    Retry {
        operation: String,
    },
    Expect {
        #[serde(flatten)]
        expected: Box<Expected>,
    },
    Quit,
}

pub(super) struct DemoHint {
    keys: SharedString,
    label: SharedString,
    shown: Instant,
}

pub(super) fn demo_overlay(colors: Palette, hint: Option<&DemoHint>) -> Option<gpui::Div> {
    let hint = hint.filter(|hint| hint.shown.elapsed() < Duration::from_millis(4000))?;
    Some(
        div()
            .absolute()
            .bottom(px(124.))
            .left_0()
            .right_0()
            .flex()
            .justify_center()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_4()
                    .py_3()
                    .rounded_lg()
                    .bg(rgb(colors.tooltip_bg))
                    .shadow_lg()
                    .child(
                        div()
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(colors.selected))
                            .text_color(rgb(colors.accent))
                            .text_size(px(20.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(hint.keys.clone()),
                    )
                    .child(
                        div()
                            .text_color(rgb(colors.tooltip_text))
                            .text_size(px(16.))
                            .child(hint.label.clone()),
                    ),
            ),
    )
}

#[derive(Default, Deserialize, Serialize)]
struct Expected {
    theme: Option<Theme>,
    theme_preference: Option<ThemePreference>,
    system_theme_matches: Option<bool>,
    header_stacked: Option<bool>,
    theme_menu: Option<bool>,
    theme_menu_fits: Option<bool>,
    input_values: Option<BTreeMap<String, String>>,
    server_unchanged: Option<bool>,
    theme_bg: Option<u32>,
    connection_fits: Option<bool>,
    connection_scroll_y: Option<f32>,
    connection_scrolled_to_bottom: Option<bool>,
    connected: Option<bool>,
    failures: Option<Vec<String>>,
    loaded: Option<bool>,
    show_help: Option<bool>,
    find_summary: Option<String>,
    volume: Option<f32>,
    shuffle: Option<bool>,
    page: Option<String>,
    path: Option<String>,
    selected: Option<String>,
    cursor: Option<usize>,
    filter: Option<String>,
    columns: Option<BTreeMap<String, String>>,
    preview_song: Option<String>,
    size: Option<[f32; 2]>,
    visible: Option<bool>,
    sidebar_width: Option<f32>,
    sidebar_icons: Option<BTreeMap<String, [f32; 2]>>,
    shortcut_hints: Option<BTreeMap<String, String>>,
    footer_hints: Option<Vec<String>>,
    footer_fits: Option<bool>,
    current_track: Option<String>,
    queue_len: Option<usize>,
    queue_last: Option<String>,
    focus: Option<String>,
    playback: Option<String>,
    media_title: Option<String>,
    media_state: Option<String>,
    position_min: Option<u64>,
    position_max: Option<u64>,
    // Audio position only, including platforms without native Now Playing.
    audio_position_min: Option<u64>,
    audio_position_max: Option<u64>,
}

#[derive(Serialize)]
struct Snapshot {
    theme: Theme,
    theme_preference: ThemePreference,
    system_theme_matches: bool,
    theme_menu: bool,
    server_unchanged: bool,
    connection_scroll_max: f32,
    connection_scroll_y: f32,
    connected: bool,
    loaded: bool,
    show_help: bool,
    volume: f32,
    shuffle: bool,
    page: String,
    path: String,
    selected: String,
    cursor: usize,
    filter: String,
    rendered: RenderProbe,
    size: [f32; 2],
    visible: bool,
    sidebar_width: f32,
    current_track: String,
    queue_len: usize,
    queue_last: String,
    focus: String,
    playback: String,
    media_title: String,
    media_state: String,
    media_position: u64,
    position: u64,
}

impl Snapshot {
    fn read(view: &PlayerView, window: &Window, cx: &gpui::App) -> Self {
        let state = view.browser.state();
        let page = match state.page {
            Page::Artists => "artists",
            Page::Albums => "albums",
            Page::Songs => "songs",
            Page::Liked => "liked",
            Page::Artist(_) => "artist",
            Page::Album(_) => "album",
            Page::Folder(_) => "folders",
        };
        let selected = view
            .browser
            .entries()
            .get(state.cursor)
            .map(|entry| match entry {
                BrowserEntry::Folder(path) => path.clone(),
                BrowserEntry::Song(_) => view
                    .browser
                    .entry_song(entry)
                    .map(|song| song.id.clone())
                    .unwrap_or_default(),
                BrowserEntry::Artist(index) => state.artists[*index].id.clone(),
                BrowserEntry::Album(index) => state.albums[*index].id.clone(),
            })
            .unwrap_or_default();
        let scroll = view.list_scroll.0.borrow();
        let height = f32::from(scroll.base_handle.bounds().size.height);
        let row = scroll.last_item_size.map_or(0., |size| {
            f32::from(size.contents.height) / view.browser.entries().len().max(1) as f32
        });
        let top = state.cursor as f32 * row + f32::from(scroll.base_handle.offset().y);
        let (media_title, media_position, media_state) = view.transport.test_media_snapshot();
        Self {
            theme: *cx.global::<Theme>(),
            theme_preference: view.settings.value().theme,
            system_theme_matches: *cx.global::<Theme>()
                == ThemePreference::System.resolve(cx.window_appearance()),
            theme_menu: view.theme_menu.is_some(),
            server_unchanged: std::env::var("NAVIDROME_URL")
                .is_ok_and(|url| view.server.read(cx).value() == url),
            connection_scroll_max: f32::from(view.connection_scroll.max_offset().height),
            connection_scroll_y: f32::from(view.connection_scroll.offset().y),
            playback: match view.transport.state().status {
                PlaybackStatus::Idle => "idle",
                PlaybackStatus::Loading => "loading",
                PlaybackStatus::Playing => "playing",
                PlaybackStatus::Paused => "paused",
                PlaybackStatus::Finished => "finished",
                PlaybackStatus::Failed(_) => "failed",
            }
            .into(),
            position: view.transport.state().position.as_secs(),
            media_title,
            media_state,
            media_position,
            connected: view.connection.read(cx).client.is_some() || view.demo,
            loaded: state.loaded && !view.browser.is_loading(),
            show_help: view.show_help,
            volume: view.settings.value().volume,
            shuffle: view.transport.queue().is_shuffled(),
            page: page.into(),
            path: if let Page::Folder(path) = &state.page {
                path.clone()
            } else {
                String::new()
            },
            selected,
            cursor: state.cursor,
            filter: state.filter_text.clone(),
            rendered: view.ui_probe.borrow().clone(),
            sidebar_width: view.ui_probe.borrow().sidebar_width,
            size: [
                f32::from(window.viewport_size().width),
                f32::from(window.viewport_size().height),
            ],
            visible: row > 0. && top >= -1. && top + row <= height + 1.,
            current_track: view
                .transport
                .queue()
                .current()
                .map(|song| song.id.clone())
                .unwrap_or_default(),
            queue_len: view.transport.queue().test_songs().len(),
            queue_last: view
                .transport
                .queue()
                .test_songs()
                .last()
                .map(|song| song.id.clone())
                .unwrap_or_default(),
            focus: [
                (&view.server, "server"),
                (&view.username, "username"),
                (&view.password, "password"),
                (&view.filter, "filter"),
            ]
            .into_iter()
            .find_map(|(input, name)| {
                input
                    .focus_handle(cx)
                    .is_focused(window)
                    .then(|| name.to_owned())
            })
            .unwrap_or_default(),
        }
    }

    fn matches(&self, expected: &Expected) -> bool {
        macro_rules! field {
            ($field:ident) => {
                expected
                    .$field
                    .as_ref()
                    .is_none_or(|value| value == &self.$field)
            };
        }
        field!(theme)
            && field!(theme_preference)
            && field!(system_theme_matches)
            && field!(theme_menu)
            && expected
                .theme_menu_fits
                .is_none_or(|fits| self.rendered.theme_menu_fits == Some(fits))
            && field!(server_unchanged)
            && expected
                .header_stacked
                .is_none_or(|value| self.rendered.header_stacked == value)
            && expected
                .theme_bg
                .is_none_or(|bg| self.rendered.theme_bg == bg)
            && field!(connected)
            && expected
                .connection_fits
                .is_none_or(|fits| self.rendered.connection_fits == Some(fits))
            && expected
                .connection_scroll_y
                .is_none_or(|y| (self.connection_scroll_y - y).abs() < 0.1)
            && expected.connection_scrolled_to_bottom.is_none_or(|bottom| {
                bottom
                    == (self.connection_scroll_max > 0.
                        && (self.connection_scroll_y + self.connection_scroll_max).abs() < 0.1)
            })
            && expected
                .failures
                .as_ref()
                .is_none_or(|failures| &self.rendered.failures == failures)
            && field!(loaded)
            && field!(show_help)
            && expected
                .find_summary
                .as_ref()
                .is_none_or(|summary| summary == &self.rendered.find_summary)
            && field!(shuffle)
            && expected
                .volume
                .is_none_or(|volume| (self.volume - volume).abs() < 0.001)
            && field!(page)
            && field!(path)
            && field!(selected)
            && field!(cursor)
            && field!(filter)
            && field!(visible)
            && field!(sidebar_width)
            && expected.sidebar_icons.as_ref().is_none_or(|icons| {
                icons.iter().all(|(name, center)| {
                    self.rendered.sidebar_icons.get(name).is_some_and(|actual| {
                        (actual[0] - center[0]).abs() < 0.1 && (actual[1] - center[1]).abs() < 0.1
                    })
                })
            })
            && expected
                .shortcut_hints
                .as_ref()
                .is_none_or(|hints| hints == &self.rendered.shortcut_hints)
            && field!(current_track)
            && field!(queue_len)
            && field!(queue_last)
            && expected
                .footer_hints
                .as_ref()
                .is_none_or(|hints| hints == &self.rendered.footer_hints)
            && expected
                .footer_fits
                .is_none_or(|fits| fits == self.rendered.footer_fits)
            && field!(focus)
            && field!(playback)
            && field!(media_title)
            && field!(media_state)
            && expected
                .position_min
                .is_none_or(|min| self.position >= min && self.media_position >= min)
            && expected
                .position_max
                .is_none_or(|max| self.position <= max && self.media_position <= max)
            && expected
                .audio_position_min
                .is_none_or(|min| self.position >= min)
            && expected
                .audio_position_max
                .is_none_or(|max| self.position <= max)
            && expected
                .columns
                .as_ref()
                .is_none_or(|columns| columns == &self.rendered.columns)
            && expected
                .preview_song
                .as_ref()
                .is_none_or(|song| self.rendered.preview_song.as_ref() == Some(song))
            && expected.size.is_none_or(|size| {
                (size[0] - self.size[0]).abs() < 1. && (size[1] - self.size[1]).abs() < 1.
            })
    }
}

struct Run {
    view: Entity<PlayerView>,
    steps: VecDeque<Step>,
    result: PathBuf,
    passed: usize,
    snapshots: Vec<Snapshot>,
    waiting_since: Instant,
    painted_focus: Option<FocusHandle>,
    stable_focus_frames: usize,
}

impl Run {
    fn finish(&self, success: bool, error: Option<String>) {
        let result = serde_json::json!({"success": success, "steps": self.passed, "snapshots": self.snapshots, "error": error});
        if let Err(error) = fs::write(
            &self.result,
            serde_json::to_vec_pretty(&result).expect("serialize UI results"),
        ) {
            crate::diagnostics::event(&format!("Could not write UI test results: {error}"));
        }
    }

    fn step(&mut self, window: &mut Window, cx: &mut gpui::App) -> Result<bool> {
        let step = self.steps.front().context("UI script must end with quit")?;
        match step {
            Step::SetTheme { preference } => {
                self.view
                    .update(cx, |view, cx| view.set_theme(*preference, cx));
            }
            Step::SystemAppearance { dark } => {
                let appearance = if *dark {
                    gpui::WindowAppearance::Dark
                } else {
                    gpui::WindowAppearance::Light
                };
                self.view
                    .read(cx)
                    .settings
                    .value()
                    .theme
                    .follow_appearance(appearance, cx);
            }
            Step::ThemeMenu => {
                self.view.update(cx, |view, cx| {
                    view.open_theme_menu(window, cx);
                });
            }
            Step::ToggleTheme => {
                self.view.update(cx, |view, cx| view.toggle_theme(cx));
            }
            Step::Wait { milliseconds } => {
                if self.waiting_since.elapsed() < Duration::from_millis(*milliseconds) {
                    return Ok(true);
                }
            }
            Step::DemoHint { keys, label } => {
                self.view.update(cx, |view, cx| {
                    view.demo_hint = Some(DemoHint {
                        keys: keys.clone().into(),
                        label: label.clone().into(),
                        shown: Instant::now(),
                    });
                    cx.notify();
                });
            }
            Step::Recording { start } => {
                let directory = PathBuf::from(
                    std::env::var_os("STILL_DEMO_CAPTURE")
                        .context("Recording requires STILL_DEMO_CAPTURE")?,
                );
                let (signal, acknowledgement) = if *start {
                    ("ready", "started")
                } else {
                    ("done", "finished")
                };
                fs::write(directory.join(signal), b"")?;
                if !directory.join(acknowledgement).exists() {
                    if self.waiting_since.elapsed() > Duration::from_secs(60) {
                        bail!("Recorder did not acknowledge {signal}");
                    }
                    return Ok(true);
                }
            }
            Step::Key { key } => {
                window.dispatch_keystroke(gpui::Keystroke::parse(key)?, cx);
            }
            Step::KeyBurst { keys } => {
                for key in keys {
                    window.dispatch_keystroke(gpui::Keystroke::parse(key)?, cx);
                }
            }
            Step::Text { text, field } => {
                let view = self.view.read(cx);
                let inputs = [
                    (&view.server, "server"),
                    (&view.username, "username"),
                    (&view.password, "password"),
                    (&view.filter, "filter"),
                ];
                let focused = inputs
                    .iter()
                    .find(|(input, _)| input.focus_handle(cx).is_focused(window));
                // Logical focus can change before the platform IME handler is
                // installed by paint. Require two completed, stable focus frames.
                let ready = self.stable_focus_frames >= 2
                    && focused.is_some_and(|(_, name)| {
                        field.as_deref().is_none_or(|expected| expected == *name)
                    });
                if !ready {
                    if self.waiting_since.elapsed() > Duration::from_secs(3) {
                        bail!(
                            "Text input is not ready: expected field {:?}, focused {:?}, stable painted frames {}",
                            field,
                            focused.map(|(_, name)| name),
                            self.stable_focus_frames
                        );
                    }
                    return Ok(true);
                }
                let target = focused.expect("ready focused input").1;
                let before: Vec<_> = inputs
                    .iter()
                    .map(|(input, name)| ((*input).clone(), *name, input.read(cx).value()))
                    .collect();
                for character in text.chars() {
                    if !window.dispatch_keystroke(
                        gpui::Keystroke {
                            key: character.to_string(),
                            key_char: Some(character.to_string()),
                            ..Default::default()
                        },
                        cx,
                    ) {
                        bail!("Text dispatch was not accepted by field {target}");
                    }
                }
                for (input, name, value) in before {
                    if name != target && input.read(cx).value() != value {
                        bail!("Text was routed to {name} while {target} had focus");
                    }
                }
            }
            Step::Resize { size } => window.resize(gpui::size(px(size[0]), px(size[1]))),
            Step::ScrollConnection { delta } => self.view.update(cx, |view, cx| {
                let mut offset = view.connection_scroll.offset();
                offset.y += px(*delta);
                view.connection_scroll.set_offset(offset);
                cx.notify();
            }),
            Step::Media { command } => {
                if !self.view.read(cx).transport.test_media_command(command) {
                    bail!("Native media command was not accepted: {command:?}");
                }
            }
            Step::MediaBurst { commands } => {
                for command in commands {
                    if !self.view.read(cx).transport.test_media_command(command) {
                        bail!("Native media command was not accepted: {command:?}");
                    }
                }
            }
            Step::StopAudioWorker => self.view.read(cx).transport.test_stop_worker(),
            Step::FailConnectionSave => self
                .view
                .read(cx)
                .connection
                .clone()
                .update(cx, |connection, cx| connection.test_save_failure(cx)),
            Step::CredentialStoreAvailable => self
                .view
                .read(cx)
                .connection
                .clone()
                .update(cx, |connection, _| connection.test_store_available()),
            Step::Retry { operation } => {
                let target = match operation.as_str() {
                    "connection" => FailureTarget::Connection,
                    "library" => FailureTarget::Operation(Operation::Library),
                    "collection" => FailureTarget::Operation(Operation::Collection),
                    "playback" => FailureTarget::Operation(Operation::Playback),
                    _ => bail!("Unknown retry operation"),
                };
                self.view.update(cx, |view, cx| view.retry(target, cx));
            }
            Step::Disconnect => self.view.update(cx, |view, cx| view.disconnect(window, cx)),
            Step::Expect { expected } => {
                let snapshot = Snapshot::read(self.view.read(cx), window, cx);
                let inputs_match = expected.input_values.as_ref().is_none_or(|values| {
                    let view = self.view.read(cx);
                    values.iter().all(|(name, value)| {
                        let input = match name.as_str() {
                            "server" => &view.server,
                            "username" => &view.username,
                            "password" => &view.password,
                            "filter" => &view.filter,
                            _ => return false,
                        };
                        input.read(cx).value() == *value
                    })
                });
                if !snapshot.matches(expected) || !inputs_match {
                    if self.waiting_since.elapsed() > Duration::from_secs(12) {
                        bail!(
                            "Step {}: expected {}; observed {}",
                            self.passed + 1,
                            serde_json::to_string(expected)?,
                            serde_json::to_string(&snapshot)?
                        );
                    }
                    return Ok(true);
                }
                self.snapshots.push(snapshot);
            }
            Step::Quit => {
                self.passed += 1;
                self.finish(true, None);
                window.dispatch_keystroke(gpui::Keystroke::parse("cmd-q")?, cx);
                return Ok(false);
            }
        }
        self.steps.pop_front();
        self.passed += 1;
        self.waiting_since = Instant::now();
        Ok(true)
    }
}

fn schedule(run: Rc<RefCell<Run>>, window: &mut Window, cx: &mut gpui::App) {
    // Drive native GPUI layout and paint even when macOS stops display callbacks
    // for a covered window. Assertions must not depend on window visibility.
    window
        .spawn(cx, async move |cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(50))
                    .await;
                let keep_running = cx
                    .update(|window, cx| {
                        let focus_before = window.focused(cx);
                        window.refresh();
                        window.draw(cx).clear();
                        let focus_after = window.focused(cx);
                        let mut run = run.borrow_mut();
                        if focus_before == focus_after && run.painted_focus == focus_after {
                            run.stable_focus_frames += 1;
                        } else {
                            run.stable_focus_frames = 0;
                        }
                        run.painted_focus = focus_after;
                        let result = run.step(window, cx);
                        match result {
                            Ok(keep_running) => keep_running,
                            Err(error) => {
                                run.finish(false, Some(format!("{error:#}")));
                                cx.quit();
                                false
                            }
                        }
                    })
                    .unwrap_or(false);
                if !keep_running {
                    break;
                }
            }
        })
        .detach();
}

pub(crate) fn start_ui_test(view: Entity<PlayerView>, window: &mut Window, cx: &mut gpui::App) {
    let Some(script) = std::env::var_os("STILL_UI_TEST_SCRIPT") else {
        return;
    };
    assert!(
        std::env::var_os("STILL_CONFIG_DIR").is_some(),
        "UI tests require isolated preferences"
    );
    let result = std::env::var_os("STILL_UI_TEST_RESULT").expect("UI test result path");
    let steps = serde_json::from_slice(&fs::read(script).expect("read UI script"))
        .expect("decode UI script");
    schedule(
        Rc::new(RefCell::new(Run {
            view,
            steps,
            result: result.into(),
            passed: 0,
            snapshots: Vec::new(),
            waiting_since: Instant::now(),
            painted_focus: None,
            stable_focus_frames: 0,
        })),
        window,
        cx,
    );
}
