//! Selection Actions: the built-in actions, the user's custom ones, and their settings (SA-3).

use serde::{Deserialize, Serialize};
use std::path::Path;

pub const SETTINGS_FILE: &str = "selection-settings.json";
pub const MAX_CUSTOM: usize = 12;
pub const MAX_LABEL_CHARS: usize = 40;
pub const MAX_INSTRUCTION_CHARS: usize = 1000;

/// What an action asks the brain to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    pub label: String,
    /// Shown while the brain works ("Fixing grammar").
    pub progress: String,
    pub instruction: String,
    /// The action decides the output language itself (Translate), so the "answer in" rule is left out.
    pub sets_language: bool,
    /// Summaries are short; rewrites are about as long as the input.
    pub summary: bool,
}

/// A user-defined action ("Custom prompt", SA-3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomAction {
    pub label: String,
    pub instruction: String,
}

/// The hotkey as stored: macOS key code plus modifier names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HotkeySetting {
    pub key_code: u32,
    pub modifiers: Vec<String>,
}

impl Default for HotkeySetting {
    /// Cmd+Shift+J (spec SA-1). `kVK_ANSI_J` is 38.
    fn default() -> Self {
        Self {
            key_code: 38,
            modifiers: vec!["cmd".into(), "shift".into()],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SelectionSettings {
    pub hotkey: HotkeySetting,
    pub custom: Vec<CustomAction>,
}

impl SelectionSettings {
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

    /// Bounded and cleaned: empty actions dropped, labels single-line, at most `MAX_CUSTOM`.
    pub fn sanitized(mut self) -> Self {
        self.custom = self
            .custom
            .into_iter()
            .filter_map(|c| {
                let label: String = c
                    .label
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .chars()
                    .take(MAX_LABEL_CHARS)
                    .collect();
                let instruction: String = c
                    .instruction
                    .trim()
                    .chars()
                    .take(MAX_INSTRUCTION_CHARS)
                    .collect();
                (!label.is_empty() && !instruction.is_empty())
                    .then_some(CustomAction { label, instruction })
            })
            .take(MAX_CUSTOM)
            .collect();
        self
    }
}

fn builtin(label: &str, progress: &str, instruction: &str) -> Action {
    Action {
        label: label.into(),
        progress: progress.into(),
        instruction: instruction.into(),
        sets_language: false,
        summary: false,
    }
}

/// The menu, in order: built-ins, then the user's custom actions. `language` is the Personalize setting
/// (empty: English is the translation target).
pub fn menu(settings: &SelectionSettings, language: &str) -> Vec<Action> {
    let target = if language.trim().is_empty() {
        "English"
    } else {
        language.trim()
    };
    let mut v = vec![
        builtin(
            "Fix grammar",
            "Fixing grammar",
            "Task: correct the spelling, grammar and punctuation of the user's text. Keep the wording, meaning, \
             tone, line breaks and formatting otherwise unchanged. If it is already correct, return it unchanged.",
        ),
        builtin(
            "Rephrase",
            "Rephrasing",
            "Task: rewrite the user's text so it reads clearly and naturally. Keep the meaning, the language and \
             about the same length.",
        ),
        builtin(
            "Make shorter",
            "Shortening",
            "Task: make the user's text shorter and more direct. Keep every key fact.",
        ),
        builtin(
            "Make friendlier",
            "Warming it up",
            "Task: rewrite the user's text in a warmer, friendlier tone. Keep the meaning.",
        ),
        builtin(
            "Make professional",
            "Polishing",
            "Task: rewrite the user's text in a clear, professional tone. Keep the meaning.",
        ),
        Action {
            summary: true,
            ..builtin(
                "Summarize",
                "Summarizing",
                "Task: summarize the user's text in a few sentences, or a short bulleted list if it is long.",
            )
        },
        Action {
            sets_language: true,
            ..builtin(
                &format!("Translate to {target}"),
                "Translating",
                &format!(
                    "Task: translate the user's text into {target}. Keep the formatting, names, numbers and \
                     links. If it is already in {target}, return it unchanged."
                ),
            )
        },
    ];
    v.extend(settings.custom.iter().map(|c| {
        builtin(
            &c.label,
            &c.label,
            &format!(
                "Task (defined by the user): {}\nApply it to the user's text.",
                c.instruction
            ),
        )
    }));
    v
}

/// Token budget for an action's output: summaries are short, rewrites are about as long as the input.
pub fn max_tokens(action: &Action, input_chars: usize) -> u32 {
    let approx_tokens = (input_chars / 3) as u32 + 16;
    if action.summary {
        (approx_tokens / 2).clamp(64, 400)
    } else {
        (approx_tokens * 3 / 2 + 32).clamp(64, 2048)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_menu_has_the_spec_built_ins_then_custom_actions() {
        let s = SelectionSettings {
            custom: vec![CustomAction {
                label: "Make it rhyme".into(),
                instruction: "Turn it into a rhyming couplet.".into(),
            }],
            ..Default::default()
        };
        let labels: Vec<String> = menu(&s, "").into_iter().map(|a| a.label).collect();
        for needed in [
            "Fix grammar",
            "Rephrase",
            "Summarize",
            "Translate to English",
        ] {
            assert!(labels.contains(&needed.to_string()), "{needed}");
        }
        assert_eq!(labels.last().unwrap(), "Make it rhyme");
        let t = menu(&s, "Hungarian")
            .into_iter()
            .find(|a| a.sets_language)
            .unwrap();
        assert_eq!(t.label, "Translate to Hungarian");
        assert!(t.instruction.contains("into Hungarian"));
    }

    #[test]
    fn custom_actions_are_sanitized_and_bounded() {
        let s = SelectionSettings {
            custom: (0..20)
                .map(|i| CustomAction {
                    label: format!("  Action\n{i}  "),
                    instruction: "x".repeat(5000),
                })
                .chain([CustomAction {
                    label: " ".into(),
                    instruction: "y".into(),
                }])
                .collect(),
            ..Default::default()
        }
        .sanitized();
        assert_eq!(s.custom.len(), MAX_CUSTOM);
        assert_eq!(s.custom[0].label, "Action 0");
        assert_eq!(s.custom[0].instruction.len(), MAX_INSTRUCTION_CHARS);
    }

    #[test]
    fn token_budgets_follow_the_input() {
        let menu = menu(&SelectionSettings::default(), "");
        let fix = &menu[0];
        let sum = menu.iter().find(|a| a.summary).unwrap();
        assert!(max_tokens(fix, 3000) > max_tokens(sum, 3000));
        assert_eq!(max_tokens(fix, 0), 64);
        assert_eq!(max_tokens(fix, 1_000_000), 2048);
    }

    #[test]
    fn settings_round_trip_and_default_hotkey_is_cmd_shift_j() {
        let dir = std::env::temp_dir().join(format!("bob-sel-{}", rand::random::<u32>()));
        let p = dir.join(SETTINGS_FILE);
        let d = SelectionSettings::load(&p);
        assert_eq!(d.hotkey.key_code, 38);
        assert_eq!(d.hotkey.modifiers, vec!["cmd", "shift"]);
        let s = SelectionSettings {
            hotkey: HotkeySetting {
                key_code: 49,
                modifiers: vec!["control".into(), "option".into()],
            },
            custom: vec![CustomAction {
                label: "Emoji".into(),
                instruction: "Add fitting emoji.".into(),
            }],
        };
        s.save(&p).unwrap();
        assert_eq!(SelectionSettings::load(&p), s);
        std::fs::write(&p, "{nope").unwrap();
        assert_eq!(SelectionSettings::load(&p), SelectionSettings::default());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
