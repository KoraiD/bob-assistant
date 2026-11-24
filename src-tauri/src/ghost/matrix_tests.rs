//! Ghost Text across setting combinations (Task 12 test brief): every context layer, language, quote style
//! and sharing permission, every quiet zone, plus a randomized typing session. Each case runs the real engine
//! against a local test server and checks what reached the "model" and what reached the screen.

use super::engine::{spawn_ctx, EngineConfig, GhostHandle, Sink};
use crate::brain::registry::Role;
use crate::brain::secrets::MemoryStore;
use crate::brain::service::BrainService;
use crate::brain::testserver::{Response, TestServer};
use crate::brain::{BrainConfig, Capabilities, ProviderKind};
use crate::context::typography::Quotes;
use crate::context::{ContextHub, ContextSettings, GhostLayers};
use crate::protocol::{CoreCommand, Key, NativeEvent, Rect};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Default)]
struct Recorder(Mutex<Vec<(Instant, CoreCommand)>>);
impl Sink for Recorder {
    fn send(&self, cmd: CoreCommand) {
        self.0.lock().unwrap().push((Instant::now(), cmd));
    }
}
impl Recorder {
    fn take(&self) -> Vec<(Instant, CoreCommand)> {
        std::mem::take(&mut *self.0.lock().unwrap())
    }
}

const CARET: Rect = Rect {
    x: 100.0,
    y: 200.0,
    width: 0.0,
    height: 18.0,
};
const APP: &str = "com.apple.mail";

fn text(before: &str, secure: bool, selection_len: u32) -> NativeEvent {
    NativeEvent::TextState {
        app_bundle_id: APP.into(),
        secure,
        before_caret: before.into(),
        selection_len,
        caret: Some(CARET),
    }
}

struct Rig {
    ghost: GhostHandle,
    rec: Arc<Recorder>,
    server: TestServer,
    svc: Arc<BrainService>,
    ctx: Arc<ContextHub>,
}

async fn rig(tokens: Vec<String>) -> Rig {
    let server = TestServer::start(move |_| {
        let events: Vec<String> = tokens
            .iter()
            .map(|t| serde_json::json!({"choices":[{"text": t}]}).to_string())
            .collect();
        let refs: Vec<&str> = events.iter().map(String::as_str).collect();
        Response::sse(&refs, Duration::ZERO).chunk_after(Duration::ZERO, "data: [DONE]\n\n")
    })
    .await;
    let root = std::env::temp_dir().join(format!("bob-matrix-{}", rand::random::<u32>()));
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
    let ctx = Arc::new(ContextHub::in_memory(Default::default(), ""));
    let rec = Arc::new(Recorder::default());
    let config = EngineConfig {
        quiet: Duration::from_millis(5),
        ..EngineConfig::default()
    };
    let ghost = spawn_ctx(svc.clone(), rec.clone(), config, None, ctx.clone());
    ghost.remember("as soon as I can");
    ghost.native(NativeEvent::Focus {
        app_bundle_id: APP.into(),
        app_name: "Mail".into(),
        secure: false,
        editable: true,
    });
    ctx.observe(&NativeEvent::Focus {
        app_bundle_id: APP.into(),
        app_name: "Mail".into(),
        secure: false,
        editable: true,
    });
    ctx.observe(&NativeEvent::ContextState {
        app_bundle_id: APP.into(),
        window_title: Some("Re: Dinner".into()),
        after_caret: String::new(),
    });
    Rig {
        ghost,
        rec,
        server,
        svc,
        ctx,
    }
}

/// Wait until a chip appeared and then nothing new arrived for 40 ms (the stream finished).
async fn wait_for_chip(rec: &Recorder, ms: u64) -> Vec<(Instant, CoreCommand)> {
    let until = Instant::now() + Duration::from_millis(ms);
    let mut last_len = 0;
    let mut quiet_since = Instant::now();
    loop {
        tokio::time::sleep(Duration::from_millis(5)).await;
        let (got, len) = {
            let v = rec.0.lock().unwrap();
            (
                v.iter()
                    .any(|(_, c)| matches!(c, CoreCommand::ShowChip { .. })),
                v.len(),
            )
        };
        if len != last_len {
            last_len = len;
            quiet_since = Instant::now();
        }
        if (got && quiet_since.elapsed() > Duration::from_millis(40)) || Instant::now() > until {
            return rec.take();
        }
    }
}

