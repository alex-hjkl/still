use crate::api::Song;
use anyhow::{Result, bail};
use std::{collections::BTreeMap, sync::Arc};

const REAL_PATH_HELP: &str = "To browse folders, enable Report Real Path for Still in Navidrome → Players, then click Refresh folders.";

#[derive(Default)]
pub struct Folder {
    pub children: Vec<String>,
    pub songs: Arc<Vec<Song>>,
}

#[derive(Default)]
pub struct FolderLibrary {
    folders: BTreeMap<String, Folder>,
}

// Interpret server paths independently of the desktop OS. Navidrome on Linux
// returns absolute POSIX paths with Report Real Path; its fake paths are relative.
pub fn validate_path(path: Option<&str>) -> Result<&str> {
    let Some(path) = path else {
        bail!("{REAL_PATH_HELP}");
    };
    if !path.starts_with('/') || path.ends_with('/') || path.contains('\0') {
        bail!("{REAL_PATH_HELP}");
    }
    if path.split('/').any(|part| part == ".." || part == ".") {
        bail!("Navidrome returned an invalid file path");
    }
    Ok(path)
}

pub fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

pub fn parent_path(path: &str) -> Option<&str> {
    (!path.is_empty()).then(|| path.rsplit_once('/').map_or("", |(parent, _)| parent))
}

impl FolderLibrary {
    pub fn from_songs(songs: Vec<Song>) -> Result<Self> {
        let mut library = Self::default();
        library.folders.insert(String::new(), Folder::default());
        for song in songs {
            let path = validate_path(song.path.as_deref())?;
            let mut parts: Vec<_> = path.split('/').filter(|part| !part.is_empty()).collect();
            parts.pop(); // Filename; the remaining components are real directories.
            let mut parent = String::new();
            for part in parts {
                let child = format!("{parent}/{part}");
                if !library.folders.contains_key(&child) {
                    library.folders.insert(child.clone(), Folder::default());
                    library
                        .folders
                        .get_mut(&parent)
                        .unwrap()
                        .children
                        .push(child.clone());
                }
                parent = child;
            }
            Arc::make_mut(&mut library.folders.get_mut(&parent).unwrap().songs).push(song);
        }
        for folder in library.folders.values_mut() {
            folder
                .children
                .sort_by_cached_key(|path| file_name(path).to_lowercase());
            Arc::make_mut(&mut folder.songs)
                .sort_by_cached_key(|song| song.path.as_deref().unwrap_or_default().to_lowercase());
        }
        Ok(library)
    }

    pub fn folder(&self, path: &str) -> Option<&Folder> {
        self.folders.get(path)
    }

    pub fn songs_recursive(&self, path: &str) -> Vec<Song> {
        let mut songs = Vec::new();
        let mut pending = vec![path];
        while let Some(path) = pending.pop() {
            if let Some(folder) = self.folder(path) {
                songs.extend(folder.songs.iter().cloned());
                pending.extend(folder.children.iter().rev().map(String::as_str));
            }
        }
        songs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn song(id: &str, path: &str, artist: &str) -> Song {
        serde_json::from_value(json!({
            "id": id, "title": "Theme", "artist": artist, "path": path
        }))
        .unwrap()
    }

    #[test]
    fn soundtracks_stay_together_regardless_of_artist_and_album_tags() {
        let library = FolderLibrary::from_songs(vec![
            song("b", "/music/Саундтреки/Фильм/02 Theme.flac", "Composer B"),
            song("a", "/music/Саундтреки/Фильм/01 Opening.flac", "Composer A"),
            song(
                "c",
                "/music/Саундтреки/Фильм/CD 2/01 Finale.flac",
                "Composer C",
            ),
            song(
                "d",
                "/music/Саундтреки/Фильм 2/01 Opening.flac",
                "Composer A",
            ),
        ])
        .unwrap();
        let folder = library.folder("/music/Саундтреки/Фильм").unwrap();
        assert_eq!(folder.children, ["/music/Саундтреки/Фильм/CD 2"]);
        assert_eq!(
            folder
                .songs
                .iter()
                .map(|song| song.id.as_str())
                .collect::<Vec<_>>(),
            ["a", "b"]
        );
        assert_eq!(
            library.folder("/music/Саундтреки").unwrap().children.len(),
            2
        );
        assert_eq!(library.folder("").unwrap().children, ["/music"]);
    }

    #[test]
    fn empty_libraries_and_files_at_root_are_supported() {
        assert!(
            FolderLibrary::from_songs(Vec::new())
                .unwrap()
                .folder("")
                .is_some()
        );
        let library = FolderLibrary::from_songs(vec![song("a", "/song.mp3", "A")]).unwrap();
        assert_eq!(library.folder("").unwrap().songs.len(), 1);
    }

    #[test]
    fn playing_a_folder_includes_its_discs_in_order_and_excludes_neighbours() {
        let library = FolderLibrary::from_songs(vec![
            song("disc2", "/music/Film/CD 2/01.wav", "C"),
            song("direct", "/music/Film/01.wav", "A"),
            song("disc1", "/music/Film/CD 1/01.wav", "B"),
            song("other", "/music/Film 2/01.wav", "A"),
        ])
        .unwrap();
        assert_eq!(
            library
                .songs_recursive("/music/Film")
                .iter()
                .map(|s| s.id.as_str())
                .collect::<Vec<_>>(),
            ["direct", "disc1", "disc2"]
        );
        assert_eq!(library.songs_recursive("").len(), 4);
        assert!(library.songs_recursive("/missing").is_empty());
    }

    #[test]
    fn fake_missing_and_invalid_paths_are_not_presented_as_real_folders() {
        for path in [
            None,
            Some("Artist/Album/01 - Theme.mp3"),
            Some("/music/"),
            Some("/music/../theme.mp3"),
            Some("/music/./theme.mp3"),
        ] {
            assert!(validate_path(path).is_err());
        }
        assert!(FolderLibrary::from_songs(vec![song("a", "Artist/Album/Theme.mp3", "A")]).is_err());
    }
}
