//! `style-bench`: what the writing-language and writing-style settings change, measured on the real local
//! model (Task 12 test brief). Three parts:
//!
//! A. Ghost Text language on short Hungarian and English openings (where small models drift, like the
//!    Vietnamese suggestion after "Hát."): no preference, preferred languages (filter), and filter + hint.
//! B. Selection Actions "Rephrase" on English sentences: tone, US/UK spelling and style notes.
//! C. Rewrites of Hungarian text: with and without preferred languages, and a per-app mode answering in English.
//!
//! ```text
//! cargo run --release --bin style-bench            # model: qwen2.5-1.5b-instruct-q4km
//! ```
//! Prints tables and writes JSON to bench-results/local/. Curated numbers go to docs/PERFORMANCE.md.

use bob_lib::brain::registry::Role;
use bob_lib::brain::secrets::MemoryStore;
use bob_lib::brain::service::BrainService;
use bob_lib::brain::{models, CompletionRequest};
use bob_lib::context::languages::{detect, filter_chip};
use bob_lib::context::typography::Spelling;
use bob_lib::context::{AppMode, ContextHub, ContextSettings, GhostLayers, Tone};
use bob_lib::ghost::chip;
use bob_lib::protocol::NativeEvent;
use bob_lib::selection::{actions, engine as sel};
use futures_util::StreamExt;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

const GHOST_HU: &[&str] = &[
    "Hát.",
    "Szia!",
    "Köszönöm a",
    "Nem tudom, hogy",
    "Holnap reggel",
    "Jó reggelt! Ma",
    "Bocs, de",
    "Megyek a boltba, kell",
];
const GHOST_EN: &[&str] = &[
    "Hi!",
    "Thanks for the",
    "Tomorrow I",
    "Sorry, but",
    "See you",
];

const STYLE: &[&str] = &[
    "I think we should organize the meeting about the new color scheme next week.",
    "Can you send me the report when you have a minute, it is kind of urgent.",
    "The team did a great job on the launch and the customers really like it.",
    "We are going to analyze the results and then decide what our favorite option is.",
    "I can't make it to the dinner tonight because something came up at work.",
    "Please check the center of the page, the logo looks a bit off.",
];
const HU_TEXT: &[&str] = &[
    "Szia, holnap nem tudok menni a megbeszélésre, mert beteg vagyok.",
    "Köszönöm a gyors választ, a jövő héten átnézem a dokumentumot.",
    "Elnézést a késésért, a vonat negyven percet késett ma reggel.",
    "A projekt jól halad, de még két hét kell a teszteléshez.",
];

#[derive(Serialize, Default, Clone)]
struct GhostRow {
    condition: String,
    prompts: usize,
    raw_in_expected_language: usize,
    raw_wrong_language: usize,
    chips_shown: usize,
    chips_wrong_language: usize,
    p50_ms: u64,
}

#[derive(Serialize, Default, Clone)]
struct StyleRow {
    condition: String,
    outputs: usize,
    avg_words: f64,
    contractions: usize,
    exclamations: usize,
    uk_spellings: usize,
    us_spellings: usize,
    emoji: usize,
    within_12_words: usize,
    p50_ms: u64,
}

#[derive(Serialize, Default, Clone)]
struct LangRow {
    condition: String,
    outputs: usize,
    hungarian: usize,
    english: usize,
    other: usize,
}

fn p50(mut v: Vec<u64>) -> u64 {
    v.sort_unstable();
    v.get(v.len() / 2).copied().unwrap_or(0)
}

const UK: &[&str] = &[
    "organise",
    "colour",
    "analyse",
    "favourite",
    "centre",
    "realise",
];
const US: &[&str] = &[
    "organize", "color", "analyze", "favorite", "center", "realize",
];

fn count_any(text: &str, words: &[&str]) -> usize {
    let t = text.to_lowercase();
    words.iter().filter(|w| t.contains(*w)).count()
}

async fn raw_completion(svc: &BrainService, prompt: String) -> (String, u64) {
    let brain = svc.resolve_role(Role::Ghost).await.expect("ghost brain");
    let t0 = Instant::now();
    let mut out = String::new();
    if let Ok(mut s) = brain
        .complete_stream(&CompletionRequest::ghost(prompt))
        .await
    {
        while let Some(Ok(tok)) = s.next().await {
            out.push_str(&tok);
            if out.split_whitespace().count() > 8 {
                break;
            }
        }
    }
    (out, t0.elapsed().as_millis() as u64)
}

