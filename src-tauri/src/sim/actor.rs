//! The user in a simulated session: something that looks at the screen and decides the next key.

use crate::protocol::Key;
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
