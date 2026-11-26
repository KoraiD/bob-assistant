//! The user in a simulated session: something that looks at the screen and decides the next key.

use super::model::VOCAB;
use super::world::TYPOS;
use crate::protocol::Key;
use rand::rngs::StdRng;
use rand::Rng;
use std::collections::VecDeque;
use std::time::Duration;

/// What the user sees when deciding.
#[derive(Debug)]
pub struct View<'a> {
    pub at_ms: u64,
    /// The field's text before the caret.
    pub before: &'a str,
    /// The chip on screen.
    pub chip: Option<&'a str>,
    /// The document the visible chip promises (text before the caret when it was sent, plus the chip).
    pub claim: Option<&'a str>,
    /// The chip offers a typo fix ("teh → the").
    pub fix_chip: bool,
}

impl View<'_> {
    /// The chip's next characters as seen from the field now, when the chip still continues it.
    pub fn chip_rest(&self) -> Option<&str> {
        self.claim?
            .strip_prefix(self.before)
            .filter(|rest| !rest.is_empty())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Do nothing this turn (waiting, thinking).
    Idle,
    Type(char),
    Backspace,
    Press(Key),
    /// Arrow keys: move the caret by this many characters.
    Caret(i32),
    /// Switch to another app for this long, then come back.
    SwitchApp(Duration),
}

pub trait Actor {
    /// The next action and how long until the next turn; `None` ends the session.
    fn next(&mut self, view: &View) -> Option<(Action, Duration)>;
}

/// A step of a hand-written session.
#[derive(Debug, Clone)]
pub enum Step {
    /// Type the text, one key every `gap`.
    Type(&'static str, Duration),
    Wait(Duration),
    Press(Key),
    Backspace,
    /// Arrow keys: move the caret by this many characters.
    Caret(i32),
    /// Type the chip's next `n` characters (stops early when there is no chip).
    TypeChip(usize, Duration),
    /// Wait until a chip is visible, at most this long.
    WaitChip(Duration),
}

/// Plays a fixed list of [`Step`]s.
pub struct Script {
    steps: VecDeque<Step>,
    pending: VecDeque<char>,
    gap: Duration,
    waiting_since: Option<u64>,
}

impl Script {
    pub fn new(steps: impl IntoIterator<Item = Step>) -> Self {
        Self {
            steps: steps.into_iter().collect(),
            pending: VecDeque::new(),
            gap: Duration::ZERO,
            waiting_since: None,
        }
    }
}

const POLL: Duration = Duration::from_millis(10);

impl Actor for Script {
    fn next(&mut self, view: &View) -> Option<(Action, Duration)> {
        if let Some(c) = self.pending.pop_front() {
            return Some((Action::Type(c), self.gap));
        }
        match self.steps.front().cloned()? {
            Step::Type(text, gap) => {
                self.steps.pop_front();
                self.pending = text.chars().collect();
                self.gap = gap;
                let c = self.pending.pop_front()?;
                Some((Action::Type(c), gap))
            }
            Step::Wait(d) => {
                self.steps.pop_front();
                Some((Action::Idle, d))
            }
            Step::Press(key) => {
                self.steps.pop_front();
                Some((Action::Press(key), POLL))
            }
            Step::Backspace => {
                self.steps.pop_front();
                Some((Action::Backspace, Duration::from_millis(160)))
            }
            Step::Caret(delta) => {
                self.steps.pop_front();
                Some((Action::Caret(delta), Duration::from_millis(200)))
            }
            Step::TypeChip(n, gap) => {
                let next = view.chip_rest().and_then(|r| r.chars().next());
                match next {
                    Some(c) if n > 0 => {
                        if let Some(Step::TypeChip(left, _)) = self.steps.front_mut() {
                            *left -= 1;
                        }
                        Some((Action::Type(c), gap))
                    }
                    _ => {
                        self.steps.pop_front();
                        Some((Action::Idle, POLL))
                    }
                }
            }
            Step::WaitChip(max) => {
                let since = *self.waiting_since.get_or_insert(view.at_ms);
                if view.chip.is_some() || view.at_ms.saturating_sub(since) >= max.as_millis() as u64
                {
                    self.steps.pop_front();
                    self.waiting_since = None;
                }
                Some((Action::Idle, POLL))
            }
        }
    }
}

/// One decision of the random typist: a roll (0 to 99) picks the kind of action, `timing` the human delay,
/// `pick` the word or character. A session is a list of these, so proptest can shrink a failing session to
/// the few decisions that matter.
pub type Choice = (u8, u16, u8);

/// Random choices with the same distribution proptest uses (seeded soak runs).
pub fn random_choices(rng: &mut StdRng, n: usize) -> Vec<Choice> {
    (0..n)
        .map(|_| (rng.random_range(0..100), rng.random(), rng.random()))
        .collect()
}

/// A person typing at a normal speed (150 to 350 ms between keys, with pauses) who reacts to the chip like
/// people do: types part of it, takes a word or all of it (sometimes tapping Tab again quickly), dismisses it,
/// types something else, corrects with Backspace, moves the caret, switches apps.
pub struct Typist {
    choices: VecDeque<Choice>,
    word: VecDeque<char>,
    last: Action,
    /// Words to type first (a canary for the log check, a credential for the redaction check).
    inject: VecDeque<String>,
}

impl Typist {
    pub fn new(choices: impl IntoIterator<Item = Choice>) -> Self {
        Self {
            choices: choices.into_iter().collect(),
            word: VecDeque::new(),
            last: Action::Idle,
            inject: VecDeque::new(),
        }
    }

