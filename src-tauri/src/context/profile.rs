//! The user's personalization file (PC-1): `profile.md` in BOB's config directory.
//!
//! Plain Markdown (GitHub flavor, emoji welcome) edited in BOB's Personalize tab or any editor. It stays on
//! this Mac; brains only receive it when the user allows it per brain (PC-6).

use std::path::Path;

pub const FILE: &str = "profile.md";
/// Upper bound for the file (about 8,000 words). Larger saves are refused, not truncated.
pub const MAX_BYTES: usize = 32 * 1024;

/// Shown in the editor when no profile exists yet. Never sent to a brain as is.
pub const TEMPLATE: &str = "# About me ✍️\n\n\
- **Name:** \n\
- **Role:** \n\
- **I mostly write:** emails, chat messages, notes\n\
- **Tone I like:** friendly and concise\n\n\
## Names and words I use\n\n\
- \n\n\
## How I sign off\n\n\
- Best,\n";

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ProfileError {
    #[error("the profile is too long ({0} bytes, at most {MAX_BYTES})")]
    TooLong(usize),
    #[error("cannot save the profile: {0}")]
    Io(String),
}

/// The profile text, or an empty string when there is none (or it cannot be read).
pub fn load(path: &Path) -> String {
    std::fs::read_to_string(path)
        .map(|s| {
            if s.len() > MAX_BYTES {
                let mut end = MAX_BYTES;
                while !s.is_char_boundary(end) {
                    end -= 1;
                }
                s[..end].to_string()
            } else {
                s
            }
        })
        .unwrap_or_default()
}

/// Save atomically (write + rename), private to the user (0600).
pub fn save(path: &Path, text: &str) -> Result<(), ProfileError> {
    if text.len() > MAX_BYTES {
        return Err(ProfileError::TooLong(text.len()));
    }
    let io = |e: std::io::Error| ProfileError::Io(e.to_string());
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(io)?;
    }
    let tmp = path.with_extension("md.tmp");
    std::fs::write(&tmp, text).map_err(io)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600)).map_err(io)?;
    }
    std::fs::rename(&tmp, path).map_err(io)
}

/// The profile as one compact paragraph for short prompts: Markdown markers and HTML comments removed,
/// whitespace collapsed, cut at a word boundary within `max_chars`.
pub fn condense(text: &str, max_chars: usize) -> String {
    let mut without_comments = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("<!--") {
        without_comments.push_str(&rest[..start]);
        rest = match rest[start..].find("-->") {
            Some(end) => &rest[start + end + 3..],
            None => "",
        };
    }
    without_comments.push_str(rest);

    let mut parts: Vec<String> = vec![];
    for line in without_comments.lines() {
        let l = line
            .trim()
            .trim_start_matches(['#', '>', '-', '*', '+'])
            .trim_start_matches("[ ]")
            .trim_start_matches("[x]")
            .replace("**", "")
            .replace("__", "")
            .replace('`', "");
        let l = l.trim();
        if l.is_empty() || l.ends_with(':') && l.split_whitespace().count() <= 4 && l.len() < 24 {
            continue; // blank lines and empty "Field:" stubs
        }
        parts.push(l.to_string());
    }
    let joined = parts.join("; ");
    if joined.chars().count() <= max_chars {
        return joined;
    }
    let cut: String = joined.chars().take(max_chars).collect();
    match cut.rfind(char::is_whitespace) {
        Some(i) if i > max_chars / 2 => cut[..i].trim_end_matches([';', ',', ' ']).to_string(),
        _ => cut,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn save_and_load_round_trip_privately_and_refuse_oversized_text() {
        let dir = std::env::temp_dir().join(format!("bob-profile-{}", rand::random::<u32>()));
        let p = dir.join(FILE);
        assert_eq!(load(&p), "");
        save(&p, "# Me 🎃\n\nI write **short** emails.").unwrap();
        assert_eq!(load(&p), "# Me 🎃\n\nI write **short** emails.");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&p).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
        let big = "x".repeat(MAX_BYTES + 1);
        assert_eq!(save(&p, &big), Err(ProfileError::TooLong(MAX_BYTES + 1)));
        assert!(
            load(&p).starts_with("# Me"),
            "a refused save keeps the old file"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn condense_strips_markdown_comments_and_empty_fields() {
        let md = "# About me ✍️\n<!-- private note -->\n- **Name:** Dávid\n- **Role:** \n- I write `emails`\n\n## Sign-offs\n- Best,\n";
        assert_eq!(
            condense(md, 200),
            "About me ✍️; Name: Dávid; I write emails; Sign-offs; Best,"
        );
    }

    #[test]
    fn the_template_condenses_to_little() {
        // Unfilled stubs are dropped, so an untouched template adds few tokens.
        assert!(condense(TEMPLATE, 500).chars().count() < 140);
    }

    proptest! {
        #[test]
        fn condense_is_bounded_and_never_panics(text in "\\PC{0,600}", max in 1usize..300) {
            prop_assert!(condense(&text, max).chars().count() <= max);
        }
    }
}
