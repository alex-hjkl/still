//! Runs optimistic like writes and rejects completions from an old connection.
use crate::{
    api::Navidrome,
    failure::{Failure, Failures, Operation, RetryAction},
    likes::Likes,
    models::Song,
};
use anyhow::Result;
use gpui::{Context, EventEmitter, Task};
use std::collections::HashMap;

pub enum LikesEvent {
    Changed,
    Saved,
}

#[derive(Default)]
pub struct LikesController {
    state: Likes,
    session: u64,
    tasks: HashMap<String, Task<()>>,
    failures: Failures,
}

impl EventEmitter<LikesEvent> for LikesController {}

impl LikesController {
    pub fn state(&self) -> &Likes {
        &self.state
    }

    pub fn failures(&self) -> &Failures {
        &self.failures
    }

    pub fn dismiss_failures(&mut self) {
        self.failures = Failures::default();
    }

    pub fn reset(&mut self) {
        self.session = self.session.wrapping_add(1);
        self.tasks.clear();
        self.state = Likes::default();
        self.dismiss_failures();
    }

    pub fn accept_server(&mut self, songs: Vec<Song>, revision: u64) -> Vec<Song> {
        self.state.accept_server(songs, revision)
    }

    // A missing client is the local demo; callers only enable actions in a demo or a session.
    pub fn toggle(&mut self, client: Option<Navidrome>, song: Song, cx: &mut Context<Self>) {
        if let Some(liked) = self.state.begin(&song) {
            self.send(client, song, liked, cx);
        }
    }

    pub fn set(
        &mut self,
        client: Option<Navidrome>,
        song: Song,
        liked: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(liked) = self.state.begin_set(&song, liked) {
            self.send(client, song, liked, cx);
        }
    }

    pub fn retry(&mut self, client: Option<Navidrome>, id: &str, cx: &mut Context<Self>) {
        let action = self
            .failures
            .take(&Operation::Like(id.to_owned()))
            .and_then(|failure| failure.retry);
        if let Some(RetryAction::Like { song, liked }) = action {
            self.set(client, song, liked, cx);
        }
    }

    fn send(&mut self, client: Option<Navidrome>, song: Song, liked: bool, cx: &mut Context<Self>) {
        self.failures.clear(&Operation::Like(song.id.clone()));
        let Some(client) = client else {
            self.finish(self.session, &song, liked, Ok(()));
            cx.emit(LikesEvent::Saved);
            return;
        };
        let session = self.session;
        let id = song.id.clone();
        let worker_id = id.clone();
        let worker = cx
            .background_executor()
            .spawn(async move { client.set_liked(&worker_id, liked) });
        let task = cx.spawn(async move |this, cx| {
            let result = worker.await;
            let _ = this.update(cx, |this, cx| {
                if this.session != session {
                    return;
                }
                let saved = this.finish(session, &song, liked, result);
                this.tasks.remove(&song.id);
                cx.emit(if saved {
                    LikesEvent::Saved
                } else {
                    LikesEvent::Changed
                });
            });
        });
        self.tasks.insert(id, task);
        cx.emit(LikesEvent::Changed);
    }

    fn finish(&mut self, session: u64, song: &Song, liked: bool, result: Result<()>) -> bool {
        if self.session != session {
            return false;
        }
        self.state.finish(song, result.is_ok());
        match result {
            Ok(()) => {
                self.failures.clear(&Operation::Like(song.id.clone()));
                true
            }
            Err(error) => {
                self.failures.set(
                    Operation::Like(song.id.clone()),
                    Failure::new(
                        format!("Could not save the like for “{}”: {error:#}", song.title),
                        RetryAction::Like {
                            song: song.clone(),
                            liked,
                        },
                    ),
                );
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_previous_session_cannot_complete_or_fail_a_new_like_for_the_same_song() {
        let mut controller = LikesController::default();
        let song: Song = serde_json::from_value(json!({"id":"same", "title":"Song"})).unwrap();
        controller.state.begin(&song);
        let old = controller.session;
        controller.reset();
        controller.state.begin(&song);
        assert!(!controller.finish(old, &song, true, Ok(())));
        assert!(!controller.finish(old, &song, true, Err(anyhow::anyhow!("old error"))));
        assert!(controller.state().is_pending(&song.id));
        assert_eq!(controller.failures().iter().count(), 0);
        assert!(controller.finish(controller.session, &song, true, Ok(())));
        assert!(controller.state().is_liked(&song));
        assert!(!controller.state().is_pending(&song.id));
    }

    #[test]
    fn a_failed_write_rolls_back_the_like_and_retains_the_requested_value() {
        let mut controller = LikesController::default();
        let song: Song = serde_json::from_value(json!({"id":"song", "title":"Song"})).unwrap();
        controller.state.begin(&song);
        controller.finish(
            controller.session,
            &song,
            true,
            Err(anyhow::anyhow!("fixture failure")),
        );
        assert!(!controller.state().is_liked(&song));
        assert!(!controller.state().is_pending(&song.id));
        assert!(matches!(
            controller.failures().iter().next().unwrap().1.retry,
            Some(RetryAction::Like { liked: true, .. })
        ));
    }
}
