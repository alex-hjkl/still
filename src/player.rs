use reqwest::Url;
use rodio::{Decoder, DeviceSinkBuilder, Player as AudioPlayer, Source};
use std::{
    error::Error,
    fmt,
    io::BufReader,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::Duration,
};
use stream_download::{
    Settings, StreamDownload,
    http::{HttpStream, HttpStreamError},
    storage::temp::TempStorageProvider,
};
use tokio_util::sync::CancellationToken;

type AudioSource = Box<dyn Source<Item = f32> + Send>;

enum Command {
    Play(u64, Url),
    Prepared(u64, Result<AudioSource, AudioError>),
    Toggle,
    SetPaused(bool),
    Seek(Seek, u64),
    Seeked(u64, Result<(), rodio::source::SeekError>),
    Volume(f32),
    Stop,
    Shutdown,
    #[cfg(test)]
    Barrier(mpsc::Sender<()>),
}

#[derive(Clone, Copy)]
enum Seek {
    Absolute(Duration),
    Relative(f64),
}

impl Seek {
    fn resolve(self, position: Duration, duration: u64) -> Duration {
        let target = match self {
            Self::Absolute(target) => target,
            Self::Relative(seconds) => {
                Duration::try_from_secs_f64((position.as_secs_f64() + seconds).max(0.))
                    .unwrap_or(Duration::MAX)
            }
        };
        if duration == 0 {
            target
        } else {
            target.min(Duration::from_secs(duration))
        }
    }
}

#[derive(Default)]
struct Seeking {
    active: Option<Duration>,
    pending: Option<Duration>,
}

impl Seeking {
    fn request(&mut self, request: Seek, position: Duration, duration: u64) {
        let base = self.pending.or(self.active).unwrap_or(position);
        self.pending = Some(request.resolve(base, duration));
    }

    fn start(
        &mut self,
        sink: &Arc<AudioPlayer>,
        generation: u64,
        runtime: &tokio::runtime::Runtime,
        feedback: &mpsc::Sender<Command>,
    ) {
        if self.active.is_some() {
            return;
        }
        let Some(target) = self.pending.take() else {
            return;
        };
        self.active = Some(target);
        let sink = sink.clone();
        let feedback = feedback.clone();
        // Rodio waits for the source to acknowledge a seek. Keep this wait off
        // the command worker so Stop/Play can cancel the old download promptly.
        runtime.spawn_blocking(move || {
            let result = sink.try_seek(target);
            let _ = feedback.send(Command::Seeked(generation, result));
        });
    }
}

struct AudioError {
    message: &'static str,
    source: Option<Box<dyn Error + Send + Sync>>,
    redacted_url: Option<Url>,
}

impl AudioError {
    fn message(message: &'static str) -> Self {
        Self {
            message,
            source: None,
            redacted_url: None,
        }
    }
    fn caused_by(message: &'static str, source: impl Error + Send + Sync + 'static) -> Self {
        Self {
            message,
            source: Some(Box::new(source)),
            redacted_url: None,
        }
    }
    fn http(source: HttpStreamError<reqwest::Client>) -> Self {
        // HttpStreamError does not expose its nested reqwest error through
        // Error::source, so capture its URL explicitly before boxing it.
        let redacted_url = match &source {
            HttpStreamError::FetchFailure(error) => error.url(),
            HttpStreamError::ResponseFailure(error) => error.source().url(),
        }
        .cloned();
        Self {
            redacted_url,
            ..Self::caused_by("Could not load audio from Navidrome", source)
        }
    }
    fn diagnostic(&self) -> String {
        // Parent errors may repeat a nested reqwest error, so redact URLs from
        // the entire chain after collecting them. Never read response bodies.
        let mut parts = vec![self.message.to_owned()];
        let mut urls: Vec<String> = self.redacted_url.iter().map(ToString::to_string).collect();
        let mut source = self.source();
        while let Some(error) = source {
            parts.push(error.to_string());
            if let Some(error) = error.downcast_ref::<reqwest::Error>()
                && let Some(url) = error.url()
            {
                urls.push(url.to_string());
            }
            source = error.source();
        }
        let mut detail = parts.join(": ");
        for url in urls {
            detail = detail.replace(&url, "[redacted URL]");
        }
        detail
    }
}
impl fmt::Display for AudioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message)
    }
}
impl fmt::Debug for AudioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.diagnostic())
    }
}
impl Error for AudioError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source.as_deref().map(|source| source as &dyn Error)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum PlaybackStatus {
    #[default]
    Idle,
    Loading,
    Playing,
    Paused,
    Finished,
    Failed(String),
}

