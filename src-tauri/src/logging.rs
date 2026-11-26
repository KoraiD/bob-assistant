//! Multi-level logging: stderr plus a rotating file in `~/Library/Logs/BOB/`, and the last lines in memory
//! for Settings > Logs. The level is a setting (error to trace) and can change while BOB runs; `RUST_LOG`
//! still wins when set (developers). Dependencies log at warn unless the level is trace.
//!
//! Privacy: BOB logs metadata only (sizes, timings, states). As a safety net every line also passes through
//! the same redaction as prompts, so a credential that slips into a message is still never written.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

pub const FILE: &str = "bob.log";
/// Rotate at this size; keep [`KEEP`] old files.
pub const MAX_BYTES: u64 = 5 * 1024 * 1024;
pub const KEEP: usize = 3;
/// Lines kept in memory for the UI.
pub const TAIL: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Error,
    Warn,
    #[default]
    Info,
    Debug,
    Trace,
}

impl Level {
    pub fn filter(self) -> log::LevelFilter {
        match self {
            Level::Error => log::LevelFilter::Error,
            Level::Warn => log::LevelFilter::Warn,
            Level::Info => log::LevelFilter::Info,
            Level::Debug => log::LevelFilter::Debug,
            Level::Trace => log::LevelFilter::Trace,
        }
    }
    fn from_usize(v: usize) -> log::LevelFilter {
        [
            log::LevelFilter::Off,
            log::LevelFilter::Error,
            log::LevelFilter::Warn,
            log::LevelFilter::Info,
            log::LevelFilter::Debug,
            log::LevelFilter::Trace,
        ][v.min(5)]
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Line {
    /// Unix milliseconds.
    pub at: u64,
    pub level: String,
    pub target: String,
    pub message: String,
}

struct Sink {
    path: Option<PathBuf>,
    file: Option<std::fs::File>,
    written: u64,
    tail: VecDeque<Line>,
}

struct Logger {
    /// BOB's own level (log::LevelFilter as usize).
    level: AtomicUsize,
    sink: Mutex<Sink>,
}

fn logger() -> &'static Logger {
    static L: OnceLock<Logger> = OnceLock::new();
    L.get_or_init(|| Logger {
        level: AtomicUsize::new(log::LevelFilter::Info as usize),
        sink: Mutex::new(Sink {
            path: None,
            file: None,
            written: 0,
            tail: VecDeque::new(),
        }),
    })
}

fn ours(target: &str) -> bool {
    target.starts_with("bob") || target == "helper" || target == "audit"
}

/// Whether a record at `level` from `target` passes with BOB's level `own` (pure, tested).
pub fn passes(target: &str, level: log::Level, own: log::LevelFilter) -> bool {
    let limit = if ours(target) {
        own
    } else if own == log::LevelFilter::Trace {
        log::LevelFilter::Debug
    } else {
        own.min(log::LevelFilter::Warn)
    };
    level <= limit
}

impl log::Log for Logger {
    fn enabled(&self, m: &log::Metadata) -> bool {
        passes(
            m.target(),
            m.level(),
            Level::from_usize(self.level.load(Ordering::Relaxed)),
        )
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let message = crate::ghost::redact::scrub(&record.args().to_string());
        let at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let line = Line {
            at,
            level: record.level().to_string(),
            target: record.target().to_string(),
            message,
        };
        let text = format!(
            "{} {:<5} {}: {}\n",
            crate::pause::local_stamp(at),
            line.level,
            line.target,
            line.message
        );
        #[cfg(not(test))]
        let _ = std::io::stderr().write_all(text.as_bytes());
        #[cfg(test)]
        capture::push(&text);
        let mut s = self
            .sink
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(f) = s.file.as_mut() {
            if f.write_all(text.as_bytes()).is_ok() {
                s.written += text.len() as u64;
            }
        }
        if s.written > MAX_BYTES {
            rotate(&mut s);
        }
        s.tail.push_back(line);
        while s.tail.len() > TAIL {
            s.tail.pop_front();
        }
    }

    fn flush(&self) {
        if let Some(f) = self
            .sink
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .file
            .as_mut()
        {
            let _ = f.flush();
        }
    }
}

fn open(path: &Path) -> Option<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(path)
        .ok()
}

/// `bob.log` -> `bob.log.1` -> ... -> `bob.log.KEEP` (dropped).
pub fn rotate_files(path: &Path, keep: usize) {
    for i in (1..keep).rev() {
        let from = PathBuf::from(format!("{}.{i}", path.display()));
        let to = PathBuf::from(format!("{}.{}", path.display(), i + 1));
        let _ = std::fs::rename(from, to);
    }
    let _ = std::fs::rename(path, PathBuf::from(format!("{}.1", path.display())));
}

fn rotate(s: &mut Sink) {
    let Some(path) = s.path.clone() else { return };
    s.file = None;
    rotate_files(&path, KEEP);
    s.file = open(&path);
    s.written = 0;
}

/// `~/Library/Logs/BOB` (macOS convention for app logs; Console.app shows it).
pub fn default_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("Library/Logs/BOB")
}

