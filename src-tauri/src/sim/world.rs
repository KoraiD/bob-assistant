//! Everything around the engine, as one discrete-event world on the virtual clock: the text field of an app,
//! the Swift helper (Accessibility reads, key tap, chip panel, text insertion) and what the user sees.
//!
//! Timing follows the helper's code: a change in the field is coalesced for 4 ms, read, sent to the engine,
//! and re-read after 40, 120 and 300 ms (`FocusObserver.settleDelaysMs`); while a chip is visible the field is
//! also re-read every 250 ms (`setWatching`). Tab, Shift+Tab and Esc are swallowed only while the engine
//! captures keys (`KeyDecision`), otherwise they reach the app. Apps differ in how late Accessibility shows a
//! change and whether the caret moves before the text ([`Profile`]).

use super::actor::{Action, Actor, View};
use super::check::{Defect, Kind};
use super::model::Calls;
use crate::ghost::chip;
use crate::ghost::engine::GhostHandle;
use crate::protocol::{CoreCommand, Key, NativeEvent, Rect};
use rand::rngs::StdRng;
use rand::Rng;
use std::cmp::Ordering;
use std::collections::{BinaryHeap, VecDeque};
use std::time::Duration;
use tokio::time::Instant;

/// How an app reports its text field through Accessibility.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Profile {
    pub name: &'static str,
    pub bundle_id: &'static str,
    /// Delay from a change in the field to Accessibility showing it, in ms (inclusive range).
    pub ax_lag_ms: (u64, u64),
    /// The caret moves in Accessibility about a millisecond after the change, the text only after the lag.
    pub caret_first: bool,
    /// Spaces near the caret are reported as no-break spaces (contenteditable).
    pub nbsp: bool,
}

pub const TEXTEDIT: Profile = Profile {
    name: "TextEdit",
    bundle_id: "com.apple.TextEdit",
    ax_lag_ms: (1, 4),
    caret_first: false,
    nbsp: false,
};

/// Electron editors such as the Kiro editor and VS Code: text shows up late, the caret first.
pub const ELECTRON: Profile = Profile {
    name: "Electron",
    bundle_id: "dev.kiro.desktop",
    ax_lag_ms: (30, 220),
    caret_first: true,
    nbsp: false,
};

/// Chrome contenteditable (Gmail, Slack web): like Electron, plus no-break spaces.
pub const CHROME: Profile = Profile {
    name: "Chrome",
    bundle_id: "com.google.Chrome",
    ax_lag_ms: (20, 160),
    caret_first: true,
    nbsp: true,
};

/// How long a chip may keep showing text that no longer continues the field before it counts as stale.
pub const STALE_LIMIT: Duration = Duration::from_millis(700);
/// Settle re-reads after each delivered snapshot (`FocusObserver.settleDelaysMs`).
const SETTLE_MS: [u64; 3] = [40, 120, 300];
const WATCH_MS: u64 = 250;
const COALESCE_MS: u64 = 4;
/// Unix socket hop between helper and core.
const HOP: Duration = Duration::from_micros(200);
/// From a key event or inserted text to the field changing.
const INPUT: Duration = Duration::from_millis(1);
/// Width of one character in the fake field, for the caret rect.
const CHAR_PX: f64 = 7.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    User,
    /// Accepted suggestion (Tab, Shift+Tab).
    Bob,
    /// Typo fix (`ReplaceBeforeCaret`).
    BobFix,
}

#[derive(Debug, Clone)]
enum Edit {
    Insert(String),
    Backspace,
}

#[derive(Debug)]
enum Ev {
    UserTurn,
    AppEdit {
        edit: Edit,
        by: Origin,
    },
    /// Accessibility starts showing these values.
    Report {
        text: Option<String>,
        caret: Option<usize>,
    },
    AxNotify,
    /// `None`: the coalesced read after a notification; `Some(generation)`: a settle re-read.
    HelperRead {
        settle: Option<u64>,
    },
    WatchTick {
        generation: u64,
    },
    ToEngine(NativeEvent),
    /// A command from the engine reaching the helper; for `ShowChip`, the continuation the chip claims.
    HelperCmd {
        cmd: CoreCommand,
        claim: Option<String>,
    },
    End,
}

