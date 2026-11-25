//! Turning raw model output into a Ghost Text chip, and the chip operations (GT-7, GT-8, GT-10, GT-15).
//!
//! The rules come from real output of the default model (docs/PERFORMANCE.md): an instruct model used for
//! raw continuation finishes the sentence well, then drifts into unrelated text ("... as soon as possible.
//! A. 正确 B.", "... what you mean. Which of the following options"). So a chip ends at the first sentence
//! boundary, is capped in words and characters, and refuses script switches, repeats and meta text.

/// Most words in a chip.
pub const MAX_WORDS: usize = 6;
/// Most characters in a chip.
pub const MAX_CHARS: usize = 48;
/// A character repeated this many times in a row is model degeneration ("1000000000000", "!!!!!!!!"),
/// not writing: the chip is cut before the word that contains it. Long enough for "10,000,000" and
/// "1000000" (six zeros).
pub const DEGENERATE_RUN: usize = 8;

/// Byte index of the start of the first word that contains a run of `DEGENERATE_RUN` identical
/// non-space characters.
fn degenerate_at(s: &str) -> Option<usize> {
    let mut run = 0;
    let mut prev = None;
    let mut word_start = 0;
    for (i, c) in s.char_indices() {
        if c.is_whitespace() {
            run = 0;
            prev = None;
            word_start = i + c.len_utf8();
            continue;
        }
        run = if prev == Some(c) { run + 1 } else { 1 };
        prev = Some(c);
        if run >= DEGENERATE_RUN {
            return Some(word_start);
        }
    }
    None
}

/// Coarse Unicode script class, enough to spot a model switching languages mid-suggestion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Script {
    Latin,
    Cyrillic,
    Greek,
    Cjk,
    Arabic,
    Hebrew,
    Other,
}

fn script(c: char) -> Option<Script> {
    if !c.is_alphabetic() {
        return None;
    }
    let u = c as u32;
    Some(match u {
        0x0041..=0x024F | 0x1E00..=0x1EFF => Script::Latin,
        0x0370..=0x03FF => Script::Greek,
        0x0400..=0x052F => Script::Cyrillic,
        0x0590..=0x05FF => Script::Hebrew,
        0x0600..=0x06FF => Script::Arabic,
        0x3040..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xAC00..=0xD7AF | 0xF900..=0xFAFF => {
            Script::Cjk
        }
        _ => Script::Other,
    })
}

fn scripts_in(s: &str) -> Vec<Script> {
    let mut v: Vec<Script> = s.chars().filter_map(script).collect();
    v.sort_by_key(|x| *x as u8);
    v.dedup();
    v
}

/// The user is in the middle of a word (the chip must finish it, not start a new one).
pub fn mid_word(before_caret: &str) -> bool {
    before_caret
        .chars()
        .last()
        .is_some_and(|c| c.is_alphanumeric() || c == '\'')
}

/// Starts of model output that are never a continuation of the user's own text.
const META_STARTS: &[&str] = &[
    "sure",
    "here is",
    "here's",
    "certainly",
    "as an ai",
    "i'm sorry",
    "i cannot",
    "note:",
    "answer:",
    "question:",
    "response:",
    "assistant",
    "user:",
];

fn is_meta(s: &str) -> bool {
    let original = s.trim_start();
    let t = original.to_lowercase();
    // Exam / quiz drift: "A. ", "B) ", "1. " at the very start. Checked on the original casing.
    let mut c = original.chars();
    let quiz = matches!(
        (c.next(), c.next(), c.next()),
        (Some(a), Some('.' | ')'), Some(' ')) if a.is_ascii_uppercase() || a.is_ascii_digit()
    );
    quiz || META_STARTS.iter().any(|m| t.starts_with(m))
        || t.starts_with(['#', '*', '`', '>', '|', '"', '“'])
}

