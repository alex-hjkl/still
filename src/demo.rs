use super::{Album, Artist, FolderLibrary, LibraryData, Page, PlayerView, Song};

impl PlayerView {
    pub(super) fn load_demo_page(&mut self) {
        self.browser.invalidate_requests();
        let mut data = match self.browser.state().page.clone() {
            Page::Artists => LibraryData::Artists(demo_artists()),
            Page::Albums => LibraryData::Albums(demo_albums()),
            Page::Artist(artist) => LibraryData::Albums(
                demo_albums()
                    .into_iter()
                    .filter(|album| album.artist == artist.name)
                    .collect(),
            ),
            Page::Songs => LibraryData::Songs(
                demo_albums()
                    .into_iter()
                    .flat_map(|album| album.song)
                    .collect(),
            ),
            Page::Liked => LibraryData::Songs(
                self.likes.merge_liked(
                    demo_albums()
                        .into_iter()
                        .flat_map(|album| album.song)
                        .filter(|song| song.starred.is_some())
                        .collect(),
                ),
            ),
            Page::Album(album) => LibraryData::Album(
                demo_albums()
                    .into_iter()
                    .find(|candidate| candidate.id == album.id)
                    .unwrap_or(album),
            ),
            Page::Folder(_) => {
                if self.browser.use_cached_folders() {
                    return;
                }
                let songs = demo_albums()
                    .into_iter()
                    .flat_map(|album| {
                        album.song.into_iter().map(move |mut song| {
                            song.path = Some(format!(
                                "/music/{}/{} - {}.flac",
                                album.name, song.track, song.title
                            ));
                            song
                        })
                    })
                    .collect();
                LibraryData::Folders(FolderLibrary::from_songs(songs).expect("valid demo paths"))
            }
        };
        if let LibraryData::Albums(albums) = &mut data {
            crate::api::sort_albums(albums);
        }
        self.browser.apply(data, false);
    }
}

fn demo_artists() -> Vec<Artist> {
    [
        ("1", "Nils Frahm", 3),
        ("2", "Ólafur Arnalds", 2),
        ("3", "Hania Rani", 2),
        ("4", "Ryuichi Sakamoto", 2),
        ("5", "Max Richter", 1),
    ]
    .into_iter()
    .map(|(id, name, album_count)| Artist {
        id: id.into(),
        name: name.into(),
        album_count,
    })
    .collect()
}

fn demo_albums() -> Vec<Album> {
    [
        ("Spaces", "Nils Frahm", 2013),
        ("All Melody", "Nils Frahm", 2018),
        ("Music for Animals", "Nils Frahm", 2022),
        ("re:member", "Ólafur Arnalds", 2018),
        ("some kind of peace", "Ólafur Arnalds", 2020),
        ("Esja", "Hania Rani", 2019),
        ("Ghosts", "Hania Rani", 2023),
        ("async", "Ryuichi Sakamoto", 2017),
        ("12", "Ryuichi Sakamoto", 2023),
        ("The Blue Notebooks", "Max Richter", 2004),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (name, artist, year))| {
        let songs = [
            "Opening",
            "A quiet morning",
            "Drift",
            "Somewhere",
            "Still here",
        ]
        .into_iter()
        .enumerate()
        .map(|(track, title)| Song {
            id: format!("demo-{index}-{track}"),
            title: title.into(),
            artist: artist.into(),
            album: name.into(),
            duration: 180 + track as u64 * 23,
            track: track as u32 + 1,
            disc_number: 1,
            cover_art: None,
            path: None,
            starred: (index == 0 && track < 2).then(|| "2026-10-15T12:00:00Z".into()),
        })
        .collect();
        Album {
            id: format!("demo-{index}"),
            name: name.into(),
            artist: artist.into(),
            year: Some(year),
            song_count: 5,
            cover_art: None,
            song: songs,
        }
    })
    .collect()
}
