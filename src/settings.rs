use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write as _,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::oneshot;

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Section {
    #[default]
    Artists,
    Albums,
    Songs,
    Folders,
    Liked,
}

impl Section {
    pub fn from_index(index: usize) -> Self {
        match index {
            0 => Self::Artists,
            1 => Self::Albums,
            2 => Self::Songs,
            3 => Self::Folders,
            _ => Self::Liked,
        }
    }
    pub fn index(self) -> usize {
        match self {
            Self::Artists => 0,
            Self::Albums => 1,
            Self::Songs => 2,
            Self::Folders => 3,
            Self::Liked => 4,
        }
    }
}

// Connection details stay in Keychain. This file only contains UI preferences.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum FolderSelection {
    Folder(String),
    Song(String),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SavedFolder {
    pub library_id: String,
    pub path: String,
    pub selected: Option<FolderSelection>,
    pub filter: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Preferences {
    pub volume: f32,
    pub shuffle: bool,
    pub sidebar_compact: bool,
    pub last_section: Section,
    pub window_size: [f32; 2],
    pub last_folder: Option<SavedFolder>,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            volume: 0.7,
            shuffle: false,
            sidebar_compact: true,
            last_section: Section::Artists,
            window_size: [1120., 760.],
            last_folder: None,
        }
    }
}

impl Preferences {
    fn normalized(mut self) -> Self {
        self.volume = if self.volume.is_finite() {
            self.volume.clamp(0., 1.)
        } else {
            0.7
        };
        for (value, minimum) in self.window_size.iter_mut().zip([780., 640.]) {
            *value = if value.is_finite() {
                value.clamp(minimum, 10000.)
            } else {
                minimum
            };
        }
        self
    }
}

#[derive(Clone)]
pub struct SettingsStore {
    path: PathBuf,
}

impl Default for SettingsStore {
    fn default() -> Self {
        let directory = if let Some(path) = std::env::var_os("STILL_CONFIG_DIR") {
            PathBuf::from(path)
        } else if cfg!(target_os = "macos") {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
                .join("Library/Application Support/Still")
        } else if cfg!(target_os = "windows") {
            PathBuf::from(std::env::var_os("APPDATA").unwrap_or_default()).join("Still")
        } else {
            std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config")
                })
                .join("still")
        };
        Self {
            path: directory.join("preferences.json"),
        }
    }
}

impl SettingsStore {
    pub fn load(&self) -> Result<Preferences> {
        match fs::read(&self.path) {
            Ok(bytes) => Ok(serde_json::from_slice::<Preferences>(&bytes)?.normalized()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(Preferences::default())
            }
            Err(error) => Err(error.into()),
        }
    }

    fn save(&self, preferences: &Preferences) -> Result<()> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let parent = self.path.parent().context("Missing settings directory")?;
        fs::create_dir_all(parent)?;
        let temporary = parent.join(format!(
            "preferences-{}-{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let result = (|| {
            fs::write(&temporary, serde_json::to_vec_pretty(preferences)?)?;
            self.preserve_corrupt()?;
            fs::rename(&temporary, &self.path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(temporary);
        }
        result
    }

    fn preserve_corrupt(&self) -> Result<()> {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        if serde_json::from_slice::<Preferences>(&bytes).is_ok() {
            return Ok(());
        }
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let backup = self.path.with_file_name(format!(
            "preferences.corrupt-{stamp}-{}.json",
            std::process::id()
        ));
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&backup)
            .context("Could not preserve damaged preferences")?;
        if let Err(error) = file.write_all(&bytes).and_then(|_| file.sync_all()) {
            drop(file);
            let _ = fs::remove_file(&backup);
            return Err(error).context("Could not preserve damaged preferences");
        }
        crate::diagnostics::event("Preserved damaged preferences before replacing them");
        Ok(())
    }
}

// A new value debounces normal writes, but does not reset the backoff after a
// failure. Flush and shutdown always make an immediate final attempt.
#[derive(Default)]
struct SaveSchedule {
    deadline: Option<Instant>,
    failures: usize,
}
impl SaveSchedule {
    fn changed(&mut self, now: Instant) {
        if self.failures == 0 {
            self.deadline = Some(now + Duration::from_millis(400));
        }
    }
    fn completed(&mut self, success: bool, now: Instant) {
        if success {
            *self = Self::default();
        } else {
            let seconds = [1, 2, 5, 15, 30][self.failures.min(4)];
            self.failures = self.failures.saturating_add(1);
            self.deadline = Some(now + Duration::from_secs(seconds));
        }
    }
}

enum Write {
    Save(Preferences),
    Flush(oneshot::Sender<Result<()>>),
    Shutdown,
}

