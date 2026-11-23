//! Output cleaner for Selection Actions (P5): small models like to add "Here is the corrected text:",
//! wrap the answer in quotes or a code fence, or pad it with blank lines. The cleaner removes those
//! wrappers (only when the original text did not have them) and keeps the original's surrounding
//! whitespace, so replacing a selection never eats a space or a line break.

const PREAMBLE_STARTS: &[&str] = &[
    "here is",
    "here's",
    "sure",
    "certainly",
    "of course",
    "corrected",
    "rewritten",
    "rephrased",
    "revised",
    "translation",
    "translated",
    "summary",
    "shorter version",
    "friendlier version",
    "professional version",
];

const QUOTES: &[(char, char)] = &[
    ('"', '"'),
    ('\u{201C}', '\u{201D}'),
    ('\'', '\''),
    ('«', '»'),
];

/// The cleaned core text (no surrounding whitespace). Never longer than `output` (P5).
pub fn clean_core(output: &str, original: &str) -> String {
    let mut t = output.trim().to_string();
    // Every step only removes text, so this reaches a fixed point (P5: idempotent).
    loop {
        let before = t.clone();
        t = strip_preamble(&t);
        t = strip_fence(&t, original);
        t = strip_quotes(&t, original);
        t = t.trim().to_string();
        if t == before {
            break;
        }
    }
    t
}

/// The replacement text: the cleaned core with the original's leading and trailing whitespace.
pub fn clean(output: &str, original: &str) -> String {
    let core = clean_core(output, original);
    if core.is_empty() {
        return core;
    }
    let lead = &original[..original.len() - original.trim_start().len()];
    let trail = &original[original.trim_end().len()..];
    format!("{lead}{core}{trail}")
}

/// Drop a first line like "Here is the corrected text:" when more text follows.
fn strip_preamble(t: &str) -> String {
    let Some((first, rest)) = t.split_once('\n') else {
        return t.to_string();
    };
    let f = first.trim().to_lowercase();
    let looks_like_preamble = f.ends_with(':')
        && f.chars().count() <= 80
        && PREAMBLE_STARTS.iter().any(|p| f.starts_with(p));
    if looks_like_preamble && !rest.trim().is_empty() {
        rest.to_string()
    } else {
        t.to_string()
    }
}

/// Remove one ``` fence around the whole answer, unless the original was fenced too.
fn strip_fence(t: &str, original: &str) -> String {
    if t.len() < 6
        || original.trim_start().starts_with("```")
        || !t.starts_with("```")
        || !t.ends_with("```")
    {
        return t.to_string();
    }
    let inner = &t[3..t.len() - 3];
    // Drop an info string such as ```text on the opening line.
    match inner.split_once('\n') {
        Some((info, body)) if !info.contains(' ') => body.to_string(),
        _ => inner.to_string(),
    }
}

/// Remove one pair of matching quotes around the whole answer, unless the original was quoted.
fn strip_quotes(t: &str, original: &str) -> String {
    let o = original.trim();
    for &(l, r) in QUOTES {
        if t.chars().count() >= 2 && t.starts_with(l) && t.ends_with(r) {
            if o.starts_with(l) && o.ends_with(r) {
                return t.to_string();
            }
            let inner = &t[l.len_utf8()..t.len() - r.len_utf8()];
            // Keep quotes that are part of the text ("a" and "b").
            if inner.contains(l) || inner.contains(r) {
                return t.to_string();
            }
            return inner.to_string();
        }
    }
    t.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn strips_preambles_quotes_and_fences() {
        assert_eq!(
            clean_core(
                "Here is the corrected text:\nThe quick fox.",
                "teh quick fox"
            ),
            "The quick fox."
        );
        assert_eq!(
            clean_core("\"The quick fox.\"", "teh quick fox"),
            "The quick fox."
        );
        assert_eq!(
            clean_core("```text\nThe quick fox.\n```", "teh quick fox"),
            "The quick fox."
        );
        assert_eq!(
            clean_core("Sure! Here's the rewrite:\n\n“Hello there.”", "hello"),
            "Hello there."
        );
    }

    #[test]
    fn keeps_wrappers_the_original_had_or_that_belong_to_the_text() {
        assert_eq!(clean_core("\"Hi.\"", "\"hi\""), "\"Hi.\"");
        assert_eq!(clean_core("\"a\" and \"b\"", "a and b"), "\"a\" and \"b\"");
        assert_eq!(
            clean_core("Note: keep this.\nSecond line", "x"),
            "Note: keep this.\nSecond line"
        );
        // A one-line answer that ends with a colon is content, not a preamble.
        assert_eq!(
            clean_core("Here is what I need:", "here is what i need"),
            "Here is what I need:"
        );
    }

    #[test]
    fn keeps_the_original_surrounding_whitespace() {
        assert_eq!(clean("Fixed text.", "  teh text \n"), "  Fixed text. \n");
        assert_eq!(clean("   ", " x "), "");
    }

    proptest! {
        /// P5: idempotent and never longer than the model output.
        #[test]
        fn cleaner_is_idempotent_and_never_grows(output in "\\PC{0,200}", original in "\\PC{0,80}") {
            let once = clean_core(&output, &original);
            prop_assert!(once.len() <= output.len());
            prop_assert_eq!(clean_core(&once, &original), once.clone());
        }

        #[test]
        fn wrapped_answers_unwrap(body in "[a-zA-Z][a-zA-Z ,.]{0,60}[a-zA-Z.]") {
            let wrapped = format!("Here is the rewritten text:\n\"{body}\"");
            prop_assert_eq!(clean_core(&wrapped, "x"), body.trim().to_string());
        }
    }
}
