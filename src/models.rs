//! Music library records shared by browsing, playback and the API adapter.
use serde::Deserialize;

/// Number of records requested by the paginated album and song browsers.
pub const PAGE_SIZE: usize = 200;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Artist {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub album_count: usize,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Album {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub year: Option<u32>,
    #[serde(default)]
    pub song_count: usize,
    #[serde(default)]
    pub cover_art: Option<String>,
    #[serde(default)]
    pub song: Vec<Song>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Song {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub album: String,
    #[serde(default)]
    pub duration: u64,
    #[serde(default)]
    pub track: u32,
    #[serde(default)]
    pub disc_number: u32,
    #[serde(default)]
    pub cover_art: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub starred: Option<String>,
}

impl Song {
    pub fn artwork_id(&self) -> Option<&str> {
        let album = self.album.trim();
        // Navidrome can fall back from media-file art to album art. Untagged
        // files can share Unknown Album across unrelated folders, so even an
        // mf- cover ID does not establish that its image belongs to this file.
        if album.is_empty()
            || album.eq_ignore_ascii_case("[Unknown Album]")
            || album.eq_ignore_ascii_case("Unknown Album")
        {
            return None;
        }
        self.cover_art.as_deref().filter(|id| !id.trim().is_empty())
    }
}

pub fn sort_albums(albums: &mut [Album]) {
    albums.sort_by(|a, b| a.year.cmp(&b.year).then_with(|| a.name.cmp(&b.name)));
}

pub fn time_label(seconds: u64) -> String {
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn untagged_songs_do_not_use_shared_unknown_album_artwork() {
        for album in ["", "[Unknown Album]", " unknown album "] {
            for cover in ["al-shared", "mf-song"] {
                let song: Song = serde_json::from_value(json!({
                    "id": "song", "title": "04 - Последний глоток",
                    "album": album, "coverArt": cover
                }))
                .unwrap();
                assert_eq!(song.artwork_id(), None);
            }
        }
    }

    #[test]
    fn identified_albums_keep_server_cover_ids_even_without_an_artist() {
        for cover in [None, Some(""), Some(" "), Some("opaque-cover_123")] {
            let song: Song = serde_json::from_value(json!({
                "id": "song", "title": "Theme", "album": "Film", "coverArt": cover
            }))
            .unwrap();
            assert_eq!(song.artwork_id(), cover.filter(|id| !id.trim().is_empty()));
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct SearchResults {
    #[serde(default, rename = "artist", deserialize_with = "null_as_empty")]
    pub artists: Vec<Artist>,
    #[serde(default, rename = "album", deserialize_with = "null_as_empty")]
    pub albums: Vec<Album>,
    #[serde(default, rename = "song", deserialize_with = "null_as_empty")]
    pub songs: Vec<Song>,
}

fn null_as_empty<'de, D, T>(deserializer: D) -> std::result::Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Option::<Vec<T>>::deserialize(deserializer)?.unwrap_or_default())
}
