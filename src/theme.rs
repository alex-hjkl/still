use gpui::{App, Global, WindowAppearance};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[default]
    Latte,
    Macchiato,
}

impl Global for Theme {}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ThemePreference {
    #[default]
    System,
    Latte,
    Macchiato,
}

impl ThemePreference {
    pub const ALL: [Self; 3] = [Self::System, Self::Latte, Self::Macchiato];

    pub fn resolve(self, appearance: WindowAppearance) -> Theme {
        match self {
            Self::Latte => Theme::Latte,
            Self::Macchiato => Theme::Macchiato,
            Self::System => match appearance {
                WindowAppearance::Dark | WindowAppearance::VibrantDark => Theme::Macchiato,
                WindowAppearance::Light | WindowAppearance::VibrantLight => Theme::Latte,
            },
        }
    }

    pub fn apply(self, cx: &mut App) {
        #[cfg(target_os = "macos")]
        {
            use objc2::MainThreadMarker;
            use objc2_app_kit::{
                NSAppearance, NSAppearanceNameAqua, NSAppearanceNameDarkAqua, NSApplication,
            };
            let main_thread =
                MainThreadMarker::new().expect("theme changes require the main thread");
            // Clearing the app override restores native OS appearance tracking.
            // SAFETY: AppKit's exported appearance names are immutable strings.
            let name = unsafe {
                match self {
                    Self::System => None,
                    Self::Latte => Some(NSAppearanceNameAqua),
                    Self::Macchiato => Some(NSAppearanceNameDarkAqua),
                }
            };
            let appearance = name.and_then(NSAppearance::appearanceNamed);
            NSApplication::sharedApplication(main_thread).setAppearance(appearance.as_deref());
        }
        let theme = self.resolve(cx.window_appearance());
        cx.set_global(theme);
        cx.refresh_windows();
    }

    pub fn follow_appearance(self, appearance: WindowAppearance, cx: &mut App) {
        if self == Self::System {
            let theme = self.resolve(appearance);
            if cx.global::<Theme>() != &theme {
                cx.set_global(theme);
                cx.refresh_windows();
            }
        }
    }
}

#[derive(Clone, Copy)]
pub struct Palette {
    pub bg: u32,
    pub panel: u32,
    pub border: u32,
    pub row_border: u32,
    pub text: u32,
    pub muted: u32,
    pub accent: u32,
    pub on_accent: u32,
    pub accent_hover: u32,
    pub selected: u32,
    pub keyboard_focus: u32,
    pub liked: u32,
    pub hover: u32,
    pub artwork_bg: u32,
    pub artwork_icon: u32,
    pub error_bg: u32,
    pub error_text: u32,
    pub input_bg: u32,
    pub input_border: u32,
    pub placeholder: u32,
    pub selection: u32,
    pub tooltip_bg: u32,
    pub tooltip_text: u32,
    pub backdrop: u32,
}

impl Theme {
    pub fn palette(self) -> Palette {
        // Catppuccin's official palette: https://github.com/catppuccin/palette
        // Blue is the shared accent; translucent selections and errors use it
        // and Red over each flavor's Base.
        match self {
            Self::Latte => Palette {
                bg: 0xeff1f5,
                panel: 0xe6e9ef,
                border: 0xbcc0cc,
                row_border: 0xe0e3eb,
                text: 0x4c4f69,
                muted: 0x5c5f77,
                accent: 0x1e66f5,
                on_accent: 0xffffff,
                accent_hover: 0x1a59d5,
                selected: 0xd8e3f5,
                keyboard_focus: 0x1e66f5,
                liked: 0xd20f39,
                hover: 0xdce0e8,
                artwork_bg: 0xccd0da,
                artwork_icon: 0x6c6f85,
                error_bg: 0xf0e9ed,
                error_text: 0xd20f39,
                input_bg: 0xeff1f5,
                input_border: 0x9ca0b0,
                placeholder: 0x6c6f85,
                selection: 0x1e66f540,
                tooltip_bg: 0x4c4f69,
                tooltip_text: 0xeff1f5,
                backdrop: 0x4c4f6940,
            },
            Self::Macchiato => Palette {
                bg: 0x24273a,
                panel: 0x1e2030,
                border: 0x494d64,
                row_border: 0x303449,
                text: 0xcad3f5,
                muted: 0xb8c0e0,
                accent: 0x8aadf4,
                on_accent: 0x181926,
                accent_hover: 0xb7bdf8,
                selected: 0x363e56,
                keyboard_focus: 0x8aadf4,
                liked: 0xed8796,
                hover: 0x363a4f,
                artwork_bg: 0x363a4f,
                artwork_icon: 0xa5adcb,
                error_bg: 0x382e43,
                error_text: 0xed8796,
                input_bg: 0x1e2030,
                input_border: 0x6e738d,
                placeholder: 0xa5adcb,
                selection: 0x8aadf440,
                tooltip_bg: 0x363a4f,
                tooltip_text: 0xcad3f5,
                backdrop: 0x18192680,
            },
        }
    }
}

pub fn colors(cx: &App) -> Palette {
    cx.global::<Theme>().palette()
}

// Shared typography sizes in logical pixels.
pub const TEXT_BODY: f32 = 16.;
pub const TEXT_SECONDARY: f32 = 15.;
pub const TEXT_CAPTION: f32 = 14.;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_follows_both_appearances_while_manual_choices_stay_fixed() {
        for appearance in [
            WindowAppearance::Light,
            WindowAppearance::VibrantLight,
            WindowAppearance::Dark,
            WindowAppearance::VibrantDark,
        ] {
            assert_eq!(ThemePreference::Latte.resolve(appearance), Theme::Latte);
            assert_eq!(
                ThemePreference::Macchiato.resolve(appearance),
                Theme::Macchiato
            );
            assert_eq!(
                ThemePreference::System.resolve(appearance),
                if matches!(
                    appearance,
                    WindowAppearance::Dark | WindowAppearance::VibrantDark
                ) {
                    Theme::Macchiato
                } else {
                    Theme::Latte
                }
            );
        }
        assert_eq!(
            serde_json::from_str::<ThemePreference>("\"latte\"").unwrap(),
            ThemePreference::Latte
        );
        assert_eq!(
            serde_json::from_str::<ThemePreference>("\"macchiato\"").unwrap(),
            ThemePreference::Macchiato
        );
    }

    fn luminance(color: u32) -> f64 {
        let channel = |shift: u32| {
            let value = ((color >> shift) & 255u32) as f64 / 255.;
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0)
    }

    #[test]
    fn both_themes_keep_text_legible_on_their_surfaces() {
        for theme in [Theme::Latte, Theme::Macchiato] {
            let p = theme.palette();
            for (foreground, background) in [
                (p.text, p.bg),
                (p.muted, p.bg),
                (p.text, p.panel),
                (p.text, p.input_bg),
                (p.text, p.selected),
                (p.muted, p.hover),
                (p.on_accent, p.accent),
                (p.on_accent, p.accent_hover),
                (p.tooltip_text, p.tooltip_bg),
                (p.error_text, p.error_bg),
            ] {
                let a = luminance(foreground);
                let b = luminance(background);
                let contrast = (a.max(b) + 0.05) / (a.min(b) + 0.05);
                assert!(
                    contrast >= 4.5,
                    "{theme:?}: {foreground:06x} on {background:06x}: {contrast}"
                );
            }
        }
    }
}