struct Item {
    at: Instant,
    seq: u64,
    ev: Ev,
}

impl PartialEq for Item {
    fn eq(&self, o: &Self) -> bool {
        (self.at, self.seq) == (o.at, o.seq)
    }
}
impl Eq for Item {}
impl PartialOrd for Item {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Item {
    /// Reversed, so the heap pops the earliest event first (ties in insertion order).
    fn cmp(&self, o: &Self) -> Ordering {
        (o.at, o.seq).cmp(&(self.at, self.seq))
    }
}

/// The chip as the user sees it.
#[derive(Debug, Clone)]
struct Screen {
    text: String,
    /// The text before the caret that the engine had when it sent the chip, plus the chip: the document the
    /// chip promises.
    claim: String,
}

/// A swallowed Tab or Shift+Tab, waiting for the text it asked for.
#[derive(Debug, Clone)]
struct Accept {
    key: Key,
    claim: Option<String>,
    before: String,
}

/// One line of the session timeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub at_us: u64,
    pub who: &'static str,
    pub what: String,
}

#[derive(Debug, Default, Clone)]
pub struct Stats {
    pub keys: u32,
    pub accepts: u32,
    pub inserts: u32,
    pub chips_painted: u32,
    pub texts_sent: u32,
}

pub struct World {
    start: Instant,
    profile: Profile,
    rng: StdRng,
    queue: BinaryHeap<Item>,
    seq: u64,
    // The app's text field.
    doc: String,
    caret: usize,
    rep_text: String,
    rep_caret: usize,
    last_text_report: Instant,
    // The helper.
    capturing: bool,
    read_pending: bool,
    last_delivered: Option<(String, Rect)>,
    settle_generation: u64,
    watch_generation: u64,
    watching: bool,
    screen: Option<Screen>,
    accepts: VecDeque<Accept>,
    // What the engine was last told, and what it asked for.
    engine_before: String,
    engine_chip: bool,
    engine_capture: bool,
    capture_mismatch: bool,
    ghost: GhostHandle,
    calls: Calls,
    stale_since: Option<Instant>,
    stale_reported: bool,
    ended: bool,
    pub log: Vec<Line>,
    pub defects: Vec<Defect>,
    pub stats: Stats,
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

/// The tail of `s` for the timeline (keeps lines short).
fn tail(s: &str, n: usize) -> String {
    let count = s.chars().count();
    let t: String = s.chars().skip(count.saturating_sub(n)).collect();
    if count > n {
        format!("…{t}")
    } else {
        t
    }
}

impl World {
    pub fn new(
        profile: Profile,
        rng: StdRng,
        ghost: GhostHandle,
        calls: Calls,
        start_text: &str,
    ) -> Self {
        let now = Instant::now();
        let mut w = Self {
            start: now,
            profile,
            rng,
            queue: BinaryHeap::new(),
            seq: 0,
            doc: start_text.to_string(),
            caret: start_text.len(),
            rep_text: start_text.to_string(),
            rep_caret: start_text.chars().count(),
            last_text_report: now,
            capturing: false,
            read_pending: false,
            last_delivered: None,
            settle_generation: 0,
            watch_generation: 0,
            watching: false,
            screen: None,
            accepts: VecDeque::new(),
            engine_before: String::new(),
            engine_chip: false,
            engine_capture: false,
            capture_mismatch: false,
            ghost,
            calls,
            stale_since: None,
            stale_reported: false,
            ended: false,
            log: vec![],
            defects: vec![],
            stats: Stats::default(),
        };
        w.note("app", format!("{} field focused", profile.name));
        w.ghost.native(NativeEvent::Focus {
            app_bundle_id: profile.bundle_id.into(),
            app_name: profile.name.into(),
            secure: false,
            editable: true,
        });
        w.push(ms(10), Ev::AxNotify);
        w.push(ms(20), Ev::UserTurn);
        w
    }

