//! Persisted Ghost Text settings: on/off, typo fixes, mid-sentence suggestions and the apps where BOB stays
//! quiet (GT-3).
//!
//! Quiet by default:
//! * terminals: Tab is shell completion there, and the "text before the caret" is the whole scrollback;
//! * password managers: never read or suggest near credentials;
//! * BOB itself.
//!
//! Stored as `ghost-settings.json` in the app config directory (no text, only bundle identifiers).

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Bundle identifiers that are quiet unless the user turns them back on.
pub const DEFAULT_QUIET: &[(&str, &str)] = &[
    ("com.apple.Terminal", "Terminal"),
    ("com.googlecode.iterm2", "iTerm"),
    ("dev.warp.Warp-Stable", "Warp"),
    ("com.mitchellh.ghostty", "Ghostty"),
    ("net.kovidgoyal.kitty", "kitty"),
    ("org.alacritty", "Alacritty"),
    ("com.github.wez.wezterm", "WezTerm"),
    ("com.1password.1password", "1Password"),
    ("com.agilebits.onepassword7", "1Password 7"),
    ("com.bitwarden.desktop", "Bitwarden"),
    ("com.apple.Passwords", "Passwords"),
    ("com.apple.keychainaccess", "Keychain Access"),
    ("dev.bob.assistant", "BOB"),
];

/// Upper bound on the quiet list.
pub const MAX_QUIET_APPS: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuietApp {
    pub bundle_id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GhostSettings {
    pub enabled: bool,
    pub quiet_apps: Vec<QuietApp>,
    /// Offer a fix for a misspelled word right after you finish it (Tab replaces it).
    pub typo_fixes: bool,
    /// Suggest inside a sentence too, using the words after the cursor.
    pub mid_sentence: bool,
}

impl Default for GhostSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            quiet_apps: DEFAULT_QUIET
                .iter()
                .map(|(b, n)| QuietApp {
                    bundle_id: (*b).into(),
                    name: (*n).into(),
                })
                .collect(),
            typo_fixes: true,
            mid_sentence: true,
        }
    }
}

impl GhostSettings {
    pub fn is_quiet(&self, bundle_id: &str) -> bool {
        self.quiet_apps.iter().any(|a| a.bundle_id == bundle_id)
    }

    /// Add or remove an app from the quiet list. Bundle ids are unique in the list.
    /// Add or remove a quiet app. Bundle ids typed by hand are checked: letters, digits, `.`, `-` and `_`
    /// only, at most 255 characters; anything else is ignored. Names become one line of at most 80 chars.
    pub fn set_quiet(&mut self, bundle_id: &str, name: &str, quiet: bool) {
        let bundle_id = bundle_id.trim();
        self.quiet_apps.retain(|a| a.bundle_id != bundle_id);
        let valid = !bundle_id.is_empty()
            && bundle_id.len() <= 255
            && bundle_id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || ".-_".contains(c));
        if quiet && valid && self.quiet_apps.len() < MAX_QUIET_APPS {
            let name: String = name
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .chars()
                .take(80)
                .collect();
            self.quiet_apps.push(QuietApp {
                bundle_id: bundle_id.into(),
                name,
            });
        }
    }

    /// Missing file: defaults. Corrupt file: defaults, the bad file kept as `.corrupt`.
    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|_| {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn hand_typed_bundle_ids_are_checked() {
        let mut g = GhostSettings::default();
        let n = g.quiet_apps.len();
        g.set_quiet(" com.example.Notes ", "  My\nNotes ", true);
        assert_eq!(g.quiet_apps.len(), n + 1);
        let added = g.quiet_apps.last().unwrap();
        assert_eq!(
            (added.bundle_id.as_str(), added.name.as_str()),
            ("com.example.Notes", "My Notes")
        );
        for bad in ["", "com.example/../x", "rm -rf", "a\u{0}b"] {
            g.set_quiet(bad, "x", true);
        }
        assert_eq!(g.quiet_apps.len(), n + 1);
        g.set_quiet("com.example.Notes", "", false);
        assert_eq!(g.quiet_apps.len(), n);
    }

    #[test]
    fn terminals_password_managers_and_bob_are_quiet_by_default() {
        let s = GhostSettings::default();
        assert!(s.enabled);
        for id in [
            "com.googlecode.iterm2",
            "com.apple.Terminal",
            "com.1password.1password",
            "dev.bob.assistant",
        ] {
            assert!(s.is_quiet(id), "{id}");
        }
        assert!(!s.is_quiet("com.apple.Notes"));
    }

    #[test]
    fn quiet_list_can_be_edited() {
        let mut s = GhostSettings::default();
        s.set_quiet("com.googlecode.iterm2", "iTerm", false);
        assert!(!s.is_quiet("com.googlecode.iterm2"));
        s.set_quiet("com.tinyspeck.slackmacgap", "Slack", true);
        s.set_quiet("com.tinyspeck.slackmacgap", "Slack", true);
        assert_eq!(
            s.quiet_apps
                .iter()
                .filter(|a| a.bundle_id == "com.tinyspeck.slackmacgap")
                .count(),
            1
        );
        s.set_quiet("", "nothing", true);
        assert!(!s.is_quiet(""));
    }

    #[test]
    fn save_load_round_trip_and_corrupt_fallback() {
        let dir = std::env::temp_dir().join(format!("bob-gs-{}", rand::random::<u32>()));
        let p = dir.join("ghost-settings.json");
        assert_eq!(GhostSettings::load(&p), GhostSettings::default());
        let mut s = GhostSettings {
            enabled: false,
            ..Default::default()
        };
        s.set_quiet("com.apple.Notes", "Notes", true);
        s.save(&p).unwrap();
        assert_eq!(GhostSettings::load(&p), s);
        std::fs::write(&p, "{broken").unwrap();
        assert_eq!(GhostSettings::load(&p), GhostSettings::default());
        assert!(dir.join("ghost-settings.json.corrupt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    proptest! {
        /// Whatever the edits, each bundle id appears at most once, and the last edit wins.
        #[test]
        fn edits_keep_ids_unique_and_last_edit_wins(ops in proptest::collection::vec(("[a-c]", any::<bool>()), 0..40)) {
            let mut s = GhostSettings { enabled: true, quiet_apps: vec![], ..Default::default() };
            let mut expected = std::collections::HashMap::new();
            for (id, q) in &ops {
                s.set_quiet(id, id, *q);
                expected.insert(id.clone(), *q);
            }
            let mut ids: Vec<_> = s.quiet_apps.iter().map(|a| a.bundle_id.clone()).collect();
            let n = ids.len();
            ids.sort();
            ids.dedup();
            prop_assert_eq!(ids.len(), n);
            for (id, q) in expected { prop_assert_eq!(s.is_quiet(&id), q); }
        }
    }
}
