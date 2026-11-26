//! Selection Actions in the simulator: the real engine against a fake text field with a selection, the
//! helper's menu panel and replacement (`SelectionIO.replace` only replaces when the selection still holds
//! the text the action ran on), and a random user.
//!
//! The user selects text (sometimes empty, in a password field, too long, or with a credential in it),
//! presses the hotkey or an action's own shortcut, picks an item, cancels, picks an invalid row, presses the
//! hotkey again while an action runs, or changes the selection before the result arrives; in a preview or an
//! answer (Explain, Reply) they replace, copy, insert or discard.
//!
//! Checked: BOB replaces or copies only what the user chose, in the episode they chose it, and only the text
//! the action ran on; password fields, empty and oversized selections never reach a brain; credential-like
//! text never reaches a cloud brain; selected text never reaches the log.

use super::check::{Defect, Kind};
use super::model::{Model, Quirks, VOCAB};
use super::world::Line;
use super::Brains;
use crate::context::ContextHub;
use crate::ghost::engine::Sink;
use crate::protocol::{CoreCommand, NativeEvent, SelectionSource};
use crate::selection::actions::{CustomAction, HotkeySetting, SelectionSettings};
use crate::selection::engine::{self as sel, ANSWER_ITEMS, PREVIEW_ITEMS};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
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
    pub preview: bool,
    pub episodes: usize,
}

#[derive(Debug, Default)]
pub struct Report {
    pub defects: Vec<Defect>,
    pub log: Vec<Line>,
    pub episodes: usize,
    pub replaced: usize,
    pub copied: usize,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Menu {
    Actions,
    Preview,
    Answer,
}

/// What the user's last choice allows BOB to do, in the episode it was made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Grant {
    Replace,
    InsertAfter,
    Copy,
}

struct World {
    start: Instant,
    rng: StdRng,
    config: Config,
    doc: String,
    /// Selected byte range.
    selection: (usize, usize),
    secure: bool,
    /// The selection is in another field (a password field, or a very long document) for this episode.
    other_field: Option<String>,
    menu: Option<(Menu, Vec<String>)>,
    episode: usize,
    /// The text the current episode's action runs on.
    selected: String,
    grants: Vec<(usize, Grant)>,
    /// Selections that must never reach a brain (password field, empty, oversized).
    forbidden: Vec<String>,
    canary: String,
    custom: Vec<CustomAction>,
    handle: sel::SelectionHandle,
    log: Vec<Line>,
    defects: Vec<Defect>,
    replaced: usize,
    copied: usize,
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

    fn selected_text(&self) -> &str {
        match &self.other_field {
            Some(t) => t,
            None => &self.doc[self.selection.0..self.selection.1],
        }
    }

    /// Select a random run of words (with the canary or a credential in it now and then).
    fn select_something(&mut self) {
        let words: Vec<(usize, usize)> = {
            let mut v = vec![];
            let mut start = None;
            for (i, c) in self.doc.char_indices() {
                match (c.is_whitespace(), start) {
                    (false, None) => start = Some(i),
                    (true, Some(s)) => {
                        v.push((s, i));
                        start = None;
                    }
                    _ => {}
                }
            }
            if let Some(s) = start {
                v.push((s, self.doc.len()));
            }
            v
        };
        if words.is_empty() {
            self.selection = (0, 0);
            return;
        }
        let a = self.rng.random_range(0..words.len());
        let b = (a + self.rng.random_range(0..6)).min(words.len() - 1);
        self.selection = (words[a].0, words[b].1);
    }

