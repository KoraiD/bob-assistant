//! Voice Dictation settings (`dictation-settings.json`).

use crate::selection::actions::HotkeySetting;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const SETTINGS_FILE: &str = "dictation-settings.json";
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
        std::fs::write(
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
