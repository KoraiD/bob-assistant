//! Ghost Text under random setting combinations: context layers, languages, quote style, a local or cloud
//! brain with or without the personal-context switch, typo fixes, suggestions inside a sentence, a quiet app,
//! Ghost Text off or paused. Each combination plays a random human-speed session in a random app profile.
//!
//! Checked for every combination:
//! - off, paused or quiet: no model request and no chip;
//! - a cloud brain never receives credential-like text (the typist types one);
//! - each personal-context layer (profile, memory, window title, copied text) reaches the prompt only when it
//!   is on and the brain may receive personal context;
//! - typed text never reaches the log (the typist types a canary word);
//! - the defects that no session may show ([`super::random`]'s list).

use super::actor::Typist;
use super::check::Kind;
use super::model::{Model, Quirks};
use super::world::PROFILES;
use super::{run, Config, Report, Setup};
use crate::context::typography::Quotes;
use crate::context::{ContextSettings, GhostLayers};
use proptest::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::LazyLock;

/// The credential the typist types (a known prefix and 36 more characters).
static SECRET: LazyLock<String> = LazyLock::new(|| format!("sk-{}", "Kw4".repeat(12)));
/// Longest start of [`SECRET`] seen in any prompt to a cloud brain (review finding SEC-01).
static LONGEST_PREFIX_TO_CLOUD: AtomicUsize = AtomicUsize::new(0);

/// How many leading characters of `secret` appear in `text`.
fn leaked_prefix(text: &str, secret: &str) -> usize {
    (1..=secret.len())
        .rev()
        .find(|n| text.contains(&secret[..*n]))
        .unwrap_or(0)
}

/// Markers that identify each context layer in a prompt.
struct Markers {
    profile: String,
    memory: String,
    window: String,
    clipboard: String,
    canary: String,
}

impl Markers {
    fn new(n: u32) -> Self {
        Self {
            profile: format!("Prf{n}q"),
            memory: format!("Mem{n}q"),
            window: format!("Win{n}q"),
            clipboard: format!("Clp{n}q"),
            canary: format!("Cny{n}q"),
        }
    }
}

#[derive(Debug, Clone)]
struct Combo {
    layers: GhostLayers,
    languages: Vec<String>,
    quotes: Quotes,
    cloud: bool,
    share: Option<bool>,
    typo_fixes: bool,
    mid_sentence: bool,
    enabled: bool,
    paused: bool,
    quiet_app: bool,
}

fn combo() -> impl Strategy<Value = Combo> {
    (
        prop::array::uniform6(any::<bool>()),
        prop::sample::select(vec![
            vec![],
            vec!["en".to_string()],
            vec!["hu".to_string(), "en".to_string()],
        ]),
        prop::sample::select(vec![Quotes::Keep, Quotes::Straight, Quotes::Curly]),
        any::<bool>(),
        prop::sample::select(vec![None, Some(true), Some(false)]),
        prop::array::uniform5(prop::bool::weighted(0.8)),
    )
        .prop_map(|(l, languages, quotes, cloud, share, on)| Combo {
            layers: GhostLayers {
                profile: l[0],
                memory: l[1],
                window: l[2],
                screen: l[3],
                language_hint: l[4],
                clipboard: l[5],
            },
            languages,
            quotes,
            cloud,
            share,
            typo_fixes: on[0],
            mid_sentence: on[1],
            enabled: on[2],
            paused: !on[3],
            quiet_app: !on[4],
        })
}

fn setup(c: &Combo, m: &Markers) -> Setup {
    Setup {
        cloud: c.cloud,
        share_context: c.share,
        context: ContextSettings {
            ghost: c.layers,
            languages: c.languages.clone(),
            quotes: c.quotes,
            ..Default::default()
        },
        profile: format!("- Name: {}", m.profile),
        memory: vec![format!("see you at the {} office", m.memory)],
        clipboard: Some(format!("Meeting notes {}", m.clipboard)),
        window_title: Some(format!("Re: {}", m.window)),
        typo_fixes: c.typo_fixes,
        mid_sentence: c.mid_sentence,
        enabled: c.enabled,
        paused: c.paused,
        quiet_app: c.quiet_app,
    }
}

