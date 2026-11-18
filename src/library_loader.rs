//! Owns pending library work and delivers completions without borrowing a view.
use crate::{
    api::Navidrome,
    browser::{LibraryData, LibraryPage, LibraryRequest, Page},
    requests::{Cancellation, Requests},
};
use anyhow::Result;
use gpui::{Context, EventEmitter, Task};

impl LibraryRequest {
    pub fn run(&self, client: Navidrome, cancellation: &Cancellation) -> Result<LibraryPage> {
        let mut page = self.run_page(&client, self.offset, cancellation)?;
        for index in 1..self.pages {
            if !page.more {
                break;
            }
            let next = self.run_page(
                &client,
                self.offset + index * crate::models::PAGE_SIZE,
                cancellation,
            )?;
            page.append(next);
        }
        Ok(page)
    }

    fn run_page(
        &self,
        client: &Navidrome,
        offset: usize,
        cancellation: &Cancellation,
    ) -> Result<LibraryPage> {
        cancellation.check()?;
        let data = match &self.page {
            Page::Artists => LibraryData::Artists(client.artists(cancellation)?),
            Page::Albums => LibraryData::Albums(client.albums(offset, cancellation)?),
            Page::Songs => LibraryData::Songs(client.songs(offset, cancellation)?),
            Page::Search(query) => {
                LibraryData::Search(client.search(query, offset, cancellation)?)
            }
            Page::Liked => LibraryData::Songs(client.liked_songs(cancellation)?),
            Page::Artist(artist) => {
                LibraryData::Albums(client.artist_albums(&artist.id, cancellation)?)
            }
            Page::Album(album) => LibraryData::Album(client.album(&album.id, cancellation)?),
            Page::Folder(_) => LibraryData::Folders(client.folder_library(cancellation)?),
        };
        cancellation.check()?;
        Ok(data.into())
    }
}

pub struct LibraryReady;
pub struct LoadContext {
    pub request: LibraryRequest,
    pub like_revision: u64,
}
pub struct LoadOutcome {
    pub context: LoadContext,
    pub result: anyhow::Result<LibraryPage>,
}
#[derive(Default)]
pub struct LibraryLoader {
    requests: Requests,
    pending: Option<LoadContext>,
    task: Option<Task<()>>,
    outcome: Option<LoadOutcome>,
}
impl EventEmitter<LibraryReady> for LibraryLoader {}
impl LibraryLoader {
    pub fn is_loading(&self) -> bool {
        self.pending.is_some()
    }
    pub fn has_request_for(&self, page: &Page) -> bool {
        self.pending
            .as_ref()
            .map(|load| &load.request)
            .or_else(|| {
                self.outcome
                    .as_ref()
                    .map(|outcome| &outcome.context.request)
            })
            .is_some_and(|request| request.page.id() == page.id())
    }

    pub fn request_id(&self) -> u64 {
        self.requests.id()
    }
    pub fn cancel(&mut self) {
        self.requests.invalidate();
        self.pending = None;
        self.task = None;
        self.outcome = None;
    }
    pub fn take_outcome(&mut self) -> Option<LoadOutcome> {
        self.outcome.take()
    }
    pub fn start(&mut self, client: Navidrome, context: LoadContext, cx: &mut Context<Self>) {
        self.start_after(client, context, std::time::Duration::ZERO, cx);
    }

    pub fn start_after(
        &mut self,
        client: Navidrome,
        context: LoadContext,
        delay: std::time::Duration,
        cx: &mut Context<Self>,
    ) {
        let operation = context.request.clone();
        let (request_id, cancellation) = self.begin(context);
        let timer = cx.background_executor().timer(delay);
        let task = cx.background_executor().spawn(async move {
            timer.await;
            operation.run(client, &cancellation)
        });
        self.task = Some(cx.spawn(async move |loader, cx| {
            let result = task.await;
            let _ = loader.update(cx, |loader, cx| {
                if loader.finish(request_id, result) {
                    cx.emit(LibraryReady);
                }
            });
        }));
    }

    fn begin(&mut self, context: LoadContext) -> (u64, Cancellation) {
        self.task = None;
        self.outcome = None;
        let request = self.requests.start();
        self.pending = Some(context);
        request
    }

