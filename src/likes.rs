use crate::api::Song;
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub struct Likes {
    saved: HashMap<String, (Song, bool)>,
    pending: HashMap<String, bool>,
    server_ids: Option<HashSet<String>>,
    revision: u64,
}

impl Likes {
    pub fn is_liked(&self, song: &Song) -> bool {
        self.pending
            .get(&song.id)
            .copied()
            .or_else(|| self.saved.get(&song.id).map(|(_, liked)| *liked))
            .or_else(|| self.server_ids.as_ref().map(|ids| ids.contains(&song.id)))
            .unwrap_or(song.starred.is_some())
    }

    pub fn is_pending(&self, id: &str) -> bool {
        self.pending.contains_key(id)
    }

    pub fn begin(&mut self, song: &Song) -> Option<bool> {
        self.begin_set(song, !self.is_liked(song))
    }

    pub fn begin_set(&mut self, song: &Song, liked: bool) -> Option<bool> {
        if self.is_pending(&song.id) {
            return None;
        }
        self.pending.insert(song.id.clone(), liked);
        self.revision += 1;
        Some(liked)
    }

    pub fn finish(&mut self, song: &Song, success: bool) {
        if let Some(liked) = self.pending.remove(&song.id) {
            self.revision += 1;
            if success {
                self.saved.insert(song.id.clone(), (song.clone(), liked));
            }
        }
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn accept_server(&mut self, songs: Vec<Song>, revision: u64) -> Vec<Song> {
        self.server_ids = Some(songs.iter().map(|song| song.id.clone()).collect());
        if revision == self.revision && self.pending.is_empty() {
            self.saved.clear();
        }
        self.merge_liked(songs)
    }

    // A request started before star/unstar finished may return stale metadata.
    // Keep confirmed local changes consistent across lists, folders and the queue.
    pub fn merge_liked(&self, mut songs: Vec<Song>) -> Vec<Song> {
        songs.retain(|song| self.saved.get(&song.id).is_none_or(|(_, liked)| *liked));
        let mut seen: HashSet<_> = songs.iter().map(|song| song.id.clone()).collect();
        songs.extend(
            self.saved
                .values()
                .filter(|(song, liked)| *liked && seen.insert(song.id.clone()))
                .map(|(song, _)| song.clone()),
        );
        songs.sort_by_cached_key(|song| {
            (
                song.title.to_lowercase(),
                song.artist.to_lowercase(),
                song.id.clone(),
            )
        });
        songs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn song(starred: bool) -> Song {
        serde_json::from_value(json!({"id":"one", "title":"Theme", "starred": if starred {Some("2026-10-15T12:00:00Z")} else {None}})).unwrap()
    }

    #[test]
    fn failed_changes_restore_the_previous_state_and_can_be_retried() {
        for starred in [true, false] {
            let song = song(starred);
            let mut likes = Likes::default();
            assert_eq!(likes.begin(&song), Some(!starred));
            assert_eq!(likes.is_liked(&song), !starred);
            assert_eq!(likes.begin(&song), None);
            likes.finish(&song, false);
            assert_eq!(likes.is_liked(&song), starred);
            assert!(!likes.is_pending(&song.id));
            assert_eq!(likes.begin(&song), Some(!starred));
        }
    }

    #[test]
    fn confirmed_likes_survive_stale_metadata_and_inflight_list_responses() {
        let song = song(false);
        let mut likes = Likes::default();
        likes.begin(&song);
        likes.finish(&song, true);
        assert!(likes.is_liked(&song));
        assert_eq!(likes.merge_liked(vec![]).len(), 1);
        assert_eq!(likes.merge_liked(vec![song.clone()]).len(), 1);
        assert_eq!(likes.begin(&song), Some(false));
        likes.finish(&song, true);
        assert!(!likes.is_liked(&self::song(true)));
        assert!(likes.merge_liked(vec![song]).is_empty());
    }

    #[test]
    fn failed_unlike_keeps_a_previously_confirmed_like() {
        let song = song(false);
        let mut likes = Likes::default();
        likes.begin(&song);
        likes.finish(&song, true);
        likes.begin(&song);
        likes.finish(&song, false);
        assert!(likes.is_liked(&song));
        assert_eq!(likes.merge_liked(vec![]).len(), 1);
    }

    #[test]
    fn a_fresh_server_list_updates_stale_folder_and_queue_metadata() {
        let mut likes = Likes::default();
        let revision = likes.revision();
        let track = song(false);
        likes.accept_server(vec![track.clone()], revision);
        assert!(likes.is_liked(&track));
        likes.accept_server(vec![], likes.revision());
        assert!(!likes.is_liked(&song(true)));
    }

    #[test]
    fn an_old_server_list_cannot_undo_a_like_that_finished_during_loading() {
        let mut likes = Likes::default();
        let revision = likes.revision();
        let track = song(false);
        likes.begin(&track);
        likes.finish(&track, true);
        assert_eq!(likes.accept_server(vec![], revision).len(), 1);
        assert!(likes.is_liked(&track));
        // A later refresh can pick up an unlike made in another client.
        assert!(likes.accept_server(vec![], likes.revision()).is_empty());
        assert!(!likes.is_liked(&track));
    }

    #[test]
    fn retry_keeps_the_requested_state_if_another_client_changed_the_like() {
        let track = song(true);
        let mut likes = Likes::default();
        assert_eq!(likes.begin(&track), Some(false));
        likes.finish(&track, false);
        // Another client removes the like before the failed unstar is retried.
        likes.accept_server(vec![], likes.revision());
        assert_eq!(likes.begin_set(&track, false), Some(false));
        likes.finish(&track, true);
        assert!(!likes.is_liked(&track));
    }
}
