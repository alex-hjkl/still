//! Runs optimistic like writes and rejects completions from an old connection.
use crate::{
    failure::{Failure, Failures, Operation, RetryAction},
    likes::Likes,
    models::Song,
    session::Session,
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
    generation: u64,
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
        self.generation = self.generation.wrapping_add(1);
        self.tasks.clear();
        self.state = Likes::default();
        self.dismiss_failures();
    }

    pub fn accept_server(&mut self, songs: Vec<Song>, revision: u64) -> Vec<Song> {
        self.state.accept_server(songs, revision)
    }

    pub fn toggle(&mut self, session: &Session, song: Song, cx: &mut Context<Self>) {
        if let Some(liked) = self.begin(session, &song, None) {
            self.send(session, song, liked, cx);
        }
    }

    pub fn set(&mut self, session: &Session, song: Song, liked: bool, cx: &mut Context<Self>) {
        if let Some(liked) = self.begin(session, &song, Some(liked)) {
            self.send(session, song, liked, cx);
        }
    }

    pub fn retry(&mut self, session: &Session, id: &str, cx: &mut Context<Self>) {
        if !session.is_active() {
            return;
        }
        let action = self
            .failures
            .take(&Operation::Like(id.to_owned()))
            .and_then(|failure| failure.retry);
        if let Some(RetryAction::Like { song, liked }) = action {
            self.set(session, song, liked, cx);
        }
    }

    fn begin(&mut self, session: &Session, song: &Song, liked: Option<bool>) -> Option<bool> {
        if !session.is_active() {
            return None;
        }
        match liked {
            Some(liked) => self.state.begin_set(song, liked),
            None => self.state.begin(song),
        }
    }

    fn send(&mut self, session: &Session, song: Song, liked: bool, cx: &mut Context<Self>) {
        self.failures.clear(&Operation::Like(song.id.clone()));
        let client = match session {
            Session::Disconnected => return,
            Session::Demo => {
                self.finish(self.generation, &song, liked, Ok(()));
                cx.emit(LikesEvent::Saved);
                return;
            }
            Session::Connected(client) => client.clone(),
        };
        let generation = self.generation;
        let id = song.id.clone();
        let worker_id = id.clone();
        let worker = cx
            .background_executor()
            .spawn(async move { client.set_liked(&worker_id, liked) });
        let task = cx.spawn(async move |this, cx| {
            let result = worker.await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                let saved = this.finish(generation, &song, liked, result);
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

    fn finish(&mut self, generation: u64, song: &Song, liked: bool, result: Result<()>) -> bool {
        if self.generation != generation {
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
    fn disconnected_actions_do_not_become_local_demo_likes() {
        let mut controller = LikesController::default();
        let song: Song = serde_json::from_value(json!({"id":"song", "title":"Song"})).unwrap();
        let revision = controller.state().revision();
        assert_eq!(controller.begin(&Session::Disconnected, &song, None), None);
        assert_eq!(
            controller.begin(&Session::Disconnected, &song, Some(true)),
            None
        );
        assert_eq!(controller.state().revision(), revision);
        assert!(!controller.state().is_pending(&song.id));
        assert!(!controller.state().is_liked(&song));
        assert_eq!(controller.begin(&Session::Demo, &song, None), Some(true));
        assert!(controller.state().is_pending(&song.id));
        assert!(controller.finish(controller.generation, &song, true, Ok(())));
        assert!(controller.state().is_liked(&song));
    }

    #[test]
    fn a_previous_session_cannot_complete_or_fail_a_new_like_for_the_same_song() {
        let mut controller = LikesController::default();
        let song: Song = serde_json::from_value(json!({"id":"same", "title":"Song"})).unwrap();
        controller.state.begin(&song);
        let old = controller.generation;
        controller.reset();
        controller.state.begin(&song);
        assert!(!controller.finish(old, &song, true, Ok(())));
        assert!(!controller.finish(old, &song, true, Err(anyhow::anyhow!("old error"))));
        assert!(controller.state().is_pending(&song.id));
        assert_eq!(controller.failures().iter().count(), 0);
        assert!(controller.finish(controller.generation, &song, true, Ok(())));
        assert!(controller.state().is_liked(&song));
        assert!(!controller.state().is_pending(&song.id));
    }

    #[test]
    fn a_failed_write_rolls_back_the_like_and_retains_the_requested_value() {
        let mut controller = LikesController::default();
        let song: Song = serde_json::from_value(json!({"id":"song", "title":"Song"})).unwrap();
        controller.state.begin(&song);
        controller.finish(
            controller.generation,
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
