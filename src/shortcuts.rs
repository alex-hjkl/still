//! Descriptions used by keyboard help, contextual hints and button tooltips.
//! Bindings and their descriptions live together; editing and prefix rules stay in keyboard.rs.
use crate::keyboard::Command::{self, *};
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
pub const LIKE: Shortcut = Shortcut::new("f / F", "Toggle like: selected / playing")
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

pub fn footer_hints(page: &crate::browser::Page) -> Vec<(&'static str, &'static str)> {
    use crate::browser::Page;
    let mut hints = vec![MOVE.footer_hint("Move")];
    match page {
        Page::Artists => hints.extend([
            OPEN.footer_hint("Open"),
            PLAY_SELECTION.footer_hint("Play artist"),
        ]),
        Page::Albums | Page::Artist(_) => hints.extend([
            OPEN.footer_hint("Open"),
            PLAY_SELECTION.footer_hint("Play album"),
        ]),
        Page::Folder(_) => hints.extend([
            OPEN.footer_hint("Open / play"),
            BACK.footer_hint("Parent"),
            PLAY_SELECTION.footer_hint("Play folder"),
        ]),
        Page::Album(_) => hints.extend([
            OPEN.footer_hint("Play song"),
            PLAY_SELECTION.footer_hint("Play album"),
            LIKE.footer_hint("Like"),
        ]),
        Page::Liked => hints.extend([
            OPEN.footer_hint("Play song"),
            PLAY_SELECTION.footer_hint("Play all"),
            LIKE.footer_hint("Unlike"),
        ]),
        Page::Songs => hints.extend([OPEN.footer_hint("Play"), LIKE.footer_hint("Like")]),
        Page::Search(_) => {
            hints.extend([OPEN.footer_hint("Open / play"), ESCAPE.footer_hint("Back")])
        }
    }
    hints.push(FIND.footer_hint("Search"));
    hints
}
