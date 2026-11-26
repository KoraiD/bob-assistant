//! Random human-speed sessions with the [`Typist`]: a seeded soak that reports defects per 1000 keys for each
//! app profile, and a proptest property that shrinks a failing session to the few decisions that matter.
//!
//! - `cargo test sim::random::soak -- --ignored --nocapture` (`SIM_SESSIONS`, default 300 per profile).
//! - A shrunk failure prints its `choices`; paste them into [`replay`] to see the timeline.

use super::actor::{random_choices, Choice, Typist};
use super::check::Kind;
use super::model::{Model, Quirks};
use super::world::{Profile, PROFILES};
use super::{run, Config, Report};
use proptest::prelude::*;
use rand::SeedableRng;
use std::collections::BTreeMap;

/// Play `choices` as one session (fixed seed for app and model timing, so only the user's decisions vary).
pub async fn replay(profile: Profile, seed: u64, choices: Vec<Choice>) -> Report {
    let config = Config::new(seed, profile, Model::Random(Quirks::default()));
    run(config, &mut Typist::new(choices)).await
}

fn block_on<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .start_paused(true)
        .build()
        .expect("a paused test runtime")
        .block_on(f)
}

/// Defects that no session may ever show, even before the review's fixes. The others (wrong inserts, stale
/// chips, fragments, glued words) are known findings; they join this list as their fixes land (Task 13).
const NEVER: &[Kind] = &[
    Kind::UnrequestedInsert,
    Kind::EmptyChip,
    Kind::CaptureMismatch,
    Kind::TabLeaked,
];

proptest! {
    #![proptest_config(ProptestConfig { cases: 48, ..ProptestConfig::default() })]

    /// Random sessions in every app profile never show the defects in [`NEVER`].
    #[test]
    fn random_sessions_never_insert_unasked_or_show_empty_chips(
        choices in prop::collection::vec((0u8..100, any::<u16>(), any::<u8>()), 20..90),
        profile in 0usize..PROFILES.len(),
    ) {
        let r = block_on(replay(PROFILES[profile], 1, choices));
        let bad: Vec<_> = r.defects.iter().filter(|d| NEVER.contains(&d.kind)).collect();
        prop_assert!(bad.is_empty(), "{}\n{:?}", r.summary(), bad);
    }
}

/// The soak: many seeded sessions per profile, with defect rates and the first seed for each kind.
#[tokio::test(start_paused = true)]
#[ignore = "local soak; prints defect rates per app profile"]
async fn soak() {
    let sessions: u64 = std::env::var("SIM_SESSIONS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(300);
    eprintln!(
        "| Profile | Sessions | Keys | Accepts | Defects per 1000 keys | By kind (first seed) |"
    );
    eprintln!("|---|---|---|---|---|---|");
    for profile in PROFILES {
        let (mut keys, mut accepts, mut defects) = (0u64, 0u64, 0usize);
        let mut kinds: BTreeMap<Kind, (usize, u64)> = BTreeMap::new();
        for seed in 0..sessions {
            let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
            let choices = random_choices(&mut rng, 80);
            let r = replay(profile, seed, choices).await;
            keys += u64::from(r.stats.keys);
            accepts += u64::from(r.stats.accepts);
            defects += r.defects.len();
            for d in &r.defects {
                kinds.entry(d.kind).or_insert((0, seed)).0 += 1;
            }
        }
        let by_kind: Vec<String> = kinds
            .iter()
            .map(|(k, (n, s))| format!("{k:?} {:.1} ({s})", *n as f64 * 1000.0 / keys as f64))
            .collect();
        eprintln!(
            "| {} | {sessions} | {keys} | {accepts} | {:.1} | {} |",
            profile.name,
            defects as f64 * 1000.0 / keys as f64,
            by_kind.join(", ")
        );
    }
}

/// Print one soak session's timeline: `SIM_PROFILE=Electron SIM_SEED=12 cargo test sim::random::show -- --ignored --nocapture`.
#[tokio::test(start_paused = true)]
#[ignore = "local: prints one session"]
async fn show() {
    let name = std::env::var("SIM_PROFILE").unwrap_or_else(|_| "Electron".into());
    let seed: u64 = std::env::var("SIM_SEED")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let profile = PROFILES
        .iter()
        .copied()
        .find(|p| p.name == name)
        .unwrap_or(PROFILES[0]);
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
    let r = replay(profile, seed, random_choices(&mut rng, 80)).await;
    eprintln!("{}\n{}", r.summary(), r.timeline());
}
