use crate::requests::Cancellation;
use anyhow::{Context, Result, bail};
use rand::{RngExt, distr::Alphanumeric};
use reqwest::{Url, blocking::Client};
use serde::Deserialize;
#[cfg(test)]
use serde_json::Value;
use std::time::Duration;

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

#[derive(Debug, Deserialize)]
struct Envelope {
    #[serde(rename = "subsonic-response")]
    response: ApiResponse,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum ResponseStatus {
    Ok,
    Failed,
}

#[derive(Debug, Deserialize)]
struct ApiError {
    #[serde(default)]
    code: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiResponse {
    status: ResponseStatus,
    error: Option<ApiError>,
    artists: Option<Artists>,
    artist: Option<Albums>,
    album: Option<Album>,
    album_list2: Option<Albums>,
    search_result3: Option<Songs>,
    starred2: Option<Songs>,
}

#[derive(Debug, Deserialize)]
struct Artists {
    index: Option<Vec<ArtistIndex>>,
}
#[derive(Debug, Deserialize)]
struct ArtistIndex {
    artist: Option<Vec<Artist>>,
}
#[derive(Debug, Deserialize)]
struct Albums {
    album: Option<Vec<Album>>,
}
#[derive(Debug, Deserialize)]
struct Songs {
    song: Option<Vec<Song>>,
}

// Deliberately no Debug: authenticated URLs and tokens must not enter logs.
#[derive(Clone)]
pub struct Navidrome {
    base: Url,
    username: String,
    token: String,
    salt: String,
    http: Client,
}

impl Navidrome {
    // Stable across logins/password changes; contains no authentication token.
    pub fn library_id(&self) -> String {
        format!(
            "{:x}",
            md5::compute(format!("{}\0{}", self.base, self.username))
        )
    }

    pub fn new(server: &str, username: &str, password: &str) -> Result<Self> {
        let mut base = Url::parse(server.trim()).context("Enter the full server URL")?;
        if !matches!(base.scheme(), "http" | "https") || base.host_str().is_none() {
            bail!("The URL must start with http:// or https://");
        }
        if !base.username().is_empty() || base.password().is_some() {
            bail!("Enter your username and password in the separate fields");
        }
        if username.trim().is_empty() || password.is_empty() {
            bail!("Enter your username and password");
        }
        base.set_query(None);
        base.set_fragment(None);
        let path = format!("{}/", base.path().trim_end_matches('/'));
        base.set_path(&path);
        let salt: String = rand::rng()
            .sample_iter(Alphanumeric)
            .take(24)
            .map(char::from)
            .collect();
        let token = auth_token(password, &salt);
        let http = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .build()?;
        Ok(Self {
            base,
            username: username.trim().into(),
            token,
            salt,
            http,
        })
    }

    fn url(&self, method: &str, params: &[(&str, String)]) -> Url {
        let mut url = self
            .base
            .join(&format!("rest/{method}.view"))
            .expect("validated base URL");
        url.query_pairs_mut()
            .extend_pairs([
                ("u", self.username.as_str()),
                ("t", self.token.as_str()),
                ("s", self.salt.as_str()),
                ("v", "1.16.1"),
                ("c", "Still"),
                ("f", "json"),
            ])
            .extend_pairs(params.iter().map(|(key, value)| (*key, value.as_str())));
        url
    }

    fn request(&self, method: &str, params: &[(&str, String)]) -> Result<ApiResponse> {
        let response = self
            .http
            .get(self.url(method, params))
            .send()
            .map_err(reqwest::Error::without_url)
            .context("Could not connect to Navidrome")?
            .error_for_status()
            .map_err(reqwest::Error::without_url)
            .context("The server returned an HTTP error")?;
        decode_response(
            response
                .json::<Envelope>()
                .map_err(reqwest::Error::without_url)
                .context("The server returned invalid JSON")?,
        )
    }

    pub fn ping(&self) -> Result<()> {
        self.request("ping", &[])?;
        Ok(())
    }

    pub fn artists(&self) -> Result<Vec<Artist>> {
        let body = self.request("getArtists", &[])?;
        let artists = body
            .artists
            .context("Missing artists in the Navidrome response")?;
        Ok(artists
            .index
            .unwrap_or_default()
            .into_iter()
            .flat_map(|index| index.artist.unwrap_or_default())
            .collect())
    }

