//! Descriptions used by keyboard help, contextual hints and button tooltips.
//! Bindings and their descriptions live together; editing and prefix rules stay in keyboard.rs.
use crate::{
    browser::{BrowserEntry, Page, PlayTarget},
    keyboard::Command::{self, *},
};
use gpui::Keystroke;

#[derive(Clone, Copy)]
pub struct Shortcut {
    pub keys: &'static str,
    pub footer: &'static str,
    pub label: &'static str,
    primary: &'static str,
    bindings: &'static [Binding],
}

impl Shortcut {
    const fn new(keys: &'static str, label: &'static str) -> Self {
        Self {
            keys,
            footer: keys,
            label,
            primary: keys,
            bindings: &[],
        }
    }

    const fn bindings(mut self, bindings: &'static [Binding]) -> Self {
        self.bindings = bindings;
        self
    }

    const fn primary(mut self, keys: &'static str) -> Self {
        self.primary = keys;
        self
    }

    const fn footer(mut self, keys: &'static str) -> Self {
        self.footer = keys;
        self
    }

    pub fn hint(self, label: &str) -> String {
        format!("{label} · {}", self.primary)
    }

    pub fn footer_hint(self, label: &'static str) -> (&'static str, &'static str) {
        (self.footer, label)
    }
}

#[derive(Clone, Copy)]
struct Binding {
    key: &'static str,
    command: Command,
    shift: Option<bool>,
    platform: bool,
    control: bool,
}

impl Binding {
    const fn key(key: &'static str, command: Command) -> Self {
        Self {
            key,
            command,
            shift: Some(false),
            platform: false,
            control: false,
        }
    }

    const fn shifted(mut self) -> Self {
        self.shift = Some(true);
        self
    }
    const fn any_shift(mut self) -> Self {
        self.shift = None;
        self
    }
    const fn platform(mut self) -> Self {
        self.platform = true;
        self
    }
    const fn control(mut self) -> Self {
        self.control = true;
        self
    }

    fn matches(self, stroke: &Keystroke) -> bool {
        let modifiers = stroke.modifiers;
        stroke.key == self.key
            && !modifiers.alt
            && modifiers.platform == self.platform
            && modifiers.control == self.control
            && self.shift.is_none_or(|shift| shift == modifiers.shift)
    }
}

pub const MOVE: Shortcut = Shortcut::new("j / k", "Next / previous item")
    .footer("j k")
    .bindings(&[
        Binding::key("j", Down),
        Binding::key("k", Up),
        Binding::key("down", Down),
        Binding::key("up", Up),
    ]);
pub const BROWSE: Shortcut = Shortcut::new("h / l", "Back / open or play")
    .bindings(&[Binding::key("h", Back), Binding::key("l", Open)]);
pub const OPEN: Shortcut =
    Shortcut::new("Enter", "Open or play selected item").bindings(&[Binding::key("enter", Open)]);
pub const PLAY_SELECTION: Shortcut =
    Shortcut::new("Shift+Enter", "Play selected folder, artist or album")
        .footer("⇧ Enter")
        .bindings(&[Binding::key("enter", PlaySelection).shifted()]);
pub const BACK: Shortcut = Shortcut::new("Backspace", "Go back")
    .primary("h")
    .footer("h")
    .bindings(&[Binding::key("backspace", Back)]);
pub const BOUNDS: Shortcut = Shortcut::new("gg / G", "First / last item").bindings(&[
    Binding::key("home", First),
    Binding::key("end", Last),
    Binding::key("g", Last).shifted(),
    Binding::key("G", Last).any_shift(),
]);
pub const HALF_PAGE: Shortcut = Shortcut::new("Ctrl+D / U", "Half a page down / up").bindings(&[
    Binding::key("d", HalfDown).control(),
    Binding::key("u", HalfUp).control(),
]);
pub const PAGE: Shortcut = Shortcut::new("Ctrl+F / B", "Page down / up").bindings(&[
    Binding::key("f", PageDown).control(),
    Binding::key("b", PageUp).control(),
    Binding::key("pagedown", PageDown),
    Binding::key("pageup", PageUp),
]);
pub const FIND: Shortcut = Shortcut::new("/ · ⌘F", "Search library")
    .footer("/")
    .bindings(&[
        Binding::key("/", FocusSearch),
        Binding::key("f", FocusSearch).platform(),
    ]);
