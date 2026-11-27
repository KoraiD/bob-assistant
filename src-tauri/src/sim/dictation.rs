//! Voice Dictation in the simulator: the real dictation and Selection Actions engines (voice edit) against a
//! fake text field, the helper's recorder and whisper (a transcript arrives 0.2 to 0.9 s after the key is
//! released) and a random user.
//!
//! The user holds the key and speaks, taps it (too short: discarded, or latched with tap-to-toggle), stops a
//! latched recording, dictates in a password field, or speaks over a selection (voice edit). Utterances
//! sometimes carry a credential or a canary word. Settings vary per session: cleanup preset, tap-to-toggle,
//! replacements, a local or cloud brain.
//!
//! Checked: text is inserted only for a transcript with speech, never into a password field and never over a
//! selection (that is a voice edit); recording never starts in a password field; a voice edit replaces only
//! the text that was selected; credential-like text never reaches a cloud brain; dictated text never reaches
//! the log.
//!
//! Events go to the engines the way they handle them. The app's router (`lib.rs`) is not part of this: it
//! does not forward `DictationSelection` today (review finding VD-01), which these sessions cannot show.

use super::check::{Defect, Kind};
use super::model::{Model, Quirks, VOCAB};
use super::world::Line;
use super::Brains;
use crate::brain::models;
use crate::context::ContextHub;
use crate::dictation::engine::{self as dict, DictationHandle};
use crate::dictation::settings::{Cleanup, DictationSettings, Replacement, DEFAULT_MODEL};
use crate::ghost::engine::Sink;
use crate::protocol::{CoreCommand, NativeEvent};
use crate::selection::engine::{self as sel, SelectionHandle};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::Instant;

const APP: &str = "com.apple.mail";
const HOP: Duration = Duration::from_micros(200);

#[derive(Debug, Clone)]
pub struct Config {
    pub seed: u64,
    pub cloud: bool,
    pub cleanup: Cleanup,
    pub tap_to_toggle: bool,
    pub episodes: usize,
}

#[derive(Debug, Default)]
pub struct Report {
    pub defects: Vec<Defect>,
    pub log: Vec<Line>,
    pub inserted: usize,
    pub voice_edits: usize,
    pub recordings: usize,
    pub brain_calls: usize,
}

