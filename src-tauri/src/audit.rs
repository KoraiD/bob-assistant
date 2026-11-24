//! Audit trail: an append-only record of what BOB did that matters for privacy and security, separate from
//! the debug log. One JSON object per line in `~/Library/Logs/BOB/audit.jsonl` (0600), rotated at 2 MB, and
//! the latest entries in memory for Settings > Activity.
//!
//! What is recorded: settings and permission changes, brains added, edited or removed (never keys), which
//! brain received text and how much (characters, context layers; never the text), MCP clients and tool
//! calls, pause and resume, data cleared. Callers pass metadata only; every value is redacted again here.

use serde::Serialize;
use serde_json::Value;
use std::collections::VecDeque;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

pub const FILE: &str = "audit.jsonl";
pub const MAX_BYTES: u64 = 2 * 1024 * 1024;
pub const KEEP: usize = 2;
pub const RECENT: usize = 300;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Entry {
    /// Unix milliseconds.
    pub at: u64,
    /// For example "settings.ghost", "brain.removed", "data.sent", "mcp.call".
    pub kind: String,
    pub detail: Value,
}

struct Trail {
    path: Option<PathBuf>,
    recent: VecDeque<Entry>,
}

fn trail() -> &'static Mutex<Trail> {
    static T: OnceLock<Mutex<Trail>> = OnceLock::new();
    T.get_or_init(|| {
        Mutex::new(Trail {
            path: None,
            recent: VecDeque::new(),
        })
    })
}

fn lock() -> std::sync::MutexGuard<'static, Trail> {
    trail()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Write to `dir/audit.jsonl` from now on (the app). Without it entries stay in memory (tests).
pub fn init(dir: &Path) {
    lock().path = Some(dir.join(FILE));
}

/// Redact every string inside `v` (defense in depth: callers already pass metadata only).
fn scrub(v: Value) -> Value {
    match v {
        Value::String(s) => {
            Value::String(crate::ghost::redact::scrub(&s).chars().take(300).collect())
        }
        Value::Array(a) => Value::Array(a.into_iter().map(scrub).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, scrub(v))).collect()),
        other => other,
    }
}

/// Record an event. Also logged at info level (target "audit") so it shows in the debug log.
pub fn record(kind: &str, detail: Value) {
    let entry = Entry {
        at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0),
        kind: kind.to_string(),
        detail: scrub(detail),
    };
    log::info!(target: "audit", "{} {}", entry.kind, entry.detail);
    let mut t = lock();
    if let Some(path) = t.path.clone() {
        append(&path, &entry);
    }
    t.recent.push_back(entry);
    while t.recent.len() > RECENT {
        t.recent.pop_front();
    }
}

fn append(path: &Path, entry: &Entry) {
    use std::os::unix::fs::OpenOptionsExt;
    if std::fs::metadata(path).is_ok_and(|m| m.len() > MAX_BYTES) {
        crate::logging::rotate_files(path, KEEP);
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(path)
    else {
        return;
    };
    if let Ok(line) = serde_json::to_string(entry) {
        let _ = writeln!(f, "{line}");
    }
}

/// The top-level fields that differ between two settings values. Booleans, numbers and short strings are
/// shown; free text and lists only as "(changed)", so a profile or vocabulary never lands in the trail.
pub fn diff<T: Serialize>(old: &T, new: &T) -> Value {
    let (Ok(Value::Object(a)), Ok(Value::Object(b))) =
        (serde_json::to_value(old), serde_json::to_value(new))
    else {
        return Value::Null;
    };
    let mut out = serde_json::Map::new();
    for (k, v) in &b {
        if a.get(k) == Some(v) {
            continue;
        }
        let shown = match v {
            Value::Bool(_) | Value::Number(_) | Value::Null => v.clone(),
            Value::String(s) if s.chars().count() <= 24 => v.clone(),
            Value::Object(o) if o.values().all(|x| x.is_boolean()) => v.clone(),
            _ => Value::String("(changed)".into()),
        };
        out.insert(k.clone(), shown);
    }
    Value::Object(out)
}

/// Record a settings change when anything changed.
pub fn settings<T: Serialize>(kind: &str, old: &T, new: &T) {
    let d = diff(old, new);
    if d.as_object().is_some_and(|o| !o.is_empty()) {
        record(kind, serde_json::json!({ "changed": d }));
    }
}

/// Text went to a brain: which feature, which brain, whether it is a cloud brain, how many characters and
/// which context layers came with it. Never the text.
pub fn sent(feature: &str, brain: &crate::brain::BrainConfig, chars: usize, context: bool) {
    record(
        "data.sent",
        serde_json::json!({
            "feature": feature,
            "brain": brain.id,
            "cloud": brain.is_cloud(),
            "chars": chars,
            "personal_context": context,
        }),
    );
}

/// Latest entries, newest first, optionally only kinds starting with `prefix`.
pub fn recent(prefix: Option<&str>, limit: usize) -> Vec<Entry> {
    lock()
        .recent
        .iter()
        .rev()
        .filter(|e| prefix.is_none_or(|p| e.kind.starts_with(p)))
        .take(limit)
        .cloned()
        .collect()
}

pub fn path() -> Option<PathBuf> {
    lock().path.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_are_redacted_bounded_and_filterable() {
        let dir = std::env::temp_dir().join(format!("bob-audit-{}", rand::random::<u32>()));
        init(&dir);
        record(
            "test.secret",
            serde_json::json!({"note": format!("key sk-{}", "Ab1".repeat(12)), "n": 3}),
        );
        record("test.other", serde_json::json!({"long": "x".repeat(1000)}));
        let e = recent(Some("test.secret"), 10);
        assert_eq!(e.len(), 1);
        assert!(!e[0].detail["note"].as_str().unwrap().contains("Ab1Ab1"));
        assert_eq!(e[0].detail["n"], 3);
        let other = recent(Some("test.other"), 10);
        assert_eq!(other[0].detail["long"].as_str().unwrap().len(), 300);
        let text = std::fs::read_to_string(dir.join(FILE)).unwrap();
        assert!(text.lines().count() >= 2 && !text.contains("Ab1Ab1"));
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(dir.join(FILE))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
