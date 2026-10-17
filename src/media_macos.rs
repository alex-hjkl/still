//! All MediaPlayer calls stay on GPUI's main thread. Native callbacks only send
//! owned commands through a channel; they never access the view or audio sink.
use super::*;
use block2::RcBlock;
use objc2::{MainThreadMarker, rc::Retained, runtime::AnyObject};
use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSString};
use objc2_media_player::*;
use std::{
    ptr::NonNull,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
};

type Handler = RcBlock<dyn Fn(NonNull<MPRemoteCommandEvent>) -> MPRemoteCommandHandlerStatus>;
type Dispatch = Arc<dyn Fn(MediaCommand) -> MPRemoteCommandHandlerStatus + Send + Sync>;

struct Registration {
    command: Retained<MPRemoteCommand>,
    target: Retained<AnyObject>,
    // Keep the block alive, including when a command is temporarily disabled.
    _handler: Handler,
    #[cfg(feature = "ui-tests")]
    dispatch: Dispatch,
}

pub struct SystemMedia {
    _main_thread: MainThreadMarker,
    center: Retained<MPNowPlayingInfoCenter>,
    registrations: Vec<Registration>,
    available: Arc<AtomicBool>,
    events: mpsc::Receiver<MediaCommand>,
    last: Option<(String, u64, State)>,
}

impl SystemMedia {
    pub fn new() -> Self {
        let main_thread = MainThreadMarker::new().expect("media controls require the main thread");
        let (sender, events) = mpsc::channel();
        let available = Arc::new(AtomicBool::new(false));
        // SAFETY: These are public MediaPlayer APIs on the main thread. Commands
        // and opaque targets are retained until their handlers are removed.
        unsafe {
            let commands = MPRemoteCommandCenter::sharedCommandCenter();
            let mut registrations = Vec::new();
            for (command, action) in [
                (commands.playCommand(), MediaCommand::Play),
                (commands.pauseCommand(), MediaCommand::Pause),
                (commands.togglePlayPauseCommand(), MediaCommand::Toggle),
                (commands.stopCommand(), MediaCommand::Stop),
                (commands.nextTrackCommand(), MediaCommand::Next),
                (commands.previousTrackCommand(), MediaCommand::Previous),
            ] {
                registrations.push(register(
                    command,
                    sender.clone(),
                    available.clone(),
                    move |_| Some(action.clone()),
                ));
            }
            registrations.push(register(
                commands.changePlaybackPositionCommand().into_super(),
                sender.clone(),
                available.clone(),
                |event| {
                    event
                        .downcast_ref::<MPChangePlaybackPositionCommandEvent>()
                        .map(|event| MediaCommand::SeekTo {
                            seconds: event.positionTime(),
                        })
                },
            ));
            for (command, direction) in [
                (commands.skipForwardCommand(), 1.),
                (commands.skipBackwardCommand(), -1.),
            ] {
                command.setPreferredIntervals(&NSArray::from_retained_slice(&[
                    NSNumber::numberWithDouble(10.),
                ]));
                registrations.push(register(
                    command.into_super(),
                    sender.clone(),
                    available.clone(),
                    move |event| {
                        event
                            .downcast_ref::<MPSkipIntervalCommandEvent>()
                            .map(|event| MediaCommand::Skip {
                                seconds: direction * event.interval(),
                            })
                    },
                ));
            }
            Self {
                _main_thread: main_thread,
                center: MPNowPlayingInfoCenter::defaultCenter(),
                registrations,
                available,
                events,
                last: None,
            }
        }
    }

    pub fn drain(&self) -> Vec<MediaCommand> {
        self.events.try_iter().collect()
    }

    /// Exercise the same dispatchers used by the native blocks without sending
    /// global media keys to another application during isolated UI tests.
    #[cfg(feature = "ui-tests")]
    pub fn test_command(&self, command: &MediaCommand) -> bool {
        let index = match command {
            MediaCommand::Play => 0,
            MediaCommand::Pause => 1,
            MediaCommand::Toggle => 2,
            MediaCommand::Stop => 3,
            MediaCommand::Next => 4,
            MediaCommand::Previous => 5,
            MediaCommand::SeekTo { .. } => 6,
            MediaCommand::Skip { seconds } if *seconds >= 0. => 7,
            MediaCommand::Skip { .. } => 8,
        };
        let status = (self.registrations[index].dispatch)(command.clone());
        status == MPRemoteCommandHandlerStatus::Success
    }

    #[cfg(feature = "ui-tests")]
    pub fn test_snapshot(&self) -> (String, u64, String) {
        // Read the native center back, rather than trusting our cached state.
        unsafe {
            let info = self.center.nowPlayingInfo();
            let title = info
                .as_ref()
                .and_then(|info| info.objectForKey(MPMediaItemPropertyTitle))
                .and_then(|value| value.downcast::<NSString>().ok())
                .map(|s| s.to_string())
                .unwrap_or_default();
            let position = info
                .as_ref()
                .and_then(|info| info.objectForKey(MPNowPlayingInfoPropertyElapsedPlaybackTime))
                .and_then(|value| value.downcast::<NSNumber>().ok())
                .map(|n| n.doubleValue() as u64)
                .unwrap_or_default();
            let state = match self.center.playbackState() {
                MPNowPlayingPlaybackState::Playing => "playing",
                MPNowPlayingPlaybackState::Paused => "paused",
                _ => "stopped",
            };
            (title, position, state.into())
        }
    }

