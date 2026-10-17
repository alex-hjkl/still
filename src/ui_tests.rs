//! Native UI regression driver, compiled only with --features ui-tests.
//! Keystrokes use GPUI's dispatch path; assertions run after a rendered frame.
use super::*;
use crate::player::PlaybackStatus;
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
    pub columns: BTreeMap<String, String>,
    pub find_summary: String,
    pub preview_song: Option<String>,
    pub sidebar_width: f32,
    pub sidebar_icons: BTreeMap<String, [f32; 2]>,
    pub shortcut_hints: BTreeMap<String, String>,
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum Step {
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
    },
    Resize {
        size: [f32; 2],
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

pub(super) fn demo_overlay(hint: Option<&DemoHint>) -> Option<gpui::Div> {
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
                    .bg(rgb(TEXT))
                    .shadow_lg()
                    .child(
                        div()
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(SELECTED))
                            .text_color(rgb(ACCENT))
                            .text_size(px(20.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(hint.keys.clone()),
                    )
                    .child(
                        div()
                            .text_color(rgb(ON_ACCENT))
                            .text_size(px(16.))
                            .child(hint.label.clone()),
                    ),
            ),
    )
}

#[derive(Default, Deserialize, Serialize)]
struct Expected {
    connected: Option<bool>,
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
    current_track: Option<String>,
    focus: Option<String>,
    playback: Option<String>,
    media_title: Option<String>,
    media_state: Option<String>,
    position_min: Option<u64>,
    position_max: Option<u64>,
}

#[derive(Serialize)]
struct Snapshot {
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
            connected: view.connection.client.is_some() || view.demo,
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
        field!(connected)
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
            Step::Text { text } => {
                for character in text.chars() {
                    window.dispatch_keystroke(
                        gpui::Keystroke {
                            key: character.to_string(),
                            key_char: Some(character.to_string()),
                            ..Default::default()
                        },
                        cx,
                    );
                }
            }
            Step::Resize { size } => window.resize(gpui::size(px(size[0]), px(size[1]))),
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
            Step::Disconnect => self.view.update(cx, |view, cx| view.disconnect(window, cx)),
            Step::Expect { expected } => {
                let snapshot = Snapshot::read(self.view.read(cx), window, cx);
                if !snapshot.matches(expected) {
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
                        window.refresh();
                        window.draw(cx).clear();
                        let result = run.borrow_mut().step(window, cx);
                        match result {
                            Ok(keep_running) => keep_running,
                            Err(error) => {
                                run.borrow().finish(false, Some(format!("{error:#}")));
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
        })),
        window,
        cx,
    );
}