#[derive(Clone, Default)]
pub struct Playback {
    pub generation: u64,
    pub status: PlaybackStatus,
    pub position: Duration,
    pub warning: Option<(u64, String)>,
}

impl Playback {
    pub fn is_loading(&self) -> bool {
        self.status == PlaybackStatus::Loading
    }
    pub fn is_playing(&self) -> bool {
        self.status == PlaybackStatus::Playing
    }
    pub fn is_finished(&self) -> bool {
        self.status == PlaybackStatus::Finished
    }
    pub fn error(&self) -> Option<&str> {
        if let PlaybackStatus::Failed(message) = &self.status {
            Some(message)
        } else {
            None
        }
    }
    pub fn needs_restart(&self) -> bool {
        self.status == PlaybackStatus::Idle || self.is_finished() || self.error().is_some()
    }

    fn seek_failed(&mut self) {
        let id = self
            .warning
            .as_ref()
            .map_or(1, |(id, _)| id.wrapping_add(1));
        self.warning = Some((id, "This stream does not support seeking".into()));
    }
}

pub struct Player {
    commands: mpsc::Sender<Command>,
    state: tokio::sync::watch::Receiver<Playback>,
    requested_generation: AtomicU64,
}

impl Player {
    pub fn new(initial_volume: f32) -> Self {
        let (commands, receiver) = mpsc::channel();
        let (worker_state, state) = tokio::sync::watch::channel(Playback::default());
        let feedback = commands.clone();
        let worker = std::thread::Builder::new()
            .name("still-audio".into())
            .spawn(move || {
                let mut output = None;
                run_audio_worker(receiver, feedback, worker_state, initial_volume, || {
                    if output.is_none() {
                        let mut stream =
                            DeviceSinkBuilder::open_default_sink().map_err(|error| {
                                AudioError::caused_by("Could not open the audio device", error)
                            })?;
                        stream.log_on_drop(false);
                        output = Some(stream);
                    }
                    Ok(AudioPlayer::connect_new(output.as_ref().unwrap().mixer()))
                });
            });
        if let Err(error) = worker {
            crate::diagnostics::event(&format!("Could not start audio worker: {error}"));
        }
        Self {
            commands,
            state,
            requested_generation: AtomicU64::new(0),
        }
    }

    #[cfg(feature = "ui-tests")]
    pub fn test_stop_worker(&self) {
        let _ = self.commands.send(Command::Shutdown);
    }

