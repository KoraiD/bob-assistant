//! The Ghost Text engine (GT-1 to GT-14): native text events in, chip and insertion commands out.
//!
//! One task owns all state and processes inputs in order, so there are no locks around the state
//! machine. Model requests run in their own tasks; each carries a generation number, and a result whose
//! generation is stale is ignored. A new keystroke aborts the in-flight request task, which drops the
//! HTTP stream and makes the server stop generating (GT-2, BR-5).
//!
//! Privacy: the engine never logs text. It logs only timings and lengths.

use super::chip::{self, TypeThrough};
use super::memory::Memory;
use super::settings::GhostSettings;
use super::{prompt, redact};
use crate::brain::registry::Role;
use crate::brain::service::BrainService;
use crate::brain::CompletionRequest;
use crate::context::{ContextHub, GHOST_MEMORY_PHRASES};
use crate::protocol::{CoreCommand, Key, NativeEvent, Rect};
use futures_util::StreamExt;
use serde::Serialize;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
// The engine's time windows use tokio's clock: identical to `std::time::Instant` in the app, and virtual in
// the simulator (`crate::sim`), which pauses time so a session replays exactly.
use tokio::time::Instant;

/// Quiet gate after the last keystroke before asking the model (perf steering: 60 to 80 ms).
pub const QUIET: Duration = Duration::from_millis(70);
/// Tunables, swept by the benchmark (`ghost-bench`). `Default` is what the app ships.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct EngineConfig {
    pub quiet: Duration,
    pub max_tokens: u32,
    pub temperature: f32,
}

impl Default for EngineConfig {
    fn default() -> Self {
        let r = CompletionRequest::ghost("");
        Self {
            quiet: QUIET,
            max_tokens: r.max_tokens,
            temperature: r.temperature,
        }
    }
}

/// How many recent latencies to keep for the UI and the benchmark.
const STATS_WINDOW: usize = 100;

/// Where engine commands go (the native bridge in the app, a recorder in tests).
pub trait Sink: Send + Sync + 'static {
    fn send(&self, cmd: CoreCommand);
}

impl Sink for crate::native_ipc::Bridge {
    fn send(&self, cmd: CoreCommand) {
        // The helper may be restarting; a lost chip update is harmless.
        let _ = crate::native_ipc::Bridge::send(self, &cmd);
    }
}

/// Inputs processed by the engine task, in order.
#[derive(Debug)]
pub enum Input {
    Native(NativeEvent),
    /// A polished chip from request `generation`.
    Chip {
        generation: u64,
        before: String,
        text: String,
        fix: Option<Fix>,
    },
    /// Request `generation` finished (or failed) without a newer chip.
    Done {
        generation: u64,
        error: Option<String>,
    },
    /// Enable or disable Ghost Text (the UI toggle / role "Off").
    SetEnabled(bool),
    /// "Pause BOB" (menu bar): like disabled, but temporary and never saved.
    SetPaused(bool),
    /// Quiet apps changed: re-check the current app.
    SettingsChanged,
}

/// Recent first-chip latencies (keystroke -> first chip on screen), for the UI and the benchmark.
#[derive(Debug, Clone, Default, Serialize)]
pub struct GhostStats {
    pub requests: u64,
    pub chips_shown: u64,
    pub accepted_words: u64,
    pub accepted_all: u64,
    pub dismissed: u64,
    pub errors: u64,
    pub last_error: Option<String>,
    /// Milliseconds from the last keystroke to the first painted chip, most recent last.
    pub first_chip_ms: VecDeque<u64>,
    pub typing_through: u64,
}

impl GhostStats {
    pub fn percentile(&self, p: f64) -> Option<u64> {
        if self.first_chip_ms.is_empty() {
            return None;
        }
        let mut v: Vec<u64> = self.first_chip_ms.iter().copied().collect();
        v.sort_unstable();
        let idx = ((p / 100.0) * (v.len() - 1) as f64).round() as usize;
        v.get(idx).copied()
    }
}

#[derive(Debug, Clone)]
struct Context {
    app: String,
    before: String,
    caret: Rect,
}

#[derive(Debug, Clone)]
struct Chip {
    text: String,
    /// The text before the caret that this chip continues.
    before: String,
    /// A typo fix instead of a continuation: Tab replaces the word.
    fix: Option<Fix>,
}

/// A spelling fix for the word just before the caret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fix {
    /// The misspelled word, lowercased (for "Esc: never offer this again").
    word: String,
    /// What must still be before the caret: the word and what follows it (a space, punctuation).
    expected: String,
    /// What replaces it.
    text: String,
}

/// How long a caret that moved without new text is trusted to be an app lagging behind (Chrome).
pub const SIDEWAYS_WINDOW: Duration = Duration::from_millis(300);

/// An app seen recently (from focus events), for the quiet-apps picker in the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RecentApp {
    pub bundle_id: String,
    pub name: String,
}

const RECENT_APPS: usize = 12;

/// State shared between the engine task and the rest of the app.
#[derive(Default)]
struct Shared {
    settings: Mutex<GhostSettings>,
    memory: Mutex<Memory>,
    recent: Mutex<Vec<RecentApp>>,
    /// Config directory for `ghost-settings.json` and `ghost-memory.json` (None: in memory only).
    storage: Option<PathBuf>,
}

impl Shared {
    fn save_settings(&self) {
        if let Some(dir) = &self.storage {
            if let Err(e) = self
                .settings
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .save(&dir.join(SETTINGS_FILE))
            {
                log::warn!("ghost: cannot save settings: {e}");
            }
        }
    }
    fn save_memory(&self) {
        if let Some(dir) = &self.storage {
            if let Err(e) = self
                .memory
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .save(&dir.join(MEMORY_FILE))
            {
                log::warn!("ghost: cannot save memory: {e}");
            }
        }
    }
}

pub const SETTINGS_FILE: &str = "ghost-settings.json";
pub const MEMORY_FILE: &str = "ghost-memory.json";

/// Handle used by the rest of the app.
#[derive(Clone)]
pub struct GhostHandle {
    tx: mpsc::UnboundedSender<Input>,
    stats: Arc<Mutex<GhostStats>>,
    shared: Arc<Shared>,
}

impl GhostHandle {
    /// Feed a native event (callable from any thread).
    pub fn native(&self, ev: NativeEvent) {
        let _ = self.tx.send(Input::Native(ev));
    }

    /// Turn Ghost Text on or off (persisted).
    pub fn set_enabled(&self, on: bool) {
        self.shared
            .settings
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .enabled = on;
        self.shared.save_settings();
        let _ = self.tx.send(Input::SetEnabled(on));
    }

    /// Pause or resume Ghost Text without touching the saved on/off setting.
    pub fn set_paused(&self, paused: bool) {
        let _ = self.tx.send(Input::SetPaused(paused));
    }

    /// Make BOB quiet (or not) in an app (persisted). Takes effect at the next text event.
    pub fn set_quiet(&self, bundle_id: &str, name: &str, quiet: bool) {
        self.shared
            .settings
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .set_quiet(bundle_id, name, quiet);
        self.shared.save_settings();
        let _ = self.tx.send(Input::SettingsChanged);
    }

    /// Turn typo fixes or mid-sentence suggestions on or off (persisted).
    pub fn set_option(&self, typo_fixes: Option<bool>, mid_sentence: Option<bool>) {
        {
            let mut s = self
                .shared
                .settings
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(v) = typo_fixes {
                s.typo_fixes = v;
            }
            if let Some(v) = mid_sentence {
                s.mid_sentence = v;
            }
        }
        self.shared.save_settings();
    }

    /// Nudge the chip in one app (persisted); (0, 0) removes the nudge. Applies to the next chip.
    pub fn set_chip_offset(&self, bundle_id: &str, name: &str, dx: i32, dy: i32) {
        self.shared
            .settings
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .set_chip_offset(bundle_id, name, dx, dy);
        self.shared.save_settings();
    }

