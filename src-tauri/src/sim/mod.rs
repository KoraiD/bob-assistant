//! A simulator for BOB's engines (test builds only): the real Ghost Text, Selection Actions and Dictation
//! engines against a fake app, a fake Swift helper, scripted models and a scripted or random user, all on
//! tokio's virtual clock.
//!
//! Why: the bugs users report ("an extra space", "it inserted the word twice") come from the order in which
//! keys, Accessibility reports, model tokens and BOB's own insertions arrive. Real-time tests hit only a few
//! orderings and cannot replay them. Here one seed fixes every delay, so a failing session replays exactly,
//! and a ten-minute session runs in milliseconds.
//!
//! Pieces: [`model`] streams answers like llama.cpp, [`world`] models the app and the helper with the helper's
//! real timings for Ghost Text, [`actor`] is the user, [`check`] names the defects. [`run`] plays one Ghost
//! Text session and returns a [`Report`] with the timeline. [`selection`] and [`dictation`] do the same for
//! the other two features; [`settings`] runs Ghost Text under random setting combinations.
//!
//! Sessions need a paused clock: `#[tokio::test(start_paused = true)]`.

pub mod actor;
pub mod check;
pub mod model;
pub mod world;

mod dictation;
mod random;
mod scenarios;
mod selection;
mod settings;
mod tests;

use crate::brain::client::scripted;
use crate::brain::registry::Role;
use crate::brain::secrets::MemoryStore;
use crate::brain::service::BrainService;
use crate::brain::{BrainConfig, Capabilities, ProviderKind};
use crate::context::{ContextHub, ContextSettings};
use crate::ghost::engine::{spawn_ctx, EngineConfig, Sink};
use crate::protocol::{CoreCommand, NativeEvent};
use actor::Actor;
use check::{Defect, Kind};
use model::{Calls, Model};
use rand::SeedableRng;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::Instant;
use world::{Line, Profile, Stats, World};

pub struct Config {
    pub seed: u64,
    pub profile: Profile,
    pub model: Model,
    /// Text already in the field when the session starts.
    pub start_text: String,
    /// Byte offset of the caret in `start_text` (default: the end).
    pub start_caret: Option<usize>,
    /// Longest session, in virtual time.
    pub max: Duration,
    pub setup: Setup,
}

/// Settings and personal context for a Ghost Text session.
#[derive(Debug, Clone)]
pub struct Setup {
    /// The Ghost Text brain is a cloud service (text leaves the Mac).
    pub cloud: bool,
    /// The brain's personal-context switch (`None`: the default, on for local and off for cloud brains).
    pub share_context: Option<bool>,
    pub context: ContextSettings,
    pub profile: String,
    pub memory: Vec<String>,
    pub clipboard: Option<String>,
    pub window_title: Option<String>,
    pub typo_fixes: bool,
    pub mid_sentence: bool,
    pub enabled: bool,
    pub paused: bool,
    /// The app is on the quiet list.
    pub quiet_app: bool,
}

impl Default for Setup {
    /// The shipped settings, except that no personal context is shared: the prompt is exactly the typed text,
    /// so the model sees what the user sees.
    fn default() -> Self {
        Self {
            cloud: false,
            share_context: Some(false),
            context: ContextSettings::default(),
            profile: String::new(),
            memory: vec![],
            clipboard: None,
            window_title: None,
            typo_fixes: true,
            mid_sentence: true,
            enabled: true,
            paused: false,
            quiet_app: false,
        }
    }
}

impl Config {
    pub fn with_setup(mut self, setup: Setup) -> Self {
        self.setup = setup;
        self
    }

    /// Start with `text` in the field and the caret at byte `caret` (`None`: at the end).
    pub fn with_text(mut self, text: impl Into<String>, caret: Option<usize>) -> Self {
        self.start_text = text.into();
        self.start_caret = caret;
        self
    }

