//! Owns pending library work and delivers completions without borrowing a view.
use crate::{
    api::Navidrome,
    browser::{LibraryData, LibraryRequest, Page},
    requests::{Cancellation, Requests},
};
use anyhow::Result;
use gpui::{Context, EventEmitter, Task};

impl LibraryRequest {
    pub fn run(&self, client: Navidrome, cancellation: &Cancellation) -> Result<LibraryData> {
        cancellation.check()?;
        let data = match &self.page {
            Page::Artists => LibraryData::Artists(client.artists()?),
            Page::Albums => LibraryData::Albums(client.albums(self.offset)?),
            Page::Songs => LibraryData::Songs(client.songs(self.offset)?),
            Page::Search(query) => LibraryData::Search(client.search(query, self.offset)?),
            Page::Liked => LibraryData::Songs(client.liked_songs()?),
            Page::Artist(artist) => LibraryData::Albums(client.artist_albums(&artist.id)?),
            Page::Album(album) => LibraryData::Album(client.album(&album.id)?),
            Page::Folder(_) => LibraryData::Folders(client.folder_library(cancellation)?),
        };
        cancellation.check()?;
        Ok(data)
    }
}

pub struct LibraryReady;
pub struct LoadContext {
    pub request: LibraryRequest,
    pub like_revision: u64,
    pub playback_generation: u64,
}
pub struct LoadOutcome {
    pub context: LoadContext,
    pub result: anyhow::Result<LibraryData>,
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
        let operation = context.request.clone();
        let (request_id, cancellation) = self.begin(context);
        let task = cx
            .background_executor()
            .spawn(async move { operation.run(client, &cancellation) });
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

    fn finish(&mut self, request_id: u64, result: Result<LibraryData>) -> bool {
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
        failure::{Failure, RetryAction},
        models::{Album, Song},
    };
    use serde_json::json;
    use std::sync::Arc;

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
            request: LibraryRequest { page, offset },
            like_revision: 4,
            playback_generation: 7,
        }
    }

    #[test]
    fn replacing_a_load_cancels_it_and_rejects_late_successes_and_errors() {
        for stale in [
            Ok(LibraryData::Songs(vec![song(0)])),
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

            assert!(loader.finish(current, Ok(LibraryData::Songs(vec![song(1)]))));
            assert!(!loader.is_loading());
            assert!(!loader.finish(old, Err(anyhow::anyhow!("late failure after success"))));
            // A duplicate completion cannot replace an outcome waiting for the UI.
            assert!(!loader.finish(current, Err(anyhow::anyhow!("duplicate"))));
            let outcome = loader.take_outcome().unwrap();
            assert!(matches!(&outcome.context.request.page, Page::Search(query) if query == "new"));
            assert_eq!(outcome.context.like_revision, 4);
            assert_eq!(outcome.context.playback_generation, 7);
            let LibraryData::Songs(songs) = outcome.result.unwrap() else {
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
        assert!(loader.finish(request, Ok(LibraryData::Albums(vec![album(0)]))));
        loader.cancel();
        assert!(loader.take_outcome().is_none());
    }

    #[test]
    fn going_back_cancels_the_load_without_reloading_or_replacing_cached_results() {
        let mut browser = Browser::default();
        browser.navigate(Page::Songs, false);
        browser.apply(LibraryData::Songs(vec![song(0), song(1)]), false);
        browser.set_cursor(1);
        let cached = browser.state().songs.clone();
        browser.navigate(Page::Search("slow".into()), true);
        let mut loader = LibraryLoader::default();
        let (request, cancellation) = loader.begin(context(browser.state().page.clone(), 0));

        assert!(browser.back());
        loader.cancel();
        assert!(cancellation.check().is_err());
        assert!(!loader.finish(request, Ok(LibraryData::Songs(vec![song(99)]))));
        assert!(loader.take_outcome().is_none());
        assert!(browser.state().loaded);
        assert!(Arc::ptr_eq(&cached, &browser.state().songs));
        assert_eq!(browser.state().cursor, 1);
        assert_eq!(browser.state().songs[1].id, "song-1");
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
            let server = Server::new([
                Response::subsonic(method, json!({"status":"failed", "error":{"code":50}}))
                    .query(offset, 200),
                Response::subsonic(method, success).query(offset, 200),
            ]);
            let client = Navidrome::new(&server.url, "user", "pass").unwrap();
            let mut browser = Browser::default();
            browser.navigate(page, false);
            let data = if songs {
                LibraryData::Songs((0..200).map(song).collect())
            } else {
                LibraryData::Albums((0..200).map(album).collect())
            };
            browser.apply(data, false);
            browser.set_cursor(175);
            let request = browser.more_request().unwrap();
            let mut loader = LibraryLoader::default();
            let (id, cancellation) = loader.begin(context(request.page.clone(), request.offset));
            let result = request.run(client.clone(), &cancellation);
            assert!(loader.finish(id, result));
            assert!(!loader.is_loading());
            assert_eq!(browser.entries().len(), 200);
            let outcome = loader.take_outcome().unwrap();
            let failure = Failure::new(
                outcome.result.err().unwrap().to_string(),
                RetryAction::Library(outcome.context.request),
            );
            let Some(RetryAction::Library(retry)) = failure.retry else {
                panic!("library retry");
            };
            let (id, cancellation) = loader.begin(context(retry.page.clone(), retry.offset));
            assert!(loader.is_loading());
            let result = retry.run(client, &cancellation);
            assert!(loader.finish(id, result));
            let outcome = loader.take_outcome().unwrap();
            assert_eq!(outcome.context.request.offset, 200);
            browser.apply(outcome.result.unwrap(), outcome.context.request.offset > 0);
            assert_eq!(browser.entries().len(), 201);
            assert_eq!(browser.state().cursor, 175);
            assert!(!browser.state().more);
            if songs {
                assert_eq!(browser.state().songs[200].id, "last");
            } else {
                assert_eq!(browser.state().albums[200].id, "last");
            }
            server.finish();
        }
    }
}