    /// The helper reads the selection when the hotkey is pressed (`SelectionIO.read`).
    fn hotkey(&mut self, action: Option<u32>) {
        self.episode += 1;
        let kind = self.rng.random_range(0..100);
        self.secure = kind < 5;
        // Selections that must never reach a brain carry a unique marker, so finding it in a request proves
        // which selection leaked.
        let marker = format!("Mk{}x{}", self.config.seed, self.episode);
        self.other_field = None;
        match kind {
            0..=4 => {
                self.other_field = Some(format!("{marker} hunter2"));
                self.forbidden.push(marker);
            }
            5..=9 => self.selection = (self.selection.0, self.selection.0),
            10..=14 => {
                self.other_field = Some(format!(
                    "{marker} {}",
                    "word ".repeat(sel::MAX_CHARS / 5 + 10)
                ));
                self.forbidden.push(marker);
            }
            _ => self.select_something(),
        }
        let text = if self.secure {
            String::new()
        } else {
            self.selected_text().to_string()
        };
        self.selected = text.clone();
        self.note(
            "user",
            format!(
                "hotkey{} on {} chars{}",
                action.map(|i| format!(" (action {i})")).unwrap_or_default(),
                text.chars().count(),
                if self.secure { " (password field)" } else { "" }
            ),
        );
        if let Some(i) = action {
            if !self.config.preview && (i as usize) < self.custom.len() {
                self.grants.push((self.episode, Grant::Replace));
            }
        }
        self.menu = None;
        self.handle.native(NativeEvent::Selection {
            app_bundle_id: APP.into(),
            text,
            secure: self.secure,
            source: Some(SelectionSource::Accessibility),
            anchor: None,
            action,
        });
    }

    /// The user acts on the visible menu or waits.
    fn user_turn(&mut self) -> Duration {
        let roll = self.rng.random_range(0..100);
        let pause = Duration::from_millis(self.rng.random_range(150..1500));
        match self.menu.clone() {
            Some((kind, items)) => {
                if roll < 12 {
                    self.note("user", "Esc".into());
                    self.menu = None;
                    self.episode += 1; // a cancelled episode grants nothing more
                    self.handle.native(NativeEvent::MenuCancel);
                    return pause;
                }
                // Now and then a row that does not exist (a click on the panel edge).
                let index = if roll < 15 {
                    items.len()
                } else {
                    self.rng.random_range(0..items.len().max(1))
                };
                let label = items.get(index).cloned().unwrap_or_default();
                self.note("user", format!("chooses {index} {label:?}"));
                let grant = match (kind, index) {
                    (Menu::Actions, i) if i < items.len() => {
                        let answer = label == "Explain" || label == "Reply";
                        (!answer && !self.config.preview).then_some(Grant::Replace)
                    }
                    (Menu::Preview, 0) => Some(Grant::Replace),
                    (Menu::Preview, 1) | (Menu::Answer, 0) => Some(Grant::Copy),
                    (Menu::Answer, 1) => Some(Grant::InsertAfter),
                    _ => None,
                };
                if let Some(g) = grant {
                    self.grants.push((self.episode, g));
                }
                self.menu = None;
                self.handle.native(NativeEvent::MenuChoice {
                    index: index as u32,
                });
                pause
            }
            None => {
                match roll {
                    // Edit the document: the selection the action runs on may no longer be there.
                    0..=9 => {
                        self.note("user", "types in the document".into());
                        self.other_field = None;
                        let at = self.selection.0;
                        self.doc.insert_str(at, "x ");
                        self.selection = (at, at);
                    }
                    10..=59 => {
                        let shortcut = roll < 16 && !self.custom.is_empty();
                        let action = shortcut
                            .then(|| self.rng.random_range(0..self.custom.len() as u32 + 1));
                        self.hotkey(action);
                    }
                    _ => {}
                }
                pause
            }
        }
    }