    pub fn artist_albums(&self, id: &str) -> Result<Vec<Album>> {
        let body = self.request("getArtist", &[("id", id.into())])?;
        let mut albums = body
            .artist
            .context("Missing artist in the Navidrome response")?
            .album
            .unwrap_or_default();
        sort_albums(&mut albums);
        Ok(albums)
    }

    pub fn albums(&self, offset: usize) -> Result<Vec<Album>> {
        let body = self.request(
            "getAlbumList2",
            &[
                ("type", "byYear".into()),
                ("fromYear", "0".into()),
                ("toYear", "9999".into()),
                ("size", PAGE_SIZE.to_string()),
                ("offset", offset.to_string()),
            ],
        )?;
        Ok(body
            .album_list2
            .context("Missing albumList2 in the Navidrome response")?
            .album
            .unwrap_or_default())
    }

    pub fn album(&self, id: &str) -> Result<Album> {
        let body = self.request("getAlbum", &[("id", id.into())])?;
        let mut album = body
            .album
            .context("Missing album in the Navidrome response")?;
        album
            .song
            .sort_by_key(|song| (song.disc_number, song.track));
        Ok(album)
    }

    pub fn songs(&self, offset: usize) -> Result<Vec<Song>> {
        self.songs_page(offset, PAGE_SIZE)
    }

    pub fn liked_songs(&self) -> Result<Vec<Song>> {
        let body = self.request("getStarred2", &[])?;
        Ok(body
            .starred2
            .context("Missing starred2 in the Navidrome response")?
            .song
            .unwrap_or_default())
    }

    pub fn set_liked(&self, id: &str, liked: bool) -> Result<()> {
        self.request(if liked { "star" } else { "unstar" }, &[("id", id.into())])?;
        Ok(())
    }

    fn songs_page(&self, offset: usize, size: usize) -> Result<Vec<Song>> {
        // Navidrome supports an empty search3 query for enumerating the library.
        let body = self.request(
            "search3",
            &[
                ("query", "".into()),
                ("artistCount", "0".into()),
                ("albumCount", "0".into()),
                ("songCount", size.to_string()),
                ("songOffset", offset.to_string()),
            ],
        )?;
        Ok(body
            .search_result3
            .context("Missing searchResult3 in the Navidrome response")?
            .song
            .unwrap_or_default())
    }

    pub fn folder_library(
        &self,
        cancellation: &Cancellation,
    ) -> Result<crate::folders::FolderLibrary> {
        let mut songs = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let mut offset = 0;
        loop {
            cancellation.check()?;
            let page = self.songs_page(offset, 500)?;
            cancellation.check()?;
            let count = page.len();
            for song in page {
                // Fail early if Navidrome is still returning paths made from tags.
                crate::folders::validate_path(song.path.as_deref())?;
                if !seen.insert(song.id.clone()) {
                    bail!("The library changed while loading folders. Click Refresh folders.");
                }
                songs.push(song);
            }
            if count < 500 {
                break;
            }
            offset += count;
        }
        cancellation.check()?;
        crate::folders::FolderLibrary::from_songs(songs)
    }

    pub fn stream_url(&self, id: &str) -> Url {
        self.url("stream", &[("id", id.into())])
    }

    pub fn cover_url(&self, id: &str) -> String {
        self.url("getCoverArt", &[("id", id.into()), ("size", "400".into())])
            .into()
    }