    pub fn ended(&self) -> bool {
        self.ended
    }

    /// The field's text before the caret (what the user sees).
    pub fn before(&self) -> String {
        self.doc[..self.caret].to_string()
    }

    pub fn text(&self) -> &str {
        &self.doc
    }

    pub fn next_time(&self) -> Option<Instant> {
        self.queue.peek().map(|i| i.at)
    }

    fn since_start_us(&self) -> u64 {
        Instant::now().duration_since(self.start).as_micros() as u64
    }

    fn note(&mut self, who: &'static str, what: String) {
        let at_us = self.since_start_us();
        self.log.push(Line { at_us, who, what });
    }

    fn defect(&mut self, kind: Kind, detail: String) {
        let at_ms = self.since_start_us() / 1000;
        self.note("CHECK", format!("{kind:?}: {detail}"));
        self.defects.push(Defect {
            kind,
            at_ms,
            detail,
        });
    }

    fn push(&mut self, delay: Duration, ev: Ev) {
        self.push_at(Instant::now() + delay, ev);
    }

    fn push_at(&mut self, at: Instant, ev: Ev) {
        self.seq += 1;
        self.queue.push(Item {
            at,
            seq: self.seq,
            ev,
        });
    }

    /// A command from the engine, at the moment it was sent.
    pub fn on_command(&mut self, cmd: CoreCommand) {
        let claim = match &cmd {
            CoreCommand::ShowChip { text, .. } => {
                self.engine_chip = true;
                Some(format!("{}{text}", self.engine_before))
            }
            CoreCommand::HideChip => {
                self.engine_chip = false;
                None
            }
            CoreCommand::CaptureKeys { active } => {
                self.engine_capture = *active;
                None
            }
            _ => None,
        };
        self.push(HOP, Ev::HelperCmd { cmd, claim });
    }

    /// Called whenever the engine has nothing more to say at this instant: chip and key capture must agree.
    pub fn check_quiet(&mut self) {
        let mismatch = self.engine_chip != self.engine_capture;
        if mismatch && !self.capture_mismatch {
            self.defect(
                Kind::CaptureMismatch,
                format!(
                    "chip shown {} but key capture {}",
                    self.engine_chip, self.engine_capture
                ),
            );
        }
        self.capture_mismatch = mismatch;
    }

    /// Handle every event that is due now.
    pub fn fire_due(&mut self, actor: &mut dyn Actor) {
        let now = Instant::now();
        while self.queue.peek().is_some_and(|i| i.at <= now) {
            let Some(item) = self.queue.pop() else { break };
            self.handle(item.ev, actor);
            self.check_stale();
        }
    }

