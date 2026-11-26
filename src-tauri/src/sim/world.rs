//! Everything around the engine, as one discrete-event world on the virtual clock: the text field of an app,
//! the Swift helper (Accessibility reads, key tap, typo check, chip panel, text insertion) and what the user
//! sees.
//!
//! Timing follows the helper's code: a change in the field is coalesced for 4 ms, read, sent to the engine,
//! and re-read after 40, 120 and 300 ms (`FocusObserver.settleDelaysMs`); while a chip is visible the field is
//! also re-read every 250 ms (`setWatching`). Each read sends the text before the caret (at most 2000
//! characters, `TextWindow`), then a typo event when the last finished word is misspelled, then the text after
//! the caret when it changed (`HelperRuntime.forward`). Tab, Shift+Tab and Esc are swallowed only while the
//! engine captures keys (`KeyDecision`), otherwise they reach the app. Apps differ in how late Accessibility
//! shows a change, whether the caret moves before the text, and what they rewrite ([`Profile`]).

use super::actor::{Action, Actor, View};
use super::check::{Defect, Kind};
use super::model::Calls;
use crate::context::ContextHub;
use crate::ghost::chip;
use crate::ghost::engine::{normalize_typed, GhostHandle};
use crate::protocol::{CoreCommand, Key, NativeEvent, Rect};
use rand::rngs::StdRng;
use rand::Rng;
use std::cmp::Ordering;
use std::collections::{BinaryHeap, VecDeque};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::Instant;

/// How an app reports its text field through Accessibility, and what it rewrites while you type.
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
    /// Straight apostrophes and quotes become curly as they are typed (macOS smart quotes).
    pub smart_quotes: bool,
}

pub const TEXTEDIT: Profile = Profile {
    name: "TextEdit",
    bundle_id: "com.apple.TextEdit",
    ax_lag_ms: (1, 4),
    caret_first: false,
    nbsp: false,
    smart_quotes: false,
};

/// TextEdit with Edit > Substitutions > Smart Quotes on (the macOS default for rich text).
pub const TEXTEDIT_SMART: Profile = Profile {
    name: "TextEdit smart quotes",
    smart_quotes: true,
    ..TEXTEDIT
};

/// Electron editors such as the Kiro editor and VS Code: text shows up late, the caret first.
pub const ELECTRON: Profile = Profile {
    name: "Electron",
    bundle_id: "dev.kiro.desktop",
    ax_lag_ms: (30, 220),
    caret_first: true,
    nbsp: false,
    smart_quotes: false,
};

/// Chrome contenteditable (Gmail, Slack web): like Electron, plus no-break spaces.
pub const CHROME: Profile = Profile {
    name: "Chrome",
    bundle_id: "com.google.Chrome",
    ax_lag_ms: (20, 160),
    caret_first: true,
    nbsp: true,
    smart_quotes: false,
};

pub const PROFILES: [Profile; 4] = [TEXTEDIT, TEXTEDIT_SMART, ELECTRON, CHROME];

/// Misspellings the fake spell checker knows (the real helper asks `NSSpellChecker`).
pub const TYPOS: &[(&str, &str)] = &[
    ("recieve", "receive"),
    ("teh", "the"),
    ("definately", "definitely"),
    ("adress", "address"),
];