pub const FINISH_FIND: Shortcut =
    Shortcut::new("Enter / Esc", "Return to results; Esc again goes back");
pub const ESCAPE: Shortcut =
    Shortcut::new("Esc", "Close").bindings(&[Binding::key("escape", Escape).any_shift()]);
pub const TOGGLE: Shortcut = Shortcut::new("Space", "Play / pause")
    .bindings(&[Binding::key("space", Toggle), Binding::key(" ", Toggle)]);
pub const SHUFFLE: Shortcut =
    Shortcut::new("s", "Toggle shuffle").bindings(&[Binding::key("s", Shuffle)]);
pub const PREVIOUS: Shortcut =
    Shortcut::new("[", "Previous song or restart").bindings(&[Binding::key("[", Previous)]);
pub const NEXT: Shortcut = Shortcut::new("]", "Next song").bindings(&[Binding::key("]", Next)]);
pub const SEEK: Shortcut = Shortcut::new("← / → · H / L", "Seek back / forward 10s").bindings(&[
    Binding::key("left", Seek(-10)),
    Binding::key("right", Seek(10)),
    Binding::key("h", Seek(-10)).shifted(),
    Binding::key("H", Seek(-10)).any_shift(),
    Binding::key("l", Seek(10)).shifted(),
    Binding::key("L", Seek(10)).any_shift(),
]);
pub const VOLUME: Shortcut = Shortcut::new("− / +", "Volume down / up 5%").bindings(&[
    Binding::key("-", Volume(-0.05)),
    Binding::key("+", Volume(0.05)).any_shift(),
    Binding::key("=", Volume(0.05)).any_shift(),
]);
pub const LIKE: Shortcut = Shortcut::new(
    "f / F",
    "Toggle like: current; f uses selection if queue empty",
)
.primary("f")
.footer("f")
.bindings(&[
    Binding::key("f", Like),
    Binding::key("f", LikeCurrent).shifted(),
    Binding::key("F", LikeCurrent).any_shift(),
]);
pub const CYCLE: Shortcut =
    Shortcut::new("Tab / Shift+Tab", "Next / previous section").bindings(&[
        Binding::key("tab", Cycle(1)),
        Binding::key("tab", Cycle(-1)).shifted(),
    ]);
pub const REFRESH: Shortcut = Shortcut::new("r", "Refresh").bindings(&[Binding::key("r", Refresh)]);
pub const MORE: Shortcut = Shortcut::new("e", "Load more").bindings(&[Binding::key("e", More)]);
pub const HELP: Shortcut = Shortcut::new("? · F1 · Esc", "Open / close keyboard help")
    .primary("?")
    .footer("?")
    .bindings(&[
        Binding::key("?", Help).any_shift(),
        Binding::key("f1", Help).any_shift(),
        Binding::key("/", Help).shifted(),
    ]);
pub const SIDEBAR: Shortcut = Shortcut::new("⌘B", "Expand / collapse sidebar")
    .bindings(&[Binding::key("b", ToggleSidebar).platform()]);
pub const QUIT: Shortcut = Shortcut::new("⌘Q", "Quit Still");
pub const QUIT_BINDING: &str = "cmd-q";

const NAVIGATION: &[Shortcut] = &[
    MOVE,
    BROWSE,
    OPEN,
    PLAY_SELECTION,
    BACK,
    BOUNDS,
    HALF_PAGE,
    PAGE,
    FIND,
    FINISH_FIND,
];
const PLAYBACK: &[Shortcut] = &[TOGGLE, SHUFFLE, PREVIOUS, NEXT, SEEK, VOLUME, LIKE];
const APP: &[Shortcut] = &[CYCLE, REFRESH, MORE, HELP, SIDEBAR, QUIT];

pub fn command(stroke: &Keystroke) -> Option<Command> {
    NAVIGATION
        .iter()
        .chain(PLAYBACK)
        .chain(APP)
        .chain(std::iter::once(&ESCAPE))
        .flat_map(|shortcut| shortcut.bindings)
        .find(|binding| binding.matches(stroke))
        .map(|binding| binding.command)
}

