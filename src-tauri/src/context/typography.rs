//! Typography and spelling preferences (Personalize > Writing style): quote style for everything BOB types,
//! and US or UK spelling for what brains write.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Quotes {
    /// Leave quotes as the brain or whisper wrote them.
    #[default]
    Keep,
    /// "straight" and 'straight'.
    Straight,
    /// “curly” and ‘curly’, with ’ for apostrophes.
    Curly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Spelling {
    #[default]
    Any,
    Us,
    Uk,
}

impl Spelling {
    pub fn instruction(self) -> Option<&'static str> {
        match self {
            Spelling::Any => None,
            Spelling::Us => Some("Use American spelling (color, organize) when writing English."),
            Spelling::Uk => Some("Use British spelling (colour, organise) when writing English."),
        }
    }
}

/// Convert quotes and apostrophes in `text` to the chosen style. `before` is the text already in front of
/// it (the last character decides whether a leading quote opens or closes). Keep returns `text` unchanged.
pub fn apply(text: &str, quotes: Quotes, before: Option<char>) -> String {
    match quotes {
        Quotes::Keep => text.to_string(),
        Quotes::Straight => text
            .chars()
            .map(|c| match c {
                '“' | '”' | '„' | '«' | '»' => '"',
                '‘' | '’' | '‚' => '\'',
                c => c,
            })
            .collect(),
        Quotes::Curly => {
            let mut out = String::with_capacity(text.len());
            let mut prev = before;
            for c in text.chars() {
                let opens = prev.is_none_or(|p| p.is_whitespace() || "([{-–—".contains(p));
                let mapped = match c {
                    '"' | '“' | '”' => {
                        if opens {
                            '“'
                        } else {
                            '”'
                        }
                    }
                    '\'' | '‘' | '’' => {
                        if opens {
                            '‘'
                        } else {
                            '’' // closing quote or apostrophe (don't, the '90s keep ’)
                        }
                    }
                    c => c,
                };
                out.push(mapped);
                prev = Some(c);
            }
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn straight_and_curly_quotes() {
        assert_eq!(
            apply("“Hi,” she said. It’s ‘fine’.", Quotes::Straight, None),
            "\"Hi,\" she said. It's 'fine'."
        );
        assert_eq!(
            apply("\"Hi,\" she said. It's 'fine'.", Quotes::Curly, None),
            "“Hi,” she said. It’s ‘fine’."
        );
        // The text before decides whether a leading quote opens or closes.
        assert_eq!(apply("\" and more", Quotes::Curly, Some('d')), "” and more");
        assert_eq!(apply("\"quoted\"", Quotes::Curly, Some(' ')), "“quoted”");
        assert_eq!(apply("as is ‘x’", Quotes::Keep, None), "as is ‘x’");
    }

    #[test]
    fn spelling_instructions() {
        assert!(Spelling::Any.instruction().is_none());
        assert!(Spelling::Uk.instruction().unwrap().contains("British"));
    }

    proptest! {
        /// Converting never changes anything but quote characters, so the length in characters is kept.
        #[test]
        fn only_quotes_change(text in "\\PC{0,80}", curly in any::<bool>()) {
            let q = if curly { Quotes::Curly } else { Quotes::Straight };
            let out = apply(&text, q, None);
            prop_assert_eq!(out.chars().count(), text.chars().count());
            for (a, b) in text.chars().zip(out.chars()) {
                if a != b {
                    prop_assert!("\"'“”‘’„‚«»".contains(a) && "\"'“”‘’".contains(b));
                }
            }
            // Idempotent.
            prop_assert_eq!(apply(&out, q, None), out.clone());
        }
    }
}
