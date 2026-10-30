//! Resolve a play action without changing the browser's current page.
use crate::{
    api::Navidrome,
    models::{Album, Artist, Song},
    requests::Cancellation,
};
use anyhow::Result;

#[derive(Clone)]
pub enum Collection {
    Artist(Artist),
    Album(Album),
}

impl Collection {
    pub fn name(&self) -> &str {
        match self {
            Self::Artist(artist) => &artist.name,
            Self::Album(album) => &album.name,
        }
    }

    pub fn songs(&self, client: &Navidrome, cancellation: &Cancellation) -> Result<Vec<Song>> {
        cancellation.check()?;
        let albums = match self {
            Self::Artist(artist) => client.artist_albums(&artist.id)?,
            Self::Album(album) => vec![album.clone()],
        };
        let mut songs = Vec::new();
        for album in albums {
            cancellation.check()?;
            songs.extend(client.album(&album.id)?.song);
        }
        cancellation.check()?;
        Ok(songs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_http::{Response, Server};
    use serde_json::{Value, json};

    fn fixture(
        responses: Vec<(&'static str, &'static str, Value)>,
        cancel_after: Option<Cancellation>,
    ) -> (Navidrome, Server) {
        let last = responses.len() - 1;
        let server = Server::new(responses.into_iter().enumerate().map(
            |(index, (method, id, body))| {
                let response = Response::subsonic(method, body).query("id", id);
                if index == last
                    && let Some(cancellation) = cancel_after.clone()
                {
                    response.before_reply(move || cancellation.cancel())
                } else {
                    response
                }
            },
        ));
        let client = Navidrome::new(&server.url, "fixture", "fixture").unwrap();
        (client, server)
    }

    #[test]
    fn artist_queue_contains_every_album_in_year_disc_and_track_order() {
        let (client, server) = fixture(
            vec![
                (
                    "getArtist",
                    "artist",
                    json!({"status":"ok", "artist":{"album":[
                        {"id":"new", "name":"New", "year":2020}, {"id":"old", "name":"Old", "year":2000}
                    ]}}),
                ),
                (
                    "getAlbum",
                    "old",
                    json!({"status":"ok", "album":{"id":"old", "name":"Old", "song":[
                        {"id":"disc2", "title":"Second disc", "discNumber":2, "track":1},
                        {"id":"disc1", "title":"First disc", "discNumber":1, "track":1}
                    ]}}),
                ),
                (
                    "getAlbum",
                    "new",
                    json!({"status":"ok", "album":{"id":"new", "name":"New", "song":[
                        {"id":"new-song", "title":"New song"}
                    ]}}),
                ),
            ],
            None,
        );
        let collection = Collection::Artist(Artist {
            id: "artist".into(),
            name: "Artist".into(),
            album_count: 2,
        });
        let songs = collection.songs(&client, &Cancellation::default()).unwrap();
        assert_eq!(
            songs
                .iter()
                .map(|song| song.id.as_str())
                .collect::<Vec<_>>(),
            ["disc1", "disc2", "new-song"]
        );
        server.finish();
    }

    #[test]
    fn canceling_an_artist_queue_stops_before_fetching_the_next_album() {
        let cancellation = Cancellation::default();
        let (client, server) = fixture(
            vec![
                (
                    "getArtist",
                    "artist",
                    json!({"status":"ok", "artist":{"album":[
                        {"id":"old", "name":"Old", "year":2000}, {"id":"next", "name":"Next", "year":2020}
                    ]}}),
                ),
                (
                    "getAlbum",
                    "old",
                    json!({"status":"ok", "album":{"id":"old", "name":"Old", "song":[
                        {"id":"old-song", "title":"Old song"}
                    ]}}),
                ),
            ],
            Some(cancellation.clone()),
        );
        let collection = Collection::Artist(Artist {
            id: "artist".into(),
            name: "Artist".into(),
            album_count: 2,
        });
        assert!(collection.songs(&client, &cancellation).is_err());
        server.finish();
    }
}
