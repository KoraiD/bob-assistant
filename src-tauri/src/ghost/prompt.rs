//! Prefix-stable prompt window (GT-16, perf steering).
//!
//! llama.cpp reuses the KV cache for the longest shared prompt prefix (`cache_prompt`). A sliding window
//! that drops one character per keystroke would change the prefix every time and force a full re-prefill.
//! Instead, old text is dropped in coarse steps: the window start only moves when the text grows past the
//! next `STEP` boundary, so consecutive keystrokes share everything but the new characters.

/// Upper bound for the prompt, in characters (about 300 to 500 tokens of English).
pub const MAX_CHARS: usize = 1500;
/// Granularity of window moves.
pub const STEP: usize = 256;

/// Byte index where the window starts for `text`. Always on a char boundary and, when possible, just
/// after whitespace, so the prompt never starts mid-word.
pub fn window_start(text: &str) -> usize {
    let n = text.chars().count();
    if n <= MAX_CHARS {
        return 0;
    }
    // Smallest multiple of STEP that leaves at most MAX_CHARS characters.
    let skip_chars = (n - MAX_CHARS).div_ceil(STEP) * STEP;
    let mut idx = text
        .char_indices()
        .nth(skip_chars)
        .map(|(i, _)| i)
        .unwrap_or(text.len());
    // Move forward to the start of the next word (bounded, so an unbroken blob still gets cut).
    if let Some(off) = text[idx..]
        .char_indices()
        .take(64)
        .find(|(_, c)| c.is_whitespace())
        .map(|(i, c)| i + c.len_utf8())
    {
        idx += off;
    }
    idx
}

/// The prompt for `before_caret`: the window, with the window start rule above.
pub fn build(before_caret: &str) -> &str {
    &before_caret[window_start(before_caret)..]
}

/// A few-shot prompt for a gap inside a sentence (the cursor is in the middle of the text). The default model
/// does not support fill-in-the-middle tokens (measured: it returns nothing), but continues this pattern.
/// The answer ends at the first `]`.
pub fn gap(before: &str, after: &str) -> String {
    let before: String = window_tail(before, 300);
    let after: String = after.chars().take(160).collect();
    format!(
        "Fill in the missing words marked [...] so the sentence reads naturally.\n\
         Text: Thank you for [...] me the report yesterday.\nMissing: [sending]\n\
         Text: Could we meet [...] to discuss the budget?\nMissing: [on Thursday afternoon]\n\
         Text: {before}[...]{after}\nMissing: ["
    )
}

fn window_tail(s: &str, max_chars: usize) -> String {
    let n = s.chars().count();
    s.chars().skip(n.saturating_sub(max_chars)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn short_text_is_used_whole() {
        assert_eq!(build("Hello wor"), "Hello wor");
        assert_eq!(build(""), "");
    }

    #[test]
    fn long_text_is_cut_at_a_word_start_within_the_limit() {
        let text = "word ".repeat(600); // 3000 chars
        let p = build(&text);
        assert!(p.chars().count() <= MAX_CHARS);
        assert!(p.starts_with("word"));
        assert!(text.ends_with(p));
    }

    #[test]
    fn multibyte_text_never_panics_and_stays_on_char_boundaries() {
        let text = "日本語のテキスト。".repeat(300);
        let p = build(&text);
        assert!(text.ends_with(p));
        assert!(p.chars().count() <= MAX_CHARS + 64);
    }

    proptest! {
        /// GT-16 / P4: appending text keeps the window start (so the prompt prefix and the server's KV cache
        /// stay valid) unless the total length crosses a STEP boundary.
        #[test]
        fn appending_keeps_the_prefix_until_a_step_boundary(
            base in "[a-z ]{0,2000}",
            extra in "[a-z ]{0,40}",
        ) {
            let a = base.clone();
            let b = format!("{base}{extra}");
            let (sa, sb) = (window_start(&a), window_start(&b));
            let steps = |s: &str| { let n = s.chars().count(); if n <= MAX_CHARS { 0 } else { (n - MAX_CHARS).div_ceil(STEP) } };
            if steps(&a) == steps(&b) {
                prop_assert_eq!(sa, sb);
                prop_assert!(build(&b).starts_with(build(&a)));
            }
            prop_assert!(sb >= sa, "the window never moves backwards");
        }

        /// The window is a suffix of the text and never exceeds the limit by more than one word.
        #[test]
        fn window_is_a_bounded_suffix(text in "\\PC{0,3000}") {
            let p = build(&text);
            prop_assert!(text.ends_with(p));
            prop_assert!(p.chars().count() <= MAX_CHARS.max(text.chars().count().min(MAX_CHARS)) + 64);
        }
    }
}