    fn handle(&mut self, ev: Ev, actor: &mut dyn Actor) {
        match ev {
            Ev::UserTurn => self.user_turn(actor),
            Ev::AppEdit { edit, by } => self.app_edit(edit, by),
            Ev::Report { text, caret } => {
                if let Some(t) = text {
                    self.rep_text = t;
                }
                if let Some(c) = caret {
                    self.rep_caret = c;
                }
                self.push(Duration::ZERO, Ev::AxNotify);
            }
            Ev::AxNotify => {
                if !self.read_pending {
                    self.read_pending = true;
                    self.push(ms(COALESCE_MS), Ev::HelperRead { settle: None });
                }
            }
            Ev::HelperRead { settle: None } => {
                self.read_pending = false;
                let snap = self.snapshot();
                self.deliver(snap, "read");
                self.settle_generation += 1;
                let g = self.settle_generation;
                for d in SETTLE_MS {
                    self.push(ms(d), Ev::HelperRead { settle: Some(g) });
                }
            }
            Ev::HelperRead { settle: Some(g) } => {
                if g == self.settle_generation && !self.read_pending {
                    let snap = self.snapshot();
                    if self.last_delivered.as_ref() != Some(&snap) {
                        self.deliver(snap, "re-read");
                    }
                }
            }
            Ev::WatchTick { generation } => {
                if generation == self.watch_generation && self.watching {
                    if !self.read_pending {
                        let snap = self.snapshot();
                        if self.last_delivered.as_ref() != Some(&snap) {
                            self.deliver(snap, "watch");
                        }
                    }
                    self.push(ms(WATCH_MS), Ev::WatchTick { generation });
                }
            }
            Ev::ToEngine(ev) => {
                if let NativeEvent::TextState { before_caret, .. } = &ev {
                    self.engine_before = crate::ghost::engine::normalize_typed(before_caret);
                }
                self.ghost.native(ev);
            }
            Ev::HelperCmd { cmd, claim } => self.helper_cmd(cmd, claim),
            Ev::End => self.ended = true,
        }
    }

    // ---- the user ----

    fn user_turn(&mut self, actor: &mut dyn Actor) {
        let before = self.before();
        let (chip, claim) = match &self.screen {
            Some(s) => (Some(s.text.clone()), Some(s.claim.clone())),
            None => (None, None),
        };
        let view = View {
            at_ms: self.since_start_us() / 1000,
            before: &before,
            chip: chip.as_deref(),
            claim: claim.as_deref(),
        };
        match actor.next(&view) {
            None => {
                self.note("user", "done".into());
                self.push(ms(1500), Ev::End);
            }
            Some((action, wait)) => {
                self.user_action(action);
                self.push(wait, Ev::UserTurn);
            }
        }
    }

    fn user_action(&mut self, action: Action) {
        match action {
            Action::Idle => {}
            Action::Type(c) => {
                self.stats.keys += 1;
                self.note("user", format!("types {c:?}"));
                self.push(
                    INPUT,
                    Ev::AppEdit {
                        edit: Edit::Insert(c.to_string()),
                        by: Origin::User,
                    },
                );
            }
            Action::Backspace => {
                self.stats.keys += 1;
                self.note("user", "Backspace".into());
                self.push(
                    INPUT,
                    Ev::AppEdit {
                        edit: Edit::Backspace,
                        by: Origin::User,
                    },
                );
            }
            Action::Press(key) => {
                self.stats.keys += 1;
                if self.capturing {
                    self.note("user", format!("{key:?} (swallowed)"));
                    if key != Key::Escape {
                        self.stats.accepts += 1;
                        let claim = self.screen.as_ref().map(|s| s.claim.clone());
                        let before = self.before();
                        self.accepts.push_back(Accept { key, claim, before });
                    }
                    self.push(HOP, Ev::ToEngine(NativeEvent::Key { key }));
                } else {
                    self.note("user", format!("{key:?} (reaches the app)"));
                    if key == Key::Tab {
                        if self.screen.is_some() {
                            self.defect(
                                Kind::TabLeaked,
                                "Tab went to the app while a chip was visible".into(),
                            );
                        }
                        self.push(
                            INPUT,
                            Ev::AppEdit {
                                edit: Edit::Insert("\t".into()),
                                by: Origin::User,
                            },
                        );
                    }
                }
            }
        }
    }

    // ---- the app ----

