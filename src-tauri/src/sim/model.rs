//! The fake model: answers Ghost Text requests from memory on the virtual clock.
//!
//! It behaves like llama.cpp serving a small instruct model as a raw continuation: tokens arrive one by one
//! after a prefill delay, generation stops at the request's `stop` strings and `max_tokens`, and the text has
//! the quirks seen in real output (docs/PERFORMANCE.md): an extra leading space, a glued word, a repeat of the
//! last typed word, drift after the sentence end, a new line.

use crate::brain::client::scripted::Completion;
use crate::brain::client::TokenStream;
use crate::brain::dialect::CompletionRequest;
use crate::brain::BrainError;
use crate::ghost::chip;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// One request the model answered.
#[derive(Debug, Clone)]
pub struct Call {
    pub at: tokio::time::Instant,
    pub prompt: String,
    /// Everything the model would stream for this request (after `stop` and `max_tokens`).
    pub answer: String,
    pub tokens: usize,
    pub delivered: usize,
    /// The engine dropped the stream before the last token (a new keystroke cancelled it).
    pub cancelled: bool,
}

pub type Calls = Arc<Mutex<Vec<Call>>>;

/// How often each quirk shows up in a random answer (0 to 1).
#[derive(Debug, Clone, Copy)]
pub struct Quirks {
    /// A leading space although the text already ends with one.
    pub lead_space_after_space: f64,
    /// A new word glued to a finished word (no space).
    pub glue: f64,
    /// The first word repeats the last typed word.
    pub repeat_last: f64,
    /// More text after the sentence end ("as soon as possible. A. 正确").
    pub sentence_drift: f64,
    /// A new line and unrelated text (cut by the `stop` strings).
    pub newline_drift: f64,
}