    fn finish(&mut self, request_id: u64, result: Result<LibraryPage>) -> bool {
        if !self.requests.is_current(request_id) {
            return false;
        }
        let Some(context) = self.pending.take() else {
            return false;
        };
        self.task = None;
        self.outcome = Some(LoadOutcome { context, result });
        true
    }
}
impl Drop for LibraryLoader {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        browser::Browser,
        library_controller::{LibraryContext, LibraryController},
        models::{Album, Song},
        session::Session,
    };
    use gpui::TestAppContext;
    use serde_json::json;
    use std::sync::Arc;

    #[test]
    fn cancelling_search_releases_the_worker_before_the_server_finishes() {
        use std::{
            io::{BufRead, BufReader, Read, Write},
            net::TcpListener,
            sync::mpsc,
            thread,
            time::Duration,
        };

        for partial_body in [false, true] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let client = Navidrome::new(
                &format!("http://{}", listener.local_addr().unwrap()),
                "u",
                "p",
            )
            .unwrap();
            let cancellation = Cancellation::default();
            let worker_cancel = cancellation.clone();
            let (done, result) = mpsc::channel();
            let worker = thread::spawn(move || {
                let request = LibraryRequest {
                    page: Page::Search("query".into()),
                    offset: 0,
                    pages: 1,
                };
                done.send(
                    request
                        .run(client, &worker_cancel)
                        .err()
                        .map(|e| e.to_string()),
                )
                .unwrap();
            });
            let mut socket =
                crate::test_http::accept_with_timeout(&listener, Duration::from_secs(5), || false)
                    .unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut reader = BufReader::new(&socket);
            let mut header = String::new();
            loop {
                header.clear();
                assert_ne!(reader.read_line(&mut header).unwrap(), 0);
                if header == "\r\n" {
                    break;
                }
            }
            if partial_body {
                socket
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1024\r\n\r\n{")
                    .unwrap();
            }
            cancellation.cancel();
            assert_eq!(
                result
                    .recv_timeout(Duration::from_secs(2))
                    .expect("search worker still blocked")
                    .as_deref(),
                Some("Loading canceled")
            );
            let mut byte = [0];
            match socket.read(&mut byte) {
                Ok(0) => {}
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
                    ) => {}
                other => panic!("cancelled connection stayed open: {other:?}"),
            }
            worker.join().unwrap();
        }
    }

    fn album(index: usize) -> Album {
        serde_json::from_value(
            json!({"id": format!("album-{index}"), "name": format!("Album {index}")}),
        )
        .unwrap()
    }

    fn song(index: usize) -> Song {
        serde_json::from_value(
            json!({"id": format!("song-{index}"), "title": format!("Track {index}")}),
        )
        .unwrap()
    }

    fn context(page: Page, offset: usize) -> LoadContext {
        LoadContext {
            request: LibraryRequest {
                page,
                offset,
                pages: 1,
            },
            like_revision: 4,
        }
    }

    #[test]
    fn refreshing_search_combines_categories_and_keeps_the_selected_album() {
        use crate::{
            models::SearchResults,
            test_http::{Response, Server},
        };
        let albums = |range: std::ops::Range<usize>| {
            range
                .map(|i| json!({"id":format!("album-{i}"), "name":format!("Album {i}")}))
                .collect::<Vec<_>>()
        };
        let server = Server::new([
            Response::subsonic(
                "search3",
                json!({"searchResult3":{
                    "artist":[{"id":"artist", "name":"Artist"}], "album":albums(0..200),
                    "song":[{"id":"track", "title":"Track"}]
                }}),
            )
            .query("albumOffset", 0)
            .query("songOffset", 0),
            Response::subsonic(
                "search3",
                json!({"searchResult3":{"album":albums(200..215)}}),
            )
            .query("albumOffset", 200)
            .query("songOffset", 200),
        ]);
        let client = Navidrome::new(&server.url, "fixture", "fixture").unwrap();
        let mut browser = Browser::default();
        browser.navigate(Page::Search("query".into()), false);
        browser.apply(
            LibraryData::Search(SearchResults {
                albums: (0..400).map(album).collect(),
                ..Default::default()
            }),
            false,
        );
        browser.set_cursor(205);
        browser.refresh();
        let page = browser
            .request()
            .run(client, &Cancellation::default())
            .unwrap();
        browser.apply(page, false);
        assert_eq!(browser.state().albums().len(), 215);
        assert_eq!(browser.state().artists().len(), 1);
        assert_eq!(browser.state().songs().len(), 1);
        assert_eq!(browser.state().cursor, 206);
        assert!(!browser.state().more);
        assert_eq!(browser.state().albums()[205].id, "album-205");
        server.finish();
    }

    #[test]
    fn refresh_reloads_the_loaded_extent_and_uses_the_last_response_pagination() {
        use crate::test_http::{Response, Server};
        for remaining in [0, 25, 200] {
            let first: Vec<_> = (0..200)
                .map(|i| json!({"id": format!("album-{i}"), "name": format!("Album {i}")}))
                .collect();
            let last: Vec<_> = (200..200 + remaining)
                .map(|i| json!({"id": format!("album-{i}"), "name": format!("Album {i}")}))
                .collect();
            let server = Server::new([
                Response::subsonic("getAlbumList2", json!({"albumList2":{"album":first}}))
                    .query("offset", 0),
                Response::subsonic("getAlbumList2", json!({"albumList2":{"album":last}}))
                    .query("offset", 200),
            ]);
            let client = Navidrome::new(&server.url, "fixture", "fixture").unwrap();
            let mut browser = Browser::default();
            browser.navigate(Page::Albums, false);
            browser.apply(LibraryData::Albums((0..200).map(album).collect()), false);
            browser.apply(LibraryData::Albums((200..400).map(album).collect()), true);
            browser.set_cursor(210);
            browser.refresh();
            assert_eq!(browser.request().pages, 2);
            let page = browser
                .request()
                .run(client, &Cancellation::default())
                .unwrap();
            browser.apply(page, false);
            assert_eq!(browser.state().albums().len(), 200 + remaining);
            assert_eq!(browser.state().cursor, 210.min(199 + remaining));
            assert_eq!(browser.state().more, remaining == 200);
            server.finish();
        }
    }

    #[test]
    fn replacing_a_load_cancels_it_and_rejects_late_successes_and_errors() {
        for stale in [
            Ok(LibraryData::Songs(vec![song(0)]).into()),
            Err(anyhow::anyhow!("late failure")),
        ] {
            let mut loader = LibraryLoader::default();
            let (old, cancellation) = loader.begin(context(Page::Search("old".into()), 0));
            let (current, active) = loader.begin(context(Page::Search("new".into()), 0));
            assert!(cancellation.check().is_err());
            assert!(active.check().is_ok());
            assert!(loader.is_loading());
            assert!(!loader.finish(old, stale));
            assert!(loader.is_loading());
            assert!(loader.take_outcome().is_none());

            assert!(loader.finish(current, Ok(LibraryData::Songs(vec![song(1)]).into())));
            assert!(!loader.is_loading());
            assert!(!loader.finish(old, Err(anyhow::anyhow!("late failure after success"))));
            // A duplicate completion cannot replace an outcome waiting for the UI.
            assert!(!loader.finish(current, Err(anyhow::anyhow!("duplicate"))));
            let outcome = loader.take_outcome().unwrap();
            assert!(matches!(&outcome.context.request.page, Page::Search(query) if query == "new"));
            assert_eq!(outcome.context.like_revision, 4);
            let LibraryData::Songs(songs) = outcome.result.unwrap().data else {
                panic!("songs")
            };
            assert_eq!(songs[0].id, "song-1");
            assert!(loader.take_outcome().is_none());
        }
    }

    #[test]
    fn cancel_rejects_inflight_results_and_discards_unconsumed_outcomes() {
        let mut loader = LibraryLoader::default();
        let (request, cancellation) = loader.begin(context(Page::Songs, 0));
        loader.cancel();
        assert!(cancellation.check().is_err());
        assert!(!loader.is_loading());
        assert!(!loader.finish(request, Err(anyhow::anyhow!("late failure"))));
        assert!(loader.take_outcome().is_none());

        let (request, _) = loader.begin(context(Page::Albums, 0));
        assert!(loader.finish(request, Ok(LibraryData::Albums(vec![album(0)]).into())));
        loader.cancel();
        assert!(loader.take_outcome().is_none());
    }

    #[test]
    fn going_back_cancels_the_load_without_reloading_or_replacing_cached_results() {
        let app = TestAppContext::single();
        app.update(|cx| {
            let mut library = LibraryController::new(cx);
            library.navigate(Page::Songs, false, cx);
            library.replace_songs(vec![song(0), song(1)]);
            library.set_cursor(1);
            let cached = library.browser().state().song_collection().unwrap().clone();
            let search = Page::Search("slow".into());
            library.navigate(search.clone(), true, cx);
            let loader = library.loader().clone();
            let (request, cancellation) =
                loader.update(cx, |loader, _| loader.begin(context(search, 0)));
            assert!(library.is_loading(cx));

            assert!(library.back(cx));
            assert!(cancellation.check().is_err());
            assert!(!library.is_loading(cx));
            loader.update(cx, |loader, _| {
                assert!(!loader.finish(request, Ok(LibraryData::Songs(vec![song(99)]).into())));
                assert!(loader.take_outcome().is_none());
            });
            let state = library.browser().state();
            assert!(state.loaded());
            assert!(Arc::ptr_eq(&cached, state.song_collection().unwrap()));
            assert_eq!(state.songs()[state.cursor].id, "song-1");
        });
    }

    #[test]
    fn dropping_the_loader_cancels_its_worker() {
        let mut loader = LibraryLoader::default();
        let (_, cancellation) = loader.begin(context(Page::Songs, 0));
        drop(loader);
        assert!(cancellation.check().is_err());
    }
    #[test]
    fn retry_fetches_the_failed_page_over_http_and_keeps_existing_results() {
        use crate::test_http::{Response, Server};
        for page in [Page::Songs, Page::Albums] {
            let songs = matches!(page, Page::Songs);
            let method = if songs { "search3" } else { "getAlbumList2" };
            let offset = if songs { "songOffset" } else { "offset" };
            let success = if songs {
                json!({"searchResult3":{"song":[{"id":"last", "title":"Track 200"}]}})
            } else {
                json!({"albumList2":{"album":[{"id":"last", "name":"Альбом 200"}]}})
            };
            let label_field = if songs { "title" } else { "name" };
            let records: Vec<_> = (0..200)
                .map(|i| json!({"id": format!("id-{i}"), (label_field): format!("Item {i}")}))
                .collect();
            let first_page = if songs {
                json!({"searchResult3": {"song": records}})
            } else {
                json!({"albumList2": {"album": records}})
            };
            let server = Server::new([
                Response::subsonic(method, first_page).query(offset, 0),
                Response::subsonic(method, json!({"status":"failed", "error":{"code":50}}))
                    .query(offset, 200),
                Response::subsonic(method, success).query(offset, 200),
            ]);
            let client = Navidrome::new(&server.url, "user", "pass").unwrap();
            let context = || LibraryContext {
                session: Session::Connected(client.clone()),
                like_revision: 0,
            };
            let mut app = TestAppContext::single();
            let mut library = app.update(LibraryController::new);
            app.update(|cx| {
                library.navigate(page, false, cx);
                library.load_current(context(), cx);
            });
            let deliver = |library: &mut LibraryController, app: &mut TestAppContext| {
                app.run_until_parked();
                app.update(|cx| {
                    let outcome = library
                        .loader()
                        .update(cx, |loader, _| loader.take_outcome())
                        .expect("expected HTTP completion");
                    library.accept(outcome);
                });
            };
            deliver(&mut library, &mut app);
            app.update(|cx| {
                library.set_cursor(175);
                library.load_more(context(), cx);
            });
            deliver(&mut library, &mut app);
            assert!(library.failure().is_some());
            assert_eq!(library.browser().entry_count(), 200);
            app.update(|cx| {
                library.retry(context(), cx);
                assert!(library.is_loading(cx));
            });
            deliver(&mut library, &mut app);
            assert!(library.failure().is_none());
            let browser = library.browser();
            assert_eq!(browser.entry_count(), 201);
            let state = browser.state();
            assert_eq!(state.cursor, 175);
            assert!(!state.more);
            if songs {
                assert_eq!(state.songs()[state.cursor].id, "id-175");
                assert_eq!(state.songs()[200].id, "last");
            } else {
                assert_eq!(state.albums()[state.cursor].id, "id-175");
                assert_eq!(state.albums()[200].id, "last");
            }
            server.finish();
        }
    }
}