    pub fn new(seed: u64, profile: Profile, model: Model) -> Self {
        Self {
            seed,
            profile,
            model,
            start_text: String::new(),
            start_caret: None,
            max: Duration::from_secs(600),
            setup: Setup::default(),
        }
    }
}

/// The outcome of one session.
#[derive(Debug)]
pub struct Report {
    pub seed: u64,
    pub profile: &'static str,
    pub stats: Stats,
    pub defects: Vec<Defect>,
    pub log: Vec<Line>,
    /// The field's whole text at the end.
    pub text: String,
    pub requests: usize,
    pub cancelled: usize,
    /// Every model request: milliseconds since the start, and the prompt.
    pub prompts: Vec<(u64, String)>,
    /// Tokens the model would have generated, and tokens actually streamed before cancellations.
    pub tokens_offered: usize,
    pub tokens_streamed: usize,
    pub virtual_ms: u64,
    /// The Ghost Text brain was a cloud brain.
    pub cloud: bool,
}

impl Report {
    pub fn counts(&self) -> BTreeMap<Kind, usize> {
        let mut m = BTreeMap::new();
        for d in &self.defects {
            *m.entry(d.kind).or_default() += 1;
        }
        m
    }

    /// Defects per 1000 keys the user pressed.
    pub fn per_1000_keys(&self) -> f64 {
        if self.stats.keys == 0 {
            return 0.0;
        }
        self.defects.len() as f64 * 1000.0 / f64::from(self.stats.keys)
    }

    pub fn summary(&self) -> String {
        format!(
            "{} seed {}: {} keys ({} accepts), {} defects ({:.1} per 1000 keys) {:?}; {} model requests ({} cancelled, {} of {} tokens streamed), {} chips painted, {} ms virtual",
            self.profile,
            self.seed,
            self.stats.keys,
            self.stats.accepts,
            self.defects.len(),
            self.per_1000_keys(),
            self.counts(),
            self.requests,
            self.cancelled,
            self.tokens_streamed,
            self.tokens_offered,
            self.stats.chips_painted,
            self.virtual_ms
        )
    }

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

/// Engine commands go to the driver loop, which hands them to the world in order.
struct ChannelSink(mpsc::UnboundedSender<CoreCommand>);

impl Sink for ChannelSink {
    fn send(&self, cmd: CoreCommand) {
        let _ = self.0.send(cmd);
    }
}

static SESSION: AtomicU64 = AtomicU64::new(0);

/// A brain service with one scripted brain ("sim") for every role, removed again when dropped.
pub struct Brains {
    pub service: Arc<BrainService>,
    /// Ghost Text requests, and chat requests (Selection Actions, dictation cleanup, voice edit).
    pub completions: Calls,
    pub chats: Calls,
    pub cloud: bool,
    root: std::path::PathBuf,
    _script: scripted::Registration,
}

impl Drop for Brains {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

impl Brains {
    /// `model` answers Ghost Text; the chat script answers instructions. `cloud` puts the brain outside this
    /// Mac (never contacted: the scripts answer for the endpoint).
    pub async fn new(seed: u64, model: Model, cloud: bool, share_context: Option<bool>) -> Self {
        let n = SESSION.fetch_add(1, Ordering::SeqCst);
        let id = format!("sim-{}-{n}", std::process::id());
        let endpoint = if cloud {
            format!("https://sim-cloud.example.com/{id}")
        } else {
            format!("http://127.0.0.1:9/{id}")
        };
        let (completions, chats): (Calls, Calls) = Default::default();
        let script = scripted::register(
            &endpoint,
            Some(model.script(seed, completions.clone())),
            Some(model::chat_script(seed, chats.clone())),
        );
        let root = std::env::temp_dir().join(format!("bob-{id}"));
        let service = Arc::new(BrainService::new(
            root.join("c"),
            root.join("d"),
            root.join("r"),
            Box::new(MemoryStore::default()),
        ));
        service
            .upsert(
                BrainConfig {
                    id: "sim".into(),
                    name: "Simulated model".into(),
                    kind: ProviderKind::Ollama,
                    endpoint,
                    model: Some("sim".into()),
                    capabilities: Capabilities {
                        ghost: true,
                        chat: true,
                    },
                    has_api_key: false,
                    share_context,
                },
                None,
            )
            .await
            .expect("register the simulated brain");
        for role in [Role::Ghost, Role::Selection, Role::Dictation] {
            service
                .set_role(role, Some("sim"))
                .await
                .expect("assign the simulated brain");
        }
        Self {
            service,
            completions,
            chats,
            cloud,
            root,
            _script: script,
        }
    }