/// Polish `raw` (the model's continuation of `before_caret`) into a chip, or `None` when nothing worth
/// showing remains. The result never has more than [`MAX_WORDS`] words or [`MAX_CHARS`] characters, is
/// never empty or whitespace-only, never starts with a space when the caret already follows one, and
/// never crosses a line or sentence boundary.
pub fn polish(raw: &str, before_caret: &str) -> Option<String> {
    let mut s: String = raw.split(['\n', '\r']).next().unwrap_or("").to_string();

    // After a space, the chip starts at the next word.
    if before_caret.ends_with(char::is_whitespace) || before_caret.is_empty() {
        s = s.trim_start().to_string();
    }
    if s.trim().is_empty() || is_meta(&s) {
        return None;
    }
    if let Some(i) = degenerate_at(&s) {
        s.truncate(i);
        if s.trim().is_empty() {
            return None;
        }
    }

    // Script switch: cut at the first letter of a script the user is not writing in.
    let context = scripts_in(before_caret);
    if !context.is_empty() {
        if let Some((i, _)) = s
            .char_indices()
            .find(|(_, c)| script(*c).is_some_and(|sc| !context.contains(&sc)))
        {
            s.truncate(i);
        }
    }

    // Sentence boundary: keep the first terminator (and closing quote/bracket), drop the rest.
    let chars: Vec<(usize, char)> = s.char_indices().collect();
    for (k, &(i, c)) in chars.iter().enumerate() {
        if matches!(c, '.' | '!' | '?' | '。' | '！' | '？') {
            // Not a decimal point or an abbreviation dot in the middle of a word ("3.5", "e.g").
            let next = chars.get(k + 1).map(|x| x.1);
            if c == '.' && next.is_some_and(|n| n.is_alphanumeric()) {
                continue;
            }
            let mut end = i + c.len_utf8();
            if let Some(&(j, q)) = chars.get(k + 1) {
                if matches!(q, '"' | '\'' | ')' | '”' | '’') {
                    end = j + q.len_utf8();
                }
            }
            s.truncate(end);
            break;
        }
    }

    // Collapse immediate word repetitions ("what what", "the the").
    let mut out = String::new();
    let mut prev_word: Option<String> = None;
    for piece in split_keep_spaces(&s) {
        let w = piece.trim().to_lowercase();
        if !w.is_empty() && prev_word.as_deref() == Some(w.as_str()) {
            continue;
        }
        if !w.is_empty() {
            prev_word = Some(w);
        }
        out.push_str(piece);
    }
    // The first chip word must not repeat the last typed word ("I think" -> " think ..."). A chip that
    // starts with whitespace begins a new word even when the caret is right after a letter.
    if !mid_word(before_caret) || out.starts_with(char::is_whitespace) {
        if let (Some(last_typed), Some(first)) = (
            before_caret.split_whitespace().last(),
            out.split_whitespace().next(),
        ) {
            if last_typed.eq_ignore_ascii_case(first) {
                return None;
            }
        }
    }
    let mut s = out;

    // Word and character caps, cutting at a word boundary.
    let mut words = 0;
    let mut cut = s.len();
    let mut in_word = false;
    for (i, c) in s.char_indices() {
        if c.is_whitespace() {
            in_word = false;
        } else if !in_word {
            in_word = true;
            words += 1;
            if words > MAX_WORDS {
                cut = i;
                break;
            }
        }
    }
    s.truncate(cut);
    if s.chars().count() > MAX_CHARS {
        let limit = s
            .char_indices()
            .nth(MAX_CHARS)
            .map(|(i, _)| i)
            .unwrap_or(s.len());
        let head = &s[..limit];
        let at = head
            .rfind(char::is_whitespace)
            .filter(|&p| p > 0)
            .unwrap_or(limit);
        s.truncate(at);
    }
    let s = s.trim_end().to_string();
    if s.trim().is_empty() {
        return None;
    }
    Some(s)
}

/// Split into pieces of "leading whitespace + one word", keeping every character.
fn split_keep_spaces(s: &str) -> Vec<&str> {
    let mut pieces = vec![];
    let mut start = 0;
    let mut seen_word = false;
    for (i, c) in s.char_indices() {
        if c.is_whitespace() && seen_word {
            pieces.push(&s[start..i]);
            start = i;
            seen_word = false;
        } else if !c.is_whitespace() {
            seen_word = true;
        }
    }
    if start < s.len() {
        pieces.push(&s[start..]);
    }
    pieces
}

/// The app is likely showing its own completion list, and Tab belongs to it: a mention (`@ann`), a channel
/// or tag (`#gen`), an emoji code (`:smi`), or a slash command at the start of a line (`/rem`). Slack,
/// Teams, Discord, GitHub and Notion all use these. BOB stays out of the way until the word is done.
pub fn in_app_autocomplete(before_caret: &str) -> bool {
    let line = before_caret.rsplit('\n').next().unwrap_or("");
    let word = line.rsplit(char::is_whitespace).next().unwrap_or("");
    let command =
        line.trim_start().starts_with('/') && !line.trim_start().contains(char::is_whitespace);
    command || (word.len() > 1 || word == "@" || word == "#") && word.starts_with(['@', '#', ':'])
}

/// The rest of the line after the caret, when it contains words (the cursor is inside a sentence).
pub fn words_after(after_caret: &str) -> Option<&str> {
    let line = after_caret.split('\n').next().unwrap_or("");
    line.chars().any(char::is_alphanumeric).then_some(line)
}