pub fn help_groups() -> [(&'static str, Vec<(String, &'static str)>); 2] {
    let rows = |shortcuts: &[Shortcut]| {
        shortcuts
            .iter()
            .map(|shortcut| (shortcut.keys.to_owned(), shortcut.label))
            .collect::<Vec<_>>()
    };
    let navigation = rows(NAVIGATION);
    let mut playback = rows(PLAYBACK);
    let sections = &crate::navigation::Section::ALL;
    playback.push((
        format!(
            "{}–{}",
            sections[0].shortcut,
            sections[sections.len() - 1].shortcut
        ),
        "Switch sections; hold ⌘ for hints",
    ));
    playback.extend(rows(APP));
    [
        ("Navigation & browsing", navigation),
        ("Playback & app", playback),
    ]
}

impl PlayTarget {
    fn selection_hint(&self) -> Option<(&'static str, &'static str)> {
        let label = match self {
            Self::Artist(_) => "Play artist",
            Self::Album(_) | Self::OpenAlbum(_) => "Play album",
            Self::OpenFolder(_) => "Play open folder",
            Self::SelectedFolder(_) => "Play selected folder",
            Self::LikedSongs(_) => "Play all",
            Self::Track(_) => return None,
        };
        Some(PLAY_SELECTION.footer_hint(label))
    }

    pub fn play_button_hint(&self) -> String {
        match self {
            Self::Artist(_) => "Play all by this artist".into(),
            Self::Album(_) | Self::OpenAlbum(_) => PLAY_SELECTION.hint("Play album"),
            Self::OpenFolder(_) => "Play open folder and subfolders".into(),
            Self::SelectedFolder(_) => "Play this folder and subfolders".into(),
            Self::LikedSongs(_) => PLAY_SELECTION.hint("Play liked songs"),
            Self::Track(_) => OPEN.hint("Play song"),
        }
    }
}

pub fn footer_hints(
    page: &Page,
    selected: Option<&BrowserEntry>,
    play_target: Option<&PlayTarget>,
    search_focused: bool,
    has_current: bool,
) -> Vec<(&'static str, &'static str)> {
    if matches!(page, Page::Search(_)) && search_focused {
        return vec![OPEN.footer_hint("To results")];
    }
    let mut hints = vec![MOVE.footer_hint("Move")];
    let play_hint = play_target.and_then(PlayTarget::selection_hint);
    match page {
        Page::Artists | Page::Albums | Page::Artist(_) => {
            hints.push(OPEN.footer_hint("Open"));
            hints.extend(play_hint);
        }
        Page::Folder(path) => {
            if let Some(entry) = selected {
                let label = if matches!(entry, BrowserEntry::Folder(_)) {
                    "Open folder"
                } else {
                    "Play song"
                };
                hints.push(OPEN.footer_hint(label));
                if !path.is_empty() {
                    hints.push(BACK.footer_hint("Parent"));
                }
                hints.extend(play_hint);
            }
        }
        Page::Album(_) | Page::Liked => {
            hints.push(OPEN.footer_hint("Play song"));
            hints.extend(play_hint);
            let label = if matches!(page, Page::Liked) {
                "Unlike"
            } else {
                "Like"
            };
            hints.push(LIKE.footer_hint(label));
        }
        Page::Songs => hints.extend([OPEN.footer_hint("Play"), LIKE.footer_hint("Like")]),
        Page::Search(_) => {
            if let Some(entry) = selected {
                hints.push(OPEN.footer_hint(match entry {
                    BrowserEntry::Song(_) => "Play song",
                    BrowserEntry::Artist(_) => "Open artist",
                    BrowserEntry::Album(_) => "Open album",
                    BrowserEntry::Folder(_) => unreachable!("folders are not search results"),
                }));
            }
            hints.push(ESCAPE.footer_hint("Back"));
        }
    }
    if has_current {
        if let Some(hint) = hints.iter_mut().find(|(key, _)| *key == LIKE.footer) {
            *hint = LIKE.footer_hint("Like current");
        } else {
            hints.push(LIKE.footer_hint("Like current"));
        }
    }
    hints.push(FIND.footer_hint("Search"));
    hints
}