fn chips(cmds: &[(Instant, CoreCommand)]) -> Vec<String> {
    cmds.iter()
        .filter_map(|(_, c)| match c {
            CoreCommand::ShowChip { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

/// 16 layer combinations x 2 language settings x 3 quote styles x 2 sharing permissions = 192 cases. For each:
/// the prompt carries exactly the allowed notes, ends with the typed text, never carries the copied
/// secret, and the chip uses the chosen quote style. Also records engine overhead (keystroke to chip).
#[tokio::test(flavor = "multi_thread")]
async fn every_setting_combination_sends_only_what_is_allowed() {
    let r = rig(vec![" it's".into(), " \"fine\"".into(), " today.".into()]).await;
    let typed = "Hello Anna, I think";
    let secret = format!("sk-{}", "Ab1".repeat(12));
    let mut latencies = vec![];
    let mut cases = 0;
    for bits in 0..16u8 {
        let (memory, window, hint, clipboard) =
            (bits & 1 != 0, bits & 2 != 0, bits & 4 != 0, bits & 8 != 0);
        for langs in [vec![], vec!["hu".to_string(), "en".to_string()]] {
            for quotes in [Quotes::Keep, Quotes::Straight, Quotes::Curly] {
                for share in [true, false] {
                    cases += 1;
                    r.svc.set_share_context("local", share).await.unwrap();
                    r.ctx
                        .set_settings(ContextSettings {
                            ghost: GhostLayers {
                                memory,
                                window,
                                language_hint: hint,
                                clipboard,
                                ..Default::default()
                            },
                            languages: langs.clone(),
                            quotes,
                            ..Default::default()
                        })
                        .unwrap();
                    r.ctx.observe(&NativeEvent::ClipboardText {
                        text: format!("Dinner at 8, key {secret}"),
                    });
                    // A different text each time so nothing is cached or deduplicated.
                    let before = format!("{typed} {cases}");
                    let start = Instant::now();
                    r.ghost.native(text(&before, false, 0));
                    let cmds = wait_for_chip(&r.rec, 800).await;
                    let shown = chips(&cmds);
                    let label = format!(
                        "memory={memory} window={window} hint={hint} clipboard={clipboard} langs={langs:?} quotes={quotes:?} share={share}"
                    );
                    assert!(!shown.is_empty(), "no chip for {label}");
                    if let Some((t, _)) = cmds
                        .iter()
                        .find(|(_, c)| matches!(c, CoreCommand::ShowChip { .. }))
                    {
                        latencies.push(t.duration_since(start).as_millis() as u64);
                    }
                    let prompt = r.server.last_request().json()["prompt"]
                        .as_str()
                        .unwrap()
                        .to_string();
                    assert!(prompt.ends_with(&before), "{label}: {prompt}");
                    assert!(!prompt.contains("Ab1Ab1"), "{label}: secret leaked");
                    let effective_window = memory && window; // owner rule
                    assert_eq!(
                        prompt.contains("Phrases I often use"),
                        share && memory,
                        "{label}"
                    );
                    assert_eq!(
                        prompt.contains("Writing in Mail"),
                        share && effective_window,
                        "{label}"
                    );
                    assert_eq!(
                        prompt.contains("I write in Hungarian and English"),
                        share && hint && !langs.is_empty(),
                        "{label}"
                    );
                    assert_eq!(
                        prompt.contains("I just copied"),
                        share && clipboard,
                        "{label}"
                    );
                    let last = shown.last().unwrap();
                    match quotes {
                        Quotes::Curly => assert_eq!(last, " it’s “fine” today.", "{label}"),
                        _ => assert_eq!(last, " it's \"fine\" today.", "{label}"),
                    }
                    // Reset for the next case.
                    r.ghost.native(NativeEvent::Key { key: Key::Escape });
                    tokio::time::sleep(Duration::from_millis(5)).await;
                    r.rec.take();
                }
            }
        }
    }
    assert_eq!(cases, 192);
    latencies.sort_unstable();
    let p50 = latencies[latencies.len() / 2];
    let p95 = latencies[latencies.len() * 95 / 100];
    eprintln!("ghost matrix: {cases} combinations, keystroke-to-chip p50 {p50} ms, p95 {p95} ms (5 ms quiet gate, instant server)");
    assert!(p95 < 150, "engine overhead too high: p95 {p95} ms");
}

/// Quiet zones x on/off x pause: the model is only asked when every gate is open.
#[tokio::test(flavor = "multi_thread")]
async fn every_quiet_zone_combination_keeps_text_away_from_the_model() {
    let r = rig(vec![" world.".into()]).await;
    let mut n = 0;
    for bits in 0..32u8 {
        let (enabled, paused, quiet_app, secure, selected) = (
            bits & 1 != 0,
            bits & 2 != 0,
            bits & 4 != 0,
            bits & 8 != 0,
            bits & 16 != 0,
        );
        r.ghost.set_enabled(enabled);
        r.ghost.set_paused(paused);
        r.ghost.set_quiet(APP, "Mail", quiet_app);
        let before_count = r.server.request_count();
        n += 1;
        r.ghost.native(text(
            &format!("Hello {n}"),
            secure,
            if selected { 3 } else { 0 },
        ));
        tokio::time::sleep(Duration::from_millis(60)).await;
        let asked = r.server.request_count() > before_count;
        let open = enabled && !paused && !quiet_app && !secure && !selected;
        assert_eq!(
            asked, open,
            "enabled={enabled} paused={paused} quiet={quiet_app} secure={secure} selected={selected}"
        );
        let cmds = r.rec.take();
        if !open {
            assert!(chips(&cmds).is_empty());
        }
    }
}

/// Preferred languages decide whether a German suggestion survives. Known limit of a letter-based filter:
/// Hungarian also uses "ü", so for a Hungarian and English writer "für" passes (docs/PERFORMANCE.md).
#[tokio::test(flavor = "multi_thread")]
async fn preferred_languages_filter_suggestions() {
    let r = rig(vec![" für".into(), " dich.".into()]).await;
    let cases: [(&[&str], bool); 5] = [
        (&[], true),
        (&["de"], true),
        (&["en"], false),
        (&["pl", "en"], false),
        (&["hu", "en"], true),
    ];
    for (i, (langs, kept)) in cases.into_iter().enumerate() {
        r.ctx
            .set_settings(ContextSettings {
                languages: langs.iter().map(|s| s.to_string()).collect(),
                ..Default::default()
            })
            .unwrap();
        r.ghost.native(text(&format!("Das ist {i}"), false, 0));
        let cmds = wait_for_chip(&r.rec, 300).await;
        let visible_at_end = cmds
            .iter()
            .rev()
            .find_map(|(_, c)| match c {
                CoreCommand::ShowChip { text, .. } => Some(Some(text.clone())),
                CoreCommand::HideChip => Some(None),
                _ => None,
            })
            .flatten();
        assert_eq!(
            visible_at_end.is_some_and(|c| c.contains("dich")),
            kept,
            "{langs:?}: {cmds:?}"
        );
        r.ghost.native(NativeEvent::Key { key: Key::Escape });
        tokio::time::sleep(Duration::from_millis(5)).await;
        r.rec.take();
    }
}

/// A randomized typing session (seeded, so failures reproduce): typing through, accepting, dismissing,
/// stale and no-break-space events, focus changes. Invariants after every step: key capture is on exactly
/// while a chip is visible, chips are never empty, and BOB inserts text only while a chip is visible.
#[tokio::test(flavor = "multi_thread")]
async fn randomized_sessions_keep_chip_and_key_capture_consistent() {
    use rand::{Rng, SeedableRng};
    let r = rig(vec![" lazy".into(), " brown".into(), " dog.".into()]).await;
    for seed in 0..6u64 {
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        let mut typed = String::from("The quick fox jumps over the");
        let (mut visible, mut capturing) = (false, false);
        for _ in 0..25 {
            match rng.random_range(0..7) {
                0 | 1 => {
                    let c = ['a', ' ', 'l', 'z', 'y', '\u{a0}'][rng.random_range(0..6)];
                    typed.push(c);
                    r.ghost.native(text(&typed, false, 0));
                }
                2 => r.ghost.native(NativeEvent::Key { key: Key::Tab }),
                3 => r.ghost.native(NativeEvent::Key { key: Key::ShiftTab }),
                4 => r.ghost.native(NativeEvent::Key { key: Key::Escape }),
                5 => {
                    // A stale or repeated event.
                    r.ghost.native(text(&typed, false, 0));
                }
                _ => {
                    r.ghost.native(NativeEvent::Focus {
                        app_bundle_id: APP.into(),
                        app_name: "Mail".into(),
                        secure: false,
                        editable: true,
                    });
                    r.ghost.native(text(&typed, false, 0));
                }
            }
            tokio::time::sleep(Duration::from_millis(rng.random_range(2..40))).await;
            for (_, c) in r.rec.take() {
                match c {
                    CoreCommand::ShowChip { text, .. } => {
                        assert!(!text.trim().is_empty(), "seed {seed}: empty chip");
                        visible = true;
                    }
                    CoreCommand::HideChip => visible = false,
                    CoreCommand::CaptureKeys { active } => capturing = active,
                    CoreCommand::InsertText { text } => {
                        assert!(
                            visible,
                            "seed {seed}: inserted {text:?} without a visible chip"
                        );
                        typed.push_str(&text);
                    }
                    _ => {}
                }
            }
            assert_eq!(
                visible, capturing,
                "seed {seed}: capture must follow the chip"
            );
        }
        r.ghost.native(NativeEvent::Key { key: Key::Escape });
        tokio::time::sleep(Duration::from_millis(10)).await;
        r.rec.take();
    }
}