    fn app_edit(&mut self, edit: Edit, by: Origin) {
        let before = self.before();
        match &edit {
            Edit::Insert(s) => {
                self.doc.insert_str(self.caret, s);
                self.caret += s.len();
                if by != Origin::User {
                    self.note("app", format!("BOB inserted {s:?}"));
                    self.check_insert(&before, s, by);
                }
            }
            Edit::Backspace => {
                if let Some((i, _)) = self.doc[..self.caret].char_indices().last() {
                    self.doc.replace_range(i..self.caret, "");
                    self.caret = i;
                }
            }
        }
        let lag = self
            .rng
            .random_range(self.profile.ax_lag_ms.0..=self.profile.ax_lag_ms.1);
        let text_at = (Instant::now() + ms(lag)).max(self.last_text_report);
        self.last_text_report = text_at;
        let caret = self.doc[..self.caret].chars().count();
        if self.profile.caret_first {
            self.push(
                ms(1),
                Ev::Report {
                    text: None,
                    caret: Some(caret),
                },
            );
        }
        self.push_at(
            text_at,
            Ev::Report {
                text: Some(self.doc.clone()),
                caret: Some(caret),
            },
        );
    }

    // ---- the helper ----

    /// What Accessibility shows now: text before the reported caret (clamped, like `TextWindow`) and the caret.
    fn snapshot(&self) -> (String, Rect) {
        let mut before: String = self.rep_text.chars().take(self.rep_caret).collect();
        if self.profile.nbsp && before.ends_with(' ') {
            before.pop();
            before.push('\u{a0}');
        }
        let caret = Rect {
            x: 100.0 + CHAR_PX * self.rep_caret as f64,
            y: 200.0,
            width: 1.0,
            height: 18.0,
        };
        (before, caret)
    }

    fn deliver(&mut self, snap: (String, Rect), how: &str) {
        self.stats.texts_sent += 1;
        self.note(
            "helper",
            format!("{how}: text {:?}, caret x {}", tail(&snap.0, 30), snap.1.x),
        );
        self.last_delivered = Some(snap.clone());
        self.push(
            HOP,
            Ev::ToEngine(NativeEvent::TextState {
                app_bundle_id: self.profile.bundle_id.into(),
                secure: false,
                before_caret: snap.0,
                selection_len: 0,
                caret: Some(snap.1),
            }),
        );
    }

    fn helper_cmd(&mut self, cmd: CoreCommand, claim: Option<String>) {
        match cmd {
            CoreCommand::ShowChip { text, .. } => {
                self.stats.chips_painted += 1;
                if text.trim().is_empty() {
                    self.defect(Kind::EmptyChip, format!("{text:?}"));
                }
                self.note("chip", format!("shows {text:?}"));
                let claim = claim.unwrap_or_else(|| format!("{}{text}", self.before()));
                self.screen = Some(Screen { text, claim });
            }
            CoreCommand::HideChip => {
                self.note("chip", "hidden".into());
                self.screen = None;
            }
            CoreCommand::CaptureKeys { active } => {
                self.capturing = active;
                if active && !self.watching {
                    self.watching = true;
                    self.watch_generation += 1;
                    let generation = self.watch_generation;
                    self.push(ms(WATCH_MS), Ev::WatchTick { generation });
                } else if !active {
                    self.watching = false;
                    self.watch_generation += 1;
                }
            }
            CoreCommand::InsertText { text } => {
                self.push(
                    INPUT,
                    Ev::AppEdit {
                        edit: Edit::Insert(text),
                        by: Origin::Bob,
                    },
                );
            }
            CoreCommand::ReplaceBeforeCaret { expected, text } => {
                // `HelperRuntime`: re-read the field; replace only if the word is still there.
                let (before, _) = self.snapshot();
                if crate::ghost::engine::normalize_typed(&before).ends_with(&expected) {
                    for _ in 0..expected.chars().count() {
                        self.push(
                            INPUT,
                            Ev::AppEdit {
                                edit: Edit::Backspace,
                                by: Origin::BobFix,
                            },
                        );
                    }
                    self.push(
                        INPUT,
                        Ev::AppEdit {
                            edit: Edit::Insert(text),
                            by: Origin::BobFix,
                        },
                    );
                }
            }
            _ => {}
        }
    }

    // ---- checks ----

