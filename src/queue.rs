use crate::api::Song;
use rand::{Rng, seq::SliceRandom};
use std::sync::Arc;

#[derive(Default)]
pub struct PlayQueue {
    songs: Arc<Vec<Song>>,
    order: Vec<usize>,
    position: Option<usize>,
    shuffled: bool,
}

impl PlayQueue {
    pub fn with_shuffle(shuffled: bool) -> Self {
        Self {
            shuffled,
            ..Self::default()
        }
    }

    pub fn current(&self) -> Option<&Song> {
        self.songs.get(*self.order.get(self.position?)?)
    }

    pub fn is_shuffled(&self) -> bool {
        self.shuffled
    }

    #[cfg(feature = "ui-tests")]
    pub fn test_songs(&self) -> &[Song] {
        &self.songs
    }

    pub fn start(&mut self, songs: Arc<Vec<Song>>, selected: usize) -> bool {
        self.start_with_rng(songs, selected, &mut rand::rng())
    }

    fn start_with_rng(
        &mut self,
        songs: Arc<Vec<Song>>,
        selected: usize,
        rng: &mut impl Rng,
    ) -> bool {
        if selected >= songs.len() {
            return false;
        }
        self.songs = songs;
        self.order = (0..self.songs.len()).collect();
        self.position = Some(selected);
        if self.shuffled {
            self.shuffle_remaining(rng);
        }
        true
    }

    fn shuffle_remaining(&mut self, rng: &mut impl Rng) {
        if let Some(position) = self.position {
            self.order.swap(0, position);
            self.order[1..].shuffle(rng);
            self.position = Some(0);
        }
    }

    pub fn toggle_shuffle(&mut self) {
        self.toggle_with_rng(&mut rand::rng());
    }

    fn toggle_with_rng(&mut self, rng: &mut impl Rng) {
        self.shuffled = !self.shuffled;
        if self.shuffled {
            self.shuffle_remaining(rng);
        } else {
            // Restore the exact source occurrence, even if a queue repeats an ID.
            let current = self.position.map(|position| self.order[position]);
            self.order = (0..self.songs.len()).collect();
            self.position = current;
        }
    }

    pub fn advance(&mut self) -> bool {
        if let Some(position) = self.position
            && position + 1 < self.order.len()
        {
            self.position = Some(position + 1);
            true
        } else {
            false
        }
    }

    pub fn previous(&mut self) -> bool {
        if let Some(position) = self.position
            && position > 0
        {
            self.position = Some(position - 1);
            true
        } else {
            false
        }
    }

    pub fn restart(&mut self) {
        if !self.order.is_empty() {
            self.position = Some(0);
        }
    }

    pub fn clear(&mut self) {
        self.songs = Arc::new(Vec::new());
        self.order.clear();
        self.position = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{SeedableRng, rngs::StdRng};
    use std::collections::HashSet;

    fn songs() -> Arc<Vec<Song>> {
        Arc::new(
            (0..20)
                .map(|i| {
                    serde_json::from_value(serde_json::json!({
                        "id": format!("song-{i}"), "title": format!("Track {i}")
                    }))
                    .unwrap()
                })
                .collect(),
        )
    }

    #[test]
    fn shuffle_keeps_current_and_visits_each_other_song_once() {
        let mut queue = PlayQueue::default();
        let source = songs();
        queue.start(source.clone(), 7);
        queue.toggle_with_rng(&mut StdRng::seed_from_u64(42));
        assert_eq!(queue.current().unwrap().id, "song-7");
        let mut visited = HashSet::new();
        loop {
            assert!(visited.insert(queue.current().unwrap().id.clone()));
            if !queue.advance() {
                break;
            }
        }
        assert_eq!(visited.len(), source.len());
        assert_eq!(source[7].id, "song-7");
        assert!(!queue.advance());
    }

    #[test]
    fn previous_retraces_shuffle_and_disabling_restores_original_order() {
        let mut queue = PlayQueue::default();
        queue.start(songs(), 7);
        queue.toggle_with_rng(&mut StdRng::seed_from_u64(42));
        queue.advance();
        let next = queue.current().unwrap().id.clone();
        assert!(queue.previous());
        assert_eq!(queue.current().unwrap().id, "song-7");
        queue.advance();
        assert_eq!(queue.current().unwrap().id, next);
        queue.toggle_shuffle();
        let index = queue.position.unwrap();
        assert_eq!(queue.current().unwrap().id, next);
        assert_eq!(queue.order, (0..20).collect::<Vec<_>>());
        assert_eq!(queue.advance(), index < 19);
        assert_eq!(
            queue.current().unwrap().id,
            format!("song-{}", (index + 1).min(19))
        );
    }

    #[test]
    fn setting_shuffle_before_playback_applies_to_each_new_queue() {
        let mut queue = PlayQueue::default();
        queue.toggle_shuffle();
        assert!(queue.current().is_none());
        for selected in [3, 15] {
            assert!(queue.start_with_rng(songs(), selected, &mut StdRng::seed_from_u64(42)));
            assert_eq!(queue.current().unwrap().id, format!("song-{selected}"));
            assert_eq!(queue.position, Some(0));
            assert_eq!(
                queue.order.iter().copied().collect::<HashSet<_>>().len(),
                20
            );
        }
        queue.clear();
        assert!(queue.is_shuffled());
        assert!(queue.current().is_none());
        assert!(!queue.advance());
        assert!(!queue.previous());
    }

    #[test]
    fn disabling_shuffle_preserves_duplicate_occurrences_and_single_song_queues() {
        let song: Song =
            serde_json::from_value(serde_json::json!({"id":"same", "title":"A"})).unwrap();
        let mut other = song.clone();
        other.title = "B".into();
        let mut queue = PlayQueue::default();
        queue.start(Arc::new(vec![song.clone(), other]), 1);
        queue.toggle_shuffle();
        queue.toggle_shuffle();
        assert_eq!(queue.current().unwrap().title, "B");
        assert_eq!(queue.position, Some(1));
        queue.start(Arc::new(vec![song]), 0);
        queue.toggle_shuffle();
        assert!(!queue.advance());
        assert!(!queue.previous());
        assert!(!queue.start(Arc::new(Vec::new()), 0));
        assert_eq!(queue.current().unwrap().id, "same");
    }
}