impl Report {
    pub fn timeline(&self) -> String {
        self.log
            .iter()
            .map(|l| {
                format!(
                    "{:>10.1} ms  {:<6} {}",
                    l.at_us as f64 / 1000.0,
                    l.who,
                    l.what
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

struct ChannelSink(mpsc::UnboundedSender<CoreCommand>);
impl Sink for ChannelSink {
    fn send(&self, cmd: CoreCommand) {
        let _ = self.0.send(cmd);
    }
}

#[derive(Debug, Clone)]
enum Step {
    /// The user puts the caret in a field (`secure`) with `selection` selected.
    Field {
        secure: bool,
        selection: Option<String>,
    },
    Key(bool),
    Wait,
}

/// What a transcript allows: typing it, or a voice edit of the selection it was spoken over.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Grant {
    Insert,
    VoiceEdit { selected: String },
}

struct World {
    start: Instant,
    doc: String,
    secure: bool,
    selection: Option<String>,
    /// What the user says in the current episode.
    utterance: String,
    /// A word that appears in some utterances and must never reach the log.
    canary: String,
    /// The selection when the current recording started (what a voice edit runs on).
    recording_over: Option<String>,
    grants: VecDeque<Grant>,
    dictation: DictationHandle,
    selection_engine: SelectionHandle,
    log: Vec<Line>,
    defects: Vec<Defect>,
    pending: VecDeque<(Instant, NativeEvent)>,
    report: Report,
}

impl World {
    fn note(&mut self, who: &'static str, what: String) {
        let at_us = Instant::now().duration_since(self.start).as_micros() as u64;
        self.log.push(Line { at_us, who, what });
    }

    fn defect(&mut self, kind: Kind, detail: String) {
        let at_ms = Instant::now().duration_since(self.start).as_millis() as u64;
        self.note("CHECK", format!("{kind:?}: {detail}"));
        self.defects.push(Defect {
            kind,
            at_ms,
            detail,
        });
    }

    /// Send an event to the engines that handle it (after the socket hop).
    fn send_later(&mut self, delay: Duration, ev: NativeEvent) {
        self.pending.push_back((Instant::now() + delay, ev));
        self.pending.make_contiguous().sort_by_key(|(t, _)| *t);
    }

    fn deliver(&mut self, ev: NativeEvent) {
        match &ev {
            NativeEvent::ReplaceResult { .. }
            | NativeEvent::MenuCancel
            | NativeEvent::MenuChoice { .. } => self.selection_engine.native(ev),
            _ => self.dictation.native(ev),
        }
    }

    fn step(&mut self, step: Step, rng: &mut StdRng) {
        match step {
            Step::Field { secure, selection } => {
                self.secure = secure;
                self.selection = selection;
                self.note(
                    "user",
                    format!(
                        "field{}{}",
                        if secure { " (password field)" } else { "" },
                        self.selection
                            .as_ref()
                            .map(|s| format!(", {} chars selected", s.chars().count()))
                            .unwrap_or_default()
                    ),
                );
                let before = self.doc.clone();
                self.send_later(
                    HOP,
                    NativeEvent::Focus {
                        app_bundle_id: APP.into(),
                        app_name: "Mail".into(),
                        secure,
                        editable: true,
                    },
                );
                self.send_later(
                    HOP,
                    NativeEvent::TextState {
                        app_bundle_id: APP.into(),
                        secure,
                        before_caret: if secure { String::new() } else { before },
                        selection_len: self
                            .selection
                            .as_ref()
                            .map_or(0, |s| s.chars().count() as u32),
                        caret: None,
                    },
                );
            }
            Step::Key(pressed) => {
                if pressed {
                    self.utterance = utterance(rng, &self.canary);
                }
                self.note(
                    "user",
                    format!("dictation key {}", if pressed { "down" } else { "up" }),
                );
                self.send_later(HOP, NativeEvent::DictationKey { pressed });
            }
            Step::Wait => {}
        }
    }

    fn on_command(&mut self, cmd: CoreCommand, rng: &mut StdRng) {
        match cmd {
            CoreCommand::StartRecording => {
                self.report.recordings += 1;
                self.note("helper", "recording".into());
                if self.secure {
                    self.defect(
                        Kind::RecordingInSecureField,
                        "recording started in a password field".into(),
                    );
                }
                // `HelperRuntime.startRecording`: a selection (never in a password field) makes it a voice edit.
                self.recording_over = self.selection.clone().filter(|_| !self.secure);
                if let Some(text) = self.recording_over.clone() {
                    self.send_later(
                        HOP,
                        NativeEvent::DictationSelection {
                            app_bundle_id: APP.into(),
                            text,
                        },
                    );
                }
            }
            CoreCommand::StopRecording { transcribe } => {
                self.note("helper", format!("stopped (transcribe {transcribe})"));
                if transcribe {
                    let speech = rng.random_bool(0.93);
                    let text = if speech {
                        self.utterance.clone()
                    } else {
                        String::new()
                    };
                    if speech {
                        self.grants.push_back(match &self.recording_over {
                            Some(selected) => Grant::VoiceEdit {
                                selected: selected.clone(),
                            },
                            None => Grant::Insert,
                        });
                    }
                    let delay = Duration::from_millis(rng.random_range(200..900));
                    self.send_later(
                        delay,
                        NativeEvent::Transcript {
                            text,
                            language: Some("en".into()),
                            speech,
                            audio_ms: 1500,
                            transcribe_ms: delay.as_millis() as u32,
                        },
                    );
                }
            }
            CoreCommand::InsertText { text } => {
                self.note(
                    "app",
                    format!("dictation typed {} chars", text.chars().count()),
                );
                let pos = self.grants.iter().position(|g| *g == Grant::Insert);
                match pos {
                    Some(i) if !self.secure => {
                        self.grants.remove(i);
                    }
                    _ => {
                        let why = if self.secure {
                            "into a password field".to_string()
                        } else if self
                            .grants
                            .iter()
                            .any(|g| matches!(g, Grant::VoiceEdit { .. }))
                        {
                            "over a selection (should be a voice edit)".to_string()
                        } else {
                            "without a transcript to type".to_string()
                        };
                        self.defect(Kind::UnrequestedDictation, why);
                    }
                }
                self.report.inserted += 1;
                self.doc.push_str(&text);
            }
            CoreCommand::ReplaceSelection { expected, text } => {
                let pos = self
                    .grants
                    .iter()
                    .position(|g| matches!(g, Grant::VoiceEdit { .. }));
                match pos.map(|i| self.grants.remove(i)) {
                    Some(Some(Grant::VoiceEdit { selected })) => {
                        if selected != expected {
                            self.defect(
                                Kind::ReplaceOfWrongText,
                                "the voice edit aimed at other text".into(),
                            );
                        }
                    }
                    _ => self.defect(
                        Kind::UnauthorizedReplace,
                        "replacement without a voice edit".into(),
                    ),
                }
                let ok = self.selection.as_deref() == Some(expected.as_str());
                if ok {
                    self.report.voice_edits += 1;
                    self.selection = None;
                }
                self.note(
                    "helper",
                    format!(
                        "voice edit {} ({} chars)",
                        if ok { "replaced" } else { "refused" },
                        text.chars().count()
                    ),
                );
                self.send_later(
                    HOP,
                    NativeEvent::ReplaceResult {
                        ok,
                        method: ok.then(|| "paste".into()),
                        error: (!ok).then(|| "the selection changed".into()),
                    },
                );
            }
            _ => {}
        }
    }
}

/// Two to eight words; now and then a credential or the session's canary word.
fn utterance(rng: &mut StdRng, canary: &str) -> String {
    let n = rng.random_range(2..=8);
    let mut words: Vec<String> = (0..n)
        .map(|_| VOCAB[rng.random_range(0..VOCAB.len())].to_string())
        .collect();
    if rng.random_bool(0.2) {
        words.push(format!("sk-{}", "Vx3".repeat(12)));
    }
    if rng.random_bool(0.3) {
        words.push(canary.to_string());
    }
    words.join(" ")
}

fn episode(rng: &mut StdRng, doc: &str) -> Vec<(Duration, Step)> {
    let ms = |n: u64| Duration::from_millis(n);
    let secure = rng.random_bool(0.1);
    let selection = (!secure && rng.random_bool(0.2)).then(|| {
        let words: Vec<&str> = doc.split_whitespace().collect();
        let a = rng.random_range(0..words.len().max(1));
        words[a..(a + 3).min(words.len())].join(" ")
    });
    let mut steps = vec![
        (ms(100), Step::Field { secure, selection }),
        (ms(300), Step::Key(true)),
    ];
    match rng.random_range(0..100) {
        // Hold and speak.
        0..=59 => steps.push((ms(rng.random_range(400..2500)), Step::Key(false))),
        // A tap: discarded, or latched with tap-to-toggle; then a press that ends a latched recording.
        60..=84 => steps.push((ms(rng.random_range(60..240)), Step::Key(false))),
        _ => {
            steps.push((ms(rng.random_range(60..240)), Step::Key(false)));
            steps.push((ms(rng.random_range(600..2000)), Step::Key(true)));
            steps.push((ms(rng.random_range(80..200)), Step::Key(false)));
        }
    }
    steps.push((ms(rng.random_range(1500..3500)), Step::Wait));
    steps
}

/// Play one dictation session.
pub async fn run(config: Config) -> Report {
    let mut rng = StdRng::seed_from_u64(config.seed);
    let brains = Brains::new(
        config.seed,
        Model::Random(Quirks::default()),
        config.cloud,
        None,
    )
    .await;
    let context = Arc::new(ContextHub::in_memory(Default::default(), ""));
    let (tx, mut rx) = mpsc::unbounded_channel();
    let sink: Arc<dyn Sink> = Arc::new(ChannelSink(tx));
    let models_dir = brains_models_dir(&config);
    let selection_engine = sel::spawn(brains.service.clone(), context.clone(), sink.clone(), None);
    let dictation = dict::spawn(
        brains.service.clone(),
        context,
        sink,
        None,
        models_dir.clone(),
        Some(selection_engine.clone()),
    );
    dictation
        .set_settings(DictationSettings {
            cleanup: config.cleanup,
            tap_to_toggle: config.tap_to_toggle,
            replacements: vec![Replacement {
                from: "brb".into(),
                to: "be right back".into(),
            }],
            ..Default::default()
        })
        .expect("in-memory settings");
    let doc: Vec<&str> = (0..60)
        .map(|_| VOCAB[rng.random_range(0..VOCAB.len())])
        .collect();
    let mut world = World {
        start: Instant::now(),
        doc: doc.join(" "),
        secure: false,
        selection: None,
        utterance: String::new(),
        canary: format!("Dq{}vk", config.seed),
        recording_over: None,
        grants: VecDeque::new(),
        dictation,
        selection_engine,
        log: vec![],
        defects: vec![],
        pending: VecDeque::new(),
        report: Report::default(),
    };
    let mut plan: VecDeque<(Instant, Step)> = VecDeque::new();
    let mut at = Instant::now();
    for _ in 0..config.episodes {
        for (d, s) in episode(&mut rng, &world.doc) {
            at += d;
            plan.push_back((at, s));
        }
    }
    let end = at + Duration::from_secs(5);
    loop {
        let next_step = plan.front().map(|(t, _)| *t);
        let next_event = world.pending.front().map(|(t, _)| *t);
        let wake = [next_step, next_event, Some(end)]
            .into_iter()
            .flatten()
            .min()
            .unwrap_or(end);
        tokio::select! {
            biased;
            Some(cmd) = rx.recv() => world.on_command(cmd, &mut rng),
            () = tokio::time::sleep_until(wake) => {
                let now = Instant::now();
                while world.pending.front().is_some_and(|(t, _)| *t <= now) {
                    if let Some((_, ev)) = world.pending.pop_front() {
                        world.deliver(ev);
                    }
                }
                while plan.front().is_some_and(|(t, _)| *t <= now) {
                    if let Some((_, s)) = plan.pop_front() {
                        world.step(s, &mut rng);
                    }
                }
                if now >= end {
                    break;
                }
            }
        }
    }
    let _ = std::fs::remove_dir_all(models_dir.parent().unwrap_or(&models_dir));
    for p in brains.secrets_sent_to_cloud() {
        let what = if p.contains("voice transcript") {
            "dictation cleanup"
        } else {
            "voice edit"
        };
        world.defect(
            Kind::SecretToCloud,
            format!("{what}: {} chars with a credential", p.chars().count()),
        );
    }
    let leaks = crate::logging::capture::find(&world.canary);
    if !leaks.is_empty() {
        world.defect(
            Kind::TextInLog,
            format!("{} log lines contain dictated text", leaks.len()),
        );
    }
    world.report.brain_calls = brains
        .chats
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .len();
    Report {
        defects: world.defects,
        log: world.log,
        ..world.report
    }
}

/// A models folder where the default speech model "is installed" (an empty file with its catalog name).
fn brains_models_dir(config: &Config) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!(
        "bob-sim-dictation-{}-{}",
        std::process::id(),
        config.seed
    ));
    let dir = root.join("models");
    let _ = std::fs::create_dir_all(&dir);
    if let Some(entry) = models::find_speech(DEFAULT_MODEL) {
        let file = models::installed_path(&dir, entry);
        let _ = std::fs::write(&file, b"");
        models::assume_verified(&file);
    }
    dir
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(seed: u64, cloud: bool) -> Config {
        let cleanup = [
            Cleanup::None,
            Cleanup::Clean,
            Cleanup::TranslateEnglish,
            Cleanup::MeetingNotes,
        ][(seed % 4) as usize];
        Config {
            seed,
            cloud,
            cleanup,
            tap_to_toggle: seed.is_multiple_of(3),
            episodes: 20,
        }
    }

    /// Random dictation sessions with a local brain: every insertion and voice edit is one the user asked
    /// for, never in a password field, and nothing dictated reaches the log.
    #[tokio::test(start_paused = true)]
    async fn random_sessions_type_only_what_was_said_where_it_belongs() {
        crate::logging::capture::start();
        let (mut inserted, mut edits, mut recordings) = (0, 0, 0);
        for seed in 0..16 {
            let r = run(config(seed, false)).await;
            assert!(
                r.defects.is_empty(),
                "seed {seed}: {:?}\n{}",
                r.defects,
                r.timeline()
            );
            inserted += r.inserted;
            edits += r.voice_edits;
            recordings += r.recordings;
        }
        eprintln!("dictation: 16 sessions, {recordings} recordings, {inserted} insertions, {edits} voice edits");
        assert!(
            inserted > 20 && edits > 0,
            "the sessions must exercise the engine"
        );
    }

    /// The same with a cloud brain: dictated credentials must not leave the Mac. Voice edits are safe (the
    /// instruction is part of the system prompt, which `chat_system_opts` redacts); cleanup is not.
    #[tokio::test(start_paused = true)]
    async fn dictated_credentials_never_reach_a_cloud_brain() {
        crate::logging::capture::start();
        let mut leaks: Vec<String> = vec![];
        for seed in 0..16 {
            let r = run(config(seed, true)).await;
            leaks.extend(
                r.defects
                    .iter()
                    .filter(|d| d.kind == Kind::SecretToCloud)
                    .map(|d| format!("seed {seed}: {}", d.detail)),
            );
        }
        assert!(
            leaks.is_empty(),
            "{} leaks:\n{}",
            leaks.len(),
            leaks.join("\n")
        );
    }
}
