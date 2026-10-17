use std::{
    backtrace::Backtrace,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;
static LOG: OnceLock<Mutex<File>> = OnceLock::new();

fn log_directory() -> PathBuf {
    if let Some(path) = std::env::var_os("STILL_LOG_DIR") {
        return path.into();
    }
    if let Some(path) = std::env::var_os("STILL_CONFIG_DIR") {
        return PathBuf::from(path).join("logs");
    }
    if cfg!(target_os = "macos") {
        PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join("Library/Logs/Still")
    } else if cfg!(target_os = "windows") {
        PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap_or_default()).join("Still/logs")
    } else {
        std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/state")
            })
            .join("still/logs")
    }
}

fn open_log(directory: &std::path::Path) -> io::Result<File> {
    fs::create_dir_all(directory)?;
    let path = directory.join("still.log");
    if fs::metadata(&path).is_ok_and(|metadata| metadata.len() >= MAX_LOG_BYTES) {
        let previous = directory.join("still.previous.log");
        if previous.exists() {
            fs::remove_file(&previous)?;
        }
        fs::rename(&path, previous)?;
    }
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

pub fn init() {
    match open_log(&log_directory()) {
        Ok(file) => {
            let _ = LOG.set(Mutex::new(file));
        }
        Err(error) => eprintln!("Could not open diagnostic log: {error}"),
    }
    event(&format!(
        "Started Still {} on {}/{}",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
    ));
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Record before unwinding: a panic in the native event loop can abort the process.
        event(&format!("PANIC: {info}\n{}", Backtrace::force_capture()));
        previous(info);
    }));
}

pub fn event(message: &str) {
    let Some(log) = LOG.get() else {
        return;
    };
    // Avoid a second panic or a deadlock if diagnostics themselves fail.
    let Ok(mut file) = log.try_lock() else {
        return;
    };
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let _ = writeln!(
        file,
        "timestamp={timestamp} pid={} {message}",
        std::process::id()
    );
    let _ = file.flush();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        process::Command,
        sync::atomic::{AtomicU64, Ordering},
    };

    fn temp_directory() -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        std::env::temp_dir().join(format!(
            "still-log-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ))
    }

    #[test]
    fn log_rotation_keeps_the_previous_session() {
        let directory = temp_directory();
        let mut file = open_log(&directory).unwrap();
        file.write_all(b"old session").unwrap();
        file.set_len(MAX_LOG_BYTES).unwrap();
        drop(file);
        let mut file = open_log(&directory).unwrap();
        file.write_all(b"new session").unwrap();
        drop(file);
        assert_eq!(
            fs::read(directory.join("still.log")).unwrap(),
            b"new session"
        );
        assert!(
            fs::read(directory.join("still.previous.log"))
                .unwrap()
                .starts_with(b"old session")
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn panic_is_saved_with_a_backtrace_before_the_process_exits() {
        let directory = temp_directory();
        let result = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "diagnostics::tests::panic_probe"])
            .env("STILL_DIAGNOSTICS_PROBE", "1")
            .env("STILL_LOG_DIR", &directory)
            .output()
            .unwrap();
        assert!(!result.status.success());
        let log = fs::read_to_string(directory.join("still.log")).unwrap();
        assert!(log.contains("Started Still"));
        assert!(log.contains("PANIC:"));
        assert!(log.contains("diagnostic probe: Кириллица 🎵"));
        assert!(
            log.contains("panic_probe"),
            "backtrace must include function names"
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn panic_probe() {
        if std::env::var_os("STILL_DIAGNOSTICS_PROBE").is_some() {
            init();
            panic!("diagnostic probe: Кириллица 🎵");
        }
    }
}