pub struct Settings {
    value: Preferences,
    sender: mpsc::Sender<Write>,
    worker: Option<thread::JoinHandle<()>>,
    events: mpsc::Receiver<Option<String>>,
    warning: Option<String>,
}

fn write_pending(store: &SettingsStore, pending: &mut Option<Preferences>) -> Result<()> {
    if let Some(value) = pending.as_ref() {
        store.save(value)?;
    }
    *pending = None;
    Ok(())
}

fn persist(
    store: &SettingsStore,
    pending: &mut Option<Preferences>,
    previous_error: &mut Option<String>,
    events: &mpsc::Sender<Option<String>>,
) -> Result<()> {
    let had_pending = pending.is_some();
    let result = write_pending(store, pending);
    match &result {
        Err(error) => {
            let detail = format!("{error:#}");
            if previous_error.as_ref() != Some(&detail) {
                crate::diagnostics::event(&format!("Could not save preferences: {detail}"));
                let _ = events.send(Some("Could not save preferences. Changes are kept in memory; saving will be retried.".into()));
                *previous_error = Some(detail);
            }
        }
        Ok(()) if had_pending => {
            *previous_error = None;
            let _ = events.send(None);
        }
        Ok(()) => {}
    }
    result
}

impl Settings {
    pub fn from_store(store: SettingsStore) -> Self {
        match store.load() {
            Ok(value) => Self::new(value, store),
            Err(error) => {
                crate::diagnostics::event(&format!(
                    "Could not read preferences; using defaults: {error:#}"
                ));
                let mut settings = Self::new(Preferences::default(), store);
                settings.warning = Some("Could not read preferences. Defaults are in use.".into());
                settings
            }
        }
    }

    pub fn new(value: Preferences, store: SettingsStore) -> Self {
        let (sender, receiver) = mpsc::channel();
        let (updates, events) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("still-settings".into())
            .spawn(move || {
                let mut pending = None;
                let mut previous_error = None;
                let mut schedule = SaveSchedule::default();
                loop {
                    let command = if let Some(deadline) = schedule.deadline {
                        receiver.recv_timeout(deadline.saturating_duration_since(Instant::now()))
                    } else {
                        receiver
                            .recv()
                            .map_err(|_| mpsc::RecvTimeoutError::Disconnected)
                    };
                    match command {
                        Ok(Write::Save(value)) => {
                            pending = Some(value);
                            schedule.changed(Instant::now());
                        }
                        Ok(Write::Flush(reply)) => {
                            let result =
                                persist(&store, &mut pending, &mut previous_error, &updates);
                            schedule.completed(result.is_ok(), Instant::now());
                            let _ = reply.send(result);
                        }
                        Ok(Write::Shutdown) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                            let _ = persist(&store, &mut pending, &mut previous_error, &updates);
                            break;
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            let result =
                                persist(&store, &mut pending, &mut previous_error, &updates);
                            schedule.completed(result.is_ok(), Instant::now());
                        }
                    }
                }
            });
        let (worker, warning) = match worker {
            Ok(worker) => (Some(worker), None),
            Err(error) => {
                crate::diagnostics::event(&format!("Could not start preferences worker: {error}"));
                (
                    None,
                    Some(
                        "Preferences cannot be saved: the background worker is unavailable.".into(),
                    ),
                )
            }
        };
        Self {
            value: value.normalized(),
            sender,
            worker,
            events,
            warning,
        }
    }

    pub fn warning(&self) -> Option<&str> {
        self.warning.as_deref()
    }

    pub fn poll(&mut self) -> bool {
        let before = self.warning.clone();
        while let Ok(warning) = self.events.try_recv() {
            self.warning = warning;
        }
        if self
            .worker
            .as_ref()
            .is_some_and(|worker| worker.is_finished())
        {
            self.warning =
                Some("Preferences cannot be saved: the background worker stopped.".into());
        }
        self.warning != before
    }

    pub fn value(&self) -> &Preferences {
        &self.value
    }

    pub fn update(&mut self, change: impl FnOnce(&mut Preferences)) {
        let previous = self.value.clone();
        change(&mut self.value);
        self.value = self.value.clone().normalized();
        if self.value != previous {
            let _ = self.sender.send(Write::Save(self.value.clone()));
        }
    }

    pub fn flush(&self) -> oneshot::Receiver<Result<()>> {
        let (sender, receiver) = oneshot::channel();
        let _ = self.sender.send(Write::Flush(sender));
        receiver
    }
}