/// Turn the answer to [`super::prompt::gap`] into a chip for the gap between `before` and `after`.
/// `done` is true once the closing `]` arrived. Returns `None` while nothing usable is there yet.
pub fn gap_chip(raw: &str, before: &str, after: &str) -> Option<(String, bool)> {
    let (answer, done) = match raw.find(']') {
        Some(i) => (&raw[..i], true),
        None => (raw, false),
    };
    if answer.contains(['[', '\n']) || answer.contains("...") {
        return None;
    }
    let mut text = answer.trim().to_string();
    if text.is_empty()
        || text.split_whitespace().count() > MAX_WORDS
        || degenerate_at(&text).is_some()
    {
        return None;
    }
    // The user is in the middle of a word: the answer must complete it.
    if mid_word(before) {
        let partial = before
            .rsplit(|c: char| !c.is_alphanumeric() && c != '\'')
            .next()
            .unwrap_or("");
        let lower = text.to_lowercase();
        if !lower.starts_with(&partial.to_lowercase()) || lower.len() == partial.len() {
            return None;
        }
        text = text[partial.len()..].to_string();
    } else if !before.is_empty() && !before.ends_with(char::is_whitespace) {
        text.insert(0, ' ');
    }
    // Never repeat the word that follows the gap ("the meeting meeting").
    let next = after
        .split_whitespace()
        .next()
        .unwrap_or("")
        .trim_matches(|c: char| !c.is_alphanumeric());
    let last = text
        .split_whitespace()
        .last()
        .unwrap_or("")
        .trim_matches(|c: char| !c.is_alphanumeric());
    if !next.is_empty() && next.eq_ignore_ascii_case(last) {
        return None;
    }
    if after.starts_with(char::is_alphanumeric) && text.ends_with(char::is_alphanumeric) {
        text.push(' ');
    }
    Some((text, done))
}

/// Whether the polished chip is complete, so the stream can be dropped early (saves generation time).
pub fn is_final(chip: &str) -> bool {
    // A closing quote or bracket ends the chip only after sentence punctuation (`done."`), not in the
    // middle of a sentence (`it's "fine" today`), which the Ghost matrix test caught.
    let t = chip.trim_end_matches(['"', ')', '”', '’', '\'']);
    let closed = t.len() < chip.len();
    let sentence = t.ends_with(['.', '!', '?', '。', '！', '？']);
    (sentence && (closed || chip == t))
        || chip.split_whitespace().count() >= MAX_WORDS
        || chip.chars().count() >= MAX_CHARS - 4
}

/// Result of the user typing while a chip is visible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeThrough {
    /// The typed text matched the start of the chip; this is what is left to show.
    Advance(String),
    /// The typed text consumed the whole chip.
    Exhausted,
    /// The typed text differs from the chip: dismiss it and ask again.
    Diverged,
}

/// GT-7 / GT-8: compare `typed` (what was just inserted at the caret) with the chip.
pub fn type_through(chip: &str, typed: &str) -> TypeThrough {
    if typed.is_empty() {
        return TypeThrough::Advance(chip.to_string());
    }
    match chip.strip_prefix(typed) {
        Some("") => TypeThrough::Exhausted,
        Some(rest) if !rest.trim().is_empty() => TypeThrough::Advance(rest.to_string()),
        Some(_) => TypeThrough::Exhausted,
        None => TypeThrough::Diverged,
    }
}

