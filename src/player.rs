use reqwest::Url;
use rodio::{Decoder, DeviceSinkBuilder, Player as AudioPlayer, Source};
use std::{
    io::BufReader,
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};
use stream_download::{
    Settings, StreamDownload, http::HttpStream, storage::temp::TempStorageProvider,
};
use tokio_util::sync::CancellationToken;

type AudioSource = Box<dyn Source<Item = f32> + Send>;

enum Command {
    Play(u64, Url),
    Prepared(u64, Result<AudioSource, String>),
    Toggle,
    SetPaused(bool),
    Seek(Duration),
    Volume(f32),
    Stop,
    Shutdown,
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
    state: Arc<Mutex<Playback>>,
}

impl Player {
    pub fn new(initial_volume: f32) -> Self {
        let (commands, receiver) = mpsc::channel();
        let state = Arc::new(Mutex::new(Playback::default()));
        let worker_state = state.clone();
        let feedback = commands.clone();
        std::thread::Builder::new()
            .name("still-audio".into())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(_) => {
                        worker_state.lock().unwrap().status =
                            PlaybackStatus::Failed("Could not start the audio worker".into());
                        return;
                    }
                };
                let mut output = None;
                let mut sink: Option<AudioPlayer> = None;
                let mut preparation: Option<Preparation> = None;
                let mut generation = 0;
                let mut volume = initial_volume.clamp(0., 1.);
                let mut paused = false;
                loop {
                    match receiver.recv_timeout(Duration::from_millis(150)) {
                        Ok(Command::Play(id, url)) => {
                            generation = id;
                            paused = false;
                            drop(preparation.take());
                            if let Some(previous) = sink.take() {
                                previous.stop();
                            }
                            *worker_state.lock().unwrap() = Playback {
                                generation,
                                status: PlaybackStatus::Loading,
                                ..Playback::default()
                            };
                            let feedback = feedback.clone();
                            let cancel = CancellationToken::new();
                            let token = cancel.clone();
                            let task = runtime.spawn(async move {
                                let result =
                                    prepare_audio(url, token, Duration::from_secs(30)).await;
                                let _ = feedback.send(Command::Prepared(id, result));
                            });
                            preparation = Some(Preparation { cancel, task });
                        }
                        Ok(Command::Prepared(id, result)) if id == generation => {
                            let result = result.and_then(|source| {
                                if output.is_none() {
                                    let mut stream = DeviceSinkBuilder::open_default_sink()
                                        .map_err(|_| {
                                            "Could not open the audio device".to_owned()
                                        })?;
                                    stream.log_on_drop(false);
                                    output = Some(stream);
                                }
                                let next =
                                    AudioPlayer::connect_new(output.as_ref().unwrap().mixer());
                                next.set_volume(volume);
                                if paused {
                                    next.pause();
                                }
                                next.append(source);
                                sink = Some(next);
                                Ok(())
                            });
                            let mut state = worker_state.lock().unwrap();
                            state.status = match result {
                                Ok(()) if paused => PlaybackStatus::Paused,
                                Ok(()) => PlaybackStatus::Playing,
                                Err(error) => PlaybackStatus::Failed(error),
                            };
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
                        Ok(Command::Seek(position)) => {
                            if let Some(sink) = &sink
                                && let Err(error) = sink.try_seek(position)
                            {
                                crate::diagnostics::event(&format!("Audio seek failed: {error}"));
                                worker_state.lock().unwrap().seek_failed();
                            }
                        }
                        Ok(Command::Volume(value)) => {
                            volume = value.clamp(0., 1.);
                            if let Some(sink) = &sink {
                                sink.set_volume(volume);
                            }
                        }
                        Ok(Command::Stop) => {
                            // Invalidate any preparation that completed just before cancellation.
                            generation = generation.wrapping_add(1);
                            drop(preparation.take());
                            if let Some(sink) = sink.take() {
                                sink.stop();
                            }
                            *worker_state.lock().unwrap() = Playback::default();
                        }
                        Ok(Command::Shutdown) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                    }
                    if let Some(sink) = &sink {
                        let mut state = worker_state.lock().unwrap();
                        state.position = sink.get_pos();
                        state.status = if sink.empty() {
                            PlaybackStatus::Finished
                        } else if sink.is_paused() {
                            PlaybackStatus::Paused
                        } else {
                            PlaybackStatus::Playing
                        };
                    }
                }
                drop(preparation);
                if let Some(sink) = sink {
                    sink.stop();
                }
                runtime.shutdown_timeout(Duration::from_secs(1));
            })
            .expect("create audio worker");
        Self { commands, state }
    }

    pub fn play(&self, generation: u64, url: Url) {
        if self.commands.send(Command::Play(generation, url)).is_err() {
            *self.state.lock().unwrap() = Playback {
                generation,
                status: PlaybackStatus::Failed("The audio worker is unavailable".into()),
                ..Playback::default()
            };
        }
    }
    pub fn toggle(&self) {
        let _ = self.commands.send(Command::Toggle);
    }
    pub fn set_paused(&self, paused: bool) {
        let _ = self.commands.send(Command::SetPaused(paused));
    }
    pub fn seek(&self, seconds: u64) {
        let _ = self
            .commands
            .send(Command::Seek(Duration::from_secs(seconds)));
    }
    pub fn volume(&self, volume: f32) {
        let _ = self.commands.send(Command::Volume(volume));
    }
    pub fn stop(&self) {
        let _ = self.commands.send(Command::Stop);
    }
    pub fn snapshot(&self) -> Playback {
        self.state.lock().unwrap().clone()
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Shutdown);
    }
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
) -> Result<AudioSource, String> {
    let result = tokio::select! {
        biased;
        _ = cancel.cancelled() => Err("Audio loading canceled".into()),
        result = tokio::time::timeout(limit, prepare_stream(url, cancel.clone())) => {
            result.unwrap_or_else(|_| Err("The server took too long to prepare the track".to_owned()))
        }
    };
    if result.is_err() {
        cancel.cancel();
    }
    result
}

async fn prepare_stream(url: Url, cancel: CancellationToken) -> Result<AudioSource, String> {
    use std::sync::OnceLock;
    static HTTP: OnceLock<Result<reqwest::Client, String>> = OnceLock::new();
    let client = HTTP
        .get_or_init(|| {
            reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .read_timeout(Duration::from_secs(15))
                .build()
                .map_err(|_| "Could not create the audio HTTP client".to_owned())
        })
        .as_ref()
        .map_err(Clone::clone)?
        .clone();
    let stream = HttpStream::new(client, url)
        .await
        .map_err(|_| "Could not load audio from Navidrome".to_owned())?;
    let reader = StreamDownload::from_stream(
        stream,
        TempStorageProvider::new(),
        Settings::default().prefetch_bytes(64 * 1024),
    )
    .await
    .map_err(|_| "Could not buffer audio from Navidrome".to_owned())?;
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
            .map_err(|_| "Could not decode the audio format".to_owned())
    })
    .await
    .map_err(|_| "Could not prepare audio".to_owned())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader, Write},
        net::TcpListener,
    };

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
        let player = Player {
            commands,
            state: Arc::new(Mutex::new(Playback::default())),
        };
        player.play(12, "http://localhost/stream".parse().unwrap());
        let playback = player.snapshot();
        assert_eq!(playback.generation, 12);
        assert!(!playback.is_loading());
        assert!(playback.needs_restart());
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
            assert_eq!(socket.read(&mut byte).unwrap(), 0);
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
        assert!(error.contains("too long"));
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

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn decodes_remote_audio_without_an_audio_device() {
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
        let (url, server) = audio_server(wav);
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
}