    pub fn server_label(&self) -> String {
        self.base.host_str().unwrap_or("Navidrome").into()
    }
}

pub fn sort_albums(albums: &mut [Album]) {
    albums.sort_by(|a, b| a.year.cmp(&b.year).then_with(|| a.name.cmp(&b.name)));
}

fn auth_token(password: &str, salt: &str) -> String {
    format!("{:x}", md5::compute(format!("{password}{salt}")))
}

fn decode_response(envelope: Envelope) -> Result<ApiResponse> {
    let response = envelope.response;
    if response.status == ResponseStatus::Failed {
        let code = response.error.as_ref().map_or(0, |error| error.code);
        let message = match code {
            40 | 41 => "Incorrect username or password",
            50 => "Insufficient permissions for this operation",
            70 => "This item no longer exists in the library",
            _ => "Navidrome could not complete the request",
        };
        bail!("{message} (code {code})");
    }
    Ok(response)
}

pub fn time_label(seconds: u64) -> String {
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_identity_is_stable_for_a_login_and_distinguishes_accounts_and_servers() {
        let a = Navidrome::new("https://music.example", "alice", "old").unwrap();
        let b = Navidrome::new("https://music.example/", "alice", "new").unwrap();
        assert_eq!(a.library_id(), b.library_id());
        assert_ne!(
            a.library_id(),
            Navidrome::new("https://music.example", "bob", "old")
                .unwrap()
                .library_id()
        );
        assert_ne!(
            a.library_id(),
            Navidrome::new("https://other.example", "alice", "old")
                .unwrap()
                .library_id()
        );
    }
    use serde_json::json;

    fn decode_fixture(value: Value) -> Result<ApiResponse> {
        decode_response(serde_json::from_value(value)?)
    }

    #[test]
    fn token_matches_subsonic_example() {
        assert_eq!(
            auth_token("sesame", "c19b2d"),
            "26719a1196d2a940705a59634eb18eab"
        );
    }

    #[test]
    fn authenticated_urls_preserve_reverse_proxy_path_and_encode_ids() {
        let client = Navidrome::new("https://music.example/navidrome/", "a+b", "secret").unwrap();
        let url = client.stream_url("id /?&");
        assert_eq!(url.path(), "/navidrome/rest/stream.view");
        let pairs: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(pairs["id"], "id /?&");
        assert_eq!(pairs["u"], "a+b");
        assert!(!pairs.contains_key("p"));
        assert_eq!(pairs["t"], auth_token("secret", &pairs["s"]));
    }

    #[test]
    fn api_errors_are_detected_even_with_http_200() {
        let error = decode_fixture(json!({"subsonic-response": {
            "status": "failed", "error": {"code": 40, "message": "secret"}
        }}))
        .unwrap_err()
        .to_string();
        assert!(error.contains("Incorrect username"));
        assert!(!error.contains("secret"));
        assert!(decode_fixture(json!({"unrelated": true})).is_err());
    }

    #[test]
    fn missing_lists_and_optional_metadata_are_supported() {
        let empty =
            decode_fixture(json!({"subsonic-response": {"status":"ok", "searchResult3":{}}}))
                .unwrap();
        assert!(
            empty
                .search_result3
                .unwrap()
                .song
                .unwrap_or_default()
                .is_empty()
        );
        let song: Song =
            serde_json::from_value(json!({"id":"opaque-id", "title":"Песня"})).unwrap();
        assert_eq!(song.duration, 0);
        assert_eq!(time_label(125), "2:05");
    }

    #[test]
    fn rejects_invalid_server_and_missing_credentials() {
        assert!(Navidrome::new("file:///tmp", "user", "pass").is_err());
        assert!(Navidrome::new("https://user:pass@music.example", "user", "pass").is_err());
        assert!(Navidrome::new("https://music.example", "", "pass").is_err());
    }

    fn folder_server(pages: Vec<Value>) -> (Navidrome, std::thread::JoinHandle<()>) {
        folder_server_with_cancellation(pages, None)
    }

    fn folder_server_with_cancellation(
        pages: Vec<Value>,
        cancel_after_first: Option<Cancellation>,
    ) -> (Navidrome, std::thread::JoinHandle<()>) {
        use std::io::{BufRead, BufReader, Write};
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            for (page, songs) in pages.into_iter().enumerate() {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut reader = BufReader::new(socket.try_clone().unwrap());
                let mut request = String::new();
                reader.read_line(&mut request).unwrap();
                let path = request.split_whitespace().nth(1).unwrap();
                let url = Url::parse(&format!("http://localhost{path}")).unwrap();
                assert_eq!(url.path(), "/rest/search3.view");
                let params: std::collections::HashMap<_, _> =
                    url.query_pairs().into_owned().collect();
                assert_eq!(params["songCount"], "500");
                assert_eq!(params["songOffset"], (page * 500).to_string());
                assert_eq!(params["artistCount"], "0");
                assert_eq!(params["albumCount"], "0");
                assert_eq!(params["query"], "");
                loop {
                    let mut header = String::new();
                    reader.read_line(&mut header).unwrap();
                    if header.trim().is_empty() {
                        break;
                    }
                }
                let payload = json!({"subsonic-response": {
                    "status": "ok", "searchResult3": {"song": songs}
                }})
                .to_string();
                if page == 0
                    && let Some(cancellation) = &cancel_after_first
                {
                    cancellation.cancel();
                }
                write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", payload.len(), payload).unwrap();
            }
        });
        (
            Navidrome::new(&format!("http://{address}"), "user", "password").unwrap(),
            server,
        )
    }

