//! `ghost-bench`: replay the benchmark corpus through the real Ghost engine and a real local model.
//!
//! ```text
//! make sidecars                                    # llama-server
//! cargo run --release --bin ghost-bench -- \
//!   --model qwen2.5-1.5b-instruct-q4km --model qwen3-1.7b-base-q4km \
//!   --quiet-ms 70 --temperature 0.2 --max-tokens 12
//! ```
//! Models must be downloaded first (Brains tab, or the same files under the app data dir). Results print as
//! a table and are written as JSON to `bench-results/local/` (gitignored); curated numbers go to
//! docs/PERFORMANCE.md. Typing is replayed in real time, so a run takes about 3 minutes per model and setting.
//!
//! `--context none,profile,memory,window,profile+memory+window` measures the personal-context layers
//! (sample profile, seeded memory and per-trace window titles from `bench::corpus`).

use bob_lib::bench::{self, corpus, runner::Harness, Summary};
use bob_lib::brain::secrets::MemoryStore;
use bob_lib::brain::service::BrainService;
use bob_lib::brain::{models, sidecar};
use bob_lib::context::{ContextHub, ContextSettings, GhostLayers};
use bob_lib::ghost::engine::EngineConfig;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

#[derive(Serialize)]
struct Row {
    model: String,
    context: String,
    config: EngineConfig,
    summary: Summary,
    server_rss_mb: Option<u64>,
    scores: Vec<bench::TraceScore>,
}

struct Args {
    models: Vec<String>,
    quiet_ms: Vec<u64>,
    temperatures: Vec<f32>,
    max_tokens: Vec<u32>,
    contexts: Vec<String>,
    traces: usize,
    seed: u64,
    data_dir: PathBuf,
}

fn list<T: std::str::FromStr>(v: &str) -> Vec<T> {
    v.split(',').filter_map(|x| x.trim().parse().ok()).collect()
}

fn parse() -> Args {
    let home = std::env::var("HOME").unwrap_or_default();
    let mut a = Args {
        models: vec![],
        quiet_ms: vec![],
        temperatures: vec![],
        max_tokens: vec![],
        contexts: vec![],
        traces: corpus::SENTENCES.len(),
        seed: 2026,
        data_dir: PathBuf::from(home).join("Library/Application Support/dev.bob.assistant"),
    };
    let mut it = std::env::args().skip(1);
    while let Some(k) = it.next() {
        let v = it.next().unwrap_or_default();
        match k.as_str() {
            "--model" => a.models.push(v),
            "--quiet-ms" => a.quiet_ms.extend(list::<u64>(&v)),
            "--temperature" => a.temperatures.extend(list::<f32>(&v)),
            "--max-tokens" => a.max_tokens.extend(list::<u32>(&v)),
            "--context" => a
                .contexts
                .extend(v.split(',').map(|x| x.trim().to_string())),
            "--traces" => a.traces = v.parse().unwrap_or(a.traces),
            "--seed" => a.seed = v.parse().unwrap_or(a.seed),
            "--data-dir" => a.data_dir = PathBuf::from(v),
            _ => {
                eprintln!("unknown argument {k}");
                std::process::exit(2);
            }
        }
    }
    let d = EngineConfig::default();
    if a.models.is_empty() {
        a.models.push(models::CATALOG[0].id.to_string());
    }
    if a.quiet_ms.is_empty() {
        a.quiet_ms.push(d.quiet.as_millis() as u64);
    }
    if a.temperatures.is_empty() {
        a.temperatures.push(d.temperature);
    }
    if a.max_tokens.is_empty() {
        a.max_tokens.push(d.max_tokens);
    }
    if a.contexts.is_empty() {
        a.contexts.push("none".into());
    }
    for c in &a.contexts {
        if layers(c).is_none() {
            eprintln!(
                "unknown context {c:?} (use none, profile, memory, window, screen, joined with +)"
            );
            std::process::exit(2);
        }
    }
    a
}

/// "profile+memory" -> layers. None for unknown names.
fn layers(spec: &str) -> Option<GhostLayers> {
    let mut l = GhostLayers::default();
    for part in spec.split('+') {
        match part {
            "none" => {}
            "profile" => l.profile = true,
            "memory" => l.memory = true,
            "window" => l.window = true,
            "screen" => l.screen = true,
            _ => return None,
        }
    }
    Some(l)
}