impl Default for Quirks {
    fn default() -> Self {
        Self {
            lead_space_after_space: 0.2,
            glue: 0.05,
            repeat_last: 0.05,
            sentence_drift: 0.3,
            newline_drift: 0.15,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Model {
    /// The answer for the longest key that the prompt ends with; nothing when none matches. Fixed timing:
    /// first token after 40 ms, then one every 8 ms.
    Table(Vec<(&'static str, &'static str)>),
    /// Like `Table`, with the first token after `first_ms` and then one every `gap_ms` (a slow stream).
    Paced {
        table: Vec<(&'static str, &'static str)>,
        first_ms: u64,
        gap_ms: u64,
    },
    /// Generated from [`VOCAB`] with [`Quirks`]; seeded per request, so a session replays exactly.
    Random(Quirks),
}

/// Words the random model and the random typist share, so suggestions can be typed through.
pub const VOCAB: &[&str] = &[
    "the",
    "a",
    "to",
    "and",
    "you",
    "we",
    "will",
    "get",
    "back",
    "soon",
    "as",
    "can",
    "meeting",
    "tomorrow",
    "thanks",
    "for",
    "update",
    "report",
    "send",
    "please",
    "let",
    "me",
    "know",
    "if",
    "have",
    "any",
    "questions",
    "see",
    "at",
    "morning",
    "afternoon",
    "next",
    "week",
    "about",
    "project",
    "team",
    "time",
    "work",
    "on",
    "it",
    "this",
    "that",
    "great",
    "sounds",
    "good",
    "talk",
    "later",
    "call",
    "review",
    "document",
    "draft",
    "shared",
    "with",
    "everyone",
    "today",
    "possible",
    "I'm",
    "don't",
    "we're",
    "it's",
    "attached",
    "latest",
    "version",
    "budget",
];

impl Model {
    /// The scripted brain for one session. `calls` records every request.
    pub fn script(self, seed: u64, calls: Calls) -> Completion {
        let counter = Arc::new(AtomicU64::new(0));
        Arc::new(move |req: &CompletionRequest| {
            let n = counter.fetch_add(1, Ordering::SeqCst);
            let mut rng =
                StdRng::seed_from_u64(seed ^ n.wrapping_add(1).wrapping_mul(0x9E37_79B9_7F4A_7C15));
            let (raw, first_ms, gap_ms) = match &self {
                Model::Table(t) => (table_answer(t, &req.prompt), 40, 8),
                Model::Paced {
                    table,
                    first_ms,
                    gap_ms,
                } => (table_answer(table, &req.prompt), *first_ms, *gap_ms),
                Model::Random(_) if gap_subject(&req.prompt).is_some() => (
                    generate_gap(&mut rng),
                    rng.random_range(30..=70),
                    rng.random_range(5..=12),
                ),
                Model::Random(q) => (
                    generate(&mut rng, &req.prompt, q),
                    rng.random_range(30..=70),
                    rng.random_range(5..=12),
                ),
            };
            let mut tokens = tokenize(&server_cut(&raw, &req.stop));
            tokens.truncate(req.max_tokens as usize);
            let answer = tokens.concat();
            let feed: VecDeque<(Duration, String)> = tokens
                .into_iter()
                .enumerate()
                .map(|(i, t)| {
                    let ms = if i == 0 { first_ms } else { gap_ms };
                    (Duration::from_millis(ms), t)
                })
                .collect();
            let idx = {
                let mut c = calls
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                c.push(Call {
                    at: tokio::time::Instant::now(),
                    prompt: req.prompt.clone(),
                    answer,
                    tokens: feed.len(),
                    delivered: 0,
                    cancelled: false,
                });
                c.len() - 1
            };
            stream(
                feed,
                Guard {
                    calls: calls.clone(),
                    idx,
                    done: false,
                },
            )
        })
    }
}

/// The scripted chat brain (Selection Actions, dictation cleanup, voice edit). Each request is recorded with
/// `prompt` = system prompt, a `\n---\n` line and the user's text. The answer is the text, lightly edited,
/// sometimes behind a preamble that the cleaner must remove, or an explanation for Explain and Reply. The
/// first token takes 80 ms to 2.5 s, so a user can change their mind while it runs.
pub fn chat_script(seed: u64, calls: Calls) -> crate::brain::client::scripted::Chat {
    let counter = Arc::new(AtomicU64::new(0));
    Arc::new(move |req: &crate::brain::dialect::ChatRequest| {
        let n = counter.fetch_add(1, Ordering::SeqCst);
        let mut rng = StdRng::seed_from_u64(
            seed ^ 0xC4A7 ^ n.wrapping_add(1).wrapping_mul(0x9E37_79B9_7F4A_7C15),
        );
        let system = req.system.to_lowercase();
        let body = req.user.trim();
        let answer = if system.contains("explain") || system.contains("reply") {
            "It asks for an update by Friday.".to_string()
        } else if body.is_empty() {
            String::new()
        } else {
            let mut edited = body.to_string();
            if let Some(first) = edited.get(..1) {
                edited.replace_range(..1, &first.to_uppercase());
            }
            if !edited.ends_with(['.', '!', '?']) {
                edited.push('.');
            }
            if rng.random_bool(0.2) {
                format!("Here is the corrected text:\n{edited}")
            } else {
                edited
            }
        };
        let first_ms = if rng.random_bool(0.15) {
            rng.random_range(1200..=2500)
        } else {
            rng.random_range(80..=400)
        };
        let feed: VecDeque<(Duration, String)> = tokenize(&answer)
            .into_iter()
            .enumerate()
            .map(|(i, t)| (Duration::from_millis(if i == 0 { first_ms } else { 12 }), t))
            .collect();
        let idx = {
            let mut c = calls
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            c.push(Call {
                at: tokio::time::Instant::now(),
                prompt: format!("{}\n---\n{}", req.system, req.user),
                answer: answer.clone(),
                tokens: feed.len(),
                delivered: 0,
                cancelled: false,
            });
            c.len() - 1
        };
        stream(
            feed,
            Guard {
                calls: calls.clone(),
                idx,
                done: false,
            },
        )
    })
}

/// Marks the call cancelled when the engine drops the stream before its end.
struct Guard {
    calls: Calls,
    idx: usize,
    done: bool,
}

impl Guard {
    /// The stream ended normally (not cancelled).
    fn finish(&mut self) {
        self.done = true;
    }

    fn update(&self, f: impl FnOnce(&mut Call)) {
        let mut c = self
            .calls
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(call) = c.get_mut(self.idx) {
            f(call);
        }
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        if !self.done {
            self.update(|c| c.cancelled = true);
        }
    }
}

fn stream(feed: VecDeque<(Duration, String)>, guard: Guard) -> TokenStream {
    Box::pin(futures_util::stream::unfold(
        (feed, guard),
        |(mut feed, mut guard)| async move {
            let Some((delay, token)) = feed.pop_front() else {
                guard.finish();
                return None;
            };
            tokio::time::sleep(delay).await;
            guard.update(|c| c.delivered += 1);
            Some((Ok::<String, BrainError>(token), (feed, guard)))
        },
    ))
}

/// For the gap prompt (`ghost::prompt::gap`, a cursor inside a sentence): the text before the gap.
pub fn gap_subject(prompt: &str) -> Option<&str> {
    let line = prompt.rsplit("\nText: ").next()?;
    prompt.ends_with("Missing: [").then_some(())?;
    line.split_once("[...]").map(|(before, _)| before)
}

/// One or two words and the closing bracket, the way the default model answers the gap prompt.
fn generate_gap(rng: &mut StdRng) -> String {
    let n = rng.random_range(1..=2);
    let words: Vec<&str> = (0..n).map(|_| pick(rng, VOCAB)).collect();
    format!("{}]", words.join(" "))
}

fn table_answer(table: &[(&str, &str)], prompt: &str) -> String {
    let subject = gap_subject(prompt).unwrap_or(prompt);
    table
        .iter()
        .filter(|(key, _)| subject.ends_with(key))
        .max_by_key(|(key, _)| key.len())
        .map(|(_, a)| (*a).to_string())
        .unwrap_or_default()
}

/// What the server sends: generation stops before the first stop string (llama.cpp excludes it).
pub fn server_cut(raw: &str, stop: &[String]) -> String {
    let end = stop
        .iter()
        .filter(|s| !s.is_empty())
        .filter_map(|s| raw.find(s.as_str()))
        .min()
        .unwrap_or(raw.len());
    raw[..end].to_string()
}

/// Split text roughly like a BPE tokenizer: a word with its leading space is one token, long words are split
/// into pieces of 3 to 4 characters, punctuation and new lines stand alone.
pub fn tokenize(text: &str) -> Vec<String> {
    let mut pieces: Vec<String> = vec![];
    let mut cur = String::new();
    for c in text.chars() {
        let word_char = c.is_alphanumeric() || c == '\'';
        if c == '\n' || (!word_char && !c.is_whitespace()) {
            if !cur.is_empty() {
                pieces.push(std::mem::take(&mut cur));
            }
            pieces.push(c.to_string());
        } else if c.is_whitespace() && cur.chars().any(|x| !x.is_whitespace()) {
            pieces.push(std::mem::take(&mut cur));
            cur.push(c);
        } else {
            cur.push(c);
        }
    }
    if !cur.is_empty() {
        pieces.push(cur);
    }
    let mut out = vec![];
    for p in pieces {
        let lead: String = p.chars().take_while(|c| *c == ' ').collect();
        let word: Vec<char> = p.chars().skip(lead.chars().count()).collect();
        if word.len() <= 5 {
            out.push(p);
            continue;
        }
        let (first, rest) = word.split_at(4);
        out.push(format!("{lead}{}", first.iter().collect::<String>()));
        for chunk in rest.chunks(3) {
            out.push(chunk.iter().collect());
        }
    }
    out
}

fn pick<'a>(rng: &mut StdRng, words: &[&'a str]) -> &'a str {
    words[rng.random_range(0..words.len())]
}