    pub fn available(&self) -> bool {
        self.state.has_changed().is_ok()
    }
    pub fn play(&self, generation: u64, url: Url) {
        self.requested_generation
            .store(generation, Ordering::Relaxed);
        let _ = self.commands.send(Command::Play(generation, url));
    }
    pub fn toggle(&self) {
        let _ = self.commands.send(Command::Toggle);
    }
    pub fn set_paused(&self, paused: bool) {
        let _ = self.commands.send(Command::SetPaused(paused));
    }
    pub fn seek(&self, seconds: u64, duration: u64) {
        let _ = self.commands.send(Command::Seek(
            Seek::Absolute(Duration::from_secs(seconds)),
            duration,
        ));
    }
    pub fn seek_relative(&self, seconds: f64, duration: u64) {
        if seconds.is_finite() {
            let _ = self
                .commands
                .send(Command::Seek(Seek::Relative(seconds), duration));
        }
    }
    pub fn volume(&self, volume: f32) {
        let _ = self.commands.send(Command::Volume(volume));
    }
    pub fn stop(&self) {
        let _ = self.commands.send(Command::Stop);
    }
    pub fn snapshot(&self) -> Playback {
        let mut snapshot = self.state.borrow().clone();
        if !self.available() {
            snapshot.generation = self.requested_generation.load(Ordering::Relaxed);
            if snapshot.error().is_none() {
                snapshot.status = PlaybackStatus::Failed(
                    "The audio worker stopped. Retry playback to restart it.".into(),
                );
            }
        }
        snapshot
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Shutdown);
    }
}

fn run_audio_worker(
    receiver: mpsc::Receiver<Command>,
    feedback: mpsc::Sender<Command>,
    worker_state: tokio::sync::watch::Sender<Playback>,
    initial_volume: f32,
    mut connect: impl FnMut() -> Result<AudioPlayer, AudioError>,
) {
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            crate::diagnostics::event(&format!("Could not start audio runtime: {error}"));
            worker_state.send_modify(|state| {
                state.status = PlaybackStatus::Failed("Could not start the audio worker".into())
            });
            return;
        }
    };
    let mut sink: Option<Arc<AudioPlayer>> = None;
    let mut seeking = Seeking::default();
    let mut preparation: Option<Preparation> = None;
    let mut generation = 0;
    let mut volume = initial_volume.clamp(0., 1.);
    let mut paused = false;
    loop {
        match receiver.recv_timeout(Duration::from_millis(150)) {
            Ok(Command::Play(id, url)) => {
                generation = id;
                seeking = Seeking::default();
                paused = false;
                drop(preparation.take());
                if let Some(previous) = sink.take() {
                    previous.stop();
                }
                worker_state.send_replace(Playback {
                    generation,
                    status: PlaybackStatus::Loading,
                    ..Playback::default()
                });
                let feedback = feedback.clone();
                let cancel = CancellationToken::new();
                let token = cancel.clone();
                let task = runtime.spawn(async move {
                    let result = prepare_audio(url, token, Duration::from_secs(30)).await;
                    let _ = feedback.send(Command::Prepared(id, result));
                });
                preparation = Some(Preparation { cancel, task });
            }
            Ok(Command::Prepared(id, result)) if id == generation => {
                let result = result.and_then(|source| {
                    let next = connect()?;
                    next.set_volume(volume);
                    if paused {
                        next.pause();
                    }
                    next.append(source);
                    sink = Some(Arc::new(next));
                    Ok(())
                });
                worker_state.send_modify(|state| {
                    state.status = match result {
                        Ok(()) if paused => PlaybackStatus::Paused,
                        Ok(()) => PlaybackStatus::Playing,
                        Err(error) => {
                            crate::diagnostics::event(&error.diagnostic());
                            PlaybackStatus::Failed(error.to_string())
                        }
                    }
                });
            }
            Ok(Command::Prepared(_, _)) => {}
            Ok(command @ (Command::Toggle | Command::SetPaused(_))) => {
                paused = match command {
                    Command::SetPaused(value) => value,
                    _ => !paused,
                };
                if let Some(sink) = &sink {
                    if paused {
                        sink.pause();
                    } else {
                        sink.play();
                    }
                }
            }
            Ok(Command::Seek(request, duration)) => {
                if let Some(sink) = &sink {
                    seeking.request(request, sink.get_pos(), duration);
                }
            }
            Ok(Command::Seeked(id, result)) if id == generation => {
                seeking.active = None;
                if let Err(error) = result {
                    let error = AudioError::caused_by("Audio seek failed", error);
                    crate::diagnostics::event(&error.diagnostic());
                    worker_state.send_modify(Playback::seek_failed);
                }
            }
            Ok(Command::Seeked(_, _)) => {}
            Ok(Command::Volume(value)) => {
                volume = value.clamp(0., 1.);
                if let Some(sink) = &sink {
                    sink.set_volume(volume);
                }
            }
            Ok(Command::Stop) => {
                // Invalidate any preparation that completed just before cancellation.
                generation = generation.wrapping_add(1);
                seeking = Seeking::default();
                drop(preparation.take());
                if let Some(sink) = sink.take() {
                    sink.stop();
                }
                worker_state.send_replace(Playback {
                    generation,
                    ..Playback::default()
                });
            }
            Ok(Command::Shutdown) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            #[cfg(test)]
            Ok(Command::Barrier(reply)) => {
                let _ = reply.send(());
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        if let Some(sink) = &sink {
            seeking.start(sink, generation, &runtime, &feedback);
            let position = sink.get_pos();
            let status = if sink.empty() {
                PlaybackStatus::Finished
            } else if sink.is_paused() {
                PlaybackStatus::Paused
            } else {
                PlaybackStatus::Playing
            };
            worker_state.send_modify(|state| {
                state.position = position;
                state.status = status;
            });
        }
    }
    drop(preparation);
    if let Some(sink) = sink {
        sink.stop();
    }
    runtime.shutdown_timeout(Duration::from_secs(1));
}

struct Preparation {
    cancel: CancellationToken,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Preparation {
    fn drop(&mut self) {
        // Cancel the transport before aborting the async waiter. A decoder already
        // running in spawn_blocking must be woken by the stream cancellation.
        self.cancel.cancel();
        self.task.abort();
    }
}

struct DownloadGuard {
    cancel: CancellationToken,
    observer: tokio::task::JoinHandle<()>,
}
impl Drop for DownloadGuard {
    fn drop(&mut self) {
        self.cancel.cancel();
        self.observer.abort();
    }
}
struct CancellableSource {
    inner: AudioSource,
    _download: DownloadGuard,
}
impl Iterator for CancellableSource {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        self.inner.next()
    }
}
impl Source for CancellableSource {
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }
    fn channels(&self) -> rodio::ChannelCount {
        self.inner.channels()
    }
    fn sample_rate(&self) -> rodio::SampleRate {
        self.inner.sample_rate()
    }
    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }
    fn try_seek(&mut self, position: Duration) -> Result<(), rodio::source::SeekError> {
        self.inner.try_seek(position)
    }
}