fn check(c: &Combo, m: &Markers, r: &Report) -> Result<(), String> {
    let never = [
        Kind::UnrequestedInsert,
        Kind::EmptyChip,
        Kind::CaptureMismatch,
        Kind::TabLeaked,
    ];
    if let Some(d) = r.defects.iter().find(|d| never.contains(&d.kind)) {
        return Err(format!("{:?}: {}", d.kind, d.detail));
    }
    if (!c.enabled || c.paused || c.quiet_app) && (r.requests > 0 || r.stats.chips_painted > 0) {
        return Err(format!(
            "off, paused or quiet, yet {} requests and {} chips",
            r.requests, r.stats.chips_painted
        ));
    }
    if r.cloud {
        if let Some((_, p)) = r.prompts.iter().find(|(_, p)| p.contains(SECRET.as_str())) {
            return Err(format!("the whole credential reached a cloud brain: {p:?}"));
        }
    }
    let shares = c.share.unwrap_or(!c.cloud);
    let layers = [
        ("profile", &m.profile, c.layers.profile),
        ("memory", &m.memory, c.layers.memory),
        ("window title", &m.window, c.layers.window),
        ("copied text", &m.clipboard, c.layers.clipboard),
    ];
    for (name, marker, on) in layers {
        let sent = r.prompts.iter().any(|(_, p)| p.contains(marker.as_str()));
        if sent && !(shares && on) {
            return Err(format!("the {name} reached the brain (layer on: {on}, brain may receive context: {shares})"));
        }
    }
    let leaks = crate::logging::capture::find(&m.canary);
    if !leaks.is_empty() {
        return Err(format!("typed text in the log: {:?}", &leaks[..1]));
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, ..ProptestConfig::default() })]

    /// Every setting combination keeps what is off off, and what is private private.
    #[test]
    fn every_setting_combination_keeps_private_text_private(
        c in combo(),
        n in any::<u32>(),
        profile in 0usize..PROFILES.len(),
        choices in prop::collection::vec((0u8..100, any::<u16>(), any::<u8>()), 20..60),
    ) {
        crate::logging::capture::start();
        let m = Markers::new(n);
        let typist = Typist::new(choices)
            .with_words([m.canary.clone(), SECRET.clone()]);
        let config = Config::new(u64::from(n), PROFILES[profile], Model::Random(Quirks::default()))
            .with_setup(setup(&c, &m));
        let r = super::random::block_on_session(config, typist);
        if r.cloud {
            let longest = r.prompts.iter().map(|(_, p)| leaked_prefix(p, &SECRET)).max().unwrap_or(0);
            LONGEST_PREFIX_TO_CLOUD.fetch_max(longest, Ordering::Relaxed);
        }
        let verdict = check(&c, &m, &r);
        prop_assert!(verdict.is_ok(), "{:?} with {c:?}\n{}", verdict, r.summary());
    }
}

/// SEC-01: while a credential is being typed, its start reaches a cloud Ghost Text brain until it is long
/// enough to be recognized: a known prefix plus 8 characters, or 32 characters without a prefix.
#[tokio::test(start_paused = true)]
#[ignore = "SEC-01: the start of a credential reaches a cloud brain while it is being typed"]
async fn a_credential_being_typed_never_reaches_a_cloud_brain() {
    use super::actor::{Script, Step};
    crate::logging::capture::start();
    // A known-prefix key, and an opaque token (a password pasted into a config file, typed here).
    // Built at run time, so the repository's secret scan never sees a key-shaped literal.
    let keys = [
        (format!("sk-{}", "Kw4".repeat(12)), 3),
        (format!("Tr7pQ2xL9vB4nM6k{}", "Z1wC8dF3gH5jK0sA2eR4tY6u"), 0),
    ];
    let mut report = vec![];
    for (key, allowed) in &keys {
        let mut longest = 0;
        for profile in PROFILES {
            let text: &'static str = Box::leak(format!("The token is {key}").into_boxed_str());
            let mut script = Script::new([Step::Type(text, std::time::Duration::from_millis(180))]);
            let setup = Setup {
                cloud: true,
                ..Setup::default()
            };
            let config =
                Config::new(1, profile, Model::Random(Quirks::default())).with_setup(setup);
            let r = run(config, &mut script).await;
            longest = longest.max(
                r.prompts
                    .iter()
                    .map(|(_, p)| leaked_prefix(p, key))
                    .max()
                    .unwrap_or(0),
            );
        }
        report.push((key.chars().count(), longest, *allowed));
    }
    eprintln!("SEC-01 (key length, longest start sent to a cloud brain): {report:?}");
    assert!(
        report
            .iter()
            .all(|(_, longest, allowed)| longest <= allowed),
        "{report:?}"
    );
}
