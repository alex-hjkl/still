//! Resolve a play action without changing the browser's current page.
use crate::{
    api::{Album, Artist, Navidrome, Song},
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
    use serde_json::{Value, json};
    use std::{
        io::{BufRead, BufReader, Write},
        net::TcpListener,
        thread,
    };

    fn fixture(
        responses: Vec<(&'static str, &'static str, Value)>,
        cancel_after: Option<Cancellation>,
    ) -> (Navidrome, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let client = Navidrome::new(
            &format!("http://{}", listener.local_addr().unwrap()),
            "fixture",
            "fixture",
        )
        .unwrap();
        let server = thread::spawn(move || {
            let last = responses.len() - 1;
            for (index, (method, id, body)) in responses.into_iter().enumerate() {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                    .unwrap();
                let mut reader = BufReader::new(socket.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let url = reqwest::Url::parse(&format!(
                    "http://localhost{}",
                    line.split_whitespace().nth(1).unwrap()
                ))
                .unwrap();
                assert_eq!(url.path(), format!("/rest/{method}.view"));
                assert_eq!(
                    url.query_pairs().find(|(key, _)| key == "id").unwrap().1,
                    id
                );
                loop {
                    let mut header = String::new();
                    reader.read_line(&mut header).unwrap();
                    if header.trim().is_empty() {
                        break;
                    }
                }
                if index == last
                    && let Some(cancellation) = &cancel_after
                {
                    cancellation.cancel();
                }
                let payload = json!({"subsonic-response": body}).to_string();
                write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", payload.len(), payload).unwrap();
            }
        });
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
        server.join().unwrap();
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
        server.join().unwrap();
    }
}