    pub fn sync(&mut self, song: Option<&Song>, playback: &Playback) {
        let Some(song) = song else {
            self.clear();
            return;
        };
        let state = state(playback);
        let next = (song.id.clone(), playback.position.as_secs(), state);
        if self.last.as_ref() == Some(&next) {
            return;
        }
        let state_changed = self.last.as_ref().is_none_or(|last| last.2 != state);
        // SAFETY: Keys and values use the documented NSString/NSNumber types.
        // NSDictionary copies/retains its contents and the center copies it.
        unsafe {
            let title = NSString::from_str(&song.title);
            let artist = NSString::from_str(&song.artist);
            let album = NSString::from_str(&song.album);
            let elapsed = NSNumber::numberWithDouble(playback.position.as_secs_f64());
            let rate = NSNumber::numberWithDouble(if state == State::Playing { 1. } else { 0. });
            let media_type =
                NSNumber::numberWithUnsignedInteger(MPNowPlayingInfoMediaType::Audio.0);
            let mut keys = vec![
                MPMediaItemPropertyTitle,
                MPMediaItemPropertyArtist,
                MPMediaItemPropertyAlbumTitle,
                MPNowPlayingInfoPropertyElapsedPlaybackTime,
                MPNowPlayingInfoPropertyPlaybackRate,
                MPNowPlayingInfoPropertyMediaType,
            ];
            let mut values: Vec<&AnyObject> =
                vec![&title, &artist, &album, &elapsed, &rate, &media_type];
            let duration = NSNumber::numberWithDouble(song.duration as f64);
            if song.duration > 0 {
                keys.push(MPMediaItemPropertyPlaybackDuration);
                values.push(&duration);
            }
            let info = NSDictionary::from_slices(&keys, &values);
            self.center.setNowPlayingInfo(Some(&info));
            if !self.available.swap(true, Ordering::Relaxed) {
                for registration in &self.registrations {
                    registration.command.setEnabled(true);
                }
            }
            // Setting playbackState repeatedly can reclaim the system's active
            // player, so publish it only when playback actually changes.
            if state_changed {
                self.center.setPlaybackState(match state {
                    State::Playing => MPNowPlayingPlaybackState::Playing,
                    State::Paused => MPNowPlayingPlaybackState::Paused,
                    State::Stopped => MPNowPlayingPlaybackState::Stopped,
                });
            }
        }
        self.last = Some(next);
    }

    pub fn clear(&mut self) {
        self.available.store(false, Ordering::Relaxed);
        if self.last.take().is_none() {
            return;
        }
        // SAFETY: Main-thread cleanup of this process's Now Playing center.
        unsafe {
            for registration in &self.registrations {
                registration.command.setEnabled(false);
            }
            self.center
                .setPlaybackState(MPNowPlayingPlaybackState::Stopped);
            self.center.setNowPlayingInfo(None);
        }
        // Commands queued for a previous connection must not reach a new one.
        self.drain();
    }
}

fn register(
    command: Retained<MPRemoteCommand>,
    sender: mpsc::Sender<MediaCommand>,
    available: Arc<AtomicBool>,
    action: impl Fn(&MPRemoteCommandEvent) -> Option<MediaCommand> + Send + Sync + 'static,
) -> Registration {
    let dispatch: Dispatch = Arc::new(move |action| {
        if !available.load(Ordering::Relaxed) {
            return MPRemoteCommandHandlerStatus::NoActionableNowPlayingItem;
        }
        if sender.send(action).is_ok() {
            MPRemoteCommandHandlerStatus::Success
        } else {
            MPRemoteCommandHandlerStatus::CommandFailed
        }
    });
    let send = dispatch.clone();
    let handler = RcBlock::new(move |event: NonNull<MPRemoteCommandEvent>| {
        // SAFETY: MediaPlayer supplies a live event for the duration of this call.
        let Some(action) = action(unsafe { event.as_ref() }) else {
            return MPRemoteCommandHandlerStatus::CommandFailed;
        };
        send(action)
    });
    // SAFETY: The copied block has the exact public command handler signature.
    let target = unsafe {
        command.setEnabled(false);
        command.addTargetWithHandler(&handler)
    };
    Registration {
        command,
        target,
        _handler: handler,
        #[cfg(feature = "ui-tests")]
        dispatch,
    }
}

impl Drop for SystemMedia {
    fn drop(&mut self) {
        self.clear();
        // SAFETY: Remove only our retained targets before releasing the blocks.
        for registration in &self.registrations {
            unsafe {
                registration
                    .command
                    .removeTarget(Some(&registration.target));
            }
        }
    }
}
