#[cfg(target_os = "macos")]
use crate::player::PlaybackStatus;
use crate::{api::Song, player::Playback};

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "ui-tests", derive(serde::Deserialize))]
#[cfg_attr(
    feature = "ui-tests",
    serde(tag = "command", rename_all = "snake_case")
)]
pub enum MediaCommand {
    Play,
    Pause,
    Toggle,
    Stop,
    Next,
    Previous,
    SeekTo { seconds: f64 },
    Skip { seconds: f64 },
}

/// Reject invalid native positions and keep seeking inside the current track.
pub fn seek_position(seconds: f64, duration: u64) -> Option<u64> {
    seconds.is_finite().then(|| {
        let position = seconds.max(0.) as u64;
        if duration == 0 {
            position
        } else {
            position.min(duration)
        }
    })
}

#[cfg(target_os = "macos")]
#[path = "media_macos.rs"]
mod platform;
#[cfg(target_os = "macos")]
pub use platform::SystemMedia;

#[cfg(not(target_os = "macos"))]
pub struct SystemMedia;
#[cfg(not(target_os = "macos"))]
impl SystemMedia {
    pub fn new() -> Self {
        Self
    }
    pub fn drain(&self) -> Vec<MediaCommand> {
        Vec::new()
    }
    pub fn sync(&mut self, _: Option<&Song>, _: &Playback) {}
    pub fn clear(&mut self) {}
    #[cfg(feature = "ui-tests")]
    pub fn test_command(&self, _: &MediaCommand) -> bool {
        false
    }
    #[cfg(feature = "ui-tests")]
    pub fn test_snapshot(&self) -> (String, u64, String) {
        (String::new(), 0, "stopped".into())
    }
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Playing,
    Paused,
    Stopped,
}

#[cfg(target_os = "macos")]
fn state(playback: &Playback) -> State {
    match playback.status {
        PlaybackStatus::Playing | PlaybackStatus::Loading => State::Playing,
        PlaybackStatus::Paused => State::Paused,
        _ => State::Stopped,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_seek_handles_unknown_duration_and_invalid_values() {
        assert_eq!(seek_position(-10., 100), Some(0));
        assert_eq!(seek_position(120., 100), Some(100));
        assert_eq!(seek_position(120., 0), Some(120));
        assert_eq!(seek_position(f64::NAN, 100), None);
        assert_eq!(seek_position(f64::INFINITY, 100), None);
    }
}
