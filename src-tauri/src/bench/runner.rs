//! Replays traces through the real Ghost engine in real time and records what it shows.

use super::{ChipEvent, Trace};
use crate::brain::service::BrainService;
use crate::ghost::engine::{self, EngineConfig, GhostHandle, Sink};
use crate::protocol::{CoreCommand, NativeEvent, Rect};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const CARET: Rect = Rect {
    x: 100.0,
    y: 200.0,
    width: 0.0,
    height: 18.0,
};
/// Time after the last keystroke during which the final chip may still arrive.
pub const TAIL: Duration = Duration::from_millis(1500);

/// A sink that timestamps every command.
#[derive(Default)]
pub struct TimedRecorder(Mutex<Vec<(Instant, CoreCommand)>>);

impl Sink for TimedRecorder {
    fn send(&self, cmd: CoreCommand) {
        self.0.lock().expect("recorder").push((Instant::now(), cmd));
    }
}

impl TimedRecorder {
    /// Chip events after `start`, in milliseconds relative to it.
    pub fn chip_events(&self, start: Instant) -> Vec<ChipEvent> {
        self.0
            .lock()
            .expect("recorder")
            .iter()
            .filter(|(t, _)| *t >= start)
            .filter_map(|(t, c)| {
                let at_ms = t.duration_since(start).as_millis() as u64;
                match c {
                    CoreCommand::ShowChip { text, .. } => Some(ChipEvent {
                        at_ms,
                        text: Some(text.clone()),
                    }),
                    CoreCommand::HideChip => Some(ChipEvent { at_ms, text: None }),
                    _ => None,
                }
            })
            .collect()
    }
}

/// An engine wired to a timed recorder.
pub struct Harness {
    pub ghost: GhostHandle,
    pub recorder: Arc<TimedRecorder>,
}

impl Harness {
    pub fn new(brains: Arc<BrainService>, config: EngineConfig) -> Self {
        let recorder = Arc::new(TimedRecorder::default());
        let ghost = engine::spawn_with(brains, recorder.clone(), config);
        Self { ghost, recorder }
    }

    /// Type `trace` in real time and return the chip events, relative to the trace start.
    pub async fn replay(&self, trace: &Trace) -> Vec<ChipEvent> {
        // A focus change resets the engine between traces.
        self.ghost.native(NativeEvent::Focus {
            app_bundle_id: "bench".into(),
            app_name: "bench".into(),
            secure: false,
            editable: true,
        });
        tokio::time::sleep(Duration::from_millis(50)).await;
        let start = Instant::now();
        for step in &trace.steps {
            let due = start + Duration::from_millis(step.at_ms);
            tokio::time::sleep_until(due.into()).await;
            self.ghost.native(NativeEvent::TextState {
                app_bundle_id: "bench".into(),
                secure: false,
                before_caret: step.before.clone(),
                selection_len: 0,
                caret: Some(CARET),
            });
        }
        tokio::time::sleep(TAIL).await;
        self.recorder.chip_events(start)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bench::{score, summarize, trace};
    use crate::brain::registry::Role;
    use crate::brain::secrets::MemoryStore;
    use crate::brain::testserver::{Response, TestServer};
    use crate::brain::{BrainConfig, Capabilities, ProviderKind};

    /// An "oracle" brain that always continues with the true rest of the sentence: the harness must then
    /// report high accuracy and savings. Validates the measurement pipeline end to end, in CI.
    #[tokio::test]
    async fn an_oracle_brain_scores_high_and_latency_is_measured() {
        const SENTENCE: &str = "see you soon at the office";
        let server = TestServer::start(|req| {
            let prompt = req.json()["prompt"].as_str().unwrap_or("").to_string();
            let rest = SENTENCE
                .strip_prefix(prompt.as_str())
                .unwrap_or("")
                .to_string();
            let words: Vec<String> = rest.split_inclusive(' ').map(str::to_string).collect();
            let events: Vec<String> = words
                .iter()
                .map(|w| serde_json::json!({"choices":[{"text": w}]}).to_string())
                .collect();
            let refs: Vec<&str> = events.iter().map(String::as_str).collect();
            Response::sse(&refs, Duration::from_millis(5))
        })
        .await;
        let root = std::env::temp_dir().join(format!("bob-bench-{}", rand::random::<u32>()));
        let svc = Arc::new(BrainService::new(
            root.join("c"),
            root.join("d"),
            root.join("r"),
            Box::new(MemoryStore::default()),
        ));
        svc.upsert(
            BrainConfig {
                id: "oracle".into(),
                name: "Oracle".into(),
                kind: ProviderKind::Ollama,
                endpoint: server.base.clone(),
                model: None,
                capabilities: Capabilities {
                    ghost: true,
                    chat: true,
                },
                has_api_key: false,
            },
            None,
        )
        .await
        .unwrap();
        svc.set_role(Role::Ghost, Some("oracle")).await.unwrap();

        let h = Harness::new(svc, EngineConfig::default());
        // Faster trace so the test stays short: scale timings down 4x.
        let mut tr = trace("oracle", SENTENCE, 1);
        for s in &mut tr.steps {
            s.at_ms /= 4;
        }
        let events = h.replay(&tr).await;
        let sc = score(&tr, &events);
        let sum = summarize(std::slice::from_ref(&sc));
        // A correct chip is typed through, so a perfect brain needs very few model calls.
        assert!(sc.model_chips >= 1, "{sc:?}");
        assert!(sum.word_accuracy > 0.8, "oracle accuracy too low: {sum:?}");
        assert!(
            sum.keystroke_savings > 0.3,
            "oracle savings too low: {sum:?}"
        );
        let p50 = sum.p50_ms.unwrap();
        assert!(
            p50 >= engine::QUIET.as_millis() as u64 && p50 < 400,
            "latency {p50} ms"
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
