use gpui::{Keystroke, Modifiers};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Command {
    Down,
    Up,
    Open,
    PlaySelection,
    Back,
    First,
    Last,
    HalfDown,
    HalfUp,
    PageDown,
    PageUp,
    FocusSearch,
    Escape,
    Toggle,
    Shuffle,
    Next,
    Previous,
    Like,
    LikeCurrent,
    Seek(i64),
    Volume(f32),
    Section(usize),
    Cycle(i32),
    Help,
    ToggleSidebar,
    Refresh,
    More,
    Prefix,
}

#[derive(Default)]
pub struct Keys {
    prefix: Option<Instant>,
}

pub fn section_hints(modifiers: Modifiers) -> bool {
    modifiers.platform && !modifiers.control && !modifiers.alt && !modifiers.shift
}

fn section_shortcut(stroke: &Keystroke) -> Option<usize> {
    section_hints(stroke.modifiers)
        .then(|| {
            ["1", "2", "3", "4", "5"]
                .iter()
                .position(|key| stroke.key == *key)
        })
        .flatten()
}

impl Keys {
    pub fn reset(&mut self) {
        self.prefix = None;
    }

    pub fn command_in_context(&mut self, stroke: &Keystroke, editing: bool) -> Option<Command> {
        if editing
            && stroke.key != "escape"
            && !(stroke.modifiers.platform && matches!(stroke.key.as_str(), "f" | "b"))
            && section_shortcut(stroke).is_none()
        {
            self.reset();
            return None;
        }
        self.command(stroke)
    }

    pub fn command(&mut self, stroke: &Keystroke) -> Option<Command> {
        use Command::*;
        let pending = self
            .prefix
            .take()
            .is_some_and(|time| time.elapsed() < Duration::from_secs(1));
        let key = stroke.key.as_str();
        let m = stroke.modifiers;
        if m.alt {
            return None;
        }
        if m.platform {
            if let Some(index) = section_shortcut(stroke) {
                return Some(Section(index));
            }
            return match (key, m.control, m.shift) {
                ("f", false, false) => Some(FocusSearch),
                ("b", false, false) => Some(ToggleSidebar),
                _ => None,
            };
        }
        if m.control {
            return match (key, m.shift) {
                ("d", false) => Some(HalfDown),
                ("u", false) => Some(HalfUp),
                ("f", false) => Some(PageDown),
                ("b", false) => Some(PageUp),
                _ => None,
            };
        }
        if key == "g" && !m.shift {
            if pending {
                return Some(First);
            }
            self.prefix = Some(Instant::now());
            return Some(Prefix);
        }
        Some(match (key, m.shift) {
            ("j", false) => Down,
            ("k", false) => Up,
            ("down", false) => Down,
            ("up", false) => Up,
            ("h", false) => Back,
            ("l", false) => Open,
            ("left", false) => Seek(-10),
            ("right", false) => Seek(10),
            ("enter", false) => Open,
            ("enter", true) => PlaySelection,
            ("backspace", false) => Back,
            ("home", false) => First,
            ("end", false) | ("g", true) | ("G", _) => Last,
            ("pagedown", false) => PageDown,
            ("pageup", false) => PageUp,
            ("/", false) => FocusSearch,
            ("escape", _) => Escape,
            ("space" | " ", false) => Toggle,
            ("s", false) => Shuffle,
            ("]", false) => Next,
            ("[", false) => Previous,
            ("f", false) => Like,
            ("f", true) | ("F", _) => LikeCurrent,
            ("h", true) | ("H", _) => Seek(-10),
            ("l", true) | ("L", _) => Seek(10),
            ("-", false) => Volume(-0.05),
            ("+", _) | ("=", _) => Volume(0.05),
            ("tab", false) => Cycle(1),
            ("tab", true) => Cycle(-1),
            ("?" | "f1", _) | ("/", true) => Help,
            ("r", false) => Refresh,
            ("e", false) => More,
            _ => return None,
        })
    }
}

