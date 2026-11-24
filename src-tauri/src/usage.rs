//! Local usage statistics (Statistics panel): how much BOB wrote for you, per day and per feature.
//!
//! Counts only, never text: words accepted from Ghost Text, words dictated, rewrites applied, and their
//! timings. Stored in `usage.json` in the config directory, at most [`MAX_DAYS`] days, and clearable.
//! A process-wide recorder so engines can count without new plumbing; tests use the in-memory default.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

pub const FILE: &str = "usage.json";
pub const MAX_DAYS: usize = 400;
/// Typing speed used to estimate time saved (words per minute, a common average for adults).
pub const TYPING_WPM: f64 = 40.0;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Day {
    /// Words inserted by accepting Ghost Text suggestions (Tab or Shift+Tab).
    pub ghost_words: u64,
    pub ghost_accepts: u64,
    /// Suggestions shown (for the acceptance rate).
    pub ghost_shown: u64,
    pub dictated_words: u64,
    pub dictations: u64,
    /// Milliseconds of speech dictated.
    pub dictated_ms: u64,
    pub rewrites: u64,
    pub rewrite_failures: u64,
    pub voice_edits: u64,
}

impl Day {
    pub fn add(&mut self, o: &Day) {
        self.ghost_words += o.ghost_words;
        self.ghost_accepts += o.ghost_accepts;
        self.ghost_shown += o.ghost_shown;
        self.dictated_words += o.dictated_words;
        self.dictations += o.dictations;
        self.dictated_ms += o.dictated_ms;
        self.rewrites += o.rewrites;
        self.rewrite_failures += o.rewrite_failures;
        self.voice_edits += o.voice_edits;
    }

    /// Words BOB produced that you did not type (Ghost and dictation).
    pub fn words_for_you(&self) -> u64 {
        self.ghost_words + self.dictated_words
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Usage {
    /// "YYYY-MM-DD" (local) to that day's counts.
    pub days: BTreeMap<String, Day>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UsageView {
    pub today: Day,
    pub week: Day,
    pub all_time: Day,
    /// The last 14 days, oldest first (missing days are zero).
    pub recent: Vec<(String, Day)>,
    pub minutes_saved_week: f64,
    pub minutes_saved_all: f64,
    pub since: Option<String>,
}

impl Usage {
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(
            &tmp,
            serde_json::to_string(self).map_err(std::io::Error::other)?,
        )?;
        std::fs::rename(tmp, path)
    }

    pub fn record(&mut self, day: &str, f: impl FnOnce(&mut Day)) {
        f(self.days.entry(day.to_string()).or_default());
        while self.days.len() > MAX_DAYS {
            let oldest = self.days.keys().next().cloned();
            if let Some(k) = oldest {
                self.days.remove(&k);
            }
        }
    }

    /// Summary for `today` ("YYYY-MM-DD") and the 14 dates ending with it (oldest first).
    pub fn view(&self, today: &str, last14: &[String]) -> UsageView {
        let mut all = Day::default();
        for d in self.days.values() {
            all.add(d);
        }
        let recent: Vec<(String, Day)> = last14
            .iter()
            .map(|k| (k.clone(), self.days.get(k).cloned().unwrap_or_default()))
            .collect();
        let mut week = Day::default();
        for (_, d) in recent.iter().rev().take(7) {
            week.add(d);
        }
        let minutes = |d: &Day| d.words_for_you() as f64 / TYPING_WPM;
        UsageView {
            today: self.days.get(today).cloned().unwrap_or_default(),
            minutes_saved_week: minutes(&week),
            minutes_saved_all: minutes(&all),
            week,
            all_time: all,
            recent,
            since: self.days.keys().next().cloned(),
        }
    }
}

/// Word count as users understand it (whitespace-separated runs that contain a letter or digit).
pub fn words(text: &str) -> u64 {
    text.split_whitespace()
        .filter(|w| w.chars().any(char::is_alphanumeric))
        .count() as u64
}

// ---- process-wide recorder ----

struct Recorder {
    path: Option<PathBuf>,
    usage: Usage,
    dirty_since: Option<Instant>,
}

fn recorder() -> &'static Mutex<Recorder> {
    static R: OnceLock<Mutex<Recorder>> = OnceLock::new();
    R.get_or_init(|| {
        Mutex::new(Recorder {
            path: None,
            usage: Usage::default(),
            dirty_since: None,
        })
    })
}

/// Load and persist usage at `path` (the app). Without this call usage stays in memory (tests, benchmark).
pub fn init(path: PathBuf) {
    let mut r = recorder().lock().expect("usage");
    r.usage = Usage::load(&path);
    r.path = Some(path);
}

/// Count something for today. Saved at most every 30 seconds (and by [`flush`] at exit).
pub fn record(f: impl FnOnce(&mut Day)) {
    let mut r = recorder().lock().expect("usage");
    let today = crate::pause::local_date(0);
    r.usage.record(&today, f);
    let due = r
        .dirty_since
        .is_none_or(|t| t.elapsed() >= Duration::from_secs(30));
    if due {
        if let Some(p) = r.path.clone() {
            let _ = r.usage.save(&p);
        }
        r.dirty_since = Some(Instant::now());
    }
}

pub fn flush() {
    let r = recorder().lock().expect("usage");
    if let Some(p) = &r.path {
        let _ = r.usage.save(p);
    }
}

pub fn view() -> UsageView {
    let r = recorder().lock().expect("usage");
    let last14: Vec<String> = (0..14).rev().map(crate::pause::local_date).collect();
    r.usage.view(&last14[13], &last14)
}

pub fn clear() {
    let mut r = recorder().lock().expect("usage");
    r.usage = Usage::default();
    if let Some(p) = r.path.clone() {
        let _ = r.usage.save(&p);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_words_like_people_do() {
        assert_eq!(words(" Hello there, friend! "), 3);
        assert_eq!(words("— , ."), 0);
        assert_eq!(words("état d'art 2026"), 3);
    }

    #[test]
    fn summarizes_today_week_and_all_time() {
        let mut u = Usage::default();
        u.record("2026-11-10", |d| d.dictated_words = 40);
        u.record("2026-11-20", |d| d.ghost_words = 20);
        u.record("2026-11-24", |d| {
            d.ghost_words = 10;
            d.rewrites = 2;
        });
        let last14: Vec<String> = (11..=24).map(|d| format!("2026-11-{d:02}")).collect();
        let v = u.view("2026-11-24", &last14);
        assert_eq!(v.today.ghost_words, 10);
        assert_eq!(v.week.words_for_you(), 30, "only the last 7 days count");
        assert_eq!(v.all_time.words_for_you(), 70);
        assert_eq!(v.recent.len(), 14);
        assert!((v.minutes_saved_all - 70.0 / TYPING_WPM).abs() < 1e-9);
        assert_eq!(v.since.as_deref(), Some("2026-11-10"));
    }

    #[test]
    fn keeps_a_bounded_history_and_round_trips() {
        let mut u = Usage::default();
        for i in 0..(MAX_DAYS + 5) {
            u.record(&format!("d{i:05}"), |d| d.dictations += 1);
        }
        assert_eq!(u.days.len(), MAX_DAYS);
        assert!(!u.days.contains_key("d00000"), "oldest days are dropped");
        let p = std::env::temp_dir().join(format!("bob-usage-{}.json", rand::random::<u32>()));
        u.save(&p).unwrap();
        assert_eq!(Usage::load(&p), u);
        let _ = std::fs::remove_file(&p);
    }
}