/// How long a chip may keep showing text that no longer continues the field before it counts as stale.
pub const STALE_LIMIT: Duration = Duration::from_millis(700);
/// Settle re-reads after each delivered snapshot (`FocusObserver.settleDelaysMs`).
const SETTLE_MS: [u64; 3] = [40, 120, 300];
const WATCH_MS: u64 = 250;
const COALESCE_MS: u64 = 4;
/// `TextWindow.defaultMaxChars`.
pub const WINDOW_CHARS: usize = 2000;
/// Unix socket hop between helper and core.
const HOP: Duration = Duration::from_micros(200);
/// From a key event or inserted text to the field changing.
const INPUT: Duration = Duration::from_millis(1);
/// Width of one character in the fake field, for the caret rect.
const CHAR_PX: f64 = 7.0;
const OTHER_APP: &str = "com.apple.Notes";

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
    /// Move the caret by this many characters (arrow keys).
    Caret(i32),
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
    /// An event for the core. `before` is the whole text before the caret it reports (the event may carry
    /// only the last `WINDOW_CHARS`).
    ToEngine {
        ev: NativeEvent,
        before: Option<String>,
    },
    /// A command from the engine reaching the helper; for `ShowChip`, the continuation the chip claims.
    HelperCmd {
        cmd: CoreCommand,
        claim: Option<String>,
    },
    /// Typo fix: the helper checked the reported text; the deletions start now.
    FixStart {
        expected: String,
    },
    /// The user switches to another app (`true`) or back.
    Focus {
        away: bool,
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
    /// For a continuation: the text before the caret that the engine had when it sent the chip, plus the
    /// chip (the document the chip promises). `None` for a typo fix ("teh → the").
    claim: Option<String>,
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
    pub chips_hidden: u32,
    pub texts_sent: u32,
    pub fixes: u32,
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
    away: bool,
    capturing: bool,
    read_pending: bool,
    last_delivered: Option<(String, Rect)>,
    last_typo_check: String,
    last_after: Option<String>,
    window_title: Option<String>,
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
    context: Arc<ContextHub>,
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
pub fn tail(s: &str, n: usize) -> String {
    let count = s.chars().count();
    let t: String = s.chars().skip(count.saturating_sub(n)).collect();
    if count > n {
        format!("…{t}")
    } else {
        t
    }
}

/// The last finished word when the fake spell checker knows it as a misspelling: `(word, fix, trailing)`.
/// Like `TypoFinder.lastWord`: the word is followed by at most three spaces or punctuation marks.
fn find_typo(before: &str) -> Option<(String, String, String)> {
    let trailing: String = before
        .chars()
        .rev()
        .take_while(|c| " .,;:!?".contains(*c))
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    if trailing.is_empty() || trailing.chars().count() > 3 {
        return None;
    }
    let body = &before[..before.len() - trailing.len()];
    let word: String = body
        .chars()
        .rev()
        .take_while(|c| c.is_alphabetic())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let fix = TYPOS.iter().find(|(w, _)| *w == word)?.1;
    Some((word, fix.to_string(), trailing))
}

/// macOS smart quotes, as far as the fake field needs them: an apostrophe after a letter becomes ’.
fn smart_quote(before: &str, typed: &str) -> String {
    let mut prev = before.chars().last();
    typed
        .chars()
        .map(|c| {
            let out = if c == '\'' && prev.is_some_and(char::is_alphanumeric) {
                '’'
            } else {
                c
            };
            prev = Some(c);
            out
        })
        .collect()
}

impl World {
    pub fn new(
        profile: Profile,
        rng: StdRng,
        ghost: GhostHandle,
        context: Arc<ContextHub>,
        calls: Calls,
        start_text: &str,
        start_caret: Option<usize>,
    ) -> Self {
        let now = Instant::now();
        let caret = start_caret
            .unwrap_or(start_text.len())
            .min(start_text.len());
        let mut w = Self {
            start: now,
            profile,
            rng,
            queue: BinaryHeap::new(),
            seq: 0,
            doc: start_text.to_string(),
            caret,
            rep_text: start_text.to_string(),
            rep_caret: start_text[..caret].chars().count(),
            last_text_report: now,
            away: false,
            capturing: false,
            read_pending: false,
            last_delivered: None,
            last_typo_check: String::new(),
            last_after: None,
            window_title: None,
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
            context,
            calls,
            stale_since: None,
            stale_reported: false,
            ended: false,
            log: vec![],
            defects: vec![],
            stats: Stats::default(),
        };
        w.focus_event(profile.bundle_id, profile.name);
        w.push(ms(10), Ev::AxNotify);
        w.push(ms(20), Ev::UserTurn);
        w
    }

    /// The title of the app's window, sent with the text after the caret (a personal-context layer).
    pub fn set_window_title(&mut self, title: Option<String>) {
        self.window_title = title;
    }

    pub fn ended(&self) -> bool {
        self.ended
    }

    /// The field's text before the caret (what the user sees).
    pub fn before(&self) -> String {
        self.doc[..self.caret].to_string()
    }

    fn after(&self) -> &str {
        &self.doc[self.caret..]
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
                if !self.read_pending && !self.away {
                    self.read_pending = true;
                    self.push(ms(COALESCE_MS), Ev::HelperRead { settle: None });
                }
            }
            Ev::HelperRead { settle: None } => {
                self.read_pending = false;
                if self.away {
                    return;
                }
                self.deliver("read");
                self.settle_generation += 1;
                let g = self.settle_generation;
                for d in SETTLE_MS {
                    self.push(ms(d), Ev::HelperRead { settle: Some(g) });
                }
            }
            Ev::HelperRead { settle: Some(g) } => {
                if g == self.settle_generation && !self.read_pending && !self.away {
                    self.deliver_if_changed("re-read");
                }
            }
            Ev::WatchTick { generation } => {
                if generation == self.watch_generation && self.watching {
                    if !self.read_pending && !self.away {
                        self.deliver_if_changed("watch");
                    }
                    self.push(ms(WATCH_MS), Ev::WatchTick { generation });
                }
            }
            Ev::ToEngine { ev, before } => {
                if let Some(b) = before {
                    self.engine_before = normalize_typed(&b);
                }
                match &ev {
                    NativeEvent::ContextState { .. } => self.context.observe(&ev),
                    NativeEvent::Focus { .. } => {
                        self.context.observe(&ev);
                        self.ghost.native(ev);
                    }
                    _ => self.ghost.native(ev),
                }
            }
            Ev::HelperCmd { cmd, claim } => self.helper_cmd(cmd, claim),
            Ev::FixStart { expected } => self.fix_start(&expected),
            Ev::Focus { away } => {
                self.away = away;
                if away {
                    self.note("user", "switches to another app".into());
                    self.focus_event(OTHER_APP, "Notes");
                } else {
                    self.note("user", "comes back".into());
                    self.last_delivered = None;
                    self.last_after = None;
                    self.focus_event(self.profile.bundle_id, self.profile.name);
                    self.push(Duration::ZERO, Ev::AxNotify);
                }
            }
            Ev::End => self.ended = true,
        }
    }

    fn focus_event(&mut self, bundle_id: &str, name: &str) {
        self.push(
            HOP,
            Ev::ToEngine {
                ev: NativeEvent::Focus {
                    app_bundle_id: bundle_id.into(),
                    app_name: name.into(),
                    secure: false,
                    editable: true,
                },
                before: None,
            },
        );
    }

    // ---- the user ----

    fn user_turn(&mut self, actor: &mut dyn Actor) {
        let before = self.before();
        let (chip, claim, fix) = match &self.screen {
            Some(s) => (Some(s.text.clone()), s.claim.clone(), s.claim.is_none()),
            None => (None, None, false),
        };
        let view = View {
            at_ms: self.since_start_us() / 1000,
            before: &before,
            chip: chip.as_deref(),
            claim: claim.as_deref(),
            fix_chip: fix,
        };
        match actor.next(&view) {
            None => {
                self.note("user", "done".into());
                self.push(ms(1500), Ev::End);
            }
            Some((action, wait)) => {
                let wait = self.user_action(action).unwrap_or(wait);
                self.push(wait, Ev::UserTurn);
            }
        }
    }

    /// Returns a different wait when the action takes time itself (switching apps).
    fn user_action(&mut self, action: Action) -> Option<Duration> {
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
            Action::Caret(delta) => {
                self.stats.keys += 1;
                self.note("user", format!("arrow {delta:+}"));
                self.push(
                    INPUT,
                    Ev::AppEdit {
                        edit: Edit::Caret(delta),
                        by: Origin::User,
                    },
                );
            }
            Action::SwitchApp(away) => {
                self.push(Duration::ZERO, Ev::Focus { away: true });
                self.push(away, Ev::Focus { away: false });
                return Some(away + ms(300));
            }
            Action::Press(key) => {
                self.stats.keys += 1;
                if self.capturing {
                    self.note("user", format!("{key:?} (swallowed)"));
                    let continuation = self.screen.as_ref().and_then(|s| s.claim.clone());
                    if key != Key::Escape && continuation.is_some() {
                        self.stats.accepts += 1;
                        let before = self.before();
                        self.accepts.push_back(Accept {
                            key,
                            claim: continuation,
                            before,
                        });
                    }
                    self.push(
                        HOP,
                        Ev::ToEngine {
                            ev: NativeEvent::Key { key },
                            before: None,
                        },
                    );
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
        None
    }

    // ---- the app ----

    fn app_edit(&mut self, edit: Edit, by: Origin) {
        let before = self.before();
        match &edit {
            Edit::Insert(s) => {
                let landed = if self.profile.smart_quotes {
                    smart_quote(&before, s)
                } else {
                    s.clone()
                };
                if by != Origin::User {
                    self.note("app", format!("BOB inserted {s:?}"));
                    let after = self.after().to_string();
                    self.check_insert(&before, &after, s, by);
                }
                self.doc.insert_str(self.caret, &landed);
                self.caret += landed.len();
            }
            Edit::Backspace => {
                if let Some((i, _)) = self.doc[..self.caret].char_indices().last() {
                    self.doc.replace_range(i..self.caret, "");
                    self.caret = i;
                }
            }
            Edit::Caret(delta) => {
                for _ in 0..delta.unsigned_abs() {
                    let next = if *delta < 0 {
                        self.doc[..self.caret].char_indices().last().map(|(i, _)| i)
                    } else {
                        self.doc[self.caret..]
                            .chars()
                            .next()
                            .map(|c| self.caret + c.len_utf8())
                    };
                    if let Some(i) = next {
                        self.caret = i;
                    }
                }
                // Selection changes are reported right away; the text is unchanged.
                let caret = self.doc[..self.caret].chars().count();
                self.push(
                    ms(1),
                    Ev::Report {
                        text: None,
                        caret: Some(caret),
                    },
                );
                return;
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

    /// What Accessibility shows now: the whole text before the reported caret (clamped, like `TextWindow`),
    /// the text after it, and the caret rect.
    fn snapshot(&self) -> (String, String, Rect) {
        let mut before: String = self.rep_text.chars().take(self.rep_caret).collect();
        let after: String = self
            .rep_text
            .chars()
            .skip(self.rep_caret)
            .take(400)
            .collect();
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
        (before, after, caret)
    }

    fn deliver_if_changed(&mut self, how: &str) {
        let (before, _, caret) = self.snapshot();
        if self.last_delivered.as_ref() != Some(&(before, caret)) {
            self.deliver(how);
        }
    }

    /// `HelperRuntime.forward`: text state, then a typo event, then the text after the caret if it changed.
    fn deliver(&mut self, how: &str) {
        let (before, after, caret) = self.snapshot();
        self.stats.texts_sent += 1;
        self.note(
            "helper",
            format!("{how}: text {:?}, caret x {}", tail(&before, 30), caret.x),
        );
        self.last_delivered = Some((before.clone(), caret));
        let n = before.chars().count();
        let window: String = before
            .chars()
            .skip(n.saturating_sub(WINDOW_CHARS))
            .collect();
        self.push(
            HOP,
            Ev::ToEngine {
                ev: NativeEvent::TextState {
                    app_bundle_id: self.profile.bundle_id.into(),
                    secure: false,
                    before_caret: window.clone(),
                    selection_len: 0,
                    caret: Some(caret),
                },
                before: Some(before),
            },
        );
        let normalized = normalize_typed(&window);
        if normalized != self.last_typo_check {
            if let Some((word, fix, trailing)) = find_typo(&normalized) {
                self.push(
                    HOP,
                    Ev::ToEngine {
                        ev: NativeEvent::Typo {
                            word,
                            fix,
                            trailing,
                        },
                        before: None,
                    },
                );
            }
            self.last_typo_check = normalized;
        }
        if self.last_after.as_deref() != Some(after.as_str()) {
            self.last_after = Some(after.clone());
            self.push(
                HOP,
                Ev::ToEngine {
                    ev: NativeEvent::ContextState {
                        app_bundle_id: self.profile.bundle_id.into(),
                        window_title: self.window_title.clone(),
                        after_caret: after,
                    },
                    before: None,
                },
            );
        }
    }

    fn helper_cmd(&mut self, cmd: CoreCommand, claim: Option<String>) {
        match cmd {
            CoreCommand::ShowChip { text, .. } => {
                self.stats.chips_painted += 1;
                if text.trim().is_empty() {
                    self.defect(Kind::EmptyChip, format!("{text:?}"));
                }
                self.note("chip", format!("shows {text:?}"));
                // A typo fix chip ("teh → the") promises a replacement, not a continuation.
                let claim = if text.contains(" → ") { None } else { claim };
                self.screen = Some(Screen { text, claim });
            }
            CoreCommand::HideChip => {
                self.stats.chips_hidden += 1;
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
                // `HelperRuntime`: re-read the field; replace only if the word is still there (in what
                // Accessibility shows, which can lag behind the field).
                let (before, _, _) = self.snapshot();
                if normalize_typed(&before).ends_with(&expected) {
                    self.stats.fixes += 1;
                    self.push(
                        Duration::ZERO,
                        Ev::FixStart {
                            expected: expected.clone(),
                        },
                    );
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

    /// The deletions of a typo fix start now: the field itself must still end with the misspelled word.
    fn fix_start(&mut self, expected: &str) {
        let before = self.before();
        if !before.ends_with(expected) {
            self.defect(
                Kind::FixCorruptedText,
                format!(
                    "deleting {} characters for {expected:?}, but the field ends with {:?}",
                    expected.chars().count(),
                    tail(&before, 20)
                ),
            );
        }
    }

    /// BOB's insertion `s` lands between `before` and `after`.
    fn check_insert(&mut self, before: &str, after: &str, s: &str, by: Origin) {
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
        let next = after.chars().next();
        let joint: String = before
            .chars()
            .last()
            .into_iter()
            .chain(s.chars())
            .chain(next)
            .collect();
        if joint.contains("  ") {
            self.defect(
                Kind::DoubleSpace,
                format!("{:?} + {s:?} + {:?}", tail(before, 20), next),
            );
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
        if s.ends_with(char::is_alphanumeric) && next.is_some_and(char::is_alphanumeric) {
            self.defect(
                Kind::GluedWord,
                format!(
                    "{s:?} runs into {:?}",
                    tail(&after.chars().take(12).collect::<String>(), 12)
                ),
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
        let after = normalize_typed(&format!("{before}{s}"));
        let calls = self
            .calls
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        // The model's path: its prompt (the text, or its last window, when asked) plus the polished answer.
        // Find where that prompt sits in the text now; if the path continues past the insertion with a
        // letter, the insertion cut a word short.
        let next = calls.iter().rev().find_map(|c| {
            if c.prompt.is_empty() {
                return None;
            }
            let polished = chip::polish(&c.answer, &c.prompt)?;
            let path = format!("{}{polished}", c.prompt);
            after
                .match_indices(c.prompt.as_str())
                .find_map(|(i, _)| path.strip_prefix(&after[i..]))
                .and_then(|rest| rest.chars().next())
        });
        if next.is_some_and(char::is_alphanumeric) {
            self.defect(
                Kind::Fragment,
                format!("{s:?} stops inside a word (the model continued it)"),
            );
        }
    }

    /// A continuation chip must show what its promise says comes next from the field now; a typo fix chip is
    /// not checked here. Short mismatches are normal (the app or the engine catching up); longer ones are
    /// what the user sees as a wrong suggestion.
    fn check_stale(&mut self) {
        let before = self.before();
        let valid = self.screen.as_ref().is_none_or(|s| match &s.claim {
            Some(claim) => claim.strip_prefix(before.as_str()) == Some(s.text.as_str()),
            None => true,
        });
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
                    "{chip:?} shown for {} ms although {:?} needs a different continuation",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fake_spell_checker_finds_finished_misspellings() {
        assert_eq!(
            find_typo("We will recieve "),
            Some(("recieve".into(), "receive".into(), " ".into()))
        );
        assert_eq!(
            find_typo("see teh."),
            Some(("teh".into(), "the".into(), ".".into()))
        );
        assert_eq!(find_typo("We will recieve"), None, "not finished yet");
        assert_eq!(find_typo("We will receive "), None);
    }

    #[test]
    fn smart_quotes_curl_apostrophes_after_letters() {
        assert_eq!(smart_quote("I", " don't"), " don’t");
        assert_eq!(smart_quote("I don", "'t"), "’t");
        assert_eq!(smart_quote("", "'quoted'"), "'quoted’");
    }
}