/// Resident memory of the running llama-server, from its PID file.
fn server_rss_mb(runtime: &std::path::Path) -> Option<u64> {
    let pid = std::fs::read_to_string(runtime.join(sidecar::PID_FILE)).ok()?;
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", pid.trim()])
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse::<u64>()
        .ok()
        .map(|kb| kb / 1024)
}

fn pct(x: f64) -> String {
    format!("{:.0}%", x * 100.0)
}

#[tokio::main]
async fn main() {
    let args = parse();
    let runtime = std::env::temp_dir().join(format!("bob-bench-{}", std::process::id()));
    let svc = Arc::new(BrainService::new(
        std::env::temp_dir().join(format!("bob-bench-cfg-{}", std::process::id())),
        args.data_dir.clone(),
        runtime.clone(),
        Box::new(MemoryStore::default()),
    ));
    let traces: Vec<bench::Trace> = corpus::SENTENCES
        .iter()
        .take(args.traces)
        .enumerate()
        .map(|(i, (id, s))| bench::trace(id, s, args.seed + i as u64))
        .collect();

    println!(
        "{:<30} {:<22} {:>6} {:>5} {:>4} | {:>6} {:>6} {:>6} | {:>8} {:>8} {:>8} | {:>7}",
        "model",
        "context",
        "quiet",
        "temp",
        "tok",
        "p50",
        "p95",
        "max",
        "pauses",
        "pause ok",
        "saved",
        "RSS MB"
    );
    let mut rows = vec![];
    for model in &args.models {
        if let Err(e) = svc.start_builtin(model, Duration::from_secs(180)).await {
            eprintln!("{model}: cannot start ({e})");
            continue;
        }
        if let Err(e) = svc.builtin_ready(Duration::from_secs(240)).await {
            eprintln!("{model}: not ready ({e})");
            continue;
        }
        for ctx in &args.contexts {
            for &q in &args.quiet_ms {
                for &t in &args.temperatures {
                    for &n in &args.max_tokens {
                        let config = EngineConfig {
                            quiet: Duration::from_millis(q),
                            temperature: t,
                            max_tokens: n,
                        };
                        let hub = Arc::new(ContextHub::in_memory(
                            ContextSettings {
                                ghost: layers(ctx).expect("validated"),
                                ..Default::default()
                            },
                            corpus::PROFILE,
                        ));
                        let h = Harness::with_context(svc.clone(), config, hub);
                        for phrase in corpus::MEMORY.iter().rev() {
                            h.ghost.remember(phrase);
                        }
                        let mut scores = vec![];
                        for tr in &traces {
                            let events = h.replay(tr).await;
                            scores.push(bench::score(tr, &events));
                        }
                        let summary = bench::summarize(&scores);
                        let rss = server_rss_mb(&runtime);
                        let ms =
                            |x: Option<u64>| x.map(|v| v.to_string()).unwrap_or_else(|| "-".into());
                        println!(
                        "{:<30} {:<22} {:>6} {:>5} {:>4} | {:>6} {:>6} {:>6} | {:>8} {:>8} {:>8} | {:>7}",
                        model,
                        ctx,
                        q,
                        t,
                        n,
                        ms(summary.p50_ms),
                        ms(summary.p95_ms),
                        ms(summary.max_ms),
                        pct(summary.pause_coverage),
                        pct(summary.pause_accuracy),
                        pct(summary.keystroke_savings),
                        ms(rss)
                    );
                        rows.push(Row {
                            model: model.clone(),
                            context: ctx.clone(),
                            config,
                            summary,
                            server_rss_mb: rss,
                            scores,
                        });
                    }
                }
            }
        }
        svc.stop_builtin().await;
    }
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../bench-results/local");
    let _ = std::fs::create_dir_all(&dir);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let path = dir.join(format!("ghost-bench-{stamp}.json"));
    match serde_json::to_string_pretty(&rows).map(|j| std::fs::write(&path, j)) {
        Ok(Ok(())) => println!("wrote {}", path.display()),
        _ => eprintln!("could not write results"),
    }
    let _ = std::fs::remove_dir_all(&runtime);
}
