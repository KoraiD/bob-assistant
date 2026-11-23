//! Ghost Text benchmark (Task 5, perf steering): replay synthetic typing traces through the real engine and
//! score latency and suggestion quality against what the "user" types next.
//!
//! Traces are generated, not recorded: no real person's keystrokes are needed or stored. Timing follows a
//! simple, seeded typing model (key intervals of a ~50 WPM typist, longer pauses after words, punctuation and
//! occasional "thinking" pauses), so runs are reproducible and comparable across models and settings.
//!
//! Scoring uses ground truth: at any moment the rest of the sentence is what will be typed next, so a chip
//! can be checked exactly. Metrics:
//! * `first_chip_ms`: keystroke to first chip from the model (typing-through updates are counted separately).
//! * `pause_coverage`: share of pauses >= 300 ms during which a chip was visible.
//! * `word_accuracy`: share of model chips whose first word matches the next word typed.
//! * `keystroke_savings`: characters a perfect Tab user would not have typed (matched chip prefix at the
//!   moment it was shown, never counted twice), divided by all characters typed.

pub mod corpus;
pub mod runner;

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serde::Serialize;

/// One keystroke of a trace: the full text before the caret after the key, at `at_ms` from trace start.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Step {
    pub at_ms: u64,
    pub before: String,
}

/// A trace types one sentence from empty.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Trace {
    pub id: String,
    pub sentence: String,
    pub steps: Vec<Step>,
}

/// Generate a reproducible trace for `sentence`.
pub fn trace(id: &str, sentence: &str, seed: u64) -> Trace {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut t = 0u64;
    let mut before = String::new();
    let mut steps = Vec::new();
    let chars: Vec<char> = sentence.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        // Base interval of a ~50 WPM typist (about 240 ms per character, with spread).
        let mut gap: u64 = rng.random_range(110..=300);
        if let Some(prev) = i.checked_sub(1).map(|j| chars[j]) {
            if prev == ' ' {
                gap += rng.random_range(0..=350); // starting a new word
                if rng.random_bool(0.12) {
                    gap += rng.random_range(700..=1400); // thinking pause
                }
            }
            if matches!(prev, ',' | ';' | ':') {
                gap += rng.random_range(200..=600);
            }
            if matches!(prev, '.' | '!' | '?') {
                gap += rng.random_range(400..=1000);
            }
        }
        t += gap;
        before.push(c);
        steps.push(Step {
            at_ms: t,
            before: before.clone(),
        });
    }
    Trace {
        id: id.to_string(),
        sentence: sentence.to_string(),
        steps,
    }
}

/// What the engine put on screen, with timestamps relative to the trace start.
#[derive(Debug, Clone, PartialEq)]
pub struct ChipEvent {
    pub at_ms: u64,
    /// Text shown, or `None` when the chip was hidden.
    pub text: Option<String>,
}

/// Length of the common prefix of `a` and `b`, in bytes, on a char boundary.
pub fn common_prefix(a: &str, b: &str) -> usize {
    a.char_indices()
        .zip(b.chars())
        .find(|((_, x), y)| x != y)
        .map(|((i, _), _)| i)
        .unwrap_or_else(|| a.len().min(b.len()))
}

/// Is `unit` (one Tab press worth of chip text) exactly what comes next in `upcoming`, ending where the
/// user's word ends ("the" does not match "theory")?
fn unit_matches(upcoming: &str, unit: &str) -> bool {
    !unit.trim().is_empty()
        && upcoming.starts_with(unit)
        && upcoming[unit.len()..]
            .chars()
            .next()
            .is_none_or(|c| !c.is_alphanumeric())
}

/// Bytes of `upcoming` covered by the chip's leading correct words, i.e. what Tab presses would insert.
/// Tab inserts whole words, so a partial match (or just the leading space) saves nothing.
pub fn accepted_prefix(chip: &str, upcoming: &str) -> usize {
    let mut pos = 0;
    let mut rest = chip.to_string();
    loop {
        let (unit, remainder) = crate::ghost::chip::accept_word(&rest);
        if !unit_matches(&upcoming[pos..], &unit) {
            return pos;
        }
        pos += unit.len();
        if remainder.trim().is_empty() {
            return pos;
        }
        rest = remainder;
    }
}

