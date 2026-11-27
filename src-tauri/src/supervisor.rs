//! Keeps the Swift helper running (requirement NB-5): spawn it, restart it with exponential backoff
//! when it exits, and stop it when the core stops.

use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// Backoff configuration. Delay for attempt `n` (0-based) is `base * 2^n`, capped at `max`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Backoff {
    pub base: Duration,
    pub max: Duration,
}

impl Default for Backoff {
    fn default() -> Self {
        Self {
            base: Duration::from_millis(250),
            max: Duration::from_secs(5),
        }
    }
}

impl Backoff {
    pub fn delay(&self, attempt: u32) -> Duration {
        let factor = 1u32.checked_shl(attempt.min(30)).unwrap_or(u32::MAX);
        self.base.saturating_mul(factor).min(self.max)
    }
}

/// Locate the helper binary: next to the core executable (inside the app bundle). Development builds also accept
/// `BOB_NATIVE_PATH` and the SwiftPM build folder; release builds never do (code review SEC-12).
pub fn locate_helper() -> Option<PathBuf> {
    if let Some(p) = crate::paths::dev_env("BOB_NATIVE_PATH") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let sibling = dir.join("bob-native");
    if sibling.is_file() {
        return Some(sibling);
    }
    if !cfg!(debug_assertions) {
        return None;
    }
    // Development: <repo>/native/bob-native/.build/{debug,release}/bob-native
    let mut cur = Some(dir);
    while let Some(d) = cur {
        for profile in ["debug", "release"] {
            let cand = d
                .join("native/bob-native/.build")
                .join(profile)
                .join("bob-native");
            if cand.is_file() {
                return Some(cand);
            }
        }
        cur = d.parent();
    }
    None
}

/// Handle to the supervising thread. Dropping it stops the helper.
pub struct Supervisor {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Supervisor {
    /// Start supervising `program` with the socket path passed through `BOB_SOCKET`.
    pub fn start(program: PathBuf, socket: &Path, backoff: Backoff) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_flag = stop.clone();
        let socket = socket.to_path_buf();
        let thread = thread::spawn(move || run_loop(&program, &socket, backoff, &stop_flag));
        Self {
            stop,
            thread: Some(thread),
        }
    }

    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        self.stop();
    }
}

fn run_loop(program: &Path, socket: &Path, backoff: Backoff, stop: &AtomicBool) {
    let mut attempt: u32 = 0;
    while !stop.load(Ordering::SeqCst) {
        let started = Instant::now();
        let child = Command::new(program)
            .env("BOB_SOCKET", socket)
            .stderr(std::process::Stdio::piped())
            .spawn();
        match child {
            Ok(mut child) => {
                forward_stderr(&mut child);
                wait_or_kill(&mut child, stop);
                // A helper that stayed up for a while is healthy: reset the backoff.
                if started.elapsed() > Duration::from_secs(10) {
                    attempt = 0;
                }
            }
            Err(e) => log::error!("failed to start native helper: {e}"),
        }
        if stop.load(Ordering::SeqCst) {
            break;
        }
        sleep_interruptible(backoff.delay(attempt), stop);
        attempt = attempt.saturating_add(1);
    }
}

/// The helper's stderr goes into BOB's log (target "helper"): its own messages at info, whisper and Metal
/// initialization chatter at debug. The helper never prints user text.
fn forward_stderr(child: &mut Child) {
    use std::io::BufRead;
    let Some(err) = child.stderr.take() else {
        return;
    };
    std::thread::spawn(move || {
        for line in std::io::BufReader::new(err).lines().map_while(Result::ok) {
            let line: String = line.chars().take(500).collect();
            if line.starts_with("bob-native") {
                log::info!(target: "helper", "{line}");
            } else {
                log::debug!(target: "helper", "{line}");
            }
        }
    });
}

fn wait_or_kill(child: &mut Child, stop: &AtomicBool) {
    loop {
        match child.try_wait() {
            Ok(Some(_)) | Err(_) => return,
            Ok(None) => {}
        }
        if stop.load(Ordering::SeqCst) {
            let _ = child.kill();
            let _ = child.wait();
            return;
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn sleep_interruptible(total: Duration, stop: &AtomicBool) {
    let deadline = Instant::now() + total;
    while Instant::now() < deadline && !stop.load(Ordering::SeqCst) {
        thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    proptest! {
        /// Backoff is monotonic non-decreasing and never exceeds the cap (BR-10 / NB-5).
        #[test]
        fn backoff_is_monotonic_and_capped(base_ms in 1u64..2000, max_ms in 1u64..60_000, a in 0u32..64) {
            let b = Backoff { base: Duration::from_millis(base_ms), max: Duration::from_millis(max_ms) };
            prop_assert!(b.delay(a) <= b.max);
            prop_assert!(b.delay(a) <= b.delay(a + 1));
        }
    }

    #[test]
    fn default_backoff_doubles_then_caps_at_five_seconds() {
        let b = Backoff::default();
        assert_eq!(b.delay(0), Duration::from_millis(250));
        assert_eq!(b.delay(1), Duration::from_millis(500));
        assert_eq!(b.delay(2), Duration::from_millis(1000));
        assert_eq!(b.delay(10), Duration::from_secs(5));
        assert_eq!(b.delay(u32::MAX), Duration::from_secs(5));
    }

    #[test]
    fn supervisor_restarts_a_short_lived_helper_and_stops_cleanly() {
        let dir = std::env::temp_dir().join(format!("bob-sup-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let counter = dir.join("starts");
        let script = dir.join("fake-helper.sh");
        fs::write(
            &script,
            format!("#!/bin/sh\necho x >> '{}'\nexit 1\n", counter.display()),
        )
        .unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();

        let fast = Backoff {
            base: Duration::from_millis(10),
            max: Duration::from_millis(40),
        };
        let mut sup = Supervisor::start(script, &dir.join("s.sock"), fast);
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let starts = fs::read_to_string(&counter)
                .map(|s| s.lines().count())
                .unwrap_or(0);
            if starts >= 3 {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "helper was not restarted (starts={starts})"
            );
            thread::sleep(Duration::from_millis(20));
        }
        sup.stop();
        let after = fs::read_to_string(&counter).unwrap().lines().count();
        thread::sleep(Duration::from_millis(200));
        let later = fs::read_to_string(&counter).unwrap().lines().count();
        assert_eq!(after, later, "helper kept restarting after stop()");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn locate_helper_honors_the_environment_override() {
        let dir = std::env::temp_dir().join(format!("bob-loc-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let fake = dir.join("bob-native");
        fs::write(&fake, "").unwrap();
        // Single-threaded section: set, check, unset.
        std::env::set_var("BOB_NATIVE_PATH", &fake);
        assert_eq!(locate_helper(), Some(fake.clone()));
        std::env::remove_var("BOB_NATIVE_PATH");
        let _ = fs::remove_dir_all(&dir);
    }
}
