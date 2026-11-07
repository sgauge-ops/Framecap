use parking_lot::Mutex;
use std::env;
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

static LOG_FILE: OnceLock<Mutex<Option<File>>> = OnceLock::new();
static STARTUP_LOGGED: OnceLock<()> = OnceLock::new();

/// Initializes the diagnostic sink and records process context once per process.
pub fn initialize() {
    file();
    STARTUP_LOGGED.get_or_init(|| {
        info(format!(
            "Framecap diagnostic session started; version={}",
            env!("CARGO_PKG_VERSION")
        ));
        info(format!("process_id={}", std::process::id()));
        info(format!(
            "executable={}",
            std::fs::read_link("/proc/self/exe")
                .map(|path| path.display().to_string())
                .unwrap_or_else(|error| format!("<unavailable: {error}>"))
        ));
        info(format!(
            "working_directory={}",
            env::current_dir()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|error| format!("<unavailable: {error}>"))
        ));
        for name in [
            "VK_LAYER_PATH",
            "VK_INSTANCE_LAYERS",
            "VK_DRIVER_FILES",
            "VK_ICD_FILENAMES",
            "DISPLAY",
            "WAYLAND_DISPLAY",
            "HOME",
            "XDG_CONFIG_HOME",
            "XDG_STATE_HOME",
            "STEAM_COMPAT_DATA_PATH",
            "SteamAppId",
            "SteamGameId",
        ] {
            let value = env::var(name).unwrap_or_else(|error| match error {
                env::VarError::NotPresent => "<unset>".to_owned(),
                env::VarError::NotUnicode(_) => "<non-unicode>".to_owned(),
            });
            info(format!("environment {name}={value}"));
        }
    });
}

/// Writes an informational diagnostic entry.
pub fn info(message: impl AsRef<str>) {
    write_line("INFO", message.as_ref());
}

/// Writes a warning diagnostic entry.
pub fn warn(message: impl AsRef<str>) {
    write_line("WARN", message.as_ref());
}

/// Writes an error diagnostic entry.
pub fn error(message: impl AsRef<str>) {
    write_line("ERROR", message.as_ref());
}

/// Formats and flushes one diagnostic entry to the configured sink.
fn write_line(level: &str, message: &str) {
    let timestamp_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    let message = message.replace(['\r', '\n'], " ");
    let line = format!(
        "unix_ms={timestamp_ms} {level} pid={} {message}\n",
        std::process::id()
    );
    let mut logger = file().lock();
    if let Some(file) = logger.as_mut() {
        if let Err(error) = file.write_all(line.as_bytes()).and_then(|()| file.flush()) {
            eprintln!("Framecap: diagnostic log write failed: {error}; message: {message}");
            *logger = None;
        }
    } else {
        eprint!("{line}");
    }
}

/// Returns the process-wide lazily initialized diagnostic file handle.
fn file() -> &'static Mutex<Option<File>> {
    LOG_FILE.get_or_init(|| Mutex::new(open_log_file()))
}

/// Opens the configured log using its selected clear-or-append policy.
fn open_log_file() -> Option<File> {
    let (settings, _, _) = crate::layer::settings::Settings::load();
    let append_log = settings.diagnostics.append_log;
    let path = settings
        .diagnostics
        .log_path
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(default_log_path);
    match open_log_file_at(&path, append_log) {
        Ok(file) => {
            eprintln!("Framecap: diagnostic log: {}", path.display());
            Some(file)
        }
        Err(error) => {
            eprintln!(
                "Framecap: unable to open diagnostic log {}: {error}; diagnostics will use stderr",
                path.display()
            );
            None
        }
    }
}

/// Opens a log path, creating its parent directory and applying retention mode.
fn open_log_file_at(path: &Path, append_log: bool) -> std::io::Result<File> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    let mut options = OpenOptions::new();
    options.create(true).write(true);
    if append_log {
        options.append(true);
    } else {
        options.truncate(true);
    }
    options.open(path)
}

/// Resolves the default diagnostic path from the current process environment.
fn default_log_path() -> PathBuf {
    default_log_path_from(
        env::var_os("XDG_STATE_HOME"),
        env::var_os("HOME"),
        env::temp_dir(),
        unsafe { libc::getuid() },
    )
}

/// Resolves a diagnostic path using XDG state, home, then the temporary directory.
fn default_log_path_from(
    state_home: Option<OsString>,
    home: Option<OsString>,
    temp_dir: PathBuf,
    uid: u32,
) -> PathBuf {
    if let Some(state_home) = state_home.filter(|path| !path.is_empty()) {
        return PathBuf::from(state_home)
            .join("framecap")
            .join("framecap.log");
    }
    if let Some(home) = home.filter(|path| !path.is_empty()) {
        return PathBuf::from(home)
            .join(".local")
            .join("state")
            .join("framecap")
            .join("framecap.log");
    }
    temp_dir.join(format!("framecap-{uid}.log"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// Checks the default path when an XDG state directory is configured.
    fn default_log_path_uses_xdg_state_home_when_available() {
        let expected = PathBuf::from("/tmp/framecap-test-state/framecap/framecap.log");
        assert_eq!(
            default_log_path_from(
                Some("/tmp/framecap-test-state".into()),
                Some("/tmp/framecap-test-home".into()),
                PathBuf::from("/tmp"),
                1000,
            ),
            expected
        );
    }

    #[test]
    /// Checks home-directory and temporary-directory path fallbacks.
    fn default_log_path_falls_back_to_home_then_temp_dir() {
        assert_eq!(
            default_log_path_from(
                None,
                Some("/tmp/framecap-test-home".into()),
                PathBuf::from("/tmp"),
                1000,
            ),
            PathBuf::from("/tmp/framecap-test-home/.local/state/framecap/framecap.log")
        );
        assert_eq!(
            default_log_path_from(None, None, PathBuf::from("/tmp"), 1000),
            PathBuf::from("/tmp/framecap-1000.log")
        );
    }

    #[test]
    /// Checks that the default retention mode clears old log contents.
    fn opening_a_log_clears_existing_entries_by_default() {
        let path = env::temp_dir().join(format!(
            "framecap-log-test-{}-{}.log",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(&path, "previous session").unwrap();
        let file = open_log_file_at(&path, false).unwrap();
        assert_eq!(file.metadata().unwrap().len(), 0);
        drop(file);
        fs::remove_file(path).unwrap();
    }

    #[test]
    /// Checks that append mode preserves and extends existing log contents.
    fn opening_a_log_preserves_existing_entries_when_requested() {
        let path = env::temp_dir().join(format!(
            "framecap-log-test-{}-{}.log",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(&path, "previous session").unwrap();
        let mut file = open_log_file_at(&path, true).unwrap();
        assert_eq!(
            file.metadata().unwrap().len(),
            "previous session".len() as u64
        );
        file.write_all(b"\nnew session").unwrap();
        drop(file);
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "previous session\nnew session"
        );
        fs::remove_file(path).unwrap();
    }
}
