//! Resolves a collection into a queue, owning cancellation, errors and retry state.
use crate::{
    api::Navidrome,
    collection::Collection,
    failure::{Failure, RetryAction},
    models::Song,
    requests::Requests,
};
use anyhow::Result;
use gpui::{Context, EventEmitter, Task};
use std::sync::Arc;

#[derive(Debug, PartialEq, Eq)]
pub enum CollectionEvent {
    Changed,
    Ready,
}

#[derive(Default)]
pub struct CollectionController {
    requests: Requests,
    pending: Option<Collection>,
    ready: Option<Arc<Vec<Song>>>,
    failure: Option<Failure>,
    task: Option<Task<()>>,
}

impl EventEmitter<CollectionEvent> for CollectionController {}

impl CollectionController {
    pub fn pending(&self) -> Option<&Collection> {
        self.pending.as_ref()
    }

    pub fn failure(&self) -> Option<&Failure> {
        self.failure.as_ref()
    }

    pub fn take_ready(&mut self) -> Option<Arc<Vec<Song>>> {
        self.ready.take()
    }

    pub fn dismiss_failure(&mut self) {
        self.failure = None;
    }

    pub fn cancel(&mut self) {
        self.cancel_pending();
        self.failure = None;
    }

    pub fn cancel_pending(&mut self) {
        self.requests.invalidate();
        self.pending = None;
        self.ready = None;
        self.task = None;
    }

    pub fn play(&mut self, client: Navidrome, collection: Collection, cx: &mut Context<Self>) {
        self.task = None;
        let (request_id, cancellation) = self.requests.start();
        self.pending = Some(collection.clone());
        self.ready = None;
        self.failure = None;
        let worker = cx
            .background_executor()
            .spawn(async move { collection.songs(&client, &cancellation) });
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = worker.await;
            let _ = this.update(cx, |this, cx| {
                if let Some(event) = this.finish(request_id, result) {
                    cx.emit(event);
                }
            });
        }));
        cx.emit(CollectionEvent::Changed);
    }

    pub fn retry(&mut self, client: Navidrome, cx: &mut Context<Self>) {
        if let Some(RetryAction::Collection(collection)) = self
            .failure
            .as_ref()
            .and_then(|failure| failure.retry.clone())
        {
            self.play(client, collection, cx);
        }
    }

    fn finish(&mut self, request_id: u64, result: Result<Vec<Song>>) -> Option<CollectionEvent> {
        if !self.requests.is_current(request_id) {
            return None;
        }
        let collection = self.pending.take()?;
        self.task = None;
        match result {
            Ok(songs) if !songs.is_empty() => {
                self.ready = Some(Arc::new(songs));
                Some(CollectionEvent::Ready)
            }
            Ok(_) => {
                self.failure = Some(format!("No songs in {}", collection.name()).into());
                Some(CollectionEvent::Changed)
            }
            Err(error) => {
                self.failure = Some(Failure::new(
                    format!("Could not play {}: {error:#}", collection.name()),
                    RetryAction::Collection(collection),
                ));
                Some(CollectionEvent::Changed)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn album() -> Collection {
        Collection::Album(serde_json::from_value(json!({"id":"album", "name":"Album"})).unwrap())
    }

    #[test]
    fn canceled_or_replaced_results_cannot_replace_a_new_queue_or_report_errors() {
        let mut controller = CollectionController::default();
        let (old, _) = controller.requests.start();
        controller.pending = Some(album());
        controller.cancel();
        assert!(
            controller
                .finish(old, Err(anyhow::anyhow!("late")))
                .is_none()
        );
        assert!(controller.failure().is_none());

        let (current, _) = controller.requests.start();
        controller.pending = Some(album());
        assert!(controller.finish(old, Ok(Vec::new())).is_none());
        assert!(controller.pending().is_some());
        let song = serde_json::from_value(json!({"id":"song", "title":"Song"})).unwrap();
        assert_eq!(
            controller.finish(current, Ok(vec![song])),
            Some(CollectionEvent::Ready)
        );
        assert!(
            controller
                .finish(old, Err(anyhow::anyhow!("late failure after success")))
                .is_none()
        );
        assert!(controller.finish(current, Ok(Vec::new())).is_none());
        assert!(
            controller
                .finish(current, Err(anyhow::anyhow!("duplicate failure")))
                .is_none()
        );
        assert!(controller.failure().is_none());
        assert_eq!(controller.take_ready().unwrap()[0].id, "song");
        assert!(controller.pending().is_none());
    }

    #[test]
    fn empty_collection_reports_a_change_without_starting_playback() {
        let mut controller = CollectionController::default();
        let (request, _) = controller.requests.start();
        controller.pending = Some(album());
        assert_eq!(
            controller.finish(request, Ok(Vec::new())),
            Some(CollectionEvent::Changed)
        );
        assert!(controller.take_ready().is_none());
        assert!(controller.pending().is_none());
        let failure = controller.failure().unwrap();
        assert!(failure.message.contains("No songs"));
        assert!(failure.retry.is_none());
        assert!(controller.finish(request, Ok(Vec::new())).is_none());
    }

    #[test]
    fn a_failed_collection_keeps_its_retry_target() {
        let mut controller = CollectionController::default();
        let (request, _) = controller.requests.start();
        controller.pending = Some(album());
        assert_eq!(
            controller.finish(request, Err(anyhow::anyhow!("fixture failure"))),
            Some(CollectionEvent::Changed)
        );
        assert!(controller.pending().is_none());
        // Observing a transport change cancels work while keeping Retry useful.
        controller.cancel_pending();
        assert!(matches!(
            controller
                .failure()
                .and_then(|failure| failure.retry.as_ref()),
            Some(RetryAction::Collection(_))
        ));
        controller.dismiss_failure();
        assert!(controller.failure().is_none());

        let (request, _) = controller.requests.start();
        controller.pending = Some(album());
        controller.finish(request, Err(anyhow::anyhow!("fixture failure")));
        // An explicit new playback request dismisses the previous collection error.
        controller.cancel();
        assert!(controller.failure().is_none());
    }
}