fn first_word_right(chip: &str, upcoming: &str) -> bool {
    let (unit, _) = crate::ghost::chip::accept_word(chip);
    unit_matches(upcoming, &unit)
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct TraceScore {
    pub id: String,
    pub chars_typed: usize,
    pub model_chips: usize,
    pub first_chip_ms: Vec<u64>,
    pub pauses: usize,
    pub pauses_with_chip: usize,
    /// Pauses whose visible chip started with the right word.
    pub pause_chip_correct: usize,
    pub first_word_correct: usize,
    pub saved_chars: usize,
}

/// A chip counts as a fresh model chip when it appears after a keystroke later than this; earlier ones
/// are typing-through updates (no model call), which are near-instant.
pub const TYPING_THROUGH_MAX_MS: u64 = 20;
pub const PAUSE_MS: u64 = 300;

/// Score one replayed trace. `events` must be sorted by time.
pub fn score(trace: &Trace, events: &[ChipEvent]) -> TraceScore {
    let mut s = TraceScore {
        id: trace.id.clone(),
        chars_typed: trace.sentence.chars().count(),
        ..Default::default()
    };
    // Characters already credited as saved (a later chip cannot save them again).
    let mut credited_until = 0usize;
    for (i, step) in trace.steps.iter().enumerate() {
        let next_at = trace.steps.get(i + 1).map(|n| n.at_ms).unwrap_or(u64::MAX);
        let upcoming = &trace.sentence[step.before.len()..];
        let window: Vec<&ChipEvent> = events
            .iter()
            .filter(|e| e.at_ms > step.at_ms && e.at_ms < next_at)
            .collect();
        // Pause coverage: a gap long enough to read a suggestion.
        let gap = next_at.saturating_sub(step.at_ms);
        if gap >= PAUSE_MS && next_at != u64::MAX {
            s.pauses += 1;
            // The chip on screen 100 ms before the pause ends: what the user had time to read.
            let read_at = step.at_ms + gap - 100;
            let on_screen = window
                .iter()
                .rev()
                .find(|e| e.at_ms <= read_at)
                .and_then(|e| e.text.as_deref());
            if let Some(chip) = on_screen {
                s.pauses_with_chip += 1;
                if first_word_right(chip, upcoming) {
                    s.pause_chip_correct += 1;
                }
            }
        }
        // First model chip after this keystroke.
        let Some(first) = window.iter().find(|e| e.text.is_some()) else {
            continue;
        };
        let latency = first.at_ms - step.at_ms;
        if latency <= TYPING_THROUGH_MAX_MS {
            continue;
        }
        s.model_chips += 1;
        s.first_chip_ms.push(latency);
        // Quality: judge the most complete chip shown before the next keystroke.
        let shown = window
            .iter()
            .rev()
            .find_map(|e| e.text.as_deref())
            .unwrap_or_default();
        if first_word_right(shown, upcoming) {
            s.first_word_correct += 1;
        }
        let matched = accepted_prefix(shown, upcoming);
        let start = step.before.len();
        let end = start + matched;
        if end > credited_until {
            s.saved_chars += trace.sentence[start.max(credited_until)..end]
                .chars()
                .count();
            credited_until = end;
        }
    }
    s
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Summary {
    pub traces: usize,
    pub chars_typed: usize,
    pub model_chips: usize,
    pub p50_ms: Option<u64>,
    pub p95_ms: Option<u64>,
    pub max_ms: Option<u64>,
    pub pause_coverage: f64,
    pub pause_accuracy: f64,
    pub word_accuracy: f64,
    pub keystroke_savings: f64,
}

pub fn percentile(sorted: &[u64], p: f64) -> Option<u64> {
    if sorted.is_empty() {
        return None;
    }
    let idx = ((p / 100.0) * (sorted.len() - 1) as f64).round() as usize;
    sorted.get(idx).copied()
}

pub fn summarize(scores: &[TraceScore]) -> Summary {
    let mut lat: Vec<u64> = scores
        .iter()
        .flat_map(|s| s.first_chip_ms.iter().copied())
        .collect();
    lat.sort_unstable();
    let sum = |f: fn(&TraceScore) -> usize| scores.iter().map(f).sum::<usize>();
    let ratio = |a: usize, b: usize| if b == 0 { 0.0 } else { a as f64 / b as f64 };
    Summary {
        traces: scores.len(),
        chars_typed: sum(|s| s.chars_typed),
        model_chips: sum(|s| s.model_chips),
        p50_ms: percentile(&lat, 50.0),
        p95_ms: percentile(&lat, 95.0),
        max_ms: lat.last().copied(),
        pause_coverage: ratio(sum(|s| s.pauses_with_chip), sum(|s| s.pauses)),
        pause_accuracy: ratio(sum(|s| s.pause_chip_correct), sum(|s| s.pauses_with_chip)),
        word_accuracy: ratio(sum(|s| s.first_word_correct), sum(|s| s.model_chips)),
        keystroke_savings: ratio(sum(|s| s.saved_chars), sum(|s| s.chars_typed)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn traces_are_reproducible_and_type_the_whole_sentence() {
        let a = trace("t", "Hello there, friend.", 7);
        assert_eq!(a, trace("t", "Hello there, friend.", 7));
        assert_ne!(a.steps, trace("t", "Hello there, friend.", 8).steps);
        assert_eq!(a.steps.last().unwrap().before, "Hello there, friend.");
        assert!(a.steps.windows(2).all(|w| w[1].at_ms > w[0].at_ms));
        // ~50 WPM: on average well over 100 ms per key.
        let per_key = a.steps.last().unwrap().at_ms / a.steps.len() as u64;
        assert!((150..700).contains(&per_key), "{per_key}");
    }

    fn ev(at: u64, t: Option<&str>) -> ChipEvent {
        ChipEvent {
            at_ms: at,
            text: t.map(str::to_string),
        }
    }

    #[test]
    fn scores_latency_accuracy_and_savings_against_ground_truth() {
        let tr = Trace {
            id: "x".into(),
            sentence: "see you soon".into(),
            steps: vec![
                Step {
                    at_ms: 0,
                    before: "s".into(),
                },
                Step {
                    at_ms: 1000,
                    before: "se".into(),
                },
            ],
        };
        // After "s": a model chip at +100 ms "ee you" (correct word "ee", 6 chars matched), later grown.
        let events = vec![
            ev(100, Some("ee")),
            ev(150, Some("ee you")),
            ev(1002, Some("e you")),
        ];
        let s = score(&tr, &events);
        assert_eq!(
            s.model_chips, 1,
            "the +2 ms update is typing-through, not a model chip"
        );
        assert_eq!(s.first_chip_ms, vec![100]);
        assert_eq!(s.first_word_correct, 1);
        assert_eq!(s.saved_chars, 6);
        assert_eq!((s.pauses, s.pauses_with_chip), (1, 1));
    }

    #[test]
    fn wrong_or_partial_words_are_not_counted_as_correct() {
        let tr = Trace {
            id: "x".into(),
            sentence: "the theory".into(),
            steps: vec![Step {
                at_ms: 0,
                before: "the ".into(),
            }],
        };
        let s = score(&tr, &[ev(90, Some("the"))]);
        assert_eq!(s.first_word_correct, 0, "\"the\" is not \"theory\"");
        assert_eq!(
            s.saved_chars, 0,
            "Tab would insert the wrong word, so nothing is saved"
        );
        // A wrong next word saves nothing either, even though its leading space matches.
        let tr2 = Trace {
            id: "y".into(),
            sentence: "see you soon".into(),
            steps: vec![Step {
                at_ms: 0,
                before: "see".into(),
            }],
        };
        assert_eq!(score(&tr2, &[ev(90, Some(" them soon"))]).saved_chars, 0);
    }

    #[test]
    fn accepted_prefix_follows_tab_semantics() {
        assert_eq!(
            accepted_prefix(" you soon at", " you soon there"),
            " you soon".len()
        );
        assert_eq!(
            accepted_prefix("rld, and", "rld, and more"),
            "rld, and".len()
        );
        assert_eq!(accepted_prefix(" yo", " you"), 0);
        assert_eq!(accepted_prefix(" ", " you"), 0);
        assert_eq!(accepted_prefix("", " you"), 0);
    }

    #[test]
    fn summary_ratios_and_percentiles() {
        let a = TraceScore {
            id: "a".into(),
            chars_typed: 10,
            model_chips: 2,
            first_chip_ms: vec![100, 200],
            pauses: 2,
            pauses_with_chip: 1,
            pause_chip_correct: 1,
            first_word_correct: 1,
            saved_chars: 5,
        };
        let s = summarize(&[a.clone(), a]);
        assert_eq!(
            (s.p50_ms, s.p95_ms, s.max_ms),
            (Some(200), Some(200), Some(200))
        );
        assert_eq!(s.pause_coverage, 0.5);
        assert_eq!(s.pause_accuracy, 1.0);
        assert_eq!(s.word_accuracy, 0.5);
        assert_eq!(s.keystroke_savings, 0.5);
        assert_eq!(summarize(&[]), Summary::default());
    }

    proptest! {
        /// Savings are never counted twice and never exceed what was typed, whatever the chips say.
        #[test]
        fn savings_are_bounded(sentence in "[a-z ]{1,60}", chips in proptest::collection::vec("[a-z ]{0,20}", 0..30), seed in any::<u64>()) {
            let tr = trace("p", &sentence, seed);
            let mut events: Vec<ChipEvent> = tr.steps.iter().zip(chips.iter().cycle()).map(|(s, c)| ev(s.at_ms + 90, Some(c))).collect();
            events.sort_by_key(|e| e.at_ms);
            let sc = score(&tr, &events);
            prop_assert!(sc.saved_chars <= sc.chars_typed);
            prop_assert!(sc.first_word_correct <= sc.model_chips);
            prop_assert!(sc.pauses_with_chip <= sc.pauses);
            prop_assert!(sc.pause_chip_correct <= sc.pauses_with_chip);
        }

        /// Accepted text is always a whole-word prefix of what comes next.
        #[test]
        fn accepted_prefix_is_a_word_aligned_prefix(chip in "[a-z ,.]{0,30}", upcoming in "[a-z ,.]{0,30}") {
            let n = accepted_prefix(&chip, &upcoming);
            prop_assert!(n <= upcoming.len());
            prop_assert!(chip.starts_with(&upcoming[..n]));
            // Accepting nothing is always valid; anything accepted ends at a word boundary.
            if n > 0 {
                prop_assert!(upcoming[n..].chars().next().is_none_or(|c| !c.is_alphanumeric()));
            }
        }

        #[test]
        fn common_prefix_is_a_real_prefix(a in "\\PC{0,30}", b in "\\PC{0,30}") {
            let n = common_prefix(&a, &b);
            prop_assert!(a.is_char_boundary(n) && b.is_char_boundary(n));
            prop_assert_eq!(&a[..n], &b[..n]);
        }
    }
}