/// GT-10: split off the next word (with its leading whitespace and trailing punctuation) to insert on Tab.
/// Returns `(inserted, remainder)` with `inserted + remainder == chip`.
pub fn accept_word(chip: &str) -> (String, String) {
    let lead = chip.len() - chip.trim_start().len();
    let rest = &chip[lead..];
    let word_len = rest.find(char::is_whitespace).unwrap_or(rest.len());
    let split = lead + word_len;
    (chip[..split].to_string(), chip[split..].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn mentions_channels_emoji_and_commands_belong_to_the_app() {
        for t in [
            "Hi @an",
            "@",
            "see #gen",
            "#",
            "nice :smi",
            "/rem",
            "line\n/gif",
        ] {
            assert!(in_app_autocomplete(t), "{t}");
        }
        for t in [
            "Hi @anna thanks",
            "Note: the",
            "10:30 is",
            "and/or",
            "a / b",
            "/remind me in",
            "email me at",
        ] {
            assert!(!in_app_autocomplete(t), "{t}");
        }
    }

    #[test]
    fn gap_answers_fit_between_the_words() {
        let after = " to Thursday afternoon?";
        assert_eq!(
            gap_chip("meeting]", "Could we move the ", after),
            Some(("meeting".into(), true))
        );
        assert_eq!(
            gap_chip("next meeting]", "Could we move it,", " please?"),
            Some((" next meeting".into(), true))
        );
        // Completing a partial word.
        assert_eq!(
            gap_chip("meeting]", "Could we move the mee", after),
            Some(("ting".into(), true))
        );
        assert_eq!(gap_chip("call]", "Could we move the mee", after), None);
        // Text right after the gap: add a space.
        assert_eq!(
            gap_chip("latest]", "Send me the ", "report today."),
            Some(("latest ".into(), true))
        );
        // Still streaming, or drifting into the next example.
        assert_eq!(
            gap_chip("mee", "Could we move the ", after),
            Some(("mee".into(), false))
        );
        assert_eq!(
            gap_chip("email] Text: I am", "Thanks for the ", ", I will"),
            Some(("email".into(), true))
        );
        assert_eq!(gap_chip("x\nText: [...]", "a ", "b"), None);
        // Repeating the next word is refused.
        assert_eq!(
            gap_chip("the meeting]", "move ", "meeting to Thursday"),
            None
        );
        assert_eq!(
            words_after(" to Thursday.\nNext line"),
            Some(" to Thursday.")
        );
        assert_eq!(words_after(".\nmore"), None);
        assert_eq!(words_after(""), None);
    }

    #[test]
    fn a_quote_ends_a_chip_only_after_sentence_punctuation() {
        assert!(!is_final(" it's \"fine\""));
        assert!(!is_final(" (see below)"));
        assert!(is_final(" he said \"done.\""));
        assert!(is_final(" today."));
        assert!(is_final(" really?)"));
    }

    #[test]
    fn real_drift_examples_are_cut_at_the_sentence_end() {
        // Observed on Qwen2.5-1.5B-Instruct (docs/PERFORMANCE.md and the owner's screenshot).
        assert_eq!(
            polish(
                " as soon as possible. A. 正确 B.",
                "Thanks for your email. I will get back to you"
            )
            .as_deref(),
            Some(" as soon as possible.")
        );
        assert_eq!(
            polish(
                "ow what you mean. I think I know what you mean. Which of the",
                "Wait a minute I think I k"
            )
            .as_deref(),
            Some("ow what you mean.")
        );
        assert_eq!(
            polish(" lazy dog.", "The quick brown fox jumps over the").as_deref(),
            Some(" lazy dog.")
        );
    }

    #[test]
    fn script_switches_are_cut() {
        assert_eq!(
            polish(" soon 正确答案", "See you").as_deref(),
            Some(" soon")
        );
        assert_eq!(polish(" 正确", "See you"), None);
        // Writing in Japanese, Japanese continuations are fine.
        assert_eq!(polish("です。", "これはペン").as_deref(), Some("です。"));
    }

    #[test]
    fn caps_words_and_characters_at_word_boundaries() {
        let c = polish(" one two three four five six seven eight", "Count:").unwrap();
        assert_eq!(c, " one two three four five six");
        let c = polish(
            " extraordinarily comprehensive internationalization considerations",
            "Note",
        )
        .unwrap();
        assert!(c.chars().count() <= MAX_CHARS, "{c}");
        assert!(!c.ends_with(char::is_whitespace));
    }

    #[test]
    fn degenerate_runs_are_never_shown() {
        // Seen in the real app after "We will receive " (README GIF recording, 2026-11-25).
        assert_eq!(
            polish("1000000000000000000000000", "We will receive "),
            None
        );
        assert_eq!(
            polish(" the 1000000000000 items", "We will receive"),
            Some(" the".into())
        );
        assert_eq!(polish("!!!!!!!!!!", "Wow"), None);
        // Real numbers and words stay.
        assert_eq!(
            polish("1,000,000 dollars.", "We raised "),
            Some("1,000,000 dollars.".into())
        );
        assert_eq!(
            polish("1000000 users", "We have "),
            Some("1000000 users".into())
        );
        assert_eq!(gap_chip("00000000000]", "the ", " items"), None);
    }

    #[test]
    fn rejects_meta_quiz_and_echo_output() {
        for raw in [
            " Sure, here is the text",
            "\"Quoted reply\"",
            "# Heading",
            "A. first option",
            "1. step one",
            "  ",
        ] {
            assert_eq!(polish(raw, "Hello"), None, "{raw:?}");
        }
        assert_eq!(
            polish(" think I know", "I think"),
            None,
            "first word repeats the last typed word"
        );
        assert_eq!(
            polish(" what what you you mean", "I know").as_deref(),
            Some(" what you mean")
        );
    }

    #[test]
    fn spacing_follows_the_caret() {
        assert_eq!(polish(" world", "Hello ").as_deref(), Some("world"));
        assert_eq!(polish("rld", "Hello wo").as_deref(), Some("rld"));
        assert_eq!(polish("The end.", "").as_deref(), Some("The end."));
        assert!(mid_word("Hello wo") && !mid_word("Hello ") && !mid_word("Hello."));
    }

    #[test]
    fn decimals_and_abbreviations_are_not_sentence_ends() {
        assert_eq!(
            polish(" version 3.5 is out today", "We shipped").as_deref(),
            Some(" version 3.5 is out today")
        );
    }

    #[test]
    fn typing_through_and_accepting() {
        assert_eq!(
            type_through(" lazy dog.", " "),
            TypeThrough::Advance("lazy dog.".into())
        );
        assert_eq!(
            type_through(" lazy dog.", " la"),
            TypeThrough::Advance("zy dog.".into())
        );
        assert_eq!(
            type_through(" lazy dog.", " lazy dog."),
            TypeThrough::Exhausted
        );
        assert_eq!(
            type_through(" lazy dog.", " lazy "),
            TypeThrough::Advance("dog.".into())
        );
        assert_eq!(type_through(" lazy dog.", " x"), TypeThrough::Diverged);
        assert_eq!(
            accept_word(" lazy dog."),
            (" lazy".to_string(), " dog.".to_string())
        );
        assert_eq!(
            accept_word("rld, and"),
            ("rld,".to_string(), " and".to_string())
        );
        assert_eq!(accept_word("end."), ("end.".to_string(), String::new()));
    }

    #[test]
    fn completion_detection() {
        assert!(is_final(" as soon as possible."));
        assert!(is_final(" one two three four five six"));
        assert!(!is_final(" as soon"));
    }

    fn arb_chip() -> impl Strategy<Value = String> {
        "( ?[a-zA-Z,.']{1,8}){1,6}".prop_filter("non-empty", |s| !s.trim().is_empty())
    }

    proptest! {
        /// GT-15 / P3: whatever the model says, a chip respects the caps, is never blank, and never
        /// crosses a line break.
        #[test]
        fn polish_always_respects_the_contract(raw in "\\PC{0,200}", before in "\\PC{0,60}") {
            if let Some(c) = polish(&raw, &before) {
                prop_assert!(!c.trim().is_empty());
                prop_assert!(c.split_whitespace().count() <= MAX_WORDS, "{c:?}");
                prop_assert!(c.chars().count() <= MAX_CHARS, "{c:?}");
                prop_assert!(!c.contains('\n') && !c.contains('\r'));
                prop_assert!(!c.ends_with(char::is_whitespace));
                if before.ends_with(char::is_whitespace) { prop_assert!(!c.starts_with(char::is_whitespace)); }
            }
        }

        /// Polishing a chip again changes nothing (stable while tokens keep streaming in).
        #[test]
        fn polish_is_idempotent(raw in "[a-zA-Z .,!?]{0,120}", before in "[a-zA-Z ]{1,40}") {
            if let Some(c) = polish(&raw, &before) {
                let again = polish(&c, &before);
                prop_assert_eq!(again.as_deref(), Some(c.as_str()));
            }
        }

        /// GT-7 / P1: typing the chip's own next characters advances it; anything else diverges.
        #[test]
        fn type_through_matches_prefix_semantics(chip in arb_chip(), k in 0usize..12, other in "[a-zA-Z]") {
            let k = k.min(chip.chars().count());
            let typed: String = chip.chars().take(k).collect();
            match type_through(&chip, &typed) {
                TypeThrough::Advance(rest) => prop_assert_eq!(format!("{typed}{rest}"), chip.clone()),
                TypeThrough::Exhausted => prop_assert!(chip[typed.len()..].trim().is_empty()),
                TypeThrough::Diverged => prop_assert!(false, "a prefix must never diverge"),
            }
            let wrong = format!("{typed}{other}");
            if !chip.starts_with(&wrong) {
                prop_assert_eq!(type_through(&chip, &wrong), TypeThrough::Diverged);
            }
        }

        /// GT-10 / P2: accepting a word loses nothing and inserts exactly one word.
        #[test]
        fn accept_word_splits_losslessly(chip in arb_chip()) {
            let (ins, rem) = accept_word(&chip);
            prop_assert_eq!(format!("{ins}{rem}"), chip);
            prop_assert_eq!(ins.split_whitespace().count(), 1);
            prop_assert!(rem.is_empty() || rem.starts_with(char::is_whitespace));
        }
    }
}