async fn prepare_audio(
    url: Url,
    cancel: CancellationToken,
    limit: Duration,
) -> Result<AudioSource, AudioError> {
    let result = tokio::select! {
        biased;
        _ = cancel.cancelled() => Err(AudioError::message("Audio loading canceled")),
        result = tokio::time::timeout(limit, prepare_stream(url, cancel.clone())) => {
            result.unwrap_or_else(|_| Err(AudioError::message("The server took too long to prepare the track")))
        }
    };
    if result.is_err() {
        cancel.cancel();
    }
    result
}

async fn prepare_stream(url: Url, cancel: CancellationToken) -> Result<AudioSource, AudioError> {
    use std::sync::OnceLock;
    static HTTP: OnceLock<reqwest::Client> = OnceLock::new();
    let client = if let Some(client) = HTTP.get() {
        client.clone()
    } else {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .read_timeout(Duration::from_secs(15))
            .build()
            .map_err(|error| {
                AudioError::caused_by("Could not create the audio HTTP client", error)
            })?;
        let _ = HTTP.set(client.clone());
        client
    };
    let stream = HttpStream::new(client, url)
        .await
        .map_err(AudioError::http)?;
    let reader = StreamDownload::from_stream(
        stream,
        TempStorageProvider::new(),
        Settings::default().prefetch_bytes(64 * 1024),
    )
    .await
    .map_err(|error| AudioError::caused_by("Could not buffer audio from Navidrome", error))?;
    let stream_cancel = reader.cancellation_token();
    let token = stream_cancel.clone();
    let observer = tokio::spawn(async move {
        cancel.cancelled().await;
        token.cancel();
    });
    let guard = DownloadGuard {
        cancel: stream_cancel,
        observer,
    };
    let byte_len = reader.content_length();
    tokio::task::spawn_blocking(move || {
        let mut decoder = Decoder::builder()
            .with_data(BufReader::new(reader))
            .with_seekable(true);
        if let Some(byte_len) = byte_len {
            decoder = decoder.with_byte_len(byte_len);
        }
        decoder
            .build()
            .map(|decoder| {
                Box::new(CancellableSource {
                    inner: Box::new(decoder),
                    _download: guard,
                }) as AudioSource
            })
            .map_err(|error| AudioError::caused_by("Could not decode the audio format", error))
    })
    .await
    .map_err(|error| AudioError::caused_by("Could not prepare audio", error))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader, Write},
        net::TcpListener,
    };

    struct GatedSource {
        inner: AudioSource,
        entered: mpsc::Sender<Duration>,
        release: mpsc::Receiver<()>,
    }
    impl Iterator for GatedSource {
        type Item = f32;
        fn next(&mut self) -> Option<f32> {
            self.inner.next()
        }
    }
    impl Source for GatedSource {
        fn current_span_len(&self) -> Option<usize> {
            self.inner.current_span_len()
        }
        fn channels(&self) -> rodio::ChannelCount {
            self.inner.channels()
        }
        fn sample_rate(&self) -> rodio::SampleRate {
            self.inner.sample_rate()
        }
        fn total_duration(&self) -> Option<Duration> {
            self.inner.total_duration()
        }
        fn try_seek(&mut self, position: Duration) -> Result<(), rodio::source::SeekError> {
            self.entered.send(position).unwrap();
            let _ = self.release.recv_timeout(Duration::from_secs(5));
            self.inner.try_seek(position)
        }
    }

    struct SeekFixture {
        commands: mpsc::Sender<Command>,
        state: tokio::sync::watch::Receiver<Playback>,
        entered: mpsc::Receiver<Duration>,
        release: mpsc::Sender<()>,
        outputs: mpsc::Receiver<rodio::queue::SourcesQueueOutput>,
        render_cancel: CancellationToken,
        worker: Option<std::thread::JoinHandle<()>>,
        renderer: Option<std::thread::JoinHandle<()>>,
    }
    impl SeekFixture {
        fn new() -> Self {
            let (commands, receiver) = mpsc::channel();
            let (publisher, state) = tokio::sync::watch::channel(Playback::default());
            let (output, outputs) = mpsc::channel();
            let feedback = commands.clone();
            let worker = std::thread::spawn(move || {
                run_audio_worker(receiver, feedback, publisher, 0.7, || {
                    let (sink, source) = AudioPlayer::new();
                    output.send(source).unwrap();
                    Ok(sink)
                });
            });
            let (entered_tx, entered) = mpsc::channel();
            let (release, release_rx) = mpsc::channel();
            let inner = rodio::buffer::SamplesBuffer::new(
                1.try_into().unwrap(),
                1000.try_into().unwrap(),
                vec![0.; 120_000],
            );
            commands.send(Command::SetPaused(true)).unwrap();
            commands
                .send(Command::Prepared(
                    0,
                    Ok(Box::new(GatedSource {
                        inner: Box::new(inner),
                        entered: entered_tx,
                        release: release_rx,
                    })),
                ))
                .unwrap();
            let mut source = outputs.recv_timeout(Duration::from_secs(2)).unwrap();
            let render_cancel = CancellationToken::new();
            let token = render_cancel.clone();
            let renderer = std::thread::spawn(move || {
                while !token.is_cancelled() {
                    source.next();
                }
            });
            Self {
                commands,
                state,
                entered,
                release,
                outputs,
                render_cancel,
                worker: Some(worker),
                renderer: Some(renderer),
            }
        }
        fn barrier(&self) {
            let (reply, done) = mpsc::channel();
            self.commands.send(Command::Barrier(reply)).unwrap();
            done.recv_timeout(Duration::from_secs(2)).unwrap();
        }
        fn seek(&self, request: Seek) {
            self.commands.send(Command::Seek(request, 120)).unwrap();
        }
        async fn wait(&mut self, predicate: impl Fn(&Playback) -> bool) {
            tokio::time::timeout(Duration::from_secs(2), async {
                loop {
                    if predicate(&self.state.borrow_and_update()) {
                        return;
                    }
                    self.state.changed().await.unwrap();
                }
            })
            .await
            .unwrap();
        }
    }
    impl Drop for SeekFixture {
        fn drop(&mut self) {
            let _ = self.commands.send(Command::Shutdown);
            for _ in 0..4 {
                let _ = self.release.send(());
            }
            self.worker.take().unwrap().join().unwrap();
            self.render_cancel.cancel();
            self.renderer.take().unwrap().join().unwrap();
        }
    }

    #[tokio::test]
    async fn rapid_relative_seeks_accumulate_and_pending_absolute_seeks_replace_them() {
        let mut fixture = SeekFixture::new();
        fixture.seek(Seek::Absolute(Duration::from_secs(19)));
        assert_eq!(
            fixture
                .entered
                .recv_timeout(Duration::from_secs(2))
                .unwrap(),
            Duration::from_secs(19)
        );
        for _ in 0..3 {
            fixture.seek(Seek::Relative(-10.));
        }
        fixture.barrier();
        fixture.release.send(()).unwrap();
        assert_eq!(
            fixture
                .entered
                .recv_timeout(Duration::from_secs(2))
                .unwrap(),
            Duration::ZERO
        );
        fixture.release.send(()).unwrap();
        fixture
            .wait(|state| {
                state.position == Duration::ZERO && state.status == PlaybackStatus::Paused
            })
            .await;

        fixture.seek(Seek::Absolute(Duration::from_secs(40)));
        assert_eq!(
            fixture
                .entered
                .recv_timeout(Duration::from_secs(2))
                .unwrap(),
            Duration::from_secs(40)
        );
        fixture.seek(Seek::Relative(10.));
        fixture.seek(Seek::Absolute(Duration::from_secs(110)));
        fixture.seek(Seek::Relative(20.));
        fixture.seek(Seek::Relative(-10.));
        fixture.barrier();
        fixture.release.send(()).unwrap();
        assert_eq!(
            fixture
                .entered
                .recv_timeout(Duration::from_secs(2))
                .unwrap(),
            Duration::from_secs(110)
        );
        fixture.release.send(()).unwrap();
        fixture
            .wait(|state| state.position == Duration::from_secs(110))
            .await;
    }

    #[tokio::test]
    async fn stop_and_next_are_processed_before_a_blocked_seek_finishes() {
        for next in [false, true] {
            let mut fixture = SeekFixture::new();
            fixture.seek(Seek::Absolute(Duration::from_secs(19)));
            fixture
                .entered
                .recv_timeout(Duration::from_secs(2))
                .unwrap();
            fixture.seek(Seek::Relative(10.));
            let (server, _next_output) = if next {
                let (url, server) = audio_server(wav());
                fixture.commands.send(Command::Play(1, url)).unwrap();
                // Connecting the next source must not need the old seek's gate.
                let _next_output = fixture
                    .outputs
                    .recv_timeout(Duration::from_secs(2))
                    .unwrap();
                (Some(server), Some(_next_output))
            } else {
                fixture.commands.send(Command::Stop).unwrap();
                (None, None)
            };
            fixture.barrier();
            fixture
                .wait(|state| {
                    state.generation == 1
                        && if next {
                            state.is_playing()
                        } else {
                            state.status == PlaybackStatus::Idle
                        }
                })
                .await;
            fixture.release.send(()).unwrap();
            // A late completion/failure belongs to the old track only.
            fixture
                .commands
                .send(Command::Seeked(
                    0,
                    Err(rodio::source::SeekError::NotSupported {
                        underlying_source: "fixture",
                    }),
                ))
                .unwrap();
            fixture.barrier();
            assert!(fixture.state.borrow().warning.is_none());
            assert!(fixture.entered.try_recv().is_err());
            if let Some(server) = server {
                server.join().unwrap();
            }
        }
    }

    #[test]
    fn failed_seek_does_not_turn_pause_into_a_track_restart() {
        for status in [PlaybackStatus::Playing, PlaybackStatus::Paused] {
            let mut playback = Playback {
                generation: 7,
                status: status.clone(),
                position: Duration::from_secs(42),
                ..Playback::default()
            };
            playback.seek_failed();
            assert_eq!(playback.status, status);
            assert_eq!(playback.position, Duration::from_secs(42));
            assert_eq!(playback.generation, 7);
            assert!(!playback.needs_restart());
            assert!(playback.error().is_none());
            assert!(playback.warning.is_some());
        }
    }

    #[test]
    fn unavailable_worker_reports_a_failure_for_the_requested_track() {
        let (commands, receiver) = mpsc::channel();
        drop(receiver);
        let (publisher, state) = tokio::sync::watch::channel(Playback::default());
        drop(publisher);
        let player = Player {
            commands,
            state,
            requested_generation: AtomicU64::new(0),
        };
        player.play(12, "http://localhost/stream".parse().unwrap());
        let playback = player.snapshot();
        assert_eq!(playback.generation, 12);
        assert!(!playback.is_loading());
        assert!(playback.needs_restart());
    }

    #[test]
    fn a_panicked_worker_becomes_a_failed_snapshot_instead_of_a_ui_panic() {
        let (commands, receiver) = mpsc::channel();
        let (publisher, state) = tokio::sync::watch::channel(Playback::default());
        let worker = std::thread::spawn(move || {
            let _receiver = receiver;
            let _publisher = publisher;
            panic!("simulated audio worker failure");
        });
        assert!(worker.join().is_err());
        let player = Player {
            commands,
            state,
            requested_generation: AtomicU64::new(17),
        };
        player.toggle();
        player.seek(10, 0);
        assert!(!player.available());
        let snapshot = player.snapshot();
        assert_eq!(snapshot.generation, 17);
        assert!(snapshot.error().is_some());
        assert!(snapshot.needs_restart());
    }

    fn stalled_audio_server() -> (
        Url,
        tokio::sync::oneshot::Receiver<()>,
        std::thread::JoinHandle<()>,
    ) {
        use std::io::Read;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/stream", listener.local_addr().unwrap())
            .parse()
            .unwrap();
        let (ready, received) = tokio::sync::oneshot::channel();
        let worker = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut reader = BufReader::new(socket.try_clone().unwrap());
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line.trim().is_empty() {
                    break;
                }
            }
            socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: audio/wav\r\nContent-Length: 1000000\r\nConnection: close\r\n\r\nRIFF").unwrap();
            let _ = ready.send(());
            // The server deliberately stalls. Cancel/timeout must close the body
            // instead of leaving a blocking decoder and downloader alive.
            let mut byte = [0];
            match socket.read(&mut byte) {
                Ok(0) => {}
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
                    ) => {}
                result => panic!("Canceled stream stayed open: {result:?}"),
            }
        });
        (url, received, worker)
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn canceling_preparation_closes_a_stalled_stream() {
        let (url, ready, server) = stalled_audio_server();
        let token = CancellationToken::new();
        let loading = tokio::spawn(prepare_audio(url, token.clone(), Duration::from_secs(10)));
        ready.await.unwrap();
        token.cancel();
        assert!(
            tokio::time::timeout(Duration::from_secs(2), loading)
                .await
                .unwrap()
                .unwrap()
                .is_err()
        );
        server.join().unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn preparation_deadline_also_cancels_a_blocked_decoder() {
        let (url, ready, server) = stalled_audio_server();
        let token = CancellationToken::new();
        let loading = tokio::spawn(prepare_audio(
            url,
            token.clone(),
            Duration::from_millis(250),
        ));
        ready.await.unwrap();
        let error = tokio::time::timeout(Duration::from_secs(2), loading)
            .await
            .unwrap()
            .unwrap()
            .err()
            .unwrap();
        assert!(error.to_string().contains("too long"));
        assert!(token.is_cancelled());
        server.join().unwrap();
    }

    fn audio_server(body: Vec<u8>) -> (Url, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/stream", listener.local_addr().unwrap())
            .parse()
            .unwrap();
        let thread = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut reader = BufReader::new(socket.try_clone().unwrap());
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line.trim().is_empty() {
                    break;
                }
            }
            write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: audio/wav\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
            socket.write_all(&body).unwrap();
        });
        (url, thread)
    }

    fn wav() -> Vec<u8> {
        let data_bytes = 8000u32 * 2;
        let mut wav = Vec::new();
        wav.extend(b"RIFF");
        wav.extend((36 + data_bytes).to_le_bytes());
        wav.extend(b"WAVEfmt ");
        wav.extend(16u32.to_le_bytes());
        wav.extend(1u16.to_le_bytes());
        wav.extend(1u16.to_le_bytes());
        wav.extend(8000u32.to_le_bytes());
        wav.extend(16000u32.to_le_bytes());
        wav.extend(2u16.to_le_bytes());
        wav.extend(16u16.to_le_bytes());
        wav.extend(b"data");
        wav.extend(data_bytes.to_le_bytes());
        for _ in 0..8000 {
            wav.extend(8192i16.to_le_bytes());
        }
        wav
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn decodes_remote_audio_without_an_audio_device() {
        let (url, server) = audio_server(wav());
        let mut source = prepare_audio(url, CancellationToken::new(), Duration::from_secs(5))
            .await
            .unwrap();
        assert_eq!(source.sample_rate().get(), 8000);
        assert_eq!(source.channels().get(), 1);
        assert!((source.next().unwrap() - 0.25).abs() < 0.001);
        source.try_seek(Duration::from_millis(500)).unwrap();
        assert!((source.next().unwrap() - 0.25).abs() < 0.001);
        source.try_seek(Duration::ZERO).unwrap();
        assert!((source.next().unwrap() - 0.25).abs() < 0.001);
        assert_eq!(source.count(), 7999);
        server.join().unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn rejects_non_audio_server_responses() {
        let (url, server) =
            audio_server(b"{\"subsonic-response\":{\"status\":\"failed\"}}".to_vec());
        assert!(
            prepare_audio(url, CancellationToken::new(), Duration::from_secs(5))
                .await
                .is_err()
        );
        server.join().unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn http_failure_keeps_its_cause_without_exposing_the_request_url() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url: Url = format!(
            "http://{}/stream?u=fixture&t=fixture-token",
            listener.local_addr().unwrap()
        )
        .parse()
        .unwrap();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(socket.try_clone().unwrap());
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line.trim().is_empty() {
                    break;
                }
            }
            socket.write_all(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
        });
        let error = prepare_audio(
            url.clone(),
            CancellationToken::new(),
            Duration::from_secs(5),
        )
        .await
        .err()
        .unwrap();
        assert_eq!(error.to_string(), "Could not load audio from Navidrome");
        assert!(error.source().is_some());
        let detail = error.diagnostic();
        assert!(detail.contains("503"), "{detail}");
        assert!(!detail.contains(url.as_str()));
        assert!(!detail.contains("fixture-token"));
        assert!(!format!("{error:?}").contains("fixture-token"));
        server.join().unwrap();
    }
}
