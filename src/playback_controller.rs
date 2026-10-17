//! Playback policy and native media integration, independent of GPUI views.
use crate::{
    api::{Navidrome, Song},
    failure::{Failure, RetryAction},
    media::{MediaCommand, SystemMedia, seek_position},
    player::{Playback, PlaybackStatus, Player},
    queue::PlayQueue,
};
use std::sync::Arc;

pub struct PlaybackController {
    player: Player,
    queue: PlayQueue,
    state: Playback,
    generation: u64,
    media: SystemMedia,
    volume: f32,
}

#[derive(Default)]
pub struct PlaybackUpdate {
    pub changed: bool,
    pub track_changed: bool,
    pub failure: Option<Failure>,
}

impl PlaybackController {
    pub fn new(volume: f32, shuffle: bool) -> Self {
        Self {
            player: Player::new(volume),
            queue: PlayQueue::with_shuffle(shuffle),
            state: Playback::default(),
            generation: 0,
            media: SystemMedia::new(),
            volume,
        }
    }
    pub fn queue(&self) -> &PlayQueue {
        &self.queue
    }
    pub fn state(&self) -> &Playback {
        &self.state
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn play(&mut self, client: &Navidrome, songs: Arc<Vec<Song>>, index: usize) -> bool {
        self.queue.start(songs, index) && self.restart(client)
    }
    pub fn restart(&mut self, client: &Navidrome) -> bool {
        let Some(song) = self.queue.current() else {
            return false;
        };
        self.generation = self.generation.wrapping_add(1);
        if !self.player.available() {
            self.player = Player::new(self.volume);
        }
        self.player
            .play(self.generation, client.stream_url(&song.id));
        self.state = Playback {
            generation: self.generation,
            status: PlaybackStatus::Loading,
            ..Playback::default()
        };
        self.sync_media();
        true
    }
    pub fn next(&mut self, client: &Navidrome) -> bool {
        self.queue.advance() && self.restart(client)
    }
    pub fn previous(&mut self, client: &Navidrome) -> bool {
        if self.state.position.as_secs() > 3
            && !self.state.is_finished()
            && self.state.error().is_none()
        {
            self.seek(0);
            false
        } else if self.queue.previous() {
            self.restart(client)
        } else if self.queue.current().is_some()
            && (self.state.is_finished() || self.state.error().is_some())
        {
            self.queue.restart();
            self.restart(client)
        } else {
            false
        }
    }
    pub fn toggle(&mut self, client: &Navidrome) -> bool {
        if self.state.needs_restart() {
            self.restart(client)
        } else {
            if self.queue.current().is_some() {
                self.player.toggle();
            }
            false
        }
    }
    pub fn toggle_shuffle(&mut self) -> bool {
        self.queue.toggle_shuffle();
        self.queue.is_shuffled()
    }
    pub fn volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0., 1.);
        self.player.volume(self.volume);
    }
    pub fn seek(&self, seconds: u64) {
        if let Some(song) = self.queue.current() {
            self.player.seek(seconds, song.duration);
        }
    }
    pub fn seek_relative(&self, delta: i64) {
        self.skip(delta as f64);
    }
    fn skip(&self, seconds: f64) {
        if let Some(song) = self.queue.current() {
            self.player.seek_relative(seconds, song.duration);
        }
    }
    fn stop(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.player.stop();
        self.state = Playback {
            generation: self.generation,
            ..Playback::default()
        };
        self.sync_media();
    }
    pub fn disconnect(&mut self) {
        self.stop();
        self.queue.clear();
        self.media.clear();
    }
    pub fn clear_media(&mut self) {
        self.media.clear();
    }
    fn sync_media(&mut self) {
        self.media.sync(self.queue.current(), &self.state);
    }

    fn command(&mut self, command: MediaCommand, client: &Navidrome) {
        let Some(song) = self.queue.current() else {
            return;
        };
        let duration = song.duration;
        crate::diagnostics::event(&format!("System media command: {command:?}"));
        match command {
            MediaCommand::Play => {
                if self.state.needs_restart() {
                    self.restart(client);
                } else {
                    self.player.set_paused(false);
                }
            }
            MediaCommand::Pause => {
                if !self.state.needs_restart() {
                    self.player.set_paused(true);
                }
            }
            MediaCommand::Toggle => {
                self.toggle(client);
            }
            MediaCommand::Next => {
                self.next(client);
            }
            MediaCommand::Previous => {
                self.previous(client);
            }
            MediaCommand::Stop => self.stop(),
            MediaCommand::SeekTo { seconds } => {
                if let Some(position) = seek_position(seconds, duration) {
                    self.seek(position);
                }
            }
            MediaCommand::Skip { seconds } => self.skip(seconds),
        }
    }

    pub fn poll(&mut self, client: Option<&Navidrome>) -> PlaybackUpdate {
        let generation = self.generation;
        for command in self.media.drain() {
            if let Some(client) = client {
                self.command(command, client);
            }
        }
        let snapshot = self.player.snapshot();
        let mut update = PlaybackUpdate {
            changed: generation != self.generation,
            track_changed: generation != self.generation,
            failure: None,
        };
        if snapshot.generation != self.generation {
            return update;
        }
        if let Some(error) = snapshot.error()
            && self.state.error() != Some(error)
        {
            update.failure = Some(Failure::new(error, RetryAction::Playback(self.generation)));
        } else if snapshot.warning != self.state.warning
            && let Some((_, message)) = &snapshot.warning
        {
            update.failure = Some(message.clone().into());
        }
        let finished = snapshot.is_finished() && !self.state.is_finished();
        update.changed |= self.state.position.as_secs() != snapshot.position.as_secs()
            || self.state.status != snapshot.status
            || self.state.warning != snapshot.warning;
        self.state = snapshot;
        if finished && let Some(client) = client {
            update.track_changed |= self.next(client);
        }
        update.changed |= update.track_changed;
        if update.changed {
            self.sync_media();
        }
        update
    }
    #[cfg(feature = "ui-tests")]
    pub fn test_stop_worker(&self) {
        self.player.test_stop_worker();
    }
    #[cfg(feature = "ui-tests")]
    pub fn test_media_command(&self, command: &MediaCommand) -> bool {
        self.media.test_command(command)
    }
    #[cfg(feature = "ui-tests")]
    pub fn test_media_snapshot(&self) -> (String, u64, String) {
        self.media.test_snapshot()
    }
}