    pub fn settings(&self) -> GhostSettings {
        self.shared
            .settings
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    pub fn recent_apps(&self) -> Vec<RecentApp> {
        self.shared
            .recent
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    pub fn memory(&self) -> Memory {
        self.shared
            .memory
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Record a phrase as if it had been accepted (benchmark seeding; persisted when storage is set).
    pub fn remember(&self, text: &str) {
        if self
            .shared
            .memory
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .record(text)
        {
            self.shared.save_memory();
        }
    }

    /// Forget one phrase (persisted).
    pub fn forget_phrase(&self, text: &str) -> bool {
        let removed = self
            .shared
            .memory
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .forget(text);
        if removed {
            self.shared.save_memory();
        }
        removed
    }

    /// Edit one phrase (persisted). False when it is missing or the new text would not be kept.
    pub fn edit_phrase(&self, old: &str, new: &str) -> bool {
        let changed = self
            .shared
            .memory
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .edit(old, new);
        if changed {
            self.shared.save_memory();
        }
        changed
    }

    /// Forget every accepted phrase (persisted).
    pub fn clear_memory(&self) {
        self.shared
            .memory
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
        self.shared.save_memory();
    }

    pub fn stats(&self) -> GhostStats {
        self.stats
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

/// One Ghost Text suggestion for `before` without the chip (AI agents over MCP, tests): the same brain,
/// redaction, context layers, polishing and language filter as typing. `None` when nothing worth showing.
pub async fn suggest_once(
    brains: &BrainService,
    context: &ContextHub,
    memory: &[String],
    before: &str,
) -> Result<Option<String>, String> {
    let before = normalize_typed(before);
    let brain = brains
        .resolve_role(Role::Ghost)
        .await
        .map_err(|e| format!("No brain is ready for Ghost Text ({e})."))?;
    if !brain.config.capabilities.ghost {
        return Err("The Ghost Text brain cannot continue text.".into());
    }
    let typed = redact::scrub(prompt::build(&before));
    if typed.trim().is_empty() {
        return Ok(None);
    }
    let prompt_text = if brain.config.shares_context() {
        format!("{}{typed}", context.ghost_preamble(memory))
    } else {
        typed
    };
    if brain.config.is_cloud() {
        crate::audit::sent(
            "ghost",
            &brain.config,
            prompt_text.chars().count(),
            brain.config.shares_context(),
        );
    }
    let mut stream = brain
        .complete_stream(&CompletionRequest::ghost(prompt_text))
        .await
        .map_err(|e| e.to_string())?;
    let languages = context.settings().languages;
    let mut raw = String::new();
    let mut best = None;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while let Ok(Some(item)) = tokio::time::timeout_at(deadline, stream.next()).await {
        raw.push_str(&item.map_err(|e| e.to_string())?);
        let Some(polished) = chip::polish(&raw, &before) else {
            continue;
        };
        let filtered = crate::context::languages::filter_chip(&polished, &before, &languages);
        let cut = filtered.as_deref() != Some(polished.as_str());
        best = filtered.clone();
        if filtered.is_none() || cut || chip::is_final(&polished) {
            break;
        }
    }
    Ok(best.map(|c| {
        let s = context.settings();
        crate::context::typography::finish(&c, s.spelling, s.quotes, before.chars().last())
    }))
}

/// Start the engine task with the shipped settings, in memory only (tests).
pub fn spawn(brains: Arc<BrainService>, sink: Arc<dyn Sink>) -> GhostHandle {
    spawn_with(brains, sink, EngineConfig::default())
}

/// Start the engine task with explicit tunables, in memory only (benchmark).
pub fn spawn_with(
    brains: Arc<BrainService>,
    sink: Arc<dyn Sink>,
    config: EngineConfig,
) -> GhostHandle {
    spawn_full(brains, sink, config, None)
}

/// Start the engine task; settings and memory are loaded from and saved to `storage` when given. No
/// personal context.
pub fn spawn_full(
    brains: Arc<BrainService>,
    sink: Arc<dyn Sink>,
    config: EngineConfig,
    storage: Option<PathBuf>,
) -> GhostHandle {
    let context = Arc::new(ContextHub::in_memory(Default::default(), ""));
    spawn_ctx(brains, sink, config, storage, context)
}

/// Start the engine task with the app's personal context (the app and the context benchmark).
pub fn spawn_ctx(
    brains: Arc<BrainService>,
    sink: Arc<dyn Sink>,
    config: EngineConfig,
    storage: Option<PathBuf>,
    context: Arc<ContextHub>,
) -> GhostHandle {
    let (tx, rx) = mpsc::unbounded_channel();
    let stats = Arc::new(Mutex::new(GhostStats::default()));
    let (settings, memory) = match &storage {
        Some(dir) => (
            GhostSettings::load(&dir.join(SETTINGS_FILE)),
            Memory::load(&dir.join(MEMORY_FILE)),
        ),
        None => (GhostSettings::default(), Memory::default()),
    };
    let enabled = settings.enabled;
    let shared = Arc::new(Shared {
        settings: Mutex::new(settings),
        memory: Mutex::new(memory),
        recent: Mutex::new(vec![]),
        storage,
    });
    let engine = Engine {
        config,
        brains,
        sink,
        tx: tx.clone(),
        stats: stats.clone(),
        shared: shared.clone(),
        context,
        enabled,
        paused: false,
        ctx: None,
        chip: None,
        generation: 0,
        pending: None,
        dismissed_for: None,
        capturing: false,
        first_chip_pending: None,
        expect: None,
        sideways_since: None,
        dismissed_typos: Default::default(),
    };
    tokio::spawn(engine.run(rx));
    GhostHandle { tx, stats, shared }
}

struct Engine {
    config: EngineConfig,
    brains: Arc<BrainService>,
    sink: Arc<dyn Sink>,
    tx: mpsc::UnboundedSender<Input>,
    stats: Arc<Mutex<GhostStats>>,
    shared: Arc<Shared>,
    /// Personal context (profile, memory, window, screen); used only when a Ghost layer is on.
    context: Arc<ContextHub>,
    enabled: bool,
    /// Temporarily off ("Pause BOB"); not persisted.
    paused: bool,
    ctx: Option<Context>,
    chip: Option<Chip>,
    generation: u64,
    pending: Option<JoinHandle<()>>,
    /// Esc was pressed for exactly this text; do not suggest again until it changes (GT-12).
    dismissed_for: Option<String>,
    capturing: bool,
    /// Keystroke time for the request whose first chip is not painted yet.
    first_chip_pending: Option<(u64, Instant)>,
    /// After BOB inserted accepted text: the text expected before the caret, and when. Text events that
    /// still show an older prefix of it are stale (the app updated Accessibility late) and are ignored.
    expect: Option<(String, Instant)>,
    /// Since when the caret has moved while the text stayed the same.
    sideways_since: Option<Instant>,
    /// Words whose fix the user dismissed with Esc (this session only).
    dismissed_typos: std::collections::HashSet<String>,
}

/// How long a stale text event after an insertion is ignored before BOB believes the app.
pub const STALE_WINDOW: Duration = Duration::from_millis(500);

/// Text as the engine compares it: apps (Chrome, contenteditable) report typed spaces as no-break spaces.
pub fn normalize_typed(text: &str) -> String {
    text.replace('\u{a0}', " ")
}

impl Engine {
    async fn run(mut self, mut rx: mpsc::UnboundedReceiver<Input>) {
        while let Some(input) = rx.recv().await {
            self.handle(input);
        }
        self.cancel();
    }

    fn is_quiet(&self, app: &str) -> bool {
        self.shared
            .settings
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_quiet(app)
    }

    fn remember_app(&self, bundle_id: String, name: String) {
        if bundle_id.is_empty() {
            return;
        }
        let mut recent = self
            .shared
            .recent
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        recent.retain(|a| a.bundle_id != bundle_id);
        recent.insert(0, RecentApp { bundle_id, name });
        recent.truncate(RECENT_APPS);
    }

    /// Store accepted text locally (GT-17). Single words are only kept when they are long enough to be
    /// worth suggesting again (names, terms); whole accepted chips are always offered to the memory.
    fn remember_phrase(&self, text: &str) {
        let words = text.split_whitespace().count();
        let long_word = text.trim().chars().count() >= 6;
        if words >= 2 || long_word {
            let changed = self
                .shared
                .memory
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .record(text);
            if changed {
                self.shared.save_memory();
            }
        }
    }

    fn stat(&self, f: impl FnOnce(&mut GhostStats)) {
        f(&mut self
            .stats
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner));
    }

    fn handle(&mut self, input: Input) {
        match input {
            Input::SetPaused(paused) => {
                self.paused = paused;
                if paused {
                    self.cancel();
                    self.hide();
                }
            }
            Input::SetEnabled(on) => {
                self.enabled = on;
                if !on {
                    self.hide();
                    self.cancel();
                }
            }
            Input::Native(NativeEvent::Focus {
                app_bundle_id,
                app_name,
                ..
            }) => {
                // App or field changed: the anchor is gone (GT-9).
                self.hide();
                self.cancel();
                self.ctx = None;
                self.remember_app(app_bundle_id, app_name);
            }
            Input::SettingsChanged => {
                let quiet = self.ctx.as_ref().is_some_and(|c| self.is_quiet(&c.app));
                if quiet {
                    self.hide();
                    self.cancel();
                    self.ctx = None;
                }
            }
            Input::Native(NativeEvent::TextState {
                app_bundle_id,
                secure,
                before_caret,
                selection_len,
                caret,
            }) => {
                self.on_text(app_bundle_id, secure, before_caret, selection_len, caret);
            }
            Input::Native(NativeEvent::Key { key, shown }) => self.on_key(key, shown),
            Input::Native(NativeEvent::Typo {
                word,
                fix,
                trailing,
            }) => self.on_typo(word, fix, trailing),
            Input::Native(NativeEvent::Hello { .. }) => {
                // A (re)started helper shows no chip and does not capture keys: forget ours.
                self.cancel();
                self.chip = None;
                self.capturing = false;
                self.ctx = None;
            }
            Input::Native(_) => {}
            Input::Chip {
                generation,
                before,
                text,
                fix,
            } => {
                let current = self.ctx.as_ref().is_some_and(|c| c.before == before);
                let dismissed = self.dismissed_for.as_deref() == Some(before.as_str());
                if generation != self.generation
                    || !current
                    || dismissed
                    || !self.enabled
                    || self.paused
                {
                    log::trace!(
                        "ghost: stale chip dropped (generation {generation}, current {current})"
                    );
                    return;
                }
                let Some(caret) = self.ctx.as_ref().map(|c| c.caret) else {
                    return;
                };
                if text.is_empty() {
                    return self.hide(); // a chip taken back (wrong language)
                }
                if let Some((g, t0)) = self.first_chip_pending.take() {
                    if g == generation {
                        let ms = t0.elapsed().as_millis() as u64;
                        log::debug!(
                            "ghost: first chip in {ms} ms ({} chars)",
                            text.chars().count()
                        );
                        self.stat(|s| {
                            s.chips_shown += 1;
                            s.first_chip_ms.push_back(ms);
                            while s.first_chip_ms.len() > STATS_WINDOW {
                                s.first_chip_ms.pop_front();
                            }
                        });
                        crate::usage::record(|d| d.ghost_shown += 1);
                    }
                }
                // Writing style: quotes in the chip match what the user chose (prefix-stable while streaming).
                let style = self.context.settings();
                let text = crate::context::typography::finish(
                    &text,
                    style.spelling,
                    style.quotes,
                    before.chars().last(),
                );
                self.show(
                    Chip {
                        text: text.clone(),
                        before,
                        fix,
                    },
                    caret,
                    text,
                );
            }
            Input::Done { generation, error } => {
                log::trace!("ghost: request {generation} done");
                if let Some(e) = error {
                    if generation == self.generation {
                        log::warn!("ghost: request failed: {e}");
                        self.stat(|s| {
                            s.errors += 1;
                            s.last_error = Some(e);
                        });
                    }
                }
            }
        }
    }

    fn on_text(
        &mut self,
        app: String,
        secure: bool,
        before: String,
        selection_len: u32,
        caret: Option<Rect>,
    ) {
        let before = normalize_typed(&before);
        // Stale after an insertion: the app still reports text from before it. Wait for the real update
        // (the helper re-reads shortly after every change) instead of repainting the old chip.
        if let Some((expected, at)) = &self.expect {
            if at.elapsed() < STALE_WINDOW
                && before.len() < expected.len()
                && expected.starts_with(&before)
                && !secure
            {
                return;
            }
            self.expect = None;
        }
        // Quiet zones (GT-3, GT-4): secure fields, no caret position, an active selection.
        // Plus apps on the quiet list (terminals, password managers, BOB, user choices).
        let quiet = self.is_quiet(&app);
        let Some(caret) = caret
            .filter(|_| !secure && selection_len == 0 && self.enabled && !self.paused && !quiet)
        else {
            log::trace!(
                "ghost: quiet (secure={secure} selection={selection_len} enabled={} paused={} quiet_app={quiet} caret={})",
                self.enabled,
                self.paused,
                caret.is_some()
            );
            self.hide();
            self.cancel();
            self.ctx = None;
            return;
        };
        // The app's own completion list (Slack mentions, channels, emoji, slash commands) owns Tab.
        if chip::in_app_autocomplete(&before) {
            log::trace!("ghost: the app's own completion owns Tab");
            self.hide();
            self.cancel();
            self.ctx = None;
            return;
        }
        let now = Instant::now();
        let prev = self.ctx.replace(Context {
            app: app.clone(),
            before: before.clone(),
            caret,
        });

        // A caret that moved sideways on the same line while the text stayed the same is an app reporting
        // the caret before the text (typing in Chrome): keep the chip where it is until the text arrives.
        // Only for a short while: if the text never follows (a missed update, a window switch), the chip is
        // hidden instead of staying frozen at the old caret.
        if let (Some(_), Some(p)) = (&self.chip, prev.as_ref()) {
            if p.app == app
                && p.before == before
                && (p.caret.y - caret.y).abs() < 1.0
                && p.caret.x != caret.x
            {
                let since = *self.sideways_since.get_or_insert(now);
                if since.elapsed() < SIDEWAYS_WINDOW {
                    if let Some(c) = self.ctx.as_mut() {
                        c.caret = p.caret;
                    }
                    return;
                }
                self.sideways_since = None;
                self.hide();
                self.cancel();
                self.dismissed_for = Some(before);
                return;
            }
        }
        self.sideways_since = None;
        // Typing-through (GT-7, GT-8): the new text extends the old one.
        if let (Some(chip), Some(prev)) = (self.chip.clone(), prev.as_ref()) {
            if prev.app == app && chip.fix.is_none() && before.starts_with(&chip.before) {
                let typed = &before[chip.before.len()..];
                match chip::type_through(&chip.text, typed) {
                    TypeThrough::Advance(rest) => {
                        if !typed.is_empty() {
                            self.stat(|s| s.typing_through += 1);
                        }
                        // Keep the original anchor text so later keystrokes compare against it;
                        // display only what is left.
                        self.show(chip, caret, rest);
                        return;
                    }
                    TypeThrough::Exhausted | TypeThrough::Diverged => {}
                }
            }
        }
        if prev
            .as_ref()
            .is_some_and(|p| p.before == before && p.app == app)
            && self.chip.is_some()
        {
            // Only the caret rect changed (scroll, resize): move the chip.
            if let Some(chip) = self.chip.clone() {
                if let Some(typed) = before.strip_prefix(chip.before.as_str()) {
                    if let TypeThrough::Advance(visible) = chip::type_through(&chip.text, typed) {
                        self.sink.send(CoreCommand::ShowChip {
                            text: visible,
                            anchor: self.nudged(&app, caret),
                        });
                    }
                }
            }
            return;
        }
        self.hide();
        if self.dismissed_for.as_deref() == Some(before.as_str()) {
            return;
        }
        self.dismissed_for = None;
        self.schedule(before, now);
    }

    fn on_key(&mut self, key: Key, shown: Option<String>) {
        let (Some(chip), Some(ctx)) = (self.chip.clone(), self.ctx.clone()) else {
            return;
        };
        // What is still visible after any typing-through.
        let visible = if ctx.before.starts_with(&chip.before) {
            match chip::type_through(&chip.text, &ctx.before[chip.before.len()..]) {
                TypeThrough::Advance(rest) => rest,
                _ => return,
            }
        } else {
            return;
        };
        if let Some(fix) = &chip.fix {
            match key {
                Key::Tab | Key::ShiftTab => {
                    crate::usage::record(|d| d.typo_fixes += 1);
                    self.sink.send(CoreCommand::ReplaceBeforeCaret {
                        expected: fix.expected.clone(),
                        text: fix.text.clone(),
                    });
                }
                Key::Escape => {
                    self.dismissed_typos.insert(fix.word.clone());
                    self.dismissed_for = Some(ctx.before);
                }
            }
            self.hide();
            self.cancel();
            return;
        }
        match key {
            Key::Tab => {
                let (word, _) = chip::accept_word(&visible);
                self.stat(|s| s.accepted_words += 1);
                let n = crate::usage::words(&word);
                crate::usage::record(|d| {
                    d.ghost_accepts += 1;
                    d.ghost_words += n;
                });
                self.remember_phrase(&word);
                // The chip stays; the text event caused by the insertion advances it (typing-through).
                self.expect = Some((format!("{}{word}", ctx.before), Instant::now()));
                self.sink.send(CoreCommand::InsertText { text: word });
            }
            Key::ShiftTab => {
                // Code review GT-14: take what the user saw, finishing its last word, not words that streamed in after
                // the key press.
                let visible = chip::accept_shown(&visible, shown.as_deref());
                self.stat(|s| s.accepted_all += 1);
                let n = crate::usage::words(&visible);
                crate::usage::record(|d| {
                    d.ghost_accepts += 1;
                    d.ghost_words += n;
                });
                self.remember_phrase(&visible);
                self.expect = Some((format!("{}{visible}", ctx.before), Instant::now()));
                self.sink.send(CoreCommand::InsertText { text: visible });
            }
            Key::Escape => {
                self.stat(|s| s.dismissed += 1);
                self.dismissed_for = Some(ctx.before);
                self.hide();
                self.cancel();
            }
        }
    }

    /// A misspelled word was just finished: offer the fix instead of a continuation (no model call).
    fn on_typo(&mut self, word: String, fix: String, trailing: String) {
        let enabled = self
            .shared
            .settings
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .typo_fixes;
        let Some(ctx) = self.ctx.clone() else {
            return;
        };
        let expected = format!("{word}{trailing}");
        let key = word.to_lowercase();
        if !enabled
            || !self.enabled
            || self.paused
            || fix.trim().is_empty()
            || fix == word
            || !ctx.before.ends_with(&expected)
            || self.dismissed_typos.contains(&key)
            || self.dismissed_for.as_deref() == Some(ctx.before.as_str())
        {
            return;
        }
        // Replaces the continuation request scheduled for the same text.
        self.cancel();
        self.generation += 1;
        let (generation, tx, quiet, before) = (
            self.generation,
            self.tx.clone(),
            self.config.quiet,
            ctx.before,
        );
        let chip = Fix {
            word: key,
            text: format!("{fix}{trailing}"),
            expected,
        };
        self.pending = Some(tokio::spawn(async move {
            tokio::time::sleep(quiet).await;
            let _ = tx.send(Input::Chip {
                generation,
                before,
                text: format!("{word} → {fix}"),
                fix: Some(chip),
            });
        }));
    }

    /// Record `chip` as current and display `visible` (the chip minus anything already typed through).
    /// Every call repaints, so a chip that grows token by token reaches the screen each time.
    fn show(&mut self, chip: Chip, caret: Rect, visible: String) {
        self.chip = Some(chip);
        if !self.capturing {
            self.capturing = true;
            self.sink.send(CoreCommand::CaptureKeys { active: true });
        }
        let app = self.ctx.as_ref().map(|c| c.app.clone()).unwrap_or_default();
        self.sink.send(CoreCommand::ShowChip {
            text: visible,
            anchor: self.nudged(&app, caret),
        });
    }

    /// The caret moved by the user's chip nudge for `app` (proposal 9).
    fn nudged(&self, app: &str, caret: Rect) -> Rect {
        let (dx, dy) = self
            .shared
            .settings
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .chip_offset(app);
        Rect {
            x: caret.x + f64::from(dx),
            y: caret.y + f64::from(dy),
            ..caret
        }
    }

    fn hide(&mut self) {
        if self.chip.take().is_some() {
            self.sink.send(CoreCommand::HideChip);
        }
        if self.capturing {
            self.capturing = false;
            self.sink.send(CoreCommand::CaptureKeys { active: false });
        }
    }

    /// Stop the request in flight. Its tokens may already wait in the queue, so its generation is retired
    /// too: nothing it sent is shown afterwards (code review GT-15: a dismissed chip came back 400 ms after Esc).
    fn cancel(&mut self) {
        if let Some(h) = self.pending.take() {
            h.abort();
        }
        self.generation += 1;
        self.first_chip_pending = None;
    }

    fn schedule(&mut self, before: String, keystroke: Instant) {
        log::trace!(
            "ghost: request {} for {} chars",
            self.generation + 1,
            before.chars().count()
        );
        self.cancel();
        self.generation += 1;
        let generation = self.generation;
        self.first_chip_pending = Some((generation, keystroke));
        let (brains, tx, stats, config, context) = (
            self.brains.clone(),
            self.tx.clone(),
            self.stats.clone(),
            self.config,
            self.context.clone(),
        );
        let languages = context.settings().languages;
        let app = self.ctx.as_ref().map(|c| c.app.clone()).unwrap_or_default();
        let mid_sentence = self
            .shared
            .settings
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .mid_sentence;
        let memory = if context.settings().ghost.memory {
            self.shared
                .memory
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .most_used(GHOST_MEMORY_PHRASES)
        } else {
            vec![]
        };
        self.pending = Some(tokio::spawn(async move {
            tokio::time::sleep(config.quiet).await;
            let send_done = |error: Option<String>| {
                let _ = tx.send(Input::Done { generation, error });
            };
            let brain = match brains.resolve_role(Role::Ghost).await {
                Ok(b) => b,
                Err(_) => return send_done(None), // Ghost is off or the built-in brain is asleep
            };
            if !brain.config.capabilities.ghost {
                return send_done(None);
            }
            let typed = redact::scrub(prompt::build(&before));
            if typed.trim().is_empty() {
                return send_done(None);
            }
            // Words after the cursor (read after the quiet gate, so they belong to this text): fill the gap
            // instead of continuing as if the sentence ended at the cursor.
            let visible = context.visible();
            let after_text = (visible.app_bundle_id == app)
                .then(|| chip::words_after(&visible.after_caret).map(str::to_string))
                .flatten();
            let after = match after_text {
                Some(a) if mid_sentence => Some(redact::scrub(&a)),
                Some(_) => return send_done(None), // inside a sentence, but the user turned this off
                None => None,
            };
            // Personal context only for brains allowed to receive it (PC-6). Stable layers come first, so
            // the server's prompt cache still covers everything up to the typed text.
            let prompt_text = if let Some(after) = &after {
                prompt::gap(&typed, after)
            } else if brain.config.shares_context() {
                format!("{}{typed}", context.ghost_preamble(&memory))
            } else {
                typed
            };
            stats
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .requests += 1;
            // Local Ghost requests happen at every pause and stay on this Mac; cloud ones are audited.
            if brain.config.is_cloud() {
                crate::audit::sent(
                    "ghost",
                    &brain.config,
                    prompt_text.chars().count(),
                    brain.config.shares_context(),
                );
            }
            let mut stream = match brain
                .complete_stream(&CompletionRequest {
                    max_tokens: config.max_tokens,
                    temperature: config.temperature,
                    ..CompletionRequest::ghost(prompt_text)
                })
                .await
            {
                Ok(s) => s,
                Err(e) => return send_done(Some(e.to_string())),
            };
            let mut raw = String::new();
            let mut last_sent: Option<String> = None;
            while let Some(item) = stream.next().await {
                match item {
                    Ok(tok) => raw.push_str(&tok),
                    Err(e) => return send_done(Some(e.to_string())),
                }
                let (polished, gap_done) = match &after {
                    Some(after) => match chip::gap_chip(&raw, &before, after) {
                        Some((c, done)) => (c, done),
                        None if raw.contains(']') => break,
                        None => continue,
                    },
                    None => match chip::polish(&raw, &before) {
                        Some(c) => (c, false),
                        None => {
                            log::trace!(
                                "ghost: {} raw chars, nothing worth showing yet",
                                raw.chars().count()
                            );
                            continue;
                        }
                    },
                };
                // Wrong language inside the same script (owner's screenshot: Vietnamese after "Hát.").
                let filtered =
                    crate::context::languages::filter_chip(&polished, &before, &languages);
                let cut = filtered.as_deref() != Some(polished.as_str());
                if filtered.is_none() {
                    // The model went off in another language: nothing later can fix it. Take back a
                    // partial chip that is already on screen.
                    if last_sent.is_some() {
                        let _ = tx.send(Input::Chip {
                            generation,
                            before: before.clone(),
                            text: String::new(),
                            fix: None,
                        });
                    }
                    break;
                }
                if let Some(chip) = filtered {
                    let done = cut || gap_done || (after.is_none() && chip::is_final(&chip));
                    if last_sent.as_deref() != Some(chip.as_str()) {
                        last_sent = Some(chip.clone());
                        let _ = tx.send(Input::Chip {
                            generation,
                            before: before.clone(),
                            text: chip,
                            fix: None,
                        });
                    }
                    if done {
                        break; // dropping the stream stops generation on the server
                    }
                }
            }
            send_done(None);
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brain::secrets::MemoryStore;
    use crate::brain::testserver::{Response, TestServer};
    use crate::brain::{BrainConfig, Capabilities, ProviderKind};

    use crate::testkit::Recorder;

    const CARET: Rect = Rect {
        x: 100.0,
        y: 200.0,
        width: 0.0,
        height: 18.0,
    };

    fn text(before: &str) -> NativeEvent {
        NativeEvent::TextState {
            app_bundle_id: "com.apple.TextEdit".into(),
            secure: false,
            before_caret: before.into(),
            selection_len: 0,
            caret: Some(CARET),
        }
    }

    /// A Brain service whose Ghost role points at an OpenAI-compatible test server answering `tokens`.
    async fn setup(
        tokens: &'static [&'static str],
        gap: Duration,
    ) -> (GhostHandle, Arc<Recorder>, TestServer, std::path::PathBuf) {
        let ctx = Arc::new(ContextHub::in_memory(Default::default(), ""));
        let (g, rec, server, root, _svc) = setup_ctx(tokens, gap, ctx).await;
        (g, rec, server, root)
    }

    async fn setup_ctx(
        tokens: &'static [&'static str],
        gap: Duration,
        context: Arc<ContextHub>,
    ) -> (
        GhostHandle,
        Arc<Recorder>,
        TestServer,
        std::path::PathBuf,
        Arc<BrainService>,
    ) {
        let server = TestServer::start(move |_| {
            let events: Vec<String> = tokens
                .iter()
                .map(|t| serde_json::json!({"choices":[{"text": t}]}).to_string())
                .collect();
            let refs: Vec<&str> = events.iter().map(String::as_str).collect();
            let mut r = Response::sse(&refs, gap);
            r = r.chunk_after(gap, "data: [DONE]\n\n");
            r
        })
        .await;
        let root = std::env::temp_dir().join(format!(
            "bob-ghost-{}-{}",
            std::process::id(),
            rand::random::<u32>()
        ));
        let svc = Arc::new(BrainService::new(
            root.join("c"),
            root.join("d"),
            root.join("r"),
            Box::new(MemoryStore::default()),
        ));
        svc.upsert(
            BrainConfig {
                id: "local".into(),
                name: "Local".into(),
                kind: ProviderKind::Ollama,
                endpoint: server.base.clone(),
                model: Some("m".into()),
                capabilities: Capabilities {
                    ghost: true,
                    chat: true,
                },
                has_api_key: false,
                share_context: None,
            },
            None,
        )
        .await
        .unwrap();
        svc.set_role(Role::Ghost, Some("local")).await.unwrap();
        let rec = Arc::new(Recorder::default());
        let handle = spawn_ctx(
            svc.clone(),
            rec.clone(),
            EngineConfig::default(),
            None,
            context,
        );
        (handle, rec, server, root, svc)
    }

    async fn settle(ms: u64) {
        tokio::time::sleep(Duration::from_millis(ms)).await;
    }

    #[tokio::test]
    async fn a_per_app_nudge_moves_the_chip_in_that_app_only() {
        let (g, rec, _server, _root) = setup(&[" soon."], Duration::from_millis(5)).await;
        g.set_chip_offset("com.apple.TextEdit", "TextEdit", 4, -3);
        g.native(text("I will get back to you"));
        settle(400).await;
        let anchors: Vec<Rect> = rec
            .take()
            .iter()
            .filter_map(|c| match c {
                CoreCommand::ShowChip { anchor, .. } => Some(*anchor),
                _ => None,
            })
            .collect();
        assert!(!anchors.is_empty());
        assert!(anchors
            .iter()
            .all(|a| a.x == CARET.x + 4.0 && a.y == CARET.y - 3.0 && a.height == CARET.height));
        g.set_chip_offset("com.apple.TextEdit", "TextEdit", 0, 0);
        g.native(text("See you"));
        settle(400).await;
        assert!(rec
            .take()
            .iter()
            .any(|c| matches!(c, CoreCommand::ShowChip { anchor, .. } if *anchor == CARET)));
    }

    #[tokio::test]
    async fn pausing_shows_a_polished_chip_and_enables_key_capture() {
        let (g, rec, server, _root) = setup(
            &[" as", " soon", " as", " possible.", " A.", " 正确"],
            Duration::from_millis(5),
        )
        .await;
        g.native(text("Thanks for your email. I will get back to you"));
        settle(400).await;
        let cmds = rec.take();
        assert!(
            cmds.contains(&CoreCommand::CaptureKeys { active: true }),
            "{cmds:?}"
        );
        let chips: Vec<String> = cmds
            .iter()
            .filter_map(|c| {
                if let CoreCommand::ShowChip { text, .. } = c {
                    Some(text.clone())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(
            chips.first().map(String::as_str),
            Some(" as"),
            "first token painted at once: {chips:?}"
        );
        // ...and it keeps growing to the full polished sentence (regression: it used to stop at " as",
        // found by the real-model pipeline test).
        assert_eq!(
            chips.last().map(String::as_str),
            Some(" as soon as possible."),
            "{chips:?}"
        );
        // The engine never shows the drift after the sentence end.
        assert!(
            chips.iter().all(|c| !c.contains("A.") && !c.contains('正')),
            "{chips:?}"
        );
        assert_eq!(server.request_count(), 1);
        let sent = server.last_request().json();
        assert_eq!(
            sent["prompt"],
            "Thanks for your email. I will get back to you"
        );
        assert_eq!(g.stats().chips_shown, 1);
        assert!(g.stats().percentile(50.0).unwrap() >= QUIET.as_millis() as u64);
    }

    #[tokio::test]
    async fn a_burst_of_keystrokes_sends_one_request_after_the_quiet_gate() {
        let (g, _rec, server, _root) = setup(&[" world"], Duration::ZERO).await;
        for t in ["H", "He", "Hel", "Hell", "Hello"] {
            g.native(text(t));
            settle(15).await;
        }
        settle(300).await;
        assert_eq!(
            server.request_count(),
            1,
            "debounce must coalesce the burst"
        );
        assert_eq!(server.last_request().json()["prompt"], "Hello");
    }

    #[tokio::test]
    async fn a_keystroke_cancels_the_in_flight_request() {
        // Slow stream: 50 tokens, 20 ms apart. Typing a different character mid-stream must abort it.
        static SLOW: [&str; 50] = [" a"; 50];
        let (g, rec, server, _root) = setup(&SLOW, Duration::from_millis(20)).await;
        g.native(text("Hello"));
        settle(200).await; // request in flight, chip " a" painted
        g.native(text("Hellox")); // diverges from " a"
        let deadline = Instant::now() + Duration::from_secs(5);
        while server.aborted.load(std::sync::atomic::Ordering::SeqCst) == 0 {
            assert!(
                Instant::now() < deadline,
                "the first stream was never cancelled"
            );
            settle(10).await;
        }
        assert!(rec.take().contains(&CoreCommand::HideChip));
    }

    #[tokio::test]
    async fn typing_through_updates_the_chip_without_a_new_request() {
        let (g, rec, server, _root) = setup(&[" lazy", " dog."], Duration::ZERO).await;
        g.native(text("The quick brown fox jumps over the"));
        settle(300).await;
        assert_eq!(server.request_count(), 1);
        rec.take();
        g.native(text("The quick brown fox jumps over the la"));
        settle(150).await;
        assert_eq!(rec.chips(), vec!["zy dog.".to_string()]);
        assert_eq!(
            server.request_count(),
            1,
            "matching characters must not call the model"
        );
        assert_eq!(g.stats().typing_through, 1);
    }

    #[tokio::test]
    async fn tab_inserts_one_word_and_shift_tab_inserts_the_rest() {
        let (g, rec, _server, _root) = setup(&[" lazy", " dog."], Duration::ZERO).await;
        g.native(text("over the"));
        settle(300).await;
        rec.take();
        g.native(NativeEvent::Key {
            key: Key::Tab,
            shown: None,
        });
        settle(50).await;
        assert_eq!(
            rec.take(),
            vec![CoreCommand::InsertText {
                text: " lazy".into()
            }]
        );
        // The app reports the inserted word: the chip advances, no new request.
        g.native(text("over the lazy"));
        settle(50).await;
        assert_eq!(rec.chips(), vec![" dog.".to_string()]);
        rec.take();
        g.native(NativeEvent::Key {
            key: Key::ShiftTab,
            shown: None,
        });
        settle(50).await;
        assert_eq!(
            rec.take(),
            vec![CoreCommand::InsertText {
                text: " dog.".into()
            }]
        );
        assert_eq!((g.stats().accepted_words, g.stats().accepted_all), (1, 1));
    }

    fn text_caret(before: &str, x: f64) -> NativeEvent {
        NativeEvent::TextState {
            app_bundle_id: "com.google.Chrome".into(),
            secure: false,
            before_caret: before.into(),
            selection_len: 0,
            caret: Some(Rect { x, ..CARET }),
        }
    }

    /// The owner's screenshot: "Investigate the" with the chip "te the possibility of using". Chrome reported
    /// the new caret with the old text after an insertion, so the old chip was repainted at the new caret.
    #[tokio::test]
    async fn stale_text_after_an_accept_never_repaints_the_old_chip() {
        let (g, rec, server, _root) =
            setup(&["te", " the", " possibility", " of using"], Duration::ZERO).await;
        g.native(text_caret("We will Investiga", 100.0));
        settle(300).await;
        assert_eq!(rec.chips().last().unwrap(), "te the possibility of using");
        rec.take();
        g.native(NativeEvent::Key {
            key: Key::Tab,
            shown: None,
        });
        settle(30).await;
        rec.take();
        // Stale: caret already moved, text not yet.
        g.native(text_caret("We will Investiga", 118.0));
        settle(30).await;
        assert!(
            rec.chips().is_empty(),
            "repainted a stale chip: {:?}",
            rec.chips()
        );
        // The re-read brings the real text: the chip advances.
        g.native(text_caret("We will Investigate", 118.0));
        settle(30).await;
        assert_eq!(rec.chips(), vec![" the possibility of using".to_string()]);
        assert_eq!(server.request_count(), 1);
    }

    /// Typing ahead of Chrome's text: a sideways caret move with unchanged text keeps the chip in place.
    #[tokio::test]
    async fn a_caret_that_moves_before_the_text_does_not_move_the_chip() {
        let (g, rec, _server, _root) = setup(&[" lazy", " dog."], Duration::ZERO).await;
        g.native(text_caret("over the", 100.0));
        settle(300).await;
        rec.take();
        g.native(text_caret("over the", 108.0));
        settle(30).await;
        assert!(rec.take().is_empty());
        g.native(text_caret("over the l", 108.0));
        settle(30).await;
        assert_eq!(rec.chips(), vec!["azy dog.".to_string()]);
    }

    /// The owner's second screenshot: after the Hungarian "Hát." the model went on in Vietnamese. The
    /// partial chip ("100% là") may reach the screen, but is taken back as soon as the language is clear.
    #[tokio::test]
    async fn a_suggestion_in_another_language_is_taken_back() {
        let (g, rec, _server, _root) = setup(
            &[" 100%", " là", " một", " album", " rất hay"],
            Duration::from_millis(5),
        )
        .await;
        g.native(text("Hát."));
        settle(400).await;
        let cmds = rec.take();
        assert!(
            matches!(
                cmds.last(),
                Some(CoreCommand::HideChip) | Some(CoreCommand::CaptureKeys { active: false })
            ),
            "{cmds:?}"
        );
        assert!(!cmds
            .iter()
            .any(|c| matches!(c, CoreCommand::ShowChip { text, .. } if text.contains("một"))));
    }

    /// Owner report: in Slack, Tab after "@" picks a name from Slack's own list; BOB must not take it.
    #[tokio::test]
    async fn mentions_and_slash_commands_leave_tab_to_the_app() {
        let (g, rec, server, _root) = setup(&[" there."], Duration::ZERO).await;
        g.native(text("Hi"));
        settle(300).await;
        assert!(rec
            .take()
            .contains(&CoreCommand::CaptureKeys { active: true }));
        for t in ["Hi @an", "Thanks #gen", "/rem"] {
            g.native(text(t));
            settle(150).await;
            let cmds = rec.take();
            assert!(
                !cmds
                    .iter()
                    .any(|c| matches!(c, CoreCommand::ShowChip { .. })),
                "{t}: {cmds:?}"
            );
        }
        assert_eq!(
            server.request_count(),
            1,
            "no model call while the app completes"
        );
        g.native(NativeEvent::Key {
            key: Key::Tab,
            shown: None,
        });
        settle(30).await;
        assert!(!rec
            .take()
            .iter()
            .any(|c| matches!(c, CoreCommand::InsertText { .. })));
    }

    #[tokio::test]
    async fn a_typo_fix_replaces_the_word_and_can_be_dismissed() {
        let (g, rec, server, _root) = setup(&[" fox."], Duration::ZERO).await;
        g.native(text("I saw teh "));
        g.native(NativeEvent::Typo {
            word: "teh".into(),
            fix: "the".into(),
            trailing: " ".into(),
        });
        settle(200).await;
        assert_eq!(rec.chips(), vec!["teh → the".to_string()]);
        assert_eq!(server.request_count(), 0, "a typo fix needs no model call");
        rec.take();
        g.native(NativeEvent::Key {
            key: Key::Tab,
            shown: None,
        });
        settle(30).await;
        assert!(rec.take().contains(&CoreCommand::ReplaceBeforeCaret {
            expected: "teh ".into(),
            text: "the ".into()
        }));
        // Esc: never offered for this word again in this session.
        g.native(text("and teh "));
        g.native(NativeEvent::Typo {
            word: "teh".into(),
            fix: "the".into(),
            trailing: " ".into(),
        });
        settle(200).await;
        g.native(NativeEvent::Key {
            key: Key::Escape,
            shown: None,
        });
        settle(30).await;
        rec.take();
        g.native(text("so teh "));
        g.native(NativeEvent::Typo {
            word: "Teh".into(),
            fix: "The".into(),
            trailing: " ".into(),
        });
        settle(200).await;
        assert!(
            !rec.chips().iter().any(|c| c.contains('→')),
            "{:?}",
            rec.chips()
        );
    }

    #[tokio::test]
    async fn inside_a_sentence_the_words_after_the_cursor_shape_the_suggestion() {
        let ctx = Arc::new(ContextHub::in_memory(Default::default(), ""));
        let (g, rec, server, _root, _svc) = setup_ctx(
            &["meet", "ing]", " Text: more"],
            Duration::ZERO,
            ctx.clone(),
        )
        .await;
        ctx.observe(&NativeEvent::ContextState {
            app_bundle_id: "com.apple.TextEdit".into(),
            window_title: None,
            after_caret: " to Thursday afternoon?".into(),
        });
        // The context hub only counts text after the caret for the app that reported it.
        ctx.observe(&NativeEvent::Focus {
            app_bundle_id: "com.apple.TextEdit".into(),
            app_name: "TextEdit".into(),
            secure: false,
            editable: true,
        });
        ctx.observe(&NativeEvent::ContextState {
            app_bundle_id: "com.apple.TextEdit".into(),
            window_title: None,
            after_caret: " to Thursday afternoon?".into(),
        });
        g.native(text("Could we move the "));
        settle(300).await;
        assert_eq!(rec.chips().last().map(String::as_str), Some("meeting"));
        let prompt = server.last_request().json()["prompt"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(
            prompt.contains("Could we move the [...] to Thursday afternoon?"),
            "{prompt}"
        );
    }

    /// Owner report: after switching windows the chip stayed frozen. A caret that moves without new text is
    /// trusted only briefly; then the chip goes away.
    #[tokio::test]
    async fn a_caret_that_keeps_moving_without_new_text_hides_the_chip() {
        let (g, rec, _server, _root) = setup(&[" lazy", " dog."], Duration::ZERO).await;
        g.native(text_caret("over the", 100.0));
        settle(300).await;
        rec.take();
        g.native(text_caret("over the", 108.0));
        settle(20).await;
        assert!(rec.take().is_empty(), "briefly kept in place");
        settle(SIDEWAYS_WINDOW.as_millis() as u64).await;
        g.native(text_caret("over the", 116.0));
        settle(20).await;
        assert!(rec.take().contains(&CoreCommand::HideChip));
    }

    /// Chrome reports typed spaces as no-break spaces; typing through a space must still match.
    #[tokio::test]
    async fn no_break_spaces_from_the_app_type_through() {
        let (g, rec, server, _root) = setup(&[" lazy", " dog."], Duration::ZERO).await;
        g.native(text("over the"));
        settle(300).await;
        rec.take();
        g.native(text("over the\u{a0}lazy\u{a0}"));
        settle(50).await;
        assert_eq!(rec.chips(), vec!["dog.".to_string()]);
        assert_eq!(server.request_count(), 1);
    }

    #[tokio::test]
    async fn tokens_already_queued_never_bring_a_dismissed_chip_back() {
        // Code review GT-15 (Test Bench seed 7001): Esc hid "much cleaner"; a streamed update repainted "much cleaner and".
        let (g, rec, _server, _root) = setup(&[" much", " cleaner"], Duration::ZERO).await;
        g.native(text("The design looks"));
        settle(300).await;
        assert!(!rec.chips().is_empty());
        g.native(NativeEvent::Key {
            key: Key::Escape,
            shown: None,
        });
        // Updates of that request that were already on their way, whatever their generation.
        for generation in 0..20 {
            let _ = g.tx.send(Input::Chip {
                generation,
                before: "The design looks".into(),
                text: " much cleaner and".into(),
                fix: None,
            });
        }
        settle(50).await;
        let cmds = rec.take();
        let shown_after_hide = cmds
            .iter()
            .skip_while(|c| **c != CoreCommand::HideChip)
            .any(|c| matches!(c, CoreCommand::ShowChip { .. }));
        assert!(!shown_after_hide, "{cmds:?}");
    }

    #[tokio::test]
    async fn shift_tab_inserts_what_the_helper_showed() {
        // Code review GT-14: the engine's chip grew to " brown fox jumps" after the helper drew " brown fo".
        let (g, rec, _server, _root) = setup(&[" brown", " fox", " jumps"], Duration::ZERO).await;
        g.native(text("The quick"));
        settle(300).await;
        rec.take();
        g.native(NativeEvent::Key {
            key: Key::ShiftTab,
            shown: Some(" brown fo".into()),
        });
        settle(30).await;
        assert!(rec.take().contains(&CoreCommand::InsertText {
            text: " brown fox".into()
        }));
    }

    #[tokio::test]
    async fn escape_dismisses_until_the_text_changes() {
        let (g, rec, server, _root) = setup(&[" world"], Duration::ZERO).await;
        g.native(text("Hello"));
        settle(300).await;
        g.native(NativeEvent::Key {
            key: Key::Escape,
            shown: None,
        });
        settle(30).await;
        let cmds = rec.take();
        assert!(
            cmds.contains(&CoreCommand::HideChip)
                && cmds.contains(&CoreCommand::CaptureKeys { active: false })
        );
        // The same text again (e.g. a caret blink event) must not bring the chip back.
        g.native(text("Hello"));
        settle(300).await;
        assert_eq!(server.request_count(), 1);
        g.native(text("Hello!"));
        settle(300).await;
        assert_eq!(server.request_count(), 2);
    }

    #[tokio::test]
    async fn quiet_zones_never_reach_the_model() {
        let (g, rec, server, _root) = setup(&[" secret"], Duration::ZERO).await;
        g.native(NativeEvent::TextState {
            app_bundle_id: "a".into(),
            secure: true,
            before_caret: "pass".into(),
            selection_len: 0,
            caret: Some(CARET),
        });
        g.native(NativeEvent::TextState {
            app_bundle_id: "a".into(),
            secure: false,
            before_caret: "no caret".into(),
            selection_len: 0,
            caret: None,
        });
        g.native(NativeEvent::TextState {
            app_bundle_id: "a".into(),
            secure: false,
            before_caret: "selected".into(),
            selection_len: 4,
            caret: Some(CARET),
        });
        settle(300).await;
        assert_eq!(
            server.request_count(),
            0,
            "GT-3/GT-4: no prompt may be sent from a quiet zone"
        );
        assert!(rec.chips().is_empty());
    }

    #[tokio::test]
    async fn keys_do_nothing_without_a_chip_and_capture_is_off_by_default() {
        let (g, rec, _server, _root) = setup(&[" x"], Duration::ZERO).await;
        g.native(NativeEvent::Key {
            key: Key::Tab,
            shown: None,
        });
        settle(30).await;
        assert!(rec.take().is_empty(), "GT-13: no chip, no action");
    }

    #[tokio::test]
    async fn secrets_are_redacted_before_the_prompt_is_sent() {
        let (g, _rec, server, _root) = setup(&[" ok"], Duration::ZERO).await;
        let key = format!("sk-{}", "Ab1".repeat(12));
        g.native(text(&format!("my key is {key} and")));
        settle(300).await;
        let prompt = server.last_request().json()["prompt"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(
            !prompt.contains(&key),
            "GT-18: secret leaked into the prompt"
        );
        assert!(prompt.contains("[redacted]"));
    }

    /// PC-5 / PC-6: with a Ghost layer on, the notes precede the typed text; a brain that may not receive
    /// personal context gets the typed text only.
    #[tokio::test]
    async fn context_notes_precede_the_text_only_for_allowed_brains() {
        use crate::context::{ContextSettings, GhostLayers};
        let ctx = Arc::new(ContextHub::in_memory(
            ContextSettings {
                ghost: GhostLayers {
                    profile: true,
                    memory: true,
                    ..Default::default()
                },
                ..Default::default()
            },
            "- **Name:** Renée",
        ));
        let (g, _rec, server, _root, svc) = setup_ctx(&[" ok"], Duration::ZERO, ctx).await;
        let prompt = || {
            server.last_request().json()["prompt"]
                .as_str()
                .unwrap()
                .to_string()
        };
        g.native(text("See you"));
        settle(300).await;
        assert_eq!(
            prompt(),
            "[Notes, not part of the text]\nAbout me: Name: Renée\n[Text]\nSee you"
        );
        svc.set_share_context("local", false).await.unwrap();
        g.native(text("See you at"));
        settle(300).await;
        assert_eq!(prompt(), "See you at");
    }

    #[tokio::test]
    async fn focus_changes_and_disabling_hide_the_chip() {
        let (g, rec, _server, _root) = setup(&[" world"], Duration::ZERO).await;
        g.native(text("Hello"));
        settle(300).await;
        rec.take();
        g.native(NativeEvent::Focus {
            app_bundle_id: "com.other".into(),
            app_name: "Other".into(),
            secure: false,
            editable: true,
        });
        settle(30).await;
        assert!(rec.take().contains(&CoreCommand::HideChip));
        g.native(text("Hello"));
        settle(300).await;
        g.set_enabled(false);
        settle(30).await;
        assert!(rec.take().contains(&CoreCommand::HideChip));
    }

    #[tokio::test]
    async fn a_restarted_helper_resets_chip_and_capture_state() {
        let (g, rec, _server, _root) = setup(&[" world"], Duration::ZERO).await;
        g.native(text("Hello"));
        settle(300).await;
        g.native(NativeEvent::Hello { version: 2, pid: 1 });
        settle(30).await;
        rec.take();
        // A key now does nothing (no chip), and the next pause enables capture again.
        g.native(NativeEvent::Key {
            key: Key::Tab,
            shown: None,
        });
        g.native(text("Hello there"));
        settle(300).await;
        let cmds = rec.take();
        assert!(
            !cmds
                .iter()
                .any(|c| matches!(c, CoreCommand::InsertText { .. })),
            "{cmds:?}"
        );
        assert!(
            cmds.contains(&CoreCommand::CaptureKeys { active: true }),
            "{cmds:?}"
        );
    }

    /// Full Ghost pipeline against the real llama-server and model (everything except the native helper).
    /// Run manually after `make sidecars` and downloading the default model:
    /// `BOB_TEST_MODEL=/path/model.gguf cargo test real_ghost_pipeline -- --ignored --nocapture`
    #[tokio::test]
    #[ignore = "needs the real llama-server binary and a GGUF model"]
    async fn real_ghost_pipeline_quality_and_latency() {
        use crate::brain::sidecar;
        let model =
            std::path::PathBuf::from(std::env::var("BOB_TEST_MODEL").expect("set BOB_TEST_MODEL"));
        let root = std::env::temp_dir().join(format!("bob-real-ghost-{}", std::process::id()));
        let data = root.join("d");
        std::fs::create_dir_all(data.join("models")).unwrap();
        let entry = crate::brain::models::find("qwen2.5-1.5b-instruct-q4km").unwrap();
        // A hard link, not a symlink: BOB only loads regular files it verified (code review SEC-13).
        let link = data.join("models").join(entry.file);
        if std::fs::hard_link(&model, &link).is_err() {
            std::fs::copy(&model, &link).unwrap();
        }
        assert!(
            sidecar::locate_binary().is_some(),
            "run `make sidecars` first"
        );
        let svc = Arc::new(BrainService::new(
            root.join("c"),
            data,
            root.join("r"),
            Box::new(MemoryStore::default()),
        ));
        svc.start_builtin(entry.id, Duration::from_secs(120))
            .await
            .unwrap();
        svc.builtin_ready(Duration::from_secs(180)).await.unwrap();
        let rec = Arc::new(Recorder::default());
        let g = spawn(svc.clone(), rec.clone());

        let prompts = [
            "Thanks for your email. I will get back to you",
            "Wait a minute I think I k",
            "The quick brown fox jumps over the",
            "Could you please send me the",
            "I'm running a bit late, I should be there in",
            "Let's schedule a meeting for next",
            "Happy birthday! I hope you have a",
            "The results of the experiment show that the",
            "Dear hiring manager, I am writing to apply for the",
            "Sorry for the late reply, I was",
        ];
        for p in prompts {
            rec.take();
            g.native(text(p));
            settle(1500).await;
            let chips = rec.chips();
            println!("{p:?} -> {:?}", chips.last());
        }
        let s = g.stats();
        println!(
            "first chip ms (keystroke -> chip, includes the {} ms quiet gate): {:?}",
            QUIET.as_millis(),
            s.first_chip_ms
        );
        println!(
            "p50 = {:?} ms, p95 = {:?} ms, shown {}/{}",
            s.percentile(50.0),
            s.percentile(95.0),
            s.chips_shown,
            prompts.len()
        );
        svc.stop_builtin().await;
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn quiet_apps_never_reach_the_model_and_can_be_unquieted() {
        let (g, rec, server, _root) = setup(&[" ls -la"], Duration::ZERO).await;
        let term = |t: &str| NativeEvent::TextState {
            app_bundle_id: "com.googlecode.iterm2".into(),
            secure: false,
            before_caret: t.into(),
            selection_len: 0,
            caret: Some(CARET),
        };
        g.native(term("git status"));
        settle(300).await;
        assert_eq!(server.request_count(), 0, "terminals are quiet by default");
        assert!(rec.chips().is_empty());
        g.set_quiet("com.googlecode.iterm2", "iTerm", false);
        g.native(term("git status --"));
        settle(300).await;
        assert_eq!(
            server.request_count(),
            1,
            "after un-quieting, suggestions resume"
        );
        // Quieting the current app hides its chip at once.
        rec.take();
        g.set_quiet("com.googlecode.iterm2", "iTerm", true);
        settle(30).await;
        assert!(rec.take().contains(&CoreCommand::HideChip));
    }

    #[tokio::test]
    async fn accepted_chips_are_remembered_and_trivia_is_not() {
        let (g, _rec, _server, _root) =
            setup(&[" as", " soon", " as", " possible."], Duration::ZERO).await;
        g.native(text("I will get back to you"));
        settle(300).await;
        g.native(NativeEvent::Key {
            key: Key::Tab,
            shown: None,
        });
        settle(30).await;
        assert!(
            g.memory().phrases.is_empty(),
            "a two-letter word is not worth remembering"
        );
        g.native(text("I will get back to you as"));
        settle(50).await;
        g.native(NativeEvent::Key {
            key: Key::ShiftTab,
            shown: None,
        });
        settle(30).await;
        let m = g.memory();
        assert_eq!(
            m.phrases.first().map(|p| p.text.as_str()),
            Some("soon as possible.")
        );
        g.clear_memory();
        assert!(g.memory().phrases.is_empty());
    }

    #[tokio::test]
    async fn settings_and_memory_persist_across_restarts_and_recent_apps_are_tracked() {
        let dir = std::env::temp_dir().join(format!("bob-ghost-store-{}", rand::random::<u32>()));
        let svc = Arc::new(BrainService::new(
            dir.join("c"),
            dir.join("d"),
            dir.join("r"),
            Box::new(MemoryStore::default()),
        ));
        let g = spawn_full(
            svc.clone(),
            Arc::new(Recorder::default()),
            EngineConfig::default(),
            Some(dir.clone()),
        );
        g.set_enabled(false);
        g.set_quiet("com.apple.Notes", "Notes", true);
        g.shared
            .memory
            .lock()
            .unwrap()
            .record("Best regards, Renée");
        g.shared.save_memory();
        g.native(NativeEvent::Focus {
            app_bundle_id: "com.apple.Mail".into(),
            app_name: "Mail".into(),
            secure: false,
            editable: true,
        });
        g.native(NativeEvent::Focus {
            app_bundle_id: "com.apple.Notes".into(),
            app_name: "Notes".into(),
            secure: false,
            editable: true,
        });
        settle(30).await;
        let names: Vec<String> = g.recent_apps().into_iter().map(|a| a.name).collect();
        assert_eq!(names, vec!["Notes".to_string(), "Mail".to_string()]);
        let again = spawn_full(
            svc,
            Arc::new(Recorder::default()),
            EngineConfig::default(),
            Some(dir.clone()),
        );
        assert!(!again.settings().enabled);
        assert!(again.settings().is_quiet("com.apple.Notes"));
        assert_eq!(again.memory().phrases[0].text, "Best regards, Renée");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn percentiles() {
        let mut s = GhostStats::default();
        assert_eq!(s.percentile(50.0), None);
        s.first_chip_ms = (1..=100).collect();
        assert_eq!(s.percentile(50.0), Some(51));
        assert_eq!(s.percentile(95.0), Some(95));
    }
}
