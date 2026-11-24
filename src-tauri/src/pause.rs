//! "Pause BOB": every feature off for a while (an hour, until tomorrow morning, or until resumed), without
//! changing any saved setting. Similar tools offer the same, for screen sharing or recording a demo.
//! The deadline is kept in memory only: quitting BOB ends a pause.

use serde::Serialize;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PauseFor {
    Hour,
    /// Until 08:00 local time tomorrow.
    UntilTomorrow,
    /// Until the user resumes.
    Indefinitely,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PauseView {
    pub paused: bool,
    /// Unix seconds when the pause ends by itself; None when indefinite or not paused.
    pub until: Option<u64>,
}

#[derive(Debug, Default)]
pub struct Pause {
    /// None: running. Some(None): paused indefinitely. Some(Some(t)): paused until `t` (unix seconds).
    state: Mutex<Option<Option<u64>>>,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs()
}

/// Unix time of the next 08:00 local time "tomorrow", given the UTC offset (seconds east of UTC). Before
/// 05:00 the night is not over yet, so "tomorrow morning" is the coming 08:00 of the same date.
pub fn until_tomorrow_morning(now: u64, utc_offset: i64) -> u64 {
    let local = now as i64 + utc_offset;
    let day = local.div_euclid(86_400);
    let seconds_into_day = local.rem_euclid(86_400);
    let target_day = if seconds_into_day < 5 * 3600 {
        day
    } else {
        day + 1
    };
    let next_morning_local = target_day * 86_400 + 8 * 3600;
    (next_morning_local - utc_offset) as u64
}

/// The local UTC offset, read once per run (a DST switch applies after a restart).
fn local_utc_offset() -> i64 {
    static OFFSET: std::sync::OnceLock<i64> = std::sync::OnceLock::new();
    *OFFSET.get_or_init(read_utc_offset)
}

/// Local calendar date `days_ago` days before today, as "YYYY-MM-DD".
pub fn local_date(days_ago: i64) -> String {
    date_for(now_secs() as i64 + local_utc_offset() - days_ago * 86_400)
}

/// "YYYY-MM-DD" for local seconds since the epoch (civil-from-days, proleptic Gregorian).
pub fn date_for(local_secs: i64) -> String {
    let z = local_secs.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

/// The local UTC offset from the system's `date` (no extra crate; falls back to UTC).
fn read_utc_offset() -> i64 {
    std::process::Command::new("/bin/date")
        .arg("+%z")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| {
            let s = s.trim();
            let sign = if s.starts_with('-') { -1 } else { 1 };
            let digits = s.trim_start_matches(['+', '-']);
            let (h, m) = digits.split_at(digits.len().min(2));
            Some(sign * (h.parse::<i64>().ok()? * 3600 + m.parse::<i64>().unwrap_or(0) * 60))
        })
        .unwrap_or(0)
}

impl Pause {
    pub fn pause(&self, how: PauseFor) -> PauseView {
        let until = match how {
            PauseFor::Hour => Some(now_secs() + 3600),
            PauseFor::UntilTomorrow => Some(until_tomorrow_morning(now_secs(), local_utc_offset())),
            PauseFor::Indefinitely => None,
        };
        *self.state.lock().expect("pause") = Some(until);
        self.view()
    }

    pub fn resume(&self) -> PauseView {
        *self.state.lock().expect("pause") = None;
        self.view()
    }

    /// Current state; an expired pause counts as resumed (and is cleared).
    pub fn view(&self) -> PauseView {
        let mut s = self.state.lock().expect("pause");
        if let Some(Some(t)) = *s {
            if now_secs() >= t {
                *s = None;
            }
        }
        match *s {
            None => PauseView {
                paused: false,
                until: None,
            },
            Some(until) => PauseView {
                paused: true,
                until,
            },
        }
    }

    pub fn is_paused(&self) -> bool {
        self.view().paused
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pause_and_resume() {
        let p = Pause::default();
        assert!(!p.is_paused());
        let v = p.pause(PauseFor::Hour);
        assert!(v.paused);
        let left = v.until.unwrap() - now_secs();
        assert!((3590..=3600).contains(&left));
        assert_eq!(p.pause(PauseFor::Indefinitely).until, None);
        assert!(p.is_paused());
        assert!(!p.resume().paused);
    }

    #[test]
    fn an_expired_pause_resumes_by_itself() {
        let p = Pause::default();
        *p.state.lock().unwrap() = Some(Some(now_secs() - 1));
        assert!(!p.is_paused());
        assert_eq!(*p.state.lock().unwrap(), None);
    }

    #[test]
    fn dates_are_civil_dates() {
        assert_eq!(date_for(0), "1970-01-01");
        assert_eq!(date_for(1_795_478_400), "2026-11-24");
        assert_eq!(date_for(951_782_400), "2000-02-29");
        assert_eq!(date_for(-86_400), "1969-12-31");
        assert_eq!(local_date(0).len(), 10);
    }

    #[test]
    fn tomorrow_morning_is_eight_local_time_the_next_day() {
        // 2026-11-23 22:30 UTC.
        let now = 1_795_473_000;
        let utc = until_tomorrow_morning(now, 0);
        assert_eq!(utc % 86_400, 8 * 3600);
        assert!(utc > now && utc - now < 86_400);
        // Budapest (UTC+1): 08:00 local is 07:00 UTC.
        let cet = until_tomorrow_morning(now, 3600);
        assert_eq!(cet % 86_400, 7 * 3600);
        // Shortly after midnight, "tomorrow morning" is the coming 08:00, not the one a day later.
        let after_midnight = 1_795_478_400 + 600; // 00:10 UTC on the 24th
        assert_eq!(
            until_tomorrow_morning(after_midnight, 0) - after_midnight,
            8 * 3600 - 600
        );
    }
}