/// Install the logger (once). `RUST_LOG` (a level, or `bob_lib=<level>`) overrides the saved level.
pub fn init(dir: Option<&Path>, level: Level) {
    let l = logger();
    let own = env_override().unwrap_or(level.filter());
    l.level.store(own as usize, Ordering::Relaxed);
    if let Some(dir) = dir {
        use std::os::unix::fs::PermissionsExt;
        if std::fs::create_dir_all(dir).is_ok() {
            let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
            let path = dir.join(FILE);
            let mut s = l
                .sink
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            s.written = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            s.file = open(&path);
            s.path = Some(path);
        }
    }
    if log::set_logger(l).is_ok() {
        log::set_max_level(log::LevelFilter::Trace);
    }
}

fn env_override() -> Option<log::LevelFilter> {
    let v = std::env::var("RUST_LOG").ok()?;
    let pick = v
        .split(',')
        .find_map(|p| {
            p.strip_prefix("bob_lib=")
                .or_else(|| p.strip_prefix("bob="))
        })
        .or_else(|| v.split(',').find(|p| !p.contains('=')))?;
    pick.trim().parse().ok()
}

/// Change the level while running (Settings).
pub fn set_level(level: Level) {
    logger()
        .level
        .store(level.filter() as usize, Ordering::Relaxed);
    log::info!("log level set to {level:?}");
}

pub fn level() -> log::LevelFilter {
    Level::from_usize(logger().level.load(Ordering::Relaxed))
}

/// The last lines in memory, oldest first, at or above `min` severity.
pub fn tail(min: log::LevelFilter) -> Vec<Line> {
    logger()
        .sink
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .tail
        .iter()
        .filter(|l| l.level.parse::<log::Level>().is_ok_and(|lv| lv <= min))
        .cloned()
        .collect()
}

pub fn dir_in_use() -> Option<PathBuf> {
    logger()
        .sink
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .path
        .as_ref()
        .and_then(|p| p.parent().map(Path::to_path_buf))
}

/// Test builds: BOB's logger at trace level with every line kept in memory instead of stderr, so tests can
/// check that typed text, selections and transcripts never reach the log. The audit trail is logged too
/// (target "audit"), so this covers it.
#[cfg(test)]
pub mod capture {
    use std::sync::{Mutex, OnceLock};

    /// Enough for the longest test run; older lines are dropped first.
    const MAX_LINES: usize = 400_000;

    fn lines() -> &'static Mutex<std::collections::VecDeque<String>> {
        static L: OnceLock<Mutex<std::collections::VecDeque<String>>> = OnceLock::new();
        L.get_or_init(Default::default)
    }

    pub(super) fn push(line: &str) {
        let mut l = lines()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        l.push_back(line.to_string());
        if l.len() > MAX_LINES {
            l.pop_front();
        }
    }

    /// Install the logger at trace level (once per test process).
    pub fn start() {
        static ONCE: OnceLock<()> = OnceLock::new();
        ONCE.get_or_init(|| super::init(None, super::Level::Trace));
        super::logger().level.store(
            log::LevelFilter::Trace as usize,
            std::sync::atomic::Ordering::Relaxed,
        );
    }

    /// Every captured line that contains `needle`.
    pub fn find(needle: &str) -> Vec<String> {
        lines()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .filter(|l| l.contains(needle))
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_line_is_captured_in_tests_and_redacted() {
        capture::start();
        let secret = format!("sk-{}", "Qz7".repeat(12));
        log::info!("capture check {secret}");
        log::trace!("capture check trace");
        assert!(!capture::find("capture check").is_empty());
        assert!(
            !capture::find("capture check trace").is_empty(),
            "trace level is on"
        );
        assert!(capture::find("Qz7Qz7").is_empty(), "lines are redacted");
    }

    #[test]
    fn levels_apply_to_bob_and_dependencies_differently() {
        use log::{Level as L, LevelFilter as F};
        assert!(passes("bob_lib::ghost", L::Debug, F::Debug));
        assert!(!passes("bob_lib::ghost", L::Debug, F::Info));
        assert!(
            !passes("reqwest::connect", L::Info, F::Debug),
            "deps stay at warn"
        );
        assert!(passes("reqwest::connect", L::Warn, F::Debug));
        assert!(passes("hyper", L::Debug, F::Trace));
        assert!(passes("helper", L::Info, F::Info));
        assert!(!passes("bob_lib", L::Error, F::Off));
    }

    #[test]
    fn rotation_keeps_a_bounded_number_of_files() {
        let dir = std::env::temp_dir().join(format!("bob-log-{}", rand::random::<u32>()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(FILE);
        for i in 0..5 {
            std::fs::write(&p, format!("gen {i}")).unwrap();
            rotate_files(&p, 3);
        }
        let mut names: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        names.sort();
        assert_eq!(names, vec!["bob.log.1", "bob.log.2", "bob.log.3"]);
        assert_eq!(
            std::fs::read_to_string(dir.join("bob.log.1")).unwrap(),
            "gen 4"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