    /// BOB's insertion `s` landed after `before`.
    fn check_insert(&mut self, before: &str, s: &str, by: Origin) {
        self.stats.inserts += 1;
        if by == Origin::Bob {
            match self.accepts.pop_front() {
                None => self.defect(Kind::UnrequestedInsert, format!("{s:?} without a Tab")),
                Some(a) => match a
                    .claim
                    .as_deref()
                    .and_then(|c| expected_insert(c, &a.before, a.key))
                {
                    None => self.defect(
                        Kind::AcceptOfStaleChip,
                        format!(
                            "{:?} inserted {s:?} although the chip no longer continued {:?}",
                            a.key,
                            tail(&a.before, 30)
                        ),
                    ),
                    Some(e) if e != s => self.defect(
                        Kind::WrongInsert,
                        format!(
                            "{:?} after {:?}: expected {e:?}, inserted {s:?}",
                            a.key,
                            tail(&a.before, 30)
                        ),
                    ),
                    Some(_) => {}
                },
            }
        }
        let joint: String = before.chars().last().into_iter().chain(s.chars()).collect();
        if joint.contains("  ") {
            self.defect(Kind::DoubleSpace, format!("{:?} + {s:?}", tail(before, 20)));
        }
        if [" .", " ,", " !", " ?", " ;", " :"]
            .iter()
            .any(|p| joint.contains(p))
        {
            self.defect(
                Kind::SpaceBeforePunctuation,
                format!("{:?} + {s:?}", tail(before, 20)),
            );
        }
        if by == Origin::Bob {
            self.check_fragment(before, s);
        }
    }

    /// The insertion stops inside a word that the model's answer continued.
    fn check_fragment(&mut self, before: &str, s: &str) {
        if !s.ends_with(char::is_alphanumeric) {
            return;
        }
        let after = format!("{before}{s}");
        let calls = self
            .calls
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let next = calls.iter().rev().find_map(|c| {
            let polished = chip::polish(&c.answer, &c.prompt)?;
            let path = format!("{}{polished}", c.prompt);
            path.strip_prefix(after.as_str())
                .and_then(|rest| rest.chars().next())
        });
        if next.is_some_and(char::is_alphanumeric) {
            self.defect(
                Kind::Fragment,
                format!("{s:?} stops inside a word (the model continued it)"),
            );
        }
    }

    fn check_stale(&mut self) {
        let before = self.before();
        let valid = self
            .screen
            .as_ref()
            .is_none_or(|s| s.claim.starts_with(&before) && s.claim.len() > before.len());
        if valid {
            self.stale_since = None;
            self.stale_reported = false;
            return;
        }
        let since = *self.stale_since.get_or_insert_with(Instant::now);
        if !self.stale_reported && since.elapsed() > STALE_LIMIT {
            self.stale_reported = true;
            let chip = self
                .screen
                .as_ref()
                .map(|s| s.text.clone())
                .unwrap_or_default();
            self.defect(
                Kind::StaleChip,
                format!(
                    "{chip:?} still shown {} ms after it stopped continuing {:?}",
                    since.elapsed().as_millis(),
                    tail(&before, 30)
                ),
            );
        }
    }
}

/// What a Tab or Shift+Tab should insert, given the chip's promise `claim` and the text before the caret at
/// the key press. `None` when the chip no longer continues the text. Tab takes the next word, and the rest of
/// the chip when only whitespace would remain (a space the chip owns).
pub fn expected_insert(claim: &str, before: &str, key: Key) -> Option<String> {
    let rest = claim.strip_prefix(before)?;
    if rest.trim().is_empty() {
        return None;
    }
    match key {
        Key::Tab => {
            let (word, remainder) = chip::accept_word(rest);
            Some(if remainder.trim().is_empty() {
                rest.to_string()
            } else {
                word
            })
        }
        Key::ShiftTab => Some(rest.to_string()),
        Key::Escape => None,
    }
}