    /// Type these words before any of the person's own (in order, as whole words).
    pub fn with_words(mut self, words: impl IntoIterator<Item = String>) -> Self {
        self.inject = words.into_iter().collect();
        self
    }

    /// The next character of the person's own sentence: words from the shared vocabulary (now and then a
    /// misspelled one), separated by spaces, sometimes ending a sentence.
    fn own_char(&mut self, view: &View, pick: u8) -> char {
        if let Some(c) = self.word.pop_front() {
            return c;
        }
        let last = view.before.chars().last();
        if last.is_some_and(|c| !c.is_whitespace()) {
            return match pick % 16 {
                0 => '.',
                1 => ',',
                _ => ' ',
            };
        }
        let word = match self.inject.pop_front() {
            Some(w) => w,
            None if pick.is_multiple_of(25) => TYPOS[usize::from(pick) % TYPOS.len()].0.to_string(),
            None => VOCAB[usize::from(pick) % VOCAB.len()].to_string(),
        };
        self.word = word.chars().collect();
        self.word.pop_front().unwrap_or(' ')
    }

    /// Something other than the chip's next character.
    fn other_char(rest: &str, pick: u8) -> char {
        let next = rest.chars().next();
        let options = ['e', 'a', 's', 't', ' ', 'o', 'n', ','];
        let c = options[usize::from(pick) % options.len()];
        if Some(c) == next {
            'x'
        } else {
            c
        }
    }
}

fn human_gap(timing: u16) -> Duration {
    let t = u64::from(timing);
    match t % 10 {
        // A pause: long enough for the chip to appear.
        0 | 1 => Duration::from_millis(500 + t % 1000),
        _ => Duration::from_millis(150 + t % 200),
    }
}

impl Actor for Typist {
    fn next(&mut self, view: &View) -> Option<(Action, Duration)> {
        let (roll, timing, pick) = self.choices.pop_front()?;
        let gap = human_gap(timing);
        let action = if view.fix_chip {
            match roll {
                0..=49 => Action::Press(Key::Tab),
                50..=59 => Action::Press(Key::Escape),
                _ => Action::Type(self.own_char(view, pick)),
            }
        } else if view.chip.is_some() {
            let rest = view.chip_rest().unwrap_or("");
            match roll {
                // Tapping Tab again right after a Tab, word by word.
                0..=44 if self.last == Action::Press(Key::Tab) => Action::Press(Key::Tab),
                0..=29 => rest
                    .chars()
                    .next()
                    .map_or(Action::Type(self.own_char(view, pick)), Action::Type),
                30..=49 => Action::Press(Key::Tab),
                50..=61 => Action::Press(Key::ShiftTab),
                62..=69 => Action::Press(Key::Escape),
                70..=89 => Action::Type(Self::other_char(rest, pick)),
                90..=94 => Action::Backspace,
                95..=97 => Action::Caret(if pick % 2 == 0 { -1 } else { 1 }),
                _ => Action::SwitchApp(Duration::from_millis(800 + u64::from(timing) % 1500)),
            }
        } else {
            match roll {
                0..=3 => Action::Backspace,
                4 => Action::Caret(-1 - i32::from(pick % 3)),
                5 => Action::Caret(1),
                _ => Action::Type(self.own_char(view, pick)),
            }
        };
        // After a Tab, people often look again quickly (and may tap Tab again, see above).
        let gap = if action == Action::Press(Key::Tab) && timing % 3 == 0 {
            Duration::from_millis(80 + u64::from(timing) % 100)
        } else {
            gap
        };
        self.last = action;
        Some((action, gap))
    }
}
