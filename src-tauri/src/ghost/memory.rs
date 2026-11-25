//! Accepted-phrase memory (GT-17): what you accepted with Tab or Shift+Tab, kept on this Mac only.
//!
//! Bounded (at most [`CAP`] phrases of at most [`MAX_CHARS`] characters), de-duplicated case-insensitively,
//! most recent first. Phrases that look like secrets are never stored. Stored as `ghost-memory.json` in the app
//! config directory; the user can view and clear it in the Ghost card. How (and whether) it is used in prompts
//! is decided together with the personalization "context stack" (see docs/PRIVACY.md).

use super::redact;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const CAP: usize = 200;
pub const MAX_CHARS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Phrase {
    pub text: String,
    pub uses: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Memory {
    /// Most recent first.
    pub phrases: Vec<Phrase>,
}

/// Normalize a phrase for storage, or `None` when it should not be kept.
pub fn normalize(text: &str) -> Option<String> {
    let t = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let t = t.trim_matches(|c: char| c.is_whitespace()).to_string();
    // Single characters and bare punctuation are not worth remembering.
    if t.chars().filter(|c| c.is_alphanumeric()).count() < 2 {
        return None;
    }
    // Never keep anything that redaction would change (credentials, long opaque tokens).
    if redact::scrub(&t) != t {
        return None;
    }
    Some(
        t.chars()
            .take(MAX_CHARS)
            .collect::<String>()
            .trim_end()
            .to_string(),
    )
}

impl Memory {
    /// Record an accepted phrase. Returns true when the memory changed.
    pub fn record(&mut self, text: &str) -> bool {
        let Some(t) = normalize(text) else {
            return false;
        };
        let key = t.to_lowercase();
        let uses = match self
            .phrases
            .iter()
            .position(|p| p.text.to_lowercase() == key)
        {
            Some(i) => self.phrases.remove(i).uses.saturating_add(1),
            None => 1,
        };
        self.phrases.insert(0, Phrase { text: t, uses });
        self.phrases.truncate(CAP);
        true
    }

    pub fn clear(&mut self) {
        self.phrases.clear();
    }

    /// Forget one phrase (case-insensitive, like `record`). Returns true when it was there.
    pub fn forget(&mut self, text: &str) -> bool {
        let key = text.trim().to_lowercase();
        let before = self.phrases.len();
        self.phrases.retain(|p| p.text.to_lowercase() != key);
        self.phrases.len() != before
    }

    /// Change a phrase in place (keeps its position and use count). False when `old` is missing or `new`
    /// would not be kept (too short, or looks like a secret). A duplicate of `new` elsewhere is merged.
    pub fn edit(&mut self, old: &str, new: &str) -> bool {
        let Some(t) = normalize(new) else {
            return false;
        };
        let key = old.trim().to_lowercase();
        let Some(i) = self
            .phrases
            .iter()
            .position(|p| p.text.to_lowercase() == key)
        else {
            return false;
        };
        let new_key = t.to_lowercase();
        let mut uses = self.phrases[i].uses;
        let mut j = 0;
        self.phrases.retain(|p| {
            let keep = j == i || p.text.to_lowercase() != new_key;
            if !keep {
                uses = uses.saturating_add(p.uses);
            }
            j += 1;
            keep
        });
        if let Some(p) = self
            .phrases
            .iter_mut()
            .find(|p| p.text.to_lowercase() == key)
        {
            p.text = t;
            p.uses = uses;
        }
        true
    }

    /// Up to `n` phrases, most used first; ties keep the most recent first.
    pub fn most_used(&self, n: usize) -> Vec<String> {
        let mut v: Vec<&Phrase> = self.phrases.iter().collect();
        v.sort_by_key(|p| std::cmp::Reverse(p.uses)); // stable: recency order survives ties
        v.into_iter().take(n).map(|p| p.text.clone()).collect()
    }

    pub fn load(path: &Path) -> Self {
        let mut m: Memory = std::fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        // Re-apply the invariants to whatever was on disk.
        let old = std::mem::take(&mut m.phrases);
        for p in old.into_iter().rev() {
            if m.record(&p.text) {
                m.phrases[0].uses = p.uses.max(1);
            }
        }
        m
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        crate::fsutil::write_private_file(
            &tmp,
            serde_json::to_string_pretty(self).map_err(std::io::Error::other)?,
        )?;
        std::fs::rename(tmp, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn editing_keeps_position_merges_duplicates_and_refuses_secrets() {
        let mut m = Memory::default();
        for t in ["Best regards", "Kind regards", "see you soon"] {
            m.record(t);
        }
        m.record("Kind regards");
        // Most recent first: Kind regards (2), see you soon, Best regards.
        assert!(m.edit("see you soon", "See you soon!"));
        assert_eq!(m.phrases[1].text, "See you soon!");
        assert!(m.edit("best regards", "Kind regards"));
        assert_eq!(m.phrases.len(), 2);
        let kind = m.phrases.iter().find(|p| p.text == "Kind regards").unwrap();
        assert_eq!(kind.uses, 3);
        assert!(!m.edit("missing", "anything"));
        assert!(!m.edit("Kind regards", "x"));
        assert!(!m.edit("Kind regards", &format!("sk-{}", "Ab1".repeat(12))));
    }

    #[test]
    fn records_most_recent_first_and_counts_reuse() {
        let mut m = Memory::default();
        assert!(m.record(" as soon as I can."));
        assert!(m.record("Best regards, Zsófi"));
        assert!(m.record("AS SOON AS I CAN."));
        assert_eq!(
            m.phrases[0],
            Phrase {
                text: "AS SOON AS I CAN.".into(),
                uses: 2
            }
        );
        assert_eq!(m.phrases.len(), 2);
    }

    #[test]
    fn ignores_trivia_and_secrets() {
        let mut m = Memory::default();
        for t in ["", " ", ".", "a", " ,"] {
            assert!(!m.record(t), "{t:?}");
        }
        let key = format!("sk-{}", "Ab1".repeat(12));
        assert!(!m.record(&format!("my key {key}")));
        assert!(m.phrases.is_empty());
    }

    #[test]
    fn most_used_ranks_by_uses_then_recency() {
        let mut m = Memory::default();
        m.record("see you soon");
        m.record("best regards");
        m.record("thanks a lot");
        m.record("see you soon");
        assert_eq!(
            m.most_used(2),
            vec!["see you soon".to_string(), "thanks a lot".to_string()]
        );
        assert!(Memory::default().most_used(5).is_empty());
    }

    #[test]
    fn single_phrases_can_be_forgotten() {
        let mut m = Memory::default();
        m.record("see you soon");
        m.record("thanks a lot");
        assert!(m.forget(" SEE YOU SOON "));
        assert!(!m.forget("never stored"));
        assert_eq!(m.phrases.len(), 1);
        assert_eq!(m.phrases[0].text, "thanks a lot");
    }

    #[test]
    fn long_phrases_are_truncated() {
        let mut m = Memory::default();
        m.record(&"word ".repeat(40));
        assert!(m.phrases[0].text.chars().count() <= MAX_CHARS);
    }

    #[test]
    fn persists_and_repairs_files_on_load() {
        let dir = std::env::temp_dir().join(format!("bob-mem-{}", rand::random::<u32>()));
        let p = dir.join("ghost-memory.json");
        let mut m = Memory::default();
        m.record("see you soon");
        m.record("thanks a lot");
        m.save(&p).unwrap();
        assert_eq!(Memory::load(&p), m);
        // A hand-edited file with duplicates is repaired.
        std::fs::write(
            &p,
            r#"{"phrases":[{"text":"hi there","uses":3},{"text":"HI THERE","uses":1}]}"#,
        )
        .unwrap();
        assert_eq!(Memory::load(&p).phrases.len(), 1);
        assert_eq!(Memory::load(&dir.join("missing.json")), Memory::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    proptest! {
        /// GT-17 / P6: never more than CAP, no duplicates, the latest recorded phrase is first.
        #[test]
        fn memory_stays_bounded_unique_and_recent_first(phrases in proptest::collection::vec("[a-zA-Z ]{0,20}", 0..400)) {
            let mut m = Memory::default();
            for p in &phrases {
                let changed = m.record(p);
                if changed {
                    prop_assert_eq!(m.phrases[0].text.to_lowercase(), normalize(p).unwrap().to_lowercase());
                }
                prop_assert!(m.phrases.len() <= CAP);
                let mut keys: Vec<String> = m.phrases.iter().map(|x| x.text.to_lowercase()).collect();
                let n = keys.len();
                keys.sort();
                keys.dedup();
                prop_assert_eq!(keys.len(), n);
                prop_assert!(m.phrases.iter().all(|x| x.text.chars().count() <= MAX_CHARS && x.uses >= 1));
            }
        }
    }
}