    fn on_command(&mut self, cmd: CoreCommand) {
        match cmd {
            CoreCommand::ShowMenu {
                title,
                items,
                preview,
                ..
            } => {
                let kind = if preview.is_none() {
                    Menu::Actions
                } else if items.first().map(String::as_str) == Some(PREVIEW_ITEMS[0]) {
                    Menu::Preview
                } else if items.first().map(String::as_str) == Some(ANSWER_ITEMS[0]) {
                    Menu::Answer
                } else {
                    Menu::Actions
                };
                self.note(
                    "panel",
                    format!("{kind:?} {title:?} ({} items)", items.len()),
                );
                self.menu = Some((kind, items));
            }
            CoreCommand::MenuStatus { text, .. } => {
                self.note("panel", format!("status {text:?}"));
                self.menu = None;
            }
            CoreCommand::HideMenu => self.menu = None,
            CoreCommand::CopyText { text } => {
                self.copied += 1;
                self.note("helper", format!("copies {} chars", text.chars().count()));
                if !self.take_grant(&[Grant::Copy]) {
                    self.defect(
                        Kind::UnrequestedCopy,
                        "copied without a Copy choice in this episode".into(),
                    );
                }
            }
            CoreCommand::ReplaceSelection { expected, text } => self.replace(expected, text),
            _ => {}
        }
    }

    fn take_grant(&mut self, kinds: &[Grant]) -> bool {
        let episode = self.episode;
        match self
            .grants
            .iter()
            .position(|(e, g)| *e == episode && kinds.contains(g))
        {
            Some(i) => {
                self.grants.remove(i);
                true
            }
            None => false,
        }
    }

    fn replace(&mut self, expected: String, text: String) {
        let insert_after = self
            .grants
            .iter()
            .any(|(e, g)| *e == self.episode && *g == Grant::InsertAfter);
        if !self.take_grant(&[Grant::Replace, Grant::InsertAfter]) {
            self.defect(
                Kind::UnauthorizedReplace,
                format!(
                    "replacement of {} chars without a choice in this episode",
                    expected.chars().count()
                ),
            );
        }
        if expected != self.selected {
            self.defect(
                Kind::ReplaceOfWrongText,
                format!(
                    "replacing {:?}, but the action ran on {:?}",
                    tail(&expected),
                    tail(&self.selected)
                ),
            );
        }
        if insert_after && !text.starts_with(expected.trim_end()) {
            self.defect(
                Kind::ReplaceOfWrongText,
                "Insert after the selection dropped the selection".into(),
            );
        }
        // `SelectionIO.replace`: only when the selection still holds the expected text.
        let ok = self.other_field.is_none() && self.selected_text() == expected;
        if ok {
            let (a, b) = self.selection;
            self.doc.replace_range(a..b, &text);
            self.selection = (a + text.len(), a + text.len());
            self.replaced += 1;
        }
        self.note(
            "helper",
            format!(
                "replace {} ({} chars)",
                if ok { "done" } else { "refused" },
                text.chars().count()
            ),
        );
        let error = (!ok).then(|| "the selection changed".to_string());
        self.handle.native(NativeEvent::ReplaceResult {
            ok,
            method: ok.then(|| "paste".to_string()),
            error,
        });
    }
}

fn tail(s: &str) -> String {
    super::world::tail(s, 30)
}

