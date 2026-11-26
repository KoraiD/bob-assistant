//! Voice Dictation engine: drives [`machine`](super::machine) from helper events, runs the optional
//! cleanup on the dictation brain and inserts the result. Transcripts are never logged (VD-8).

use super::machine::{self, Config, Input, Orb, Output, State};
use super::settings::{apply_replacements, vocabulary_prompt, DictationSettings, SETTINGS_FILE};
use crate::brain::models;
use crate::brain::registry::Role;
use crate::brain::service::BrainService;
use crate::brain::ChatRequest;
use crate::context::{typography, ContextHub};
use crate::ghost::engine::Sink;
use crate::protocol::{CoreCommand, Hotkey, NativeEvent, OrbState};
use crate::selection::engine::SelectionHandle;
use futures_util::StreamExt;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
// tokio's clock: identical to `std::time::Instant` in the app, virtual in the simulator (`crate::sim`).
use tokio::time::Instant;

const CLEANUP_TIMEOUT: Duration = Duration::from_secs(20);
const TRANSCRIBE_TIMEOUT: Duration = Duration::from_secs(120);
const DONE_HIDE: Duration = Duration::from_millis(900);
const ERROR_HIDE: Duration = Duration::from_millis(3500);
/// How long the last dictation stays in memory for "Type last dictation" (proposal 4).
pub const LAST_KEEP: Duration = Duration::from_secs(120);
/// How often the idle unload is checked (proposal 11).
const IDLE_CHECK: Duration = Duration::from_secs(30);

pub const MIC_HELP: &str =
    "BOB needs the microphone: System Settings > Privacy & Security > Microphone > BOB.";
pub const MODEL_HELP: &str = "Download a dictation model first (Overview > Voice Dictation).";
pub const SECURE_HELP: &str = "BOB never types into password fields.";

#[derive(Debug, Clone, Default, Serialize)]
pub struct DictationStats {
    pub dictations: u64,
    pub discarded: u64,
    pub errors: u64,
    pub last_error: Option<String>,
    pub last_audio_ms: Option<u32>,
    pub last_transcribe_ms: Option<u32>,
    /// Key release to text sent for insertion (VD-10).
    pub last_release_to_insert_ms: Option<u64>,
    pub model_loaded: bool,
    pub model_error: Option<String>,
    /// The model was unloaded after the idle timeout; the next press loads it again.
    pub idle_unloaded: bool,
    /// A last dictation is still in memory (it is forgotten after two minutes).
    pub last_available: bool,
}

struct Shared {
    settings: Mutex<DictationSettings>,
    stats: Mutex<DictationStats>,
    storage: Option<PathBuf>,
    models_dir: PathBuf,
    unloaded: AtomicBool,
    /// The last inserted dictation, kept in memory only, for [`LAST_KEEP`].
    last: Mutex<Option<(String, Instant)>>,
}

impl Shared {
    fn settings(&self) -> DictationSettings {
        self.settings
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    fn last(&self) -> Option<String> {
        let mut last = self
            .last
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if last
            .as_ref()
            .is_some_and(|(_, at)| at.elapsed() > LAST_KEEP)
        {
            *last = None;
        }
        last.as_ref().map(|(t, _)| t.clone())
    }

    fn command(&self) -> CoreCommand {
        let s = self.settings();
        CoreCommand::SetDictation {
            hotkey: s.enabled.then(|| Hotkey {
                key_code: s.hotkey.key_code,
                modifiers: s.hotkey.modifiers.clone(),
            }),
            model_path: if s.enabled && !self.unloaded.load(Ordering::SeqCst) {
                model_path(&self.models_dir, &s).map(|p| p.display().to_string())
            } else {
                None
            },
            language: s.language.clone(),
            prompt: vocabulary_prompt(&s.vocabulary),
            sounds: s.sounds,
        }
    }
}

#[derive(Clone)]
pub struct DictationHandle {
    tx: mpsc::UnboundedSender<In>,
    shared: Arc<Shared>,
}

impl DictationHandle {
    pub fn native(&self, ev: NativeEvent) {
        let _ = self.tx.send(In::Native(ev));
    }