    #[test]
    fn folder_scan_stops_after_the_inflight_page_when_cancelled() {
        let cancellation = Cancellation::default();
        let first: Vec<_> = (0..500)
            .map(|i| {
                json!({
                    "id": format!("song-{i}"), "title":"Theme", "path":format!("/music/{i}.flac")
                })
            })
            .collect();
        let (client, server) =
            folder_server_with_cancellation(vec![json!(first)], Some(cancellation.clone()));
        let error = client.folder_library(&cancellation).err().unwrap();
        assert_eq!(error.to_string(), "Loading canceled");
        server.join().unwrap();
    }

    #[test]
    fn folder_library_reads_every_page_and_keeps_compilations_together() {
        let first: Vec<_> = (0..500)
            .map(|i| {
                json!({
                    "id": format!("song-{i}"), "title": "Theme", "artist": format!("Artist {i}"),
                    "path": format!("/music/Soundtracks/Film/{i:03}.flac")
                })
            })
            .collect();
        let (client, server) = folder_server(vec![
            json!(first),
            json!([
                {"id": "last", "title": "Finale", "path": "/music/Soundtracks/Film/CD 2/01.flac"}
            ]),
        ]);
        let library = client.folder_library(&Cancellation::default()).unwrap();
        let folder = library.folder("/music/Soundtracks/Film").unwrap();
        assert_eq!(folder.songs.len(), 500);
        assert_eq!(folder.children, ["/music/Soundtracks/Film/CD 2"]);
        assert_eq!(
            library
                .folder("/music/Soundtracks/Film/CD 2")
                .unwrap()
                .songs[0]
                .id,
            "last"
        );
        server.join().unwrap();
    }

    #[test]
    fn folder_library_explains_how_to_enable_real_paths() {
        let (client, server) = folder_server(vec![json!([
            {"id": "song", "title": "Theme", "path": "Various Artists/Film/01 - Theme.mp3"}
        ])]);
        let error = client
            .folder_library(&Cancellation::default())
            .err()
            .unwrap()
            .to_string();
        assert!(error.contains("Report Real Path"));
        assert!(error.contains("Still"));
        server.join().unwrap();
    }

    #[test]
    fn folder_library_stops_if_server_repeats_a_page() {
        let page: Vec<_> = (0..500)
            .map(|i| {
                json!({
                    "id": format!("song-{i}"), "title": "Theme", "path": format!("/music/{i}.mp3")
                })
            })
            .collect();
        let (client, server) = folder_server(vec![json!(page), json!(page)]);
        assert!(
            client
                .folder_library(&Cancellation::default())
                .err()
                .unwrap()
                .to_string()
                .contains("changed")
        );
        server.join().unwrap();
    }