impl Drop for Settings {
    fn drop(&mut self) {
        let _ = self.sender.send(Write::Shutdown);
        if let Some(worker) = self.worker.take()
            && worker.join().is_err()
        {
            crate::diagnostics::event("Preferences worker panicked during shutdown");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retries_back_off_keep_the_latest_deadline_and_reset_after_success() {
        let now = Instant::now();
        let mut schedule = SaveSchedule::default();
        schedule.changed(now);
        assert_eq!(schedule.deadline, Some(now + Duration::from_millis(400)));
        for seconds in [1, 2, 5, 15, 30, 30] {
            schedule.completed(false, now);
            assert_eq!(schedule.deadline, Some(now + Duration::from_secs(seconds)));
            schedule.changed(now + Duration::from_millis(50));
            assert_eq!(schedule.deadline, Some(now + Duration::from_secs(seconds)));
        }
        schedule.completed(true, now);
        assert!(schedule.deadline.is_none());
        schedule.changed(now);
        assert_eq!(schedule.deadline, Some(now + Duration::from_millis(400)));
    }

    #[test]
    fn failed_saves_report_a_warning_and_recover_without_losing_pending_changes() {
        let root =
            std::env::temp_dir().join(format!("still-settings-failure-{}", std::process::id()));
        fs::write(&root, b"not a directory").unwrap();
        let store = SettingsStore {
            path: root.join("preferences.json"),
        };
        let mut settings = Settings::new(Preferences::default(), store.clone());
        settings.update(|value| value.volume = 0.25);
        assert!(settings.flush().blocking_recv().unwrap().is_err());
        assert!(settings.poll());
        assert!(settings.warning().is_some());
        fs::remove_file(&root).unwrap();
        settings.update(|value| value.volume = 0.45);
        settings.flush().blocking_recv().unwrap().unwrap();
        assert!(settings.poll());
        assert!(settings.warning().is_none());
        assert_eq!(store.load().unwrap().volume, 0.45);
        drop(settings);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_preferences_use_defaults_and_explain_the_fallback() {
        let root =
            std::env::temp_dir().join(format!("still-settings-invalid-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("preferences.json");
        fs::write(&path, b"invalid json").unwrap();
        let store = SettingsStore { path: path.clone() };
        let mut settings = Settings::from_store(store.clone());
        assert_eq!(settings.value(), &Preferences::default());
        assert!(settings.warning().is_some());
        settings.update(|value| value.volume = 0.3);
        settings.flush().blocking_recv().unwrap().unwrap();
        let backups: Vec<_> = fs::read_dir(&root)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("preferences.corrupt-")
            })
            .collect();
        assert_eq!(backups.len(), 1);
        assert_eq!(fs::read(&backups[0]).unwrap(), b"invalid json");
        assert_eq!(store.load().unwrap().volume, 0.3);
        settings.update(|value| value.volume = 0.4);
        settings.flush().blocking_recv().unwrap().unwrap();
        assert_eq!(fs::read_dir(&root).unwrap().count(), 2);
        drop(settings);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn preferences_survive_restart_and_corrupt_files_fall_back_to_defaults() {
        let directory = std::env::temp_dir().join(format!("still-settings-{}", std::process::id()));
        let store = SettingsStore {
            path: directory.join("preferences.json"),
        };
        let mut settings = Settings::new(store.load().unwrap(), store.clone());
        settings.update(|value| {
            value.volume = 0.35;
            value.shuffle = true;
            value.sidebar_compact = false;
            value.last_section = Section::Albums;
            value.window_size = [980., 700.];
            value.last_folder = Some(SavedFolder {
                library_id: "fixture".into(),
                path: "/music/Сборник 🎵".into(),
                selected: Some(FolderSelection::Song("song-12".into())),
                filter: "  Theme  ".into(),
            });
        });
        settings.flush().blocking_recv().unwrap().unwrap();
        assert_eq!(store.load().unwrap(), *settings.value());
        drop(settings);
        let restored = store.load().unwrap();
        assert_eq!(restored.volume, 0.35);
        assert!(restored.shuffle);
        assert!(!restored.sidebar_compact);
        assert_eq!(restored.last_section, Section::Albums);
        assert_eq!(restored.window_size, [980., 700.]);
        assert_eq!(restored.last_folder.unwrap().filter, "  Theme  ");
        // The previous hide/show preference must never hide the sidebar now.
        fs::write(&store.path, br#"{"sidebar_visible":false}"#).unwrap();
        assert!(store.load().unwrap().sidebar_compact);
        fs::write(&store.path, b"broken JSON").unwrap();
        assert_eq!(store.load().unwrap_or_default(), Preferences::default());
        fs::write(&store.path, br#"{"volume":5,"window_size":[-1,0]}"#).unwrap();
        let restored = store.load().unwrap();
        assert_eq!(restored.volume, 1.);
        assert!(restored.sidebar_compact);
        assert_eq!(restored.window_size, [780., 640.]);
        fs::remove_dir_all(directory).unwrap();
    }
}
