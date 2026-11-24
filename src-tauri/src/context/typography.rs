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

/// US and UK spellings of common words (stems; suffixes such as -s, -ed, -ing follow). Only unambiguous
/// pairs: words like "program", "meter" or "license" depend on meaning and are left alone. The local model
/// ignores a spelling instruction (docs/PERFORMANCE.md, style benchmark), so BOB also converts the output.
const PAIRS: &[(&str, &str)] = &[
    ("color", "colour"),
    ("favor", "favour"),
    ("favorite", "favourite"),
    ("honor", "honour"),
    ("humor", "humour"),
    ("labor", "labour"),
    ("neighbor", "neighbour"),
    ("behavior", "behaviour"),
    ("flavor", "flavour"),
    ("harbor", "harbour"),
    ("rumor", "rumour"),
    ("center", "centre"),
    ("theater", "theatre"),
    ("fiber", "fibre"),
    ("liter", "litre"),
    ("catalog", "catalogue"),
    ("dialog", "dialogue"),
    ("gray", "grey"),
    ("traveled", "travelled"),
    ("traveling", "travelling"),
    ("traveler", "traveller"),
    ("canceled", "cancelled"),
    ("canceling", "cancelling"),
    ("modeling", "modelling"),
    ("labeled", "labelled"),
    ("jewelry", "jewellery"),
    ("defense", "defence"),
    ("offense", "offence"),
    ("analyze", "analyse"),
    ("paralyze", "paralyse"),
    ("organize", "organise"),
    ("organization", "organisation"),
    ("realize", "realise"),
    ("recognize", "recognise"),
    ("apologize", "apologise"),
    ("prioritize", "prioritise"),
    ("summarize", "summarise"),
    ("optimize", "optimise"),
    ("customize", "customise"),
    ("finalize", "finalise"),
    ("emphasize", "emphasise"),
    ("minimize", "minimise"),
    ("maximize", "maximise"),
    ("utilize", "utilise"),
    ("criticize", "criticise"),
    ("memorize", "memorise"),
    ("standardize", "standardise"),
    ("visualize", "visualise"),
    ("authorize", "authorise"),
    ("categorize", "categorise"),
    ("characterize", "characterise"),
    ("specialize", "specialise"),
    ("familiarize", "familiarise"),
    ("personalize", "personalise"),
    ("synchronize", "synchronise"),
];

/// Everything BOB types goes through this: preferred spelling, then quote style.
pub fn finish(text: &str, spelling: Spelling, quotes: Quotes, before: Option<char>) -> String {
    apply(&respell(text, spelling), quotes, before)
}

/// Respell whole words in `text` to the preferred spelling. `Any` leaves the text unchanged. Case is kept
/// ("Color" -> "Colour"); a word still being typed at the very end (no boundary after it) is left alone.
pub fn respell(text: &str, spelling: Spelling) -> String {
    let to_uk = match spelling {
        Spelling::Any => return text.to_string(),
        Spelling::Uk => true,
        Spelling::Us => false,
    };
    let mut out = String::with_capacity(text.len() + 8);
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String| {
        out.push_str(&convert_word(word, to_uk));
        word.clear();
    };
    for c in text.chars() {
        if c.is_alphabetic() {
            word.push(c);
        } else {
            flush(&mut word, &mut out);
            out.push(c);
        }
    }
    out.push_str(&word); // unfinished last word: unchanged
    out
}

fn convert_word(word: &str, to_uk: bool) -> String {
    if word.is_empty() {
        return String::new();
    }
    let lower = word.to_lowercase();
    for (us, uk) in PAIRS {
        let (from, to) = if to_uk { (*us, *uk) } else { (*uk, *us) };
        // Exact word or the stem plus a common suffix.
        for suffix in [
            "", "s", "d", "ed", "es", "ing", "r", "rs", "ful", "ite", "ites", "able", "ation",
            "ations",
        ] {
            if lower.len() == from.len() + suffix.len()
                && lower.starts_with(from)
                && lower.ends_with(suffix)
            {
                let mut replaced = format!("{to}{suffix}");
                let first_upper = word.chars().next().is_some_and(char::is_uppercase);
                let all_upper = word.chars().all(|c| !c.is_lowercase());
                if all_upper && word.chars().count() > 1 {
                    replaced = replaced.to_uppercase();
                } else if first_upper {
                    let mut cs = replaced.chars();
                    replaced = cs
                        .next()
                        .map(|f| f.to_uppercase().collect::<String>() + cs.as_str())
                        .unwrap_or_default();
                }
                return replaced;
            }
        }
        // -ize verbs inflected from the stem without the final e: organiz-ed / organis-ing.
        if from.ends_with("ize")
            || from.ends_with("ise")
            || from.ends_with("yze")
            || from.ends_with("yse")
        {
            let stem_from = &from[..from.len() - 1];
            let stem_to = &to[..to.len() - 1];
            for suffix in ["ed", "ing", "er", "ers", "es"] {
                if lower == format!("{stem_from}{suffix}") {
                    return keep_case(word, &format!("{stem_to}{suffix}"));
                }
            }
        }
    }
    word.to_string()
}

fn keep_case(original: &str, replaced: &str) -> String {
    if original.chars().next().is_some_and(char::is_uppercase) {
        let mut cs = replaced.chars();
        cs.next()
            .map(|f| f.to_uppercase().collect::<String>() + cs.as_str())
            .unwrap_or_default()
    } else {
        replaced.to_string()
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
    fn respells_whole_words_both_ways_and_keeps_case() {
        assert_eq!(
            respell(
                "We will organize the Color review at the center.",
                Spelling::Uk
            ),
            "We will organise the Colour review at the centre."
        );
        assert_eq!(
            respell("She organized it; we're analyzing colors.", Spelling::Uk),
            "She organised it; we're analysing colours."
        );
        assert_eq!(
            respell("Our favourite theatre, organised.", Spelling::Us),
            "Our favorite theater, organized."
        );
        assert_eq!(respell("COLOR!", Spelling::Uk), "COLOUR!");
        // Unrelated or ambiguous words, and a word still being typed, stay.
        assert_eq!(
            respell("the program meter and colo", Spelling::Uk),
            "the program meter and colo"
        );
        assert_eq!(
            respell("discolor colorful.", Spelling::Uk),
            "discolor colourful."
        );
        assert_eq!(respell("anything", Spelling::Any), "anything");
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