    #[test]
    fn browses_library_over_http_with_pagination_and_disc_order() {
        use std::io::{BufRead, BufReader, Write};
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let fixtures = vec![
            ("ping", vec![], json!({})),
            (
                "getArtists",
                vec![],
                json!({"artists":{"index":[
                    {"name":"A", "artist":[{"id":"artist-A", "name":"Artist A", "albumCount":2}]},
                    {"name":"B", "artist":[{"id":"artist-B", "name":"Artist B"}]}
                ]}}),
            ),
            (
                "getArtist",
                vec![("id", "artist-A")],
                json!({"artist":{"album":[
                    {"id":"new", "name":"New", "year":2020},
                    {"id":"same-year-z", "name":"Z", "year":2000},
                    {"id":"old", "name":"Old", "year":1990},
                    {"id":"same-year-a", "name":"A", "year":2000}
                ]}}),
            ),
            (
                "getAlbum",
                vec![("id", "new")],
                json!({"album":{"id":"new", "name":"New", "song":[
                    {"id":"s3", "title":"Disc 2", "discNumber":2, "track":1},
                    {"id":"s2", "title":"Track 2", "discNumber":1, "track":2},
                    {"id":"s1", "title":"Track 1", "discNumber":1, "track":1}
                ]}}),
            ),
            (
                "getAlbumList2",
                vec![
                    ("offset", "200"),
                    ("size", "200"),
                    ("type", "byYear"),
                    ("fromYear", "0"),
                    ("toYear", "9999"),
                ],
                json!({"albumList2":{"album":[{"id":"page-2", "name":"Second page"}]}}),
            ),
            (
                "search3",
                vec![
                    ("songOffset", "200"),
                    ("songCount", "200"),
                    ("query", ""),
                    ("artistCount", "0"),
                    ("albumCount", "0"),
                ],
                json!({"searchResult3":{"song":[{"id":"song-id", "title":"Song"}]}}),
            ),
            (
                "getStarred2",
                vec![],
                json!({"starred2":{"song":[
                    {"id":"song /?&", "title":"Liked song", "starred":"2026-10-15T12:00:00Z"}
                ]}}),
            ),
            ("unstar", vec![("id", "song /?&")], json!({})),
            ("getStarred2", vec![], json!({"starred2":{}})),
            ("star", vec![("id", "song /?&")], json!({})),
            (
                "getStarred2",
                vec![],
                json!({"starred2":{"song":[
                    {"id":"song /?&", "title":"Liked song", "starred":"2026-10-15T12:01:00Z"}
                ]}}),
            ),
            (
                "unstar",
                vec![("id", "song /?&")],
                json!({"status":"failed", "error":{"code":50}}),
            ),
            ("getArtists", vec![], json!({})),
            (
                "getAlbumList2",
                vec![("offset", "0")],
                json!({"albumList2":{"album":"invalid"}}),
            ),
        ];
        let server = std::thread::spawn(move || {
            for (method, expected, body) in fixtures {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut reader = BufReader::new(socket.try_clone().unwrap());
                let mut request = String::new();
                reader.read_line(&mut request).unwrap();
                let path = request.split_whitespace().nth(1).unwrap();
                let url = Url::parse(&format!("http://localhost{path}")).unwrap();
                assert_eq!(url.path(), format!("/music/rest/{method}.view"));
                let pairs: std::collections::HashMap<_, _> =
                    url.query_pairs().into_owned().collect();
                assert_eq!(pairs["t"], auth_token("password", &pairs["s"]));
                assert_eq!(pairs["u"], "user");
                for (key, value) in expected {
                    assert_eq!(pairs[key], value);
                }
                loop {
                    let mut header = String::new();
                    reader.read_line(&mut header).unwrap();
                    if header.trim().is_empty() {
                        break;
                    }
                }
                let mut response = json!({"status":"ok", "version":"1.16.1"});
                for (key, value) in body.as_object().unwrap() {
                    response[key] = value.clone();
                }
                let payload = json!({"subsonic-response":response}).to_string();
                write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", payload.len(), payload).unwrap();
            }
        });
        let client =
            Navidrome::new(&format!("http://{address}/music"), "user", "password").unwrap();
        client.ping().unwrap();
        let artists = client.artists().unwrap();
        assert_eq!(artists.len(), 2);
        assert_eq!(artists[1].id, "artist-B");
        assert_eq!(
            client
                .artist_albums("artist-A")
                .unwrap()
                .iter()
                .map(|album| album.id.as_str())
                .collect::<Vec<_>>(),
            ["old", "same-year-a", "same-year-z", "new"]
        );
        let songs = client.album("new").unwrap().song;
        assert_eq!(
            songs
                .iter()
                .map(|song| song.id.as_str())
                .collect::<Vec<_>>(),
            ["s1", "s2", "s3"]
        );
        assert_eq!(client.albums(200).unwrap()[0].id, "page-2");
        assert_eq!(client.songs(200).unwrap()[0].id, "song-id");
        let liked = client.liked_songs().unwrap();
        assert_eq!(liked.len(), 1);
        assert!(liked[0].starred.is_some());
        client.set_liked("song /?&", false).unwrap();
        assert!(client.liked_songs().unwrap().is_empty());
        client.set_liked("song /?&", true).unwrap();
        assert_eq!(client.liked_songs().unwrap()[0].id, "song /?&");
        assert!(
            client
                .set_liked("song /?&", false)
                .unwrap_err()
                .to_string()
                .contains("Insufficient permissions")
        );
        assert!(
            client
                .artists()
                .unwrap_err()
                .to_string()
                .contains("Missing artists")
        );
        assert!(
            client
                .albums(0)
                .unwrap_err()
                .to_string()
                .contains("invalid JSON")
        );
        server.join().unwrap();
    }
}
