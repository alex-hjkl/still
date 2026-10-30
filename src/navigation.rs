//! Section identity and presentation shared by navigation, settings and shortcuts.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Section {
    #[default]
    Artists,
    Albums,
    Songs,
    Folders,
    Liked,
}

#[derive(Clone, Copy)]
pub struct SectionInfo {
    pub section: Section,
    pub label: &'static str,
    pub icon: &'static str,
    pub key: &'static str,
    pub shortcut: &'static str,
}

impl Section {
    pub const ALL: [SectionInfo; 5] = [
        SectionInfo {
            section: Self::Artists,
            label: "Artists",
            icon: "artists",
            key: "1",
            shortcut: "⌘1",
        },
        SectionInfo {
            section: Self::Albums,
            label: "Albums",
            icon: "albums",
            key: "2",
            shortcut: "⌘2",
        },
        SectionInfo {
            section: Self::Songs,
            label: "Songs",
            icon: "songs",
            key: "3",
            shortcut: "⌘3",
        },
        SectionInfo {
            section: Self::Folders,
            label: "Folders",
            icon: "folders",
            key: "4",
            shortcut: "⌘4",
        },
        SectionInfo {
            section: Self::Liked,
            label: "Liked songs",
            icon: "heart",
            key: "5",
            shortcut: "⌘5",
        },
    ];

    pub fn from_index(index: usize) -> Self {
        Self::ALL
            .get(index)
            .map_or(Self::Liked, |info| info.section)
    }

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn info(self) -> &'static SectionInfo {
        &Self::ALL[self.index()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn section_order_and_persisted_names_round_trip() {
        for (index, info) in Section::ALL.iter().enumerate() {
            assert_eq!(info.section.index(), index);
            assert_eq!(Section::from_index(index), info.section);
            let saved = serde_json::to_string(&info.section).unwrap();
            assert_eq!(
                serde_json::from_str::<Section>(&saved).unwrap(),
                info.section
            );
        }
        assert_eq!(serde_json::to_string(&Section::Liked).unwrap(), "\"liked\"");
    }
}