    pub fn settings(&self) -> DictationSettings {
        self.shared
            .settings
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    pub fn set_settings(&self, s: DictationSettings) -> std::io::Result<DictationSettings> {
        let s = s.sanitized();
        if s.unload_after_min == 0 || !s.enabled {
            // Loaded again with the next helper configuration.
            self.shared.unloaded.store(false, Ordering::SeqCst);
        }
        *self
            .shared
            .settings
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = s.clone();
        if let Some(dir) = &self.shared.storage {
            s.save(&dir.join(SETTINGS_FILE))?;
        }
        Ok(s)
    }

    pub fn stats(&self) -> DictationStats {
        let mut st = self
            .shared
            .stats
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        st.idle_unloaded = self.shared.unloaded.load(Ordering::SeqCst);
        st.last_available = self.shared.last().is_some();
        st
    }

    /// The last dictation while it is still in memory (two minutes).
    pub fn last_dictation(&self) -> Option<String> {
        self.shared.last()
    }

    /// Type the last dictation again at the caret. False when there is none.
    pub fn paste_last(&self) -> bool {
        let available = self.shared.last().is_some();
        if available {
            let _ = self.tx.send(In::PasteLast);
        }
        available
    }

    /// Path of the chosen model when it is installed.
    pub fn model_path(&self) -> Option<PathBuf> {
        model_path(&self.shared.models_dir, &self.settings())
    }

    /// The helper configuration for the current settings (sent after every handshake and change).
    pub fn command(&self) -> CoreCommand {
        self.shared.command()
    }
}

fn model_path(dir: &std::path::Path, s: &DictationSettings) -> Option<PathBuf> {
    let entry = models::find_speech(&s.model)?;
    let p = models::installed_path(dir, entry);
    p.is_file().then_some(p)
}

enum In {
    Native(NativeEvent),
    Tick,
    Cleaned {
        generation: u64,
        result: Result<String, String>,
    },
    TranscribeTimeout {
        generation: u64,
    },
    HideOrb {
        generation: u64,
    },
    PasteLast,
    IdleCheck,
}

pub fn spawn(
    brains: Arc<BrainService>,
    context: Arc<ContextHub>,
    sink: Arc<dyn Sink>,
    storage: Option<PathBuf>,
    models_dir: PathBuf,
    selection: Option<SelectionHandle>,
) -> DictationHandle {
    let settings = storage
        .as_ref()
        .map(|d| DictationSettings::load(&d.join(SETTINGS_FILE)))
        .unwrap_or_default();
    let shared = Arc::new(Shared {
        settings: Mutex::new(settings),
        stats: Mutex::new(DictationStats::default()),
        storage,
        models_dir,
        unloaded: AtomicBool::new(false),
        last: Mutex::new(None),
    });
    let (tx, rx) = mpsc::unbounded_channel();
    let engine = Engine {
        brains,
        context,
        sink,
        tx: tx.clone(),
        shared: shared.clone(),
        state: State::Idle,
        clock: Instant::now(),
        released_at: None,
        generation: 0,
        ticker: None,
        cleanup: None,
        secure: false,
        last_char: None,
        selection,
        pending_selection: None,
        last_used: Instant::now(),
    };
    tokio::spawn(engine.run(rx));
    let idle_tx = tx.clone();
    tokio::spawn(async move {
        let mut t = tokio::time::interval(IDLE_CHECK);
        t.tick().await;
        loop {
            t.tick().await;
            if idle_tx.send(In::IdleCheck).is_err() {
                break;
            }
        }
    });
    DictationHandle { tx, shared }
}

struct Engine {
    brains: Arc<BrainService>,
    context: Arc<ContextHub>,
    sink: Arc<dyn Sink>,
    tx: mpsc::UnboundedSender<In>,
    shared: Arc<Shared>,
    state: State,
    clock: Instant,
    released_at: Option<Instant>,
    /// Bumped on every new recording; late timers and results for older ones are ignored.
    generation: u64,
    ticker: Option<JoinHandle<()>>,
    cleanup: Option<JoinHandle<()>>,
    /// The focused field is secure (VD-9), from focus and text events.
    secure: bool,
    /// Last character before the caret, to decide whether the inserted text needs a leading space.
    last_char: Option<char>,
    selection: Option<SelectionHandle>,
    /// Text selected when this recording started: the transcript becomes an instruction for it (voice edit).
    pending_selection: Option<String>,
    /// Last press or insertion, for the idle unload.
    last_used: Instant,
}

impl Engine {
    async fn run(mut self, mut rx: mpsc::UnboundedReceiver<In>) {
        while let Some(i) = rx.recv().await {
            self.handle(i);
        }
    }

