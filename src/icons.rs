use anyhow::Result;
use gpui::{AssetSource, SharedString};
use std::borrow::Cow;

// The original Still mark and Phosphor interface icons are embedded locally.
const ICONS: &[(&str, &[u8])] = &[
    (
        "brand-wave",
        include_bytes!("../assets/app-icon/still-wave.svg"),
    ),
    (
        "brand-ripples",
        include_bytes!("../assets/app-icon/still-ripples.svg"),
    ),
    ("monitor", include_bytes!("../assets/icons/monitor.svg")),
    ("sun", include_bytes!("../assets/icons/sun.svg")),
    ("moon", include_bytes!("../assets/icons/moon.svg")),
    ("artists", include_bytes!("../assets/icons/artists.svg")),
    ("albums", include_bytes!("../assets/icons/albums.svg")),
    ("songs", include_bytes!("../assets/icons/songs.svg")),
    ("folders", include_bytes!("../assets/icons/folders.svg")),
    ("play", include_bytes!("../assets/icons/play.svg")),
    ("pause", include_bytes!("../assets/icons/pause.svg")),
    ("heart", include_bytes!("../assets/icons/heart.svg")),
    ("check", include_bytes!("../assets/icons/check.svg")),
    (
        "heart-filled",
        include_bytes!("../assets/icons/heart-filled.svg"),
    ),
    ("shuffle", include_bytes!("../assets/icons/shuffle.svg")),
    ("previous", include_bytes!("../assets/icons/previous.svg")),
    ("next", include_bytes!("../assets/icons/next.svg")),
    ("back", include_bytes!("../assets/icons/back.svg")),
    ("forward", include_bytes!("../assets/icons/forward.svg")),
    ("volume", include_bytes!("../assets/icons/volume.svg")),
    (
        "volume-muted",
        include_bytes!("../assets/icons/volume-muted.svg"),
    ),
    ("search", include_bytes!("../assets/icons/search.svg")),
    ("logout", include_bytes!("../assets/icons/logout.svg")),
    ("plus", include_bytes!("../assets/icons/plus.svg")),
    ("minus", include_bytes!("../assets/icons/minus.svg")),
    ("refresh", include_bytes!("../assets/icons/refresh.svg")),
    (
        "sidebar-expand",
        include_bytes!("../assets/icons/sidebar-expand.svg"),
    ),
    (
        "sidebar-collapse",
        include_bytes!("../assets/icons/sidebar-collapse.svg"),
    ),
];

pub struct Icons;

impl AssetSource for Icons {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(ICONS
            .iter()
            .find(|(name, _)| *name == path)
            .map(|(_, data)| Cow::Borrowed(*data)))
    }

    fn list(&self, prefix: &str) -> Result<Vec<SharedString>> {
        Ok(ICONS
            .iter()
            .filter(|(name, _)| name.starts_with(prefix))
            .map(|(name, _)| (*name).into())
            .collect())
    }
}