/// A continuation of `prompt` with the configured quirks.
pub fn generate(rng: &mut StdRng, prompt: &str, q: &Quirks) -> String {
    let mut out = String::new();
    let ends_space = prompt.is_empty() || prompt.ends_with(char::is_whitespace);
    let last_word = prompt.split_whitespace().last().unwrap_or("");
    if chip::mid_word(prompt) {
        let partial = prompt
            .rsplit(|c: char| !c.is_alphanumeric() && c != '\'')
            .next()
            .unwrap_or("");
        let lower = partial.to_lowercase();
        let longer: Vec<&str> = VOCAB
            .iter()
            .copied()
            .filter(|w| w.to_lowercase().starts_with(&lower) && w.len() > partial.len())
            .collect();
        if !longer.is_empty() && rng.random_bool(0.8) {
            let w = pick(rng, &longer);
            out.push_str(&w[partial.len()..]);
        } else if rng.random_bool(q.glue) {
            out.push_str(pick(rng, VOCAB));
        }
    } else if ends_space && rng.random_bool(q.lead_space_after_space) {
        out.push(' ');
    }
    let words = rng.random_range(1..=5);
    for i in 0..words {
        let w = if i == 0 && rng.random_bool(q.repeat_last) && !last_word.is_empty() {
            last_word
        } else {
            pick(rng, VOCAB)
        };
        if !(out.is_empty() && ends_space) {
            out.push(' ');
        }
        out.push_str(w);
    }
    if rng.random_bool(0.4) {
        out.push('.');
        if rng.random_bool(q.sentence_drift) {
            out.push_str(if rng.random_bool(0.5) {
                " A. 正确 B."
            } else {
                " Then the next"
            });
        }
    }
    if rng.random_bool(q.newline_drift) {
        out.push_str("\nQuestion: what");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_rebuild_the_text_and_split_like_a_tokenizer() {
        let t = tokenize(" as soon as possible. A. 正确\nQ");
        assert_eq!(t.concat(), " as soon as possible. A. 正确\nQ");
        assert_eq!(&t[..5], &[" as", " soon", " as", " poss", "ibl"]);
        assert!(t.contains(&"\n".to_string()));
    }

    #[test]
    fn the_server_stops_before_the_stop_string() {
        let stop = vec!["\n".to_string()];
        assert_eq!(server_cut(" you soon.\nQuestion", &stop), " you soon.");
        assert_eq!(server_cut(" you soon.", &stop), " you soon.");
    }

    #[test]
    fn gap_prompts_are_answered_for_the_text_before_the_gap() {
        let prompt = crate::ghost::prompt::gap("Send me the ", "report today.");
        assert_eq!(gap_subject(&prompt), Some("Send me the "));
        assert_eq!(gap_subject("Send me the "), None);
        let t = [("me the ", "latest]")];
        assert_eq!(table_answer(&t, &prompt), "latest]");
    }

    #[test]
    fn the_table_uses_the_longest_matching_key() {
        let t = [("to", " you"), ("back to", " you soon.")];
        assert_eq!(table_answer(&t, "I will get back to"), " you soon.");
        assert_eq!(table_answer(&t, "nothing"), "");
    }

    #[test]
    fn random_answers_replay_from_the_seed() {
        let a = generate(
            &mut StdRng::seed_from_u64(9),
            "I will get back to",
            &Quirks::default(),
        );
        let b = generate(
            &mut StdRng::seed_from_u64(9),
            "I will get back to",
            &Quirks::default(),
        );
        assert_eq!(a, b);
        // Mid-word prompts mostly finish the word.
        let finished = (0..50)
            .filter(|i| {
                let s = generate(
                    &mut StdRng::seed_from_u64(*i),
                    "See you tomor",
                    &Quirks::default(),
                );
                s.starts_with("row")
            })
            .count();
        assert!(finished > 30, "{finished}");
    }
}