    fn now_ms(&self) -> u64 {
        self.clock.elapsed().as_millis() as u64
    }

    fn stats(&self, f: impl FnOnce(&mut DictationStats)) {
        f(&mut self
            .shared
            .stats
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner));
    }

    fn feed(&mut self, input: Input) {
        let s = self.shared.settings();
        let cfg = Config {
            tap_to_toggle: s.tap_to_toggle,
            ..Config::default()
        };
        // A voice-edit instruction is used as spoken; the selection brain does the rewriting.
        let cleanup = s.cleanup.task().is_some() && self.pending_selection.is_none();
        let (next, outputs) = machine::step(&self.state, input, cfg, cleanup);
        self.state = next;
        for o in outputs {
            self.apply(o);
        }
    }

    fn handle(&mut self, i: In) {
        match i {
            In::Native(NativeEvent::DictationKey { pressed: true }) => {
                if matches!(self.state, State::Latched { .. }) {
                    self.released_at = Some(Instant::now());
                    self.feed(Input::Press {
                        at_ms: self.now_ms(),
                    });
                    if matches!(self.state, State::Idle) {
                        self.stats(|s| s.discarded += 1);
                    }
                    return;
                }
                if !matches!(self.state, State::Idle) {
                    return;
                }
                if self.secure {
                    return self.orb(OrbState::Error, Some(SECURE_HELP.into()));
                }
                let installed = model_path(&self.shared.models_dir, &self.settings()).is_some();
                if !installed {
                    self.stats(|s| s.last_error = Some(MODEL_HELP.into()));
                    return self.orb(OrbState::Error, Some(MODEL_HELP.into()));
                }
                self.last_used = Instant::now();
                if self.shared.unloaded.swap(false, Ordering::SeqCst) {
                    // Reload; the helper's whisper queue is serial, so this recording waits for it.
                    self.sink.send(self.shared.command());
                }
                self.generation += 1;
                self.pending_selection = None;
                self.feed(Input::Press {
                    at_ms: self.now_ms(),
                });
            }
            In::Native(NativeEvent::DictationKey { pressed: false }) => {
                if matches!(self.state, State::Recording { .. }) {
                    self.released_at = Some(Instant::now());
                }
                let was_recording = matches!(self.state, State::Recording { .. });
                self.feed(Input::Release {
                    at_ms: self.now_ms(),
                });
                if was_recording && matches!(self.state, State::Idle) {
                    self.stats(|s| s.discarded += 1);
                }
            }
            In::Native(NativeEvent::Transcript {
                text,
                speech,
                audio_ms,
                transcribe_ms,
                ..
            }) => {
                self.stats(|s| {
                    s.last_audio_ms = Some(audio_ms);
                    s.last_transcribe_ms = speech.then_some(transcribe_ms);
                });
                self.feed(Input::Transcript { text, speech });
            }
            In::Native(NativeEvent::DictationError { reason }) => {
                let message = match reason.as_str() {
                    "microphone" => MIC_HELP.to_string(),
                    "no_model" => MODEL_HELP.to_string(),
                    other => format!("Dictation stopped: {other}."),
                };
                self.stats(|s| {
                    s.errors += 1;
                    s.last_error = Some(message.clone());
                });
                self.feed(Input::Failed { reason: message });
            }
            In::Native(NativeEvent::DictationStatus {
                model_loaded,
                error,
            }) => self.stats(|s| {
                s.model_loaded = model_loaded;
                s.model_error = error;
            }),
            In::Native(NativeEvent::Focus { secure, .. }) => {
                self.secure = secure;
                self.last_char = None;
            }
            In::Native(NativeEvent::TextState {
                secure,
                before_caret,
                ..
            }) => {
                self.secure = secure;
                self.last_char = before_caret.chars().last();
            }
            In::Native(NativeEvent::DictationSelection { text, .. }) => {
                if matches!(self.state, State::Recording { .. } | State::Latched { .. })
                    && self.selection.is_some()
                {
                    self.pending_selection = Some(text);
                }
            }
            In::Native(NativeEvent::Hello { .. }) => {
                // A restarted helper is not recording: whatever was in flight is lost.
                if !matches!(self.state, State::Idle) {
                    self.feed(Input::Failed {
                        reason: "the helper restarted".into(),
                    });
                }
                self.stats(|s| s.model_loaded = false);
            }
            In::Native(_) => {}
            In::Tick => {
                let at_ms = self.now_ms();
                self.feed(Input::Tick { at_ms });
            }
            In::Cleaned { generation, result } => {
                if generation == self.generation {
                    self.feed(Input::Cleaned(result));
                }
            }
            In::TranscribeTimeout { generation } => {
                if generation == self.generation && matches!(self.state, State::Transcribing) {
                    self.feed(Input::Failed {
                        reason: "transcription took too long".into(),
                    });
                }
            }
            In::PasteLast => {
                if let Some(text) = self.shared.last() {
                    if self.secure {
                        return self.orb(OrbState::Error, Some(SECURE_HELP.into()));
                    }
                    let text = with_spacing(self.last_char, &text);
                    self.sink.send(CoreCommand::InsertText { text });
                }
            }
            In::IdleCheck => {
                let s = self.shared.settings();
                let minutes = u64::from(s.unload_after_min);
                if minutes > 0
                    && s.enabled
                    && matches!(self.state, State::Idle)
                    && self.last_used.elapsed() >= Duration::from_secs(minutes * 60)
                    && model_path(&self.shared.models_dir, &s).is_some()
                    && !self.shared.unloaded.swap(true, Ordering::SeqCst)
                {
                    self.sink.send(self.shared.command());
                }
            }
            In::HideOrb { generation } => {
                if generation == self.generation && matches!(self.state, State::Idle) {
                    self.sink.send(CoreCommand::Orb {
                        state: OrbState::Hidden,
                        text: None,
                    });
                }
            }
        }
    }

