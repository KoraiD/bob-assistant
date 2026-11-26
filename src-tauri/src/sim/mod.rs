//! A simulator for Ghost Text (test builds only): the real engine against a fake app, a fake Swift helper, a
//! scripted model and a scripted or random user, all on tokio's virtual clock.
//!
//! Why: the bugs users report ("an extra space", "it inserted the word twice") come from the order in which
//! keys, Accessibility reports, model tokens and BOB's own insertions arrive. Real-time tests hit only a few
//! orderings and cannot replay them. Here one seed fixes every delay, so a failing session replays exactly,
//! and a ten-minute session runs in milliseconds.
//!
//! Pieces: [`model`] streams answers like llama.cpp, [`world`] models the app and the helper with the helper's
//! real timings, [`actor`] is the user, [`check`] names the defects. [`run`] plays one session and returns a
//! [`Report`] with the timeline.
//!
//! Sessions need a paused clock: `#[tokio::test(start_paused = true)]`.

pub mod actor;
pub mod check;
pub mod model;
pub mod world;

mod tests;

use crate::brain::client::scripted;
use crate::brain::registry::Role;
use crate::brain::secrets::MemoryStore;
use crate::brain::service::BrainService;
use crate::brain::{BrainConfig, Capabilities, ProviderKind};
use crate::context::ContextHub;
use crate::ghost::engine::{spawn_ctx, EngineConfig, Sink};
use crate::protocol::CoreCommand;
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
    pub start_text: &'static str,
    /// Longest session, in virtual time.
    pub max: Duration,
}

impl Config {
    pub fn new(seed: u64, profile: Profile, model: Model) -> Self {
        Self {
            seed,
            profile,
            model,
            start_text: "",
            max: Duration::from_secs(600),
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
    /// Tokens the model would have generated, and tokens actually streamed before cancellations.
    pub tokens_offered: usize,
    pub tokens_streamed: usize,
    pub virtual_ms: u64,
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

/// Play one session to its end and report it. Needs a paused tokio clock (current-thread runtime).
pub async fn run(config: Config, actor: &mut dyn Actor) -> Report {
    let n = SESSION.fetch_add(1, Ordering::SeqCst);
    // Never contacted: the scripted brain answers for this endpoint.
    let endpoint = format!("http://127.0.0.1:9/sim-{}-{n}", std::process::id());
    let calls: Calls = Default::default();
    let _script = scripted::register(
        &endpoint,
        Some(config.model.clone().script(config.seed, calls.clone())),
        None,
    );
    let root = std::env::temp_dir().join(format!("bob-sim-{}-{n}", std::process::id()));
    let brains = Arc::new(BrainService::new(
        root.join("c"),
        root.join("d"),
        root.join("r"),
        Box::new(MemoryStore::default()),
    ));
    brains
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
                // The prompt is exactly the typed text, so the model sees what the user sees.
                share_context: Some(false),
            },
            None,
        )
        .await
        .expect("register the simulated brain");
    brains
        .set_role(Role::Ghost, Some("sim"))
        .await
        .expect("assign Ghost Text");

    let (tx, mut rx) = mpsc::unbounded_channel();
    let context = Arc::new(ContextHub::in_memory(Default::default(), ""));
    let ghost = spawn_ctx(
        brains,
        Arc::new(ChannelSink(tx)),
        EngineConfig::default(),
        None,
        context,
    );
    let rng = rand::rngs::StdRng::seed_from_u64(config.seed);
    let mut world = World::new(config.profile, rng, ghost, calls.clone(), config.start_text);
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

    let _ = std::fs::remove_dir_all(&root);
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
        tokens_offered: calls.iter().map(|c| c.tokens).sum(),
        tokens_streamed: calls.iter().map(|c| c.delivered).sum(),
        virtual_ms: Instant::now().duration_since(start).as_millis() as u64,
    }
}