#[tokio::main]
async fn main() {
    let home = std::env::var("HOME").unwrap_or_default();
    let data = PathBuf::from(home).join("Library/Application Support/dev.bob.assistant");
    let runtime = std::env::temp_dir().join(format!("bob-style-{}", std::process::id()));
    let svc = Arc::new(BrainService::new(
        std::env::temp_dir().join(format!("bob-style-cfg-{}", std::process::id())),
        data,
        runtime.clone(),
        Box::new(MemoryStore::default()),
    ));
    let model = std::env::args()
        .nth(1)
        .unwrap_or_else(|| models::CATALOG[0].id.to_string());
    svc.start_builtin(&model, Duration::from_secs(180))
        .await
        .expect("start model");
    svc.builtin_ready(Duration::from_secs(240))
        .await
        .expect("model ready");
    println!("model: {model}\n");

    // ---- A. Ghost Text language
    let prefs = vec!["hu".to_string(), "en".to_string()];
    let mut ghost_rows = vec![];
    for (name, languages, hint) in [
        ("no preference", vec![], false),
        ("preferred hu+en (filter)", prefs.clone(), false),
        ("preferred hu+en + hint", prefs.clone(), true),
    ] {
        let ctx = ContextHub::in_memory(
            ContextSettings {
                languages: languages.clone(),
                ghost: GhostLayers {
                    language_hint: hint,
                    ..Default::default()
                },
                ..Default::default()
            },
            "",
        );
        let mut row = GhostRow {
            condition: name.into(),
            ..Default::default()
        };
        let mut ms = vec![];
        for (prompts, expected) in [(GHOST_HU, "hu"), (GHOST_EN, "en")] {
            for p in prompts.iter() {
                let preamble = ctx.ghost_preamble(&[]);
                let (raw, t) = raw_completion(&svc, format!("{preamble}{p}")).await;
                ms.push(t);
                row.prompts += 1;
                match detect(&format!("{p}{raw}")) {
                    Some(l) if l == expected => row.raw_in_expected_language += 1,
                    Some(_) if detect(&raw).is_some_and(|l| l != expected) => {
                        row.raw_wrong_language += 1;
                        println!("  [{name}] raw drift after {p:?}: {:?}", raw.trim());
                    }
                    _ => {}
                }
                let shown = chip::polish(&raw, p).and_then(|c| filter_chip(&c, p, &languages));
                if let Some(c) = shown {
                    row.chips_shown += 1;
                    if detect(&c).is_some_and(|l| l != expected && !(expected == "hu" && l == "en"))
                        && !c.is_ascii()
                    {
                        row.chips_wrong_language += 1;
                        println!("  [{name}] wrong-language chip after {p:?}: {c:?}");
                    }
                }
            }
        }
        row.p50_ms = p50(ms);
        ghost_rows.push(row);
    }
    println!(
        "A. Ghost Text language ({} prompts: {} Hungarian, {} English)",
        GHOST_HU.len() + GHOST_EN.len(),
        GHOST_HU.len(),
        GHOST_EN.len()
    );
    println!(
        "{:<28} {:>14} {:>12} {:>8} {:>14} {:>7}",
        "condition", "raw expected", "raw wrong", "chips", "chips wrong", "p50 ms"
    );
    for r in &ghost_rows {
        println!(
            "{:<28} {:>14} {:>12} {:>8} {:>14} {:>7}",
            r.condition,
            r.raw_in_expected_language,
            r.raw_wrong_language,
            r.chips_shown,
            r.chips_wrong_language,
            r.p50_ms
        );
    }

    // ---- B. Writing style on Rephrase
    let rephrase = actions::menu(&Default::default(), "")
        .into_iter()
        .find(|a| a.label == "Rephrase")
        .unwrap();
    let mut style_rows = vec![];
    let conditions: Vec<(&str, ContextSettings)> = vec![
        ("natural", ContextSettings::default()),
        (
            "friendly",
            ContextSettings {
                tone: Tone::Friendly,
                ..Default::default()
            },
        ),
        (
            "professional",
            ContextSettings {
                tone: Tone::Professional,
                ..Default::default()
            },
        ),
        (
            "concise",
            ContextSettings {
                tone: Tone::Concise,
                ..Default::default()
            },
        ),
        (
            "spelling US",
            ContextSettings {
                spelling: Spelling::Us,
                ..Default::default()
            },
        ),
        (
            "spelling UK",
            ContextSettings {
                spelling: Spelling::Uk,
                ..Default::default()
            },
        ),
        (
            "notes: no emoji, max 12 words",
            ContextSettings {
                style_notes: "No emoji. At most 12 words.".into(),
                ..Default::default()
            },
        ),
    ];
    for (name, settings) in conditions {
        let ctx = ContextHub::in_memory(settings, "");
        let mut row = StyleRow {
            condition: name.into(),
            ..Default::default()
        };
        let (mut words, mut ms) = (0usize, vec![]);
        for s in STYLE {
            let t0 = Instant::now();
            let Ok(out) = sel::rewrite(&svc, &ctx, &rephrase, s).await else {
                continue;
            };
            ms.push(t0.elapsed().as_millis() as u64);
            row.outputs += 1;
            let n = out.split_whitespace().count();
            words += n;
            row.within_12_words += usize::from(n <= 12);
            row.contractions += out.matches('\'').count() + out.matches('’').count();
            row.exclamations += out.matches('!').count();
            row.uk_spellings += count_any(&out, UK);
            row.us_spellings += count_any(&out, US);
            row.emoji += out.chars().filter(|c| (*c as u32) >= 0x1F300).count();
        }
        row.avg_words = (words as f64 / row.outputs.max(1) as f64 * 10.0).round() / 10.0;
        row.p50_ms = p50(ms);
        style_rows.push(row);
    }
    println!(
        "\nB. Rephrase with writing style ({} sentences each)",
        STYLE.len()
    );
    println!(
        "{:<32} {:>6} {:>9} {:>8} {:>7} {:>4} {:>4} {:>6} {:>7}",
        "condition", "words", "contract.", "excl.", "<=12w", "UK", "US", "emoji", "p50 ms"
    );
    for r in &style_rows {
        println!(
            "{:<32} {:>6} {:>9} {:>8} {:>7} {:>4} {:>4} {:>6} {:>7}",
            r.condition,
            r.avg_words,
            r.contractions,
            r.exclamations,
            r.within_12_words,
            r.uk_spellings,
            r.us_spellings,
            r.emoji,
            r.p50_ms
        );
    }

    // ---- C. Language rules on rewrites of Hungarian text
    let fix = actions::menu(&Default::default(), "")
        .into_iter()
        .next()
        .unwrap();
    let mut lang_rows = vec![];
    let slack = "com.tinyspeck.slackmacgap";
    for (name, settings, in_slack) in [
        ("no preference", ContextSettings::default(), false),
        (
            "preferred hu+en",
            ContextSettings {
                languages: prefs.clone(),
                ..Default::default()
            },
            false,
        ),
        (
            "app mode: answer in English",
            ContextSettings {
                app_modes: vec![AppMode {
                    bundle_id: slack.into(),
                    name: "Slack".into(),
                    language: "en".into(),
                    ..Default::default()
                }],
                ..Default::default()
            },
            true,
        ),
    ] {
        let ctx = ContextHub::in_memory(settings.sanitized(), "");
        if in_slack {
            ctx.observe(&NativeEvent::Focus {
                app_bundle_id: slack.into(),
                app_name: "Slack".into(),
                secure: false,
                editable: true,
            });
        }
        let mut row = LangRow {
            condition: name.into(),
            ..Default::default()
        };
        for t in HU_TEXT {
            for action in [&fix, &rephrase] {
                let Ok(out) = sel::rewrite(&svc, &ctx, action, t).await else {
                    continue;
                };
                row.outputs += 1;
                match detect(&out) {
                    Some("hu") => row.hungarian += 1,
                    Some("en") => row.english += 1,
                    _ => row.other += 1,
                }
            }
        }
        lang_rows.push(row);
    }
    println!(
        "\nC. Fix grammar and Rephrase on Hungarian text ({} texts x 2 actions)",
        HU_TEXT.len()
    );
    println!(
        "{:<30} {:>8} {:>10} {:>8} {:>6}",
        "condition", "outputs", "Hungarian", "English", "other"
    );
    for r in &lang_rows {
        println!(
            "{:<30} {:>8} {:>10} {:>8} {:>6}",
            r.condition, r.outputs, r.hungarian, r.english, r.other
        );
    }

    svc.stop_builtin().await;
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../bench-results/local");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(format!(
        "style-bench-{}.json",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    ));
    let json = serde_json::json!({"model": model, "ghost": ghost_rows, "style": style_rows, "languages": lang_rows});
    let _ = std::fs::write(
        &path,
        serde_json::to_string_pretty(&json).unwrap_or_default(),
    );
    println!("\nwrote {}", path.display());
    let _ = std::fs::remove_dir_all(&runtime);
}