    /// Requests that carried credential-like text to a cloud brain (redaction would have changed them).
    pub fn secrets_sent_to_cloud(&self) -> Vec<String> {
        if !self.cloud {
            return vec![];
        }
        let lock = |c: &Calls| {
            c.lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone()
        };
        lock(&self.completions)
            .into_iter()
            .chain(lock(&self.chats))
            .map(|c| c.prompt)
            .filter(|p| crate::ghost::redact::scrub(p) != *p)
            .collect()
    }
}

/// Play one Ghost Text session to its end and report it. Needs a paused tokio clock (current-thread runtime).
pub async fn run(config: Config, actor: &mut dyn Actor) -> Report {
    let setup = config.setup.clone();
    let brains = Brains::new(
        config.seed,
        config.model.clone(),
        setup.cloud,
        setup.share_context,
    )
    .await;
    let calls = brains.completions.clone();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let context = Arc::new(ContextHub::in_memory(setup.context.clone(), &setup.profile));
    let ghost = spawn_ctx(
        brains.service.clone(),
        Arc::new(ChannelSink(tx)),
        EngineConfig::default(),
        None,
        context.clone(),
    );
    ghost.set_option(Some(setup.typo_fixes), Some(setup.mid_sentence));
    for phrase in &setup.memory {
        ghost.remember(phrase);
    }
    if setup.quiet_app {
        ghost.set_quiet(config.profile.bundle_id, config.profile.name, true);
    }
    if !setup.enabled {
        ghost.set_enabled(false);
    }
    if setup.paused {
        ghost.set_paused(true);
    }
    if let Some(text) = &setup.clipboard {
        context.observe(&NativeEvent::ClipboardText { text: text.clone() });
    }
    let rng = rand::rngs::StdRng::seed_from_u64(config.seed);
    let mut world = World::new(
        config.profile,
        rng,
        ghost,
        context,
        calls.clone(),
        &config.start_text,
        config.start_caret,
    );
    world.set_window_title(setup.window_title.clone());
    let start = Instant::now();
    let deadline = start + config.max;

    while !world.ended() && Instant::now() < deadline {
        while let Ok(cmd) = rx.try_recv() {
            world.on_command(cmd);
        }
        world.check_quiet();
        let wake = world.next_time().unwrap_or(deadline).min(deadline);
        tokio::select! {
            biased;
            Some(cmd) = rx.recv() => world.on_command(cmd),
            () = tokio::time::sleep_until(wake) => world.fire_due(actor),
        }
    }

    let calls = calls
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    Report {
        seed: config.seed,
        profile: config.profile.name,
        stats: world.stats.clone(),
        defects: world.defects.clone(),
        log: world.log.clone(),
        text: world.text().to_string(),
        requests: calls.len(),
        cancelled: calls.iter().filter(|c| c.cancelled).count(),
        prompts: calls
            .iter()
            .map(|c| {
                (
                    c.at.duration_since(start).as_millis() as u64,
                    c.prompt.clone(),
                )
            })
            .collect(),
        tokens_offered: calls.iter().map(|c| c.tokens).sum(),
        tokens_streamed: calls.iter().map(|c| c.delivered).sum(),
        virtual_ms: Instant::now().duration_since(start).as_millis() as u64,
        cloud: brains.cloud,
    }
}
