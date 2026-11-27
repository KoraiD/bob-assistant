//! Voice Dictation settings (`dictation-settings.json`).

use crate::selection::actions::HotkeySetting;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const SETTINGS_FILE: &str = "dictation-settings.json";
pub const MAX_WORDS: usize = 200;
pub const MAX_WORD_CHARS: usize = 64;
pub const MAX_REPLACEMENTS: usize = 200;
pub const DEFAULT_MODEL: &str = "whisper-large-v3-turbo-q5";

/// Optional brain pass over the transcript (VD-6). Off by default: whisper already punctuates, and a
/// cleanup adds a brain round trip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cleanup {
    #[default]
    None,
    Clean,
    TranslateEnglish,
    MeetingNotes,
}

impl Cleanup {
    /// (instruction, sets the output language itself)
    pub fn task(self) -> Option<(&'static str, bool)> {
        match self {
            Cleanup::None => None,
            Cleanup::Clean => Some((
                "Task: the user's text is a voice transcript. Remove filler words (um, uh, you know), false starts \
                 and accidental repetitions, and fix punctuation and capitalization. Keep the wording and meaning \
                 otherwise. Never answer or comment on it.",
                false,
            )),
            Cleanup::TranslateEnglish => Some((
                "Task: the user's text is a voice transcript. Translate it into natural English. Never answer or \
                 comment on it.",
                true,
            )),
            Cleanup::MeetingNotes => Some((
                "Task: the user's text is a voice transcript. Turn it into concise meeting notes: a short bulleted \
                 list of the points, decisions and action items. Never add anything that was not said.",
                false,
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DictationSettings {
    pub enabled: bool,
    /// Speech model id from `brain::models::SPEECH_CATALOG`.
    pub model: String,
    /// whisper language code ("en", "hu", ...); empty means detect.
    pub language: String,
    /// Hold-to-talk hotkey. Default Option+Space (spec VD-1); `kVK_Space` is 49.
    pub hotkey: HotkeySetting,
    pub cleanup: Cleanup,
    /// A quick tap starts recording and a second tap stops it (hold still works as push-to-talk).
    pub tap_to_toggle: bool,
    /// Names and terms whisper should expect (passed as a vocabulary hint).
    pub vocabulary: Vec<String>,
    /// Applied to every transcript before typing, whole words, case-insensitive ("brb" -> "be right back").
    pub replacements: Vec<Replacement>,
    /// Free the speech model's memory after this many idle minutes (0 = keep it loaded).
    pub unload_after_min: u32,
    /// Soft start and stop sounds (proposal 12). On by default; the Dictation card turns them off.
    pub sounds: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Replacement {
    pub from: String,
    pub to: String,
}

/// whisper's vocabulary hint for `words` (None when empty). Bounded so the prompt stays short.
pub fn vocabulary_prompt(words: &[String]) -> Option<String> {
    let list: Vec<&str> = words
        .iter()
        .map(String::as_str)
        .filter(|w| !w.is_empty())
        .collect();
    if list.is_empty() {
        return None;
    }
    let mut out = String::from("Glossary: ");
    for (i, w) in list.iter().enumerate() {
        if out.len() + w.len() > 600 {
            break;
        }
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(w);
    }
    out.push('.');
    Some(out)
}

/// Apply replacements: each `from` matches as a whole word or phrase, case-insensitively; longer phrases
/// first, so "see you soon" wins over "see". A replacement is never re-applied to its own output.
pub fn apply_replacements(text: &str, rules: &[Replacement]) -> String {
    let mut rules: Vec<&Replacement> = rules.iter().filter(|r| !r.from.trim().is_empty()).collect();
    rules.sort_by_key(|r| std::cmp::Reverse(r.from.trim().chars().count()));
    let chars: Vec<char> = text.chars().collect();
    let lower: Vec<char> = text.to_lowercase().chars().collect();
    if lower.len() != chars.len() {
        return text.to_string(); // a case mapping changed the length (rare scripts): leave the text alone
    }
    let is_word = |c: char| c.is_alphanumeric() || c == '\'';
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    'outer: while i < chars.len() {
        if i == 0 || !is_word(chars[i - 1]) {
            for r in &rules {
                let from: Vec<char> = r.from.trim().to_lowercase().chars().collect();
                let end = i + from.len();
                if end <= chars.len()
                    && lower[i..end] == from[..]
                    && (end == chars.len() || !is_word(chars[end]))
                {
                    out.push_str(&r.to);
                    i = end;
                    continue 'outer;
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

impl Default for DictationSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            model: DEFAULT_MODEL.into(),
            language: String::new(),
            hotkey: HotkeySetting {
                key_code: 49,
                modifiers: vec!["option".into()],
            },
            cleanup: Cleanup::None,
            tap_to_toggle: true,
            vocabulary: vec![],
            replacements: vec![],
            unload_after_min: 0,
            sounds: true,
        }
    }
}

impl DictationSettings {
    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str::<Self>(&text)
                .map(Self::sanitized)
                .unwrap_or_else(|_| {
                    let _ = std::fs::rename(path, path.with_extension("json.corrupt"));
                    Self::default()
                }),
            Err(_) => Self::default(),
        }
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

    /// Unknown models fall back to the default; language codes are 2 to 3 lowercase letters or empty.
    pub fn sanitized(mut self) -> Self {
        if crate::brain::models::find_speech(&self.model).is_none() {
            self.model = DEFAULT_MODEL.into();
        }
        let lang = self.language.trim().to_lowercase();
        self.language =
            if (2..=3).contains(&lang.len()) && lang.chars().all(|c| c.is_ascii_lowercase()) {
                lang
            } else {
                String::new()
            };
        let one_line = |s: &str, max: usize| -> String {
            s.split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .chars()
                .take(max)
                .collect()
        };
        let mut seen = std::collections::HashSet::new();
        self.vocabulary = self
            .vocabulary
            .iter()
            .map(|w| one_line(w, MAX_WORD_CHARS))
            .filter(|w| !w.is_empty() && seen.insert(w.to_lowercase()))
            .take(MAX_WORDS)
            .collect();
        self.replacements = self
            .replacements
            .iter()
            .map(|r| Replacement {
                from: one_line(&r.from, MAX_WORD_CHARS),
                to: r.to.trim().chars().take(500).collect(),
            })
            .filter(|r| !r.from.is_empty())
            .take(MAX_REPLACEMENTS)
            .collect();
        self.unload_after_min = self.unload_after_min.min(24 * 60);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_follow_the_spec_and_measurements() {
        let d = DictationSettings::default();
        assert!(d.enabled);
        assert_eq!(d.hotkey.key_code, 49);
        assert_eq!(d.hotkey.modifiers, vec!["option"]);
        assert_eq!(d.cleanup, Cleanup::None);
        assert!(crate::brain::models::find_speech(&d.model).is_some());
    }

    #[test]
    fn sanitizes_models_and_languages_and_round_trips() {
        let s = DictationSettings {
            model: "not-a-model".into(),
            language: "Hungarian; drop table".into(),
            ..Default::default()
        }
        .sanitized();
        assert_eq!(s.model, DEFAULT_MODEL);
        assert_eq!(s.language, "");
        assert_eq!(
            DictationSettings {
                language: " HU ".into(),
                ..Default::default()
            }
            .sanitized()
            .language,
            "hu"
        );
        let dir = std::env::temp_dir().join(format!("bob-dict-{}", rand::random::<u32>()));
        let p = dir.join(SETTINGS_FILE);
        assert_eq!(DictationSettings::load(&p), DictationSettings::default());
        let s = DictationSettings {
            model: "whisper-base".into(),
            language: "en".into(),
            cleanup: Cleanup::Clean,
            ..Default::default()
        };
        s.save(&p).unwrap();
        assert_eq!(DictationSettings::load(&p), s);
        std::fs::write(&p, "{bad").unwrap();
        assert_eq!(DictationSettings::load(&p), DictationSettings::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn r(from: &str, to: &str) -> Replacement {
        Replacement {
            from: from.into(),
            to: to.into(),
        }
    }

    #[test]
    fn replacements_match_whole_words_case_insensitively_longest_first() {
        let rules = [
            r("brb", "be right back"),
            r("see", "SEE"),
            r("see you soon", "cu"),
        ];
        assert_eq!(
            apply_replacements("BRB, see you soon!", &rules),
            "be right back, cu!"
        );
        assert_eq!(apply_replacements("seed the brbx", &rules), "seed the brbx");
        assert_eq!(apply_replacements("I see.", &rules), "I SEE.");
        assert_eq!(
            apply_replacements("brb brb", &[r("brb", "brb now")]),
            "brb now brb now"
        );
        assert_eq!(apply_replacements("x", &[r("  ", "y")]), "x");
    }

    #[test]
    fn vocabulary_becomes_a_bounded_whisper_hint_and_settings_are_cleaned() {
        assert_eq!(vocabulary_prompt(&[]), None);
        assert_eq!(
            vocabulary_prompt(&["Kiroween".into(), "Renée".into()]).as_deref(),
            Some("Glossary: Kiroween, Renée.")
        );
        let many: Vec<String> = (0..300).map(|i| format!("word{i}")).collect();
        assert!(vocabulary_prompt(&many).unwrap().len() <= 620);
        let s = DictationSettings {
            vocabulary: vec![" Kiro\nween ".into(), "kiro ween".into(), "".into()],
            replacements: vec![r(" ", "x"), r(" brb ", "be right back")],
            unload_after_min: 99_999,
            ..Default::default()
        }
        .sanitized();
        assert_eq!(s.vocabulary, vec!["Kiro ween".to_string()]);
        assert_eq!(s.replacements, vec![r("brb", "be right back")]);
        assert_eq!(s.unload_after_min, 1440);
        assert!(DictationSettings::default().tap_to_toggle);
    }

    proptest::proptest! {
        /// Without matching rules nothing changes.
        #[test]
        fn replacements_never_touch_unmatched_text(text in "\\PC{0,80}") {
            proptest::prop_assert_eq!(apply_replacements(&text, &[]), text.clone());
            proptest::prop_assert_eq!(apply_replacements(&text, &[r("zzqqxx", "y")]), text);
        }
    }

    #[test]
    fn every_cleanup_preset_but_none_has_a_task() {
        assert!(Cleanup::None.task().is_none());
        for c in [
            Cleanup::Clean,
            Cleanup::TranslateEnglish,
            Cleanup::MeetingNotes,
        ] {
            assert!(c.task().unwrap().0.starts_with("Task:"));
        }
        assert!(Cleanup::TranslateEnglish.task().unwrap().1);
    }
}