pub fn moved_cursor(current: usize, delta: isize, count: usize) -> usize {
    current
        .saturating_add_signed(delta)
        .min(count.saturating_sub(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn key(value: &str) -> Keystroke {
        Keystroke::parse(value).unwrap()
    }

    #[test]
    fn vim_sequences_arrows_and_playback_have_distinct_commands() {
        let mut keys = Keys::default();
        for (stroke, expected) in [
            ("j", Command::Down),
            ("down", Command::Down),
            ("space", Command::Toggle),
            ("s", Command::Shuffle),
            ("enter", Command::Open),
            ("shift-enter", Command::PlaySelection),
            ("f", Command::Like),
            ("shift-f", Command::LikeCurrent),
            ("shift-h", Command::Seek(-10)),
            ("left", Command::Seek(-10)),
            ("right", Command::Seek(10)),
            ("h", Command::Back),
            ("l", Command::Open),
            ("ctrl-d", Command::HalfDown),
            ("cmd-f", Command::FocusSearch),
        ] {
            assert_eq!(keys.command(&key(stroke)), Some(expected));
        }
        assert_eq!(keys.command(&key("g")), Some(Command::Prefix));
        assert_eq!(keys.command(&key("g")), Some(Command::First));
        assert_eq!(keys.command(&key("shift-g")), Some(Command::Last));
        assert_eq!(keys.command(&key("cmd-c")), None);
        assert_eq!(keys.command(&key("alt-j")), None);
    }

    #[test]
    fn interrupted_and_expired_prefixes_do_not_trigger_a_jump() {
        let mut keys = Keys::default();
        keys.command(&key("g"));
        assert_eq!(keys.command(&key("j")), Some(Command::Down));
        assert_eq!(keys.command(&key("g")), Some(Command::Prefix));
        keys.prefix = Some(Instant::now() - Duration::from_secs(2));
        assert_eq!(keys.command(&key("g")), Some(Command::Prefix));
        keys.reset();
        assert_eq!(keys.command(&key("g")), Some(Command::Prefix));
    }

    #[test]
    fn typing_and_editing_shortcuts_never_trigger_player_commands() {
        let mut keys = Keys::default();
        for stroke in [
            "j",
            "k",
            "h",
            "l",
            "f",
            "s",
            "space",
            "3",
            "tab",
            "enter",
            "shift-enter",
            "ctrl-v",
            "cmd-a",
            "cmd-c",
            "shift-/",
            "left",
            "right",
            "shift-left",
            "shift-right",
        ] {
            assert_eq!(keys.command_in_context(&key(stroke), true), None);
        }
        assert_eq!(
            keys.command_in_context(&key("escape"), true),
            Some(Command::Escape)
        );
        assert_eq!(
            keys.command_in_context(&key("cmd-f"), true),
            Some(Command::FocusSearch)
        );
        assert_eq!(
            keys.command_in_context(&key("f"), false),
            Some(Command::Like)
        );
    }

    #[test]
    fn cursor_stays_in_bounds_in_empty_lists() {
        assert_eq!(moved_cursor(0, -10, 0), 0);
        assert_eq!(moved_cursor(3, 100, 5), 4);
        assert_eq!(moved_cursor(3, -100, 5), 0);
    }

    #[test]
    fn sidebar_shortcut_works_in_search_without_stealing_typing_or_page_up() {
        for editing in [false, true] {
            let mut keys = Keys::default();
            assert_eq!(
                keys.command_in_context(&key("cmd-b"), editing),
                Some(Command::ToggleSidebar)
            );
            for stroke in ["b", "cmd-shift-b", "cmd-alt-b", "cmd-ctrl-b"] {
                assert_eq!(keys.command_in_context(&key(stroke), editing), None);
            }
            assert_eq!(
                keys.command_in_context(&key("ctrl-b"), editing),
                (!editing).then_some(Command::PageUp)
            );
        }
    }

    #[test]
    fn command_numbers_switch_sections_even_while_editing_search() {
        for editing in [false, true] {
            let mut keys = Keys::default();
            for number in 1..=5 {
                assert_eq!(
                    keys.command_in_context(&key(&format!("cmd-{number}")), editing),
                    Some(Command::Section(number - 1))
                );
                assert_eq!(
                    keys.command_in_context(&key(&number.to_string()), editing),
                    None
                );
            }
            for stroke in ["cmd-0", "cmd-6", "cmd-shift-1", "cmd-alt-1", "cmd-ctrl-1"] {
                assert_eq!(keys.command_in_context(&key(stroke), editing), None);
            }
        }
    }

    #[test]
    fn hints_are_visible_only_when_the_advertised_shortcuts_are_available() {
        assert!(section_hints(key("cmd-1").modifiers));
        for stroke in ["1", "ctrl-1", "cmd-shift-1", "cmd-alt-1", "cmd-ctrl-1"] {
            assert!(!section_hints(key(stroke).modifiers));
        }
    }
}