/// Play one Selection Actions session.
pub async fn run(config: Config) -> Report {
    let mut rng = StdRng::seed_from_u64(config.seed);
    let canary = format!("Zq{}vk", config.seed);
    let brains = Brains::new(
        config.seed,
        Model::Random(Quirks::default()),
        config.cloud,
        None,
    )
    .await;
    let context = Arc::new(ContextHub::in_memory(Default::default(), ""));
    let (tx, mut rx) = mpsc::unbounded_channel();
    let handle = sel::spawn(
        brains.service.clone(),
        context,
        Arc::new(ChannelSink(tx)),
        None,
    );
    let custom = vec![CustomAction {
        label: "Make it rhyme".into(),
        instruction: "Rewrite it as a rhyme.".into(),
        hotkey: Some(HotkeySetting {
            key_code: 18,
            modifiers: vec!["control".into(), "option".into()],
        }),
    }];
    handle
        .set_settings(SelectionSettings {
            preview: config.preview,
            custom: custom.clone(),
            ..Default::default()
        })
        .expect("in-memory settings");
    // A document of plain words, the canary, and a credential the user might select by accident.
    let mut doc: Vec<String> = (0..120)
        .map(|_| VOCAB[rng.random_range(0..VOCAB.len())].to_string())
        .collect();
    doc.insert(rng.random_range(0..doc.len()), canary.clone());
    doc.insert(
        rng.random_range(0..doc.len()),
        format!("sk-{}", "Ab9".repeat(12)),
    );
    let mut world = World {
        start: Instant::now(),
        rng,
        config: config.clone(),
        doc: doc.join(" "),
        selection: (0, 0),
        secure: false,
        other_field: None,
        menu: None,
        episode: 0,
        selected: String::new(),
        grants: vec![],
        forbidden: vec![],
        canary: canary.clone(),
        custom,
        handle,
        log: vec![],
        defects: vec![],
        replaced: 0,
        copied: 0,
    };
    let mut next_turn = Instant::now() + Duration::from_millis(50);
    let mut turns = 0;
    while world.episode < config.episodes || world.menu.is_some() {
        turns += 1;
        if turns > config.episodes * 40 {
            break;
        }
        tokio::select! {
            biased;
            Some(cmd) = rx.recv() => {
                tokio::time::sleep(HOP).await;
                world.on_command(cmd);
            }
            () = tokio::time::sleep_until(next_turn) => {
                let wait = world.user_turn();
                next_turn = Instant::now() + wait;
            }
        }
    }
    // Let anything in flight finish (late results must not act on anything).
    let settle = Instant::now() + Duration::from_secs(4);
    loop {
        tokio::select! {
            biased;
            Some(cmd) = rx.recv() => world.on_command(cmd),
            () = tokio::time::sleep_until(settle) => break,
        }
    }
    let chats = brains
        .chats
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    for c in &chats {
        let user = c.prompt.rsplit("\n---\n").next().unwrap_or("");
        if world
            .forbidden
            .iter()
            .any(|f| c.prompt.contains(f.as_str()))
        {
            world.defect(
                Kind::ForbiddenTextSent,
                format!(
                    "{} chars from a password field or an oversized selection",
                    user.chars().count()
                ),
            );
        }
    }
    for p in brains.secrets_sent_to_cloud() {
        world.defect(
            Kind::SecretToCloud,
            format!("{} chars with a credential", p.chars().count()),
        );
    }
    let leaks = crate::logging::capture::find(&world.canary);
    if !leaks.is_empty() {
        world.defect(
            Kind::TextInLog,
            format!(
                "{} log lines contain selected text: {:?}",
                leaks.len(),
                &leaks[..1]
            ),
        );
    }
    Report {
        episodes: world.episode,
        replaced: world.replaced,
        copied: world.copied,
        brain_calls: chats.len(),
        defects: world.defects,
        log: world.log,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn session(seed: u64, cloud: bool, preview: bool) -> Report {
        crate::logging::capture::start();
        run(Config {
            seed,
            cloud,
            preview,
            episodes: 25,
        })
        .await
    }

    /// Random Selection Actions sessions, local and cloud, with and without preview.
    #[tokio::test(start_paused = true)]
    async fn random_sessions_replace_only_what_the_user_chose() {
        let (mut replaced, mut copied, mut calls, mut episodes) = (0, 0, 0, 0);
        for seed in 0..24 {
            let r = session(seed, seed % 2 == 1, seed % 4 >= 2).await;
            assert!(
                r.defects.is_empty(),
                "seed {seed}: {:?}\n{}",
                r.defects,
                r.timeline()
            );
            replaced += r.replaced;
            episodes += r.episodes;
            copied += r.copied;
            calls += r.brain_calls;
        }
        eprintln!(
            "selection: 24 sessions, {episodes} episodes, {calls} brain calls, {replaced} replacements, {copied} copies"
        );
        assert!(
            replaced > 10 && copied > 0 && calls > 30,
            "the sessions must exercise the engine"
        );
    }
}
