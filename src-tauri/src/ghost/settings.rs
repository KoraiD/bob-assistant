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
/// Largest chip nudge in points, either way.
pub const MAX_CHIP_OFFSET: i32 = 40;
/// Upper bound on per-app chip nudges.
pub const MAX_CHIP_OFFSETS: usize = 100;

/// Moves the chip in one app, for apps that report the caret a little off (proposal 9). Points, with
/// `dy` positive downwards.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChipOffset {
    pub bundle_id: String,
    pub name: String,
    pub dx: i32,
    pub dy: i32,
}

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
    /// Per-app chip nudges.
    pub chip_offsets: Vec<ChipOffset>,
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
            chip_offsets: Vec::new(),
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
        if quiet && valid_bundle_id(bundle_id) && self.quiet_apps.len() < MAX_QUIET_APPS {
            self.quiet_apps.push(QuietApp {
                bundle_id: bundle_id.into(),
                name: one_line(name, 80),
            });
        }
    }

    /// The chip nudge for an app, (0, 0) when none is set.
    pub fn chip_offset(&self, bundle_id: &str) -> (i32, i32) {
        self.chip_offsets
            .iter()
            .find(|o| o.bundle_id == bundle_id)
            .map_or((0, 0), |o| (o.dx, o.dy))
    }

    /// Set an app's chip nudge, clamped to `MAX_CHIP_OFFSET`; (0, 0) removes it. Bundle ids are checked
    /// like quiet apps.
    pub fn set_chip_offset(&mut self, bundle_id: &str, name: &str, dx: i32, dy: i32) {
        let bundle_id = bundle_id.trim();
        self.chip_offsets.retain(|o| o.bundle_id != bundle_id);
        let (dx, dy) = (
            dx.clamp(-MAX_CHIP_OFFSET, MAX_CHIP_OFFSET),
            dy.clamp(-MAX_CHIP_OFFSET, MAX_CHIP_OFFSET),
        );
        if (dx, dy) != (0, 0)
            && valid_bundle_id(bundle_id)
            && self.chip_offsets.len() < MAX_CHIP_OFFSETS
        {
            self.chip_offsets.push(ChipOffset {
                bundle_id: bundle_id.into(),
                name: one_line(name, 80),
                dx,
                dy,
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

/// Letters, digits, `.`, `-` and `_` only, at most 255 characters.
fn valid_bundle_id(bundle_id: &str) -> bool {
    !bundle_id.is_empty()
        && bundle_id.len() <= 255
        && bundle_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || ".-_".contains(c))
}

fn one_line(s: &str, max: usize) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(max)
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn chip_offsets_are_clamped_checked_and_removed_at_zero() {
        let mut g = GhostSettings::default();
        assert_eq!(g.chip_offset("com.apple.TextEdit"), (0, 0));
        g.set_chip_offset("com.apple.TextEdit", " Text\nEdit ", 3, -500);
        assert_eq!(g.chip_offset("com.apple.TextEdit"), (3, -MAX_CHIP_OFFSET));
        assert_eq!(g.chip_offsets[0].name, "Text Edit");
        g.set_chip_offset("com.apple.TextEdit", "TextEdit", -2, 4);
        assert_eq!(g.chip_offsets.len(), 1);
        assert_eq!(g.chip_offset("com.apple.TextEdit"), (-2, 4));
        g.set_chip_offset("com.apple.TextEdit", "TextEdit", 0, 0);
        assert!(g.chip_offsets.is_empty());
        g.set_chip_offset("../etc", "x", 5, 5);
        assert!(g.chip_offsets.is_empty());
        // Old settings files without the field still load.
        let old: GhostSettings = serde_json::from_str(r#"{"enabled":true}"#).unwrap();
        assert!(old.chip_offsets.is_empty());
    }

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