    fn settings(&self) -> DictationSettings {
        self.shared
            .settings
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    fn orb(&mut self, state: OrbState, text: Option<String>) {
        self.sink.send(CoreCommand::Orb { state, text });
        let hide = match state {
            OrbState::Done => Some(DONE_HIDE),
            OrbState::Error => Some(ERROR_HIDE),
            _ => None,
        };
        if let Some(d) = hide {
            let (tx, generation) = (self.tx.clone(), self.generation);
            tokio::spawn(async move {
                tokio::time::sleep(d).await;
                let _ = tx.send(In::HideOrb { generation });
            });
        }
    }

    fn apply(&mut self, o: Output) {
        match o {
            Output::StartRecording => {
                self.sink.send(CoreCommand::StartRecording);
                let tx = self.tx.clone();
                self.ticker = Some(tokio::spawn(async move {
                    let mut t = tokio::time::interval(Duration::from_secs(1));
                    loop {
                        t.tick().await;
                        if tx.send(In::Tick).is_err() {
                            break;
                        }
                    }
                }));
            }
            Output::StopRecording { transcribe } => {
                if let Some(t) = self.ticker.take() {
                    t.abort();
                }
                self.sink.send(CoreCommand::StopRecording { transcribe });
                if transcribe {
                    let (tx, generation) = (self.tx.clone(), self.generation);
                    tokio::spawn(async move {
                        tokio::time::sleep(TRANSCRIBE_TIMEOUT).await;
                        let _ = tx.send(In::TranscribeTimeout { generation });
                    });
                }
            }
            Output::Orb(orb) => {
                let (state, text) = match orb {
                    Orb::Hidden => (OrbState::Hidden, None),
                    Orb::Listening => (OrbState::Listening, None),
                    Orb::Thinking => (OrbState::Thinking, None),
                    Orb::Done => (OrbState::Done, None),
                    Orb::Error(t) => (OrbState::Error, Some(t)),
                };
                self.orb(state, text);
            }
            Output::Cleanup(text) => {
                let preset = self.settings().cleanup;
                let (brains, context, tx, generation) = (
                    self.brains.clone(),
                    self.context.clone(),
                    self.tx.clone(),
                    self.generation,
                );
                self.cleanup = Some(tokio::spawn(async move {
                    let result = tokio::time::timeout(
                        CLEANUP_TIMEOUT,
                        run_cleanup(&brains, &context, preset, &text),
                    )
                    .await
                    .unwrap_or_else(|_| Err("cleanup took too long".into()));
                    let _ = tx.send(In::Cleaned { generation, result });
                }));
            }
            Output::Insert(text) => {
                self.last_used = Instant::now();
                if let Some(selected) = self.pending_selection.take() {
                    if let Some(sel) = &self.selection {
                        self.released_at = None;
                        self.stats(|s| s.dictations += 1);
                        sel.voice_edit(selected, text);
                        return;
                    }
                }
                if self.secure {
                    self.stats(|s| s.last_error = Some(SECURE_HELP.into()));
                    return self.orb(OrbState::Error, Some(SECURE_HELP.into()));
                }
                let settings = self.shared.settings();
                let text = apply_replacements(&text, &settings.replacements);
                let style = self.context.settings();
                let text = typography::finish(&text, style.spelling, style.quotes, self.last_char);
                *self
                    .shared
                    .last
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) =
                    Some((text.clone(), Instant::now()));
                let audio_ms = self
                    .shared
                    .stats
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .last_audio_ms;
                let words = crate::usage::words(&text);
                crate::usage::record(|d| {
                    d.dictations += 1;
                    d.dictated_words += words;
                    d.dictated_ms += u64::from(audio_ms.unwrap_or(0));
                });
                let text = with_spacing(self.last_char, &text);
                let ms = self
                    .released_at
                    .take()
                    .map(|t| t.elapsed().as_millis() as u64);
                self.stats(|s| {
                    s.dictations += 1;
                    s.last_release_to_insert_ms = ms;
                });
                self.sink.send(CoreCommand::InsertText { text });
            }
        }
    }
}

/// Add a space between the existing text and the dictated text when they would otherwise run together.
pub fn with_spacing(last_char: Option<char>, text: &str) -> String {
    match (last_char, text.chars().next()) {
        (Some(c), Some(first))
            if !c.is_whitespace() && !"([{\"'“‘".contains(c) && first.is_alphanumeric() =>
        {
            format!(" {text}")
        }
        _ => text.to_string(),
    }
}

async fn run_cleanup(
    brains: &BrainService,
    context: &ContextHub,
    preset: super::settings::Cleanup,
    text: &str,
) -> Result<String, String> {
    let (task, sets_language) = preset.task().ok_or("no cleanup preset")?;
    let brain = brains
        .resolve_role(Role::Dictation)
        .await
        .map_err(|e| e.to_string())?;
    if !brain.config.capabilities.chat {
        return Err("the dictation brain cannot follow instructions".into());
    }
    let system = context.chat_system_opts(task, brain.config.shares_context(), !sets_language);
    crate::audit::sent(
        "dictation_cleanup",
        &brain.config,
        text.chars().count(),
        brain.config.shares_context(),
    );
    let mut stream = brain
        .chat_stream(&ChatRequest {
            system,
            user: text.to_string(),
            max_tokens: ((text.chars().count() / 3) as u32 * 2 + 64).min(1024),
            temperature: 0.2,
        })
        .await
        .map_err(|e| e.to_string())?;
    let mut out = String::new();
    while let Some(item) = stream.next().await {
        out.push_str(&item.map_err(|e| e.to_string())?);
    }
    Ok(crate::selection::clean::clean_core(&out, text))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brain::secrets::MemoryStore;
    use crate::testkit::Recorder;

    /// An engine whose chosen model "is installed" (an empty file with the catalog name).
    fn setup(installed: bool) -> (DictationHandle, Arc<Recorder>, PathBuf) {
        let root = std::env::temp_dir().join(format!("bob-dictation-{}", rand::random::<u32>()));
        let models_dir = root.join("models");
        std::fs::create_dir_all(&models_dir).unwrap();
        if installed {
            let entry = models::find_speech(super::super::settings::DEFAULT_MODEL).unwrap();
            std::fs::write(models::installed_path(&models_dir, entry), b"").unwrap();
        }
        let svc = Arc::new(BrainService::new(
            root.join("c"),
            root.join("d"),
            root.join("r"),
            Box::new(MemoryStore::default()),
        ));
        let ctx = Arc::new(ContextHub::in_memory(Default::default(), ""));
        let rec = Arc::new(Recorder::default());
        let sel = crate::selection::engine::spawn(svc.clone(), ctx.clone(), rec.clone(), None);
        let h = spawn(
            svc,
            ctx,
            rec.clone(),
            Some(root.join("c")),
            models_dir,
            Some(sel),
        );
        (h, rec, root)
    }

    async fn settle(ms: u64) {
        tokio::time::sleep(Duration::from_millis(ms)).await;
    }

    fn key(pressed: bool) -> NativeEvent {
        NativeEvent::DictationKey { pressed }
    }

    #[tokio::test]
    async fn hold_release_and_transcript_insert_with_spacing() {
        let (h, rec, _root) = setup(true);
        h.native(NativeEvent::TextState {
            app_bundle_id: "com.apple.Notes".into(),
            secure: false,
            before_caret: "Hello".into(),
            selection_len: 0,
            caret: None,
        });
        h.native(key(true));
        settle(300).await;
        h.native(key(false));
        settle(20).await;
        assert_eq!(
            rec.take(),
            vec![
                CoreCommand::StartRecording,
                CoreCommand::Orb {
                    state: OrbState::Listening,
                    text: None
                },
                CoreCommand::StopRecording { transcribe: true },
                CoreCommand::Orb {
                    state: OrbState::Thinking,
                    text: None
                },
            ]
        );
        h.native(NativeEvent::Transcript {
            text: "world, how are you?".into(),
            language: Some("en".into()),
            speech: true,
            audio_ms: 1200,
            transcribe_ms: 250,
        });
        settle(20).await;
        let cmds = rec.take();
        assert_eq!(
            cmds[0],
            CoreCommand::InsertText {
                text: " world, how are you?".into()
            }
        );
        let st = h.stats();
        assert_eq!(st.dictations, 1);
        assert_eq!(st.last_transcribe_ms, Some(250));
        assert!(st.last_release_to_insert_ms.is_some());
        settle(1000).await;
        assert!(rec.take().contains(&CoreCommand::Orb {
            state: OrbState::Hidden,
            text: None
        }));
    }

    #[tokio::test]
    async fn taps_are_discarded_and_secure_fields_and_missing_models_never_record() {
        let (h, rec, _root) = setup(true);
        h.set_settings(DictationSettings {
            tap_to_toggle: false,
            ..Default::default()
        })
        .unwrap();
        h.native(key(true));
        h.native(key(false));
        settle(30).await;
        assert!(rec
            .take()
            .contains(&CoreCommand::StopRecording { transcribe: false }));
        assert_eq!(h.stats().discarded, 1);

        h.native(NativeEvent::Focus {
            app_bundle_id: "com.apple.Safari".into(),
            app_name: "Safari".into(),
            secure: true,
            editable: true,
        });
        h.native(key(true));
        settle(30).await;
        let cmds = rec.take();
        assert!(!cmds.contains(&CoreCommand::StartRecording));
        assert!(cmds.contains(&CoreCommand::Orb {
            state: OrbState::Error,
            text: Some(SECURE_HELP.into())
        }));

        let (h2, rec2, _root2) = setup(false);
        h2.native(key(true));
        settle(30).await;
        let cmds = rec2.take();
        assert!(!cmds.contains(&CoreCommand::StartRecording));
        assert!(cmds.contains(&CoreCommand::Orb {
            state: OrbState::Error,
            text: Some(MODEL_HELP.into())
        }));
    }

    #[tokio::test]
    async fn microphone_denial_explains_and_resets() {
        let (h, rec, _root) = setup(true);
        h.native(key(true));
        settle(20).await;
        h.native(NativeEvent::DictationError {
            reason: "microphone".into(),
        });
        settle(20).await;
        let cmds = rec.take();
        assert!(cmds.contains(&CoreCommand::Orb {
            state: OrbState::Error,
            text: Some(MIC_HELP.into())
        }));
        // Back to Idle: the next press records again.
        h.native(key(true));
        settle(20).await;
        assert!(rec.take().contains(&CoreCommand::StartRecording));
    }

    #[tokio::test]
    async fn cleanup_without_a_ready_brain_falls_back_to_the_raw_transcript() {
        let (h, rec, _root) = setup(true);
        h.set_settings(DictationSettings {
            cleanup: super::super::settings::Cleanup::Clean,
            ..Default::default()
        })
        .unwrap();
        h.native(key(true));
        settle(300).await;
        h.native(key(false));
        h.native(NativeEvent::Transcript {
            text: "um so this is it".into(),
            language: None,
            speech: true,
            audio_ms: 900,
            transcribe_ms: 100,
        });
        settle(200).await;
        assert!(rec.take().contains(&CoreCommand::InsertText {
            text: "um so this is it".into()
        }));
    }

    fn transcript(text: &str) -> NativeEvent {
        NativeEvent::Transcript {
            text: text.into(),
            language: None,
            speech: true,
            audio_ms: 1000,
            transcribe_ms: 100,
        }
    }

    fn inserted(cmds: &[CoreCommand]) -> Vec<String> {
        cmds.iter()
            .filter_map(|c| match c {
                CoreCommand::InsertText { text } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    #[tokio::test]
    async fn a_tap_latches_and_the_next_press_transcribes() {
        let (h, rec, _root) = setup(true);
        h.native(key(true));
        h.native(key(false));
        settle(30).await;
        let cmds = rec.take();
        assert!(cmds.contains(&CoreCommand::StartRecording));
        assert!(!cmds
            .iter()
            .any(|c| matches!(c, CoreCommand::StopRecording { .. })));
        settle(300).await;
        h.native(key(true));
        h.native(key(false));
        settle(30).await;
        assert!(rec
            .take()
            .contains(&CoreCommand::StopRecording { transcribe: true }));
        h.native(transcript("hands free"));
        settle(30).await;
        assert_eq!(inserted(&rec.take()), vec!["hands free".to_string()]);
    }

    #[tokio::test]
    async fn replacements_apply_and_the_last_dictation_can_be_typed_again() {
        let (h, rec, _root) = setup(true);
        h.set_settings(DictationSettings {
            replacements: vec![super::super::settings::Replacement {
                from: "my email".into(),
                to: "me@example.com".into(),
            }],
            vocabulary: vec!["Kiroween".into()],
            ..Default::default()
        })
        .unwrap();
        let CoreCommand::SetDictation { prompt, .. } = h.command() else {
            panic!()
        };
        assert!(prompt.unwrap().contains("Kiroween"));
        assert!(!h.paste_last(), "nothing to paste yet");
        h.native(key(true));
        settle(300).await;
        h.native(key(false));
        h.native(transcript("send it to my email"));
        settle(30).await;
        assert_eq!(
            inserted(&rec.take()),
            vec!["send it to me@example.com".to_string()]
        );
        assert!(h.stats().last_available);
        assert!(h.paste_last());
        settle(30).await;
        assert_eq!(
            inserted(&rec.take()),
            vec!["send it to me@example.com".to_string()]
        );
    }

    #[tokio::test]
    async fn speaking_over_a_selection_is_a_voice_edit_not_an_insertion() {
        let (h, rec, _root) = setup(true);
        h.native(key(true));
        h.native(NativeEvent::DictationSelection {
            app_bundle_id: "com.apple.TextEdit".into(),
            text: "teh text".into(),
        });
        settle(300).await;
        h.native(key(false));
        h.native(transcript("fix the typos"));
        settle(100).await;
        let cmds = rec.take();
        assert!(inserted(&cmds).is_empty(), "{cmds:?}");
        // No selection brain in the test: the selection engine explains instead of inserting.
        assert!(cmds.iter().any(|c| matches!(
            c,
            CoreCommand::MenuStatus { error: true, text, .. } if text.contains("No brain")
        )));
        // The next plain dictation inserts as usual.
        h.native(key(true));
        settle(300).await;
        h.native(key(false));
        h.native(transcript("plain"));
        settle(30).await;
        assert_eq!(inserted(&rec.take()), vec!["plain".to_string()]);
    }

    #[test]
    fn voice_edit_instructions_are_bounded() {
        let a = crate::selection::engine::voice_action(&"x".repeat(5000));
        assert!(a.instruction.len() < 1300);
        assert!(a.sets_language);
    }

    #[test]
    fn the_helper_command_reflects_settings_and_installation() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let _g = rt.enter();
        let (h, _rec, _root) = setup(false);
        let CoreCommand::SetDictation {
            hotkey, model_path, ..
        } = h.command()
        else {
            panic!()
        };
        assert_eq!(hotkey.unwrap().key_code, 49);
        assert_eq!(model_path, None);
        h.set_settings(DictationSettings {
            enabled: false,
            ..Default::default()
        })
        .unwrap();
        let CoreCommand::SetDictation { hotkey, .. } = h.command() else {
            panic!()
        };
        assert!(hotkey.is_none(), "disabled dictation registers no hotkey");
    }

    #[test]
    fn spacing_only_where_words_would_run_together() {
        assert_eq!(with_spacing(Some('o'), "world"), " world");
        assert_eq!(with_spacing(Some(' '), "world"), "world");
        assert_eq!(with_spacing(None, "world"), "world");
        assert_eq!(with_spacing(Some('('), "world"), "world");
        assert_eq!(with_spacing(Some('o'), ", and"), ", and");
    }
}
