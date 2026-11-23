//! Selection Actions engine (spec `.kiro/specs/selection-actions`).
//!
//! `Idle -> Menu -> Running -> Replacing -> Idle`. The helper reads the selection when the hotkey is pressed
//! and owns the menu's keyboard navigation; this engine decides what to show, runs the chosen action on the
//! selection brain, cleans the output and asks the helper to replace the selection, which it does only if
//! the selection is unchanged (SA-7). Selected text is never logged.

use super::actions::{self, Action, SelectionSettings, SETTINGS_FILE};
use super::clean;
use crate::brain::registry::Role;
use crate::brain::service::BrainService;
use crate::brain::ChatRequest;
use crate::context::ContextHub;
use crate::ghost::engine::Sink;
use crate::ghost::redact;
use crate::protocol::{CoreCommand, Hotkey, NativeEvent, Rect};
use futures_util::StreamExt;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

/// Longest selection an action runs on (characters).
pub const MAX_CHARS: usize = 8000;
/// Whole-action deadline.
pub const TIMEOUT: Duration = Duration::from_secs(90);
/// How often a streamed preview is pushed to the panel.
const PREVIEW_EVERY: Duration = Duration::from_millis(150);
const DONE_HIDE: Duration = Duration::from_millis(1200);
const ERROR_HIDE: Duration = Duration::from_millis(4000);

#[derive(Debug, Clone, Default, Serialize)]
pub struct SelectionStats {
    pub menus: u64,
    pub runs: u64,
    pub replaced: u64,
    pub failed: u64,
    pub last_error: Option<String>,
    /// Milliseconds from the choice to the brain's last token, for the last successful run.
    pub last_ms: Option<u64>,
}

struct Shared {
    settings: Mutex<SelectionSettings>,
    stats: Mutex<SelectionStats>,
    storage: Option<PathBuf>,
}

#[derive(Clone)]
pub struct SelectionHandle {
    tx: mpsc::UnboundedSender<Input>,
    shared: Arc<Shared>,
}

impl SelectionHandle {
    pub fn native(&self, ev: NativeEvent) {
        let _ = self.tx.send(Input::Native(ev));
    }

    pub fn settings(&self) -> SelectionSettings {
        self.shared
            .settings
            .lock()
            .expect("selection settings")
            .clone()
    }

    /// Save (sanitized) settings. The caller pushes the hotkey to the helper.
    pub fn set_settings(&self, s: SelectionSettings) -> std::io::Result<SelectionSettings> {
        let s = s.sanitized();
        *self.shared.settings.lock().expect("selection settings") = s.clone();
        if let Some(dir) = &self.shared.storage {
            s.save(&dir.join(SETTINGS_FILE))?;
        }
        Ok(s)
    }

    pub fn hotkey_command(&self) -> CoreCommand {
        let h = self.settings().hotkey;
        CoreCommand::SetHotkey {
            hotkey: Hotkey {
                key_code: h.key_code,
                modifiers: h.modifiers,
            },
        }
    }

    pub fn stats(&self) -> SelectionStats {
        self.shared.stats.lock().expect("selection stats").clone()
    }
}

enum Input {
    Native(NativeEvent),
    Preview {
        generation: u64,
        text: String,
    },
    Finished {
        generation: u64,
        result: Result<String, String>,
        ms: u64,
    },
    Hide {
        generation: u64,
    },
}

#[derive(Debug, Clone)]
struct Capture {
    text: String,
}

enum State {
    Idle,
    Menu { capture: Capture, menu: Vec<Action> },
    Running { capture: Capture, action: Action },
    Replacing,
}

pub fn spawn(
    brains: Arc<BrainService>,
    context: Arc<ContextHub>,
    sink: Arc<dyn Sink>,
    storage: Option<PathBuf>,
) -> SelectionHandle {
    let settings = storage
        .as_ref()
        .map(|d| SelectionSettings::load(&d.join(SETTINGS_FILE)))
        .unwrap_or_default();
    let shared = Arc::new(Shared {
        settings: Mutex::new(settings),
        stats: Mutex::new(SelectionStats::default()),
        storage,
    });
    let (tx, rx) = mpsc::unbounded_channel();
    let engine = Engine {
        brains,
        context,
        sink,
        tx: tx.clone(),
        shared: shared.clone(),
        state: State::Idle,
        generation: 0,
        task: None,
    };
    tokio::spawn(engine.run(rx));
    SelectionHandle { tx, shared }
}

struct Engine {
    brains: Arc<BrainService>,
    context: Arc<ContextHub>,
    sink: Arc<dyn Sink>,
    tx: mpsc::UnboundedSender<Input>,
    shared: Arc<Shared>,
    state: State,
    generation: u64,
    task: Option<JoinHandle<()>>,
}

impl Engine {
    async fn run(mut self, mut rx: mpsc::UnboundedReceiver<Input>) {
        while let Some(input) = rx.recv().await {
            self.handle(input);
        }
        self.cancel();
    }

    fn cancel(&mut self) {
        if let Some(t) = self.task.take() {
            t.abort();
        }
    }

    fn stats(&self, f: impl FnOnce(&mut SelectionStats)) {
        f(&mut self.shared.stats.lock().expect("selection stats"));
    }

    /// Show a status line; `hide_after` closes the panel later unless something newer happened.
    fn status(&mut self, text: &str, busy: bool, error: bool, hide_after: Option<Duration>) {
        self.sink.send(CoreCommand::MenuStatus {
            text: text.into(),
            busy,
            error,
        });
        if let Some(d) = hide_after {
            let (tx, generation) = (self.tx.clone(), self.generation);
            tokio::spawn(async move {
                tokio::time::sleep(d).await;
                let _ = tx.send(Input::Hide { generation });
            });
        }
    }

    fn fail(&mut self, message: &str) {
        self.stats(|s| {
            s.failed += 1;
            s.last_error = Some(message.to_string());
        });
        self.state = State::Idle;
        self.status(message, false, true, Some(ERROR_HIDE));
    }

    fn handle(&mut self, input: Input) {
        match input {
            Input::Native(NativeEvent::Selection {
                text,
                secure,
                anchor,
                ..
            }) => self.on_selection(text, secure, anchor),
            Input::Native(NativeEvent::MenuChoice { index }) => self.on_choice(index as usize),
            Input::Native(NativeEvent::MenuCancel) => {
                self.cancel();
                self.generation += 1;
                self.state = State::Idle;
            }
            Input::Native(NativeEvent::ReplaceResult { ok, error, .. }) => {
                if !matches!(self.state, State::Replacing) {
                    return;
                }
                if ok {
                    self.stats(|s| s.replaced += 1);
                    self.state = State::Idle;
                    self.status("Done. Cmd+Z undoes it.", false, false, Some(DONE_HIDE));
                } else {
                    let reason = error.unwrap_or_else(|| "the text could not be replaced".into());
                    self.fail(&format!("Nothing changed: {reason}."));
                }
            }
            Input::Native(NativeEvent::Hello { .. }) => {
                // A restarted helper has no menu: start over.
                self.cancel();
                self.generation += 1;
                self.state = State::Idle;
            }
            Input::Native(_) => {}
            Input::Preview { generation, text } => {
                if generation != self.generation {
                    return;
                }
                if let State::Running { action, .. } = &self.state {
                    let tail: String = {
                        let chars: Vec<char> = text.trim().chars().collect();
                        let start = chars.len().saturating_sub(240);
                        chars[start..].iter().collect()
                    };
                    let line = format!("{}…\n{tail}", action.progress);
                    self.status(&line, true, false, None);
                }
            }
            Input::Finished {
                generation,
                result,
                ms,
            } => {
                if generation != self.generation {
                    return;
                }
                self.task = None;
                let State::Running { capture, .. } =
                    std::mem::replace(&mut self.state, State::Idle)
                else {
                    return;
                };
                match result {
                    Err(e) => self.fail(&e),
                    Ok(raw) => {
                        let text = clean::clean(&raw, &capture.text);
                        if text.trim().is_empty() {
                            return self.fail("The brain returned no text.");
                        }
                        self.stats(|s| s.last_ms = Some(ms));
                        if text == capture.text {
                            self.state = State::Idle;
                            return self.status(
                                "Already good: nothing to change.",
                                false,
                                false,
                                Some(DONE_HIDE),
                            );
                        }
                        self.state = State::Replacing;
                        self.sink.send(CoreCommand::ReplaceSelection {
                            expected: capture.text,
                            text,
                        });
                    }
                }
            }
            Input::Hide { generation } => {
                if generation == self.generation && matches!(self.state, State::Idle) {
                    self.sink.send(CoreCommand::HideMenu);
                }
            }
        }
    }

    fn on_selection(&mut self, text: String, secure: bool, anchor: Option<Rect>) {
        self.cancel();
        self.generation += 1;
        self.state = State::Idle;
        if secure {
            return self.status(
                "BOB never reads password fields.",
                false,
                true,
                Some(ERROR_HIDE),
            );
        }
        if text.trim().is_empty() {
            return self.status(
                "Select some text first, then press the hotkey again.",
                false,
                true,
                Some(ERROR_HIDE),
            );
        }
        let n = text.chars().count();
        if n > MAX_CHARS {
            return self.status(
                &format!("That selection is too long ({n} characters; the limit is {MAX_CHARS})."),
                false,
                true,
                Some(ERROR_HIDE),
            );
        }
        let language = self.context.settings().language;
        let menu = actions::menu(&self.settings(), &language);
        self.stats(|s| s.menus += 1);
        self.sink.send(CoreCommand::ShowMenu {
            title: "BOB".into(),
            items: menu.iter().map(|a| a.label.clone()).collect(),
            anchor,
        });
        self.state = State::Menu {
            capture: Capture { text },
            menu,
        };
    }

    fn settings(&self) -> SelectionSettings {
        self.shared
            .settings
            .lock()
            .expect("selection settings")
            .clone()
    }

    fn on_choice(&mut self, index: usize) {
        let State::Menu { capture, menu } = std::mem::replace(&mut self.state, State::Idle) else {
            return;
        };
        let Some(action) = menu.get(index).cloned() else {
            return;
        };
        self.generation += 1;
        let generation = self.generation;
        self.stats(|s| s.runs += 1);
        self.status(&format!("{}…", action.progress), true, false, None);
        let (brains, context, tx) = (self.brains.clone(), self.context.clone(), self.tx.clone());
        let text = capture.text.clone();
        let job_action = action.clone();
        self.task = Some(tokio::spawn(async move {
            let started = Instant::now();
            let result = tokio::time::timeout(
                TIMEOUT,
                run_action(&brains, &context, &job_action, &text, generation, &tx),
            )
            .await
            .unwrap_or_else(|_| Err("The brain took too long, so BOB stopped waiting.".into()));
            let _ = tx.send(Input::Finished {
                generation,
                result,
                ms: started.elapsed().as_millis() as u64,
            });
        }));
        self.state = State::Running { capture, action };
    }
}

/// Run `action` on `text` with the selection brain, streaming previews.
async fn run_action(
    brains: &BrainService,
    context: &ContextHub,
    action: &Action,
    text: &str,
    generation: u64,
    tx: &mpsc::UnboundedSender<Input>,
) -> Result<String, String> {
    let brain = brains.resolve_role(Role::Selection).await.map_err(|_| {
        "No brain is ready for Selection Actions. Pick one in Brains (or wake BOB's own brain)."
            .to_string()
    })?;
    if !brain.config.capabilities.chat {
        return Err(
            "The Selection Actions brain cannot follow instructions (no chat endpoint).".into(),
        );
    }
    // SA-9: credential-like text never goes to a cloud brain. Redacting it instead would put
    // "[redacted]" into the user's document, so the action is refused.
    if brain.config.is_cloud() && redact::scrub(text) != text {
        return Err(
            "This text looks like it contains a password or key, so BOB will not send it to a cloud brain."
                .into(),
        );
    }
    let system = context.chat_system_opts(
        &action.instruction,
        brain.config.shares_context(),
        !action.sets_language,
    );
    let mut stream = brain
        .chat_stream(&ChatRequest {
            system,
            user: text.to_string(),
            max_tokens: actions::max_tokens(action, text.chars().count()),
            temperature: 0.3,
        })
        .await
        .map_err(|e| e.to_string())?;
    let mut out = String::new();
    let mut last_preview = Instant::now();
    while let Some(item) = stream.next().await {
        out.push_str(&item.map_err(|e| e.to_string())?);
        if last_preview.elapsed() >= PREVIEW_EVERY {
            last_preview = Instant::now();
            let _ = tx.send(Input::Preview {
                generation,
                text: out.clone(),
            });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brain::secrets::MemoryStore;
    use crate::brain::testserver::{Response, TestServer};
    use crate::brain::{BrainConfig, Capabilities, ProviderKind};

    #[derive(Default)]
    struct Recorder(Mutex<Vec<CoreCommand>>);
    impl Sink for Recorder {
        fn send(&self, cmd: CoreCommand) {
            self.0.lock().unwrap().push(cmd);
        }
    }
    impl Recorder {
        fn take(&self) -> Vec<CoreCommand> {
            std::mem::take(&mut self.0.lock().unwrap())
        }
    }

    /// A selection brain backed by a chat test server answering `tokens`.
    async fn setup(
        tokens: &'static [&'static str],
        endpoint_kind: &str,
    ) -> (SelectionHandle, Arc<Recorder>, TestServer, PathBuf) {
        let server = TestServer::start(move |_| {
            let events: Vec<String> = tokens
                .iter()
                .map(|t| serde_json::json!({"choices":[{"delta":{"content": t}}]}).to_string())
                .collect();
            let refs: Vec<&str> = events.iter().map(String::as_str).collect();
            Response::sse(&refs, Duration::from_millis(2))
                .chunk_after(Duration::ZERO, "data: [DONE]\n\n")
        })
        .await;
        let root = std::env::temp_dir().join(format!("bob-selection-{}", rand::random::<u32>()));
        let svc = Arc::new(BrainService::new(
            root.join("c"),
            root.join("d"),
            root.join("r"),
            Box::new(MemoryStore::default()),
        ));
        // "cloud" uses a non-loopback name that still reaches the local test server: the label is what matters.
        let endpoint = if endpoint_kind == "cloud" {
            server.base.replace("127.0.0.1", "localhost.test")
        } else {
            server.base.clone()
        };
        svc.upsert(
            BrainConfig {
                id: "chat".into(),
                name: "Chat".into(),
                kind: ProviderKind::OpenAiCompatible,
                endpoint,
                model: Some("m".into()),
                capabilities: Capabilities {
                    ghost: false,
                    chat: true,
                },
                has_api_key: false,
                share_context: None,
            },
            None,
        )
        .await
        .unwrap();
        svc.set_role(Role::Selection, Some("chat")).await.unwrap();
        let ctx = Arc::new(ContextHub::in_memory(Default::default(), "- Name: Dávid"));
        let rec = Arc::new(Recorder::default());
        let h = spawn(svc, ctx, rec.clone(), Some(root.join("c")));
        (h, rec, server, root)
    }

    fn selection(text: &str) -> NativeEvent {
        NativeEvent::Selection {
            app_bundle_id: "com.apple.TextEdit".into(),
            text: text.into(),
            secure: false,
            source: Some(crate::protocol::SelectionSource::Accessibility),
            anchor: None,
        }
    }

    async fn settle(ms: u64) {
        tokio::time::sleep(Duration::from_millis(ms)).await;
    }

    #[tokio::test]
    async fn hotkey_menu_choice_and_replacement_happy_path() {
        let (h, rec, server, _root) = setup(
            &["Here is the corrected text:\n", "The quick", " fox."],
            "local",
        )
        .await;
        h.native(selection(" teh quick fox "));
        settle(50).await;
        let cmds = rec.take();
        let CoreCommand::ShowMenu { items, .. } = &cmds[0] else {
            panic!("{cmds:?}")
        };
        assert_eq!(items[0], "Fix grammar");
        h.native(NativeEvent::MenuChoice { index: 0 });
        settle(300).await;
        let cmds = rec.take();
        assert!(matches!(
            &cmds[0],
            CoreCommand::MenuStatus { busy: true, .. }
        ));
        // Cleaned output, original surrounding spaces kept, guarded by the original text.
        assert!(
            cmds.contains(&CoreCommand::ReplaceSelection {
                expected: " teh quick fox ".into(),
                text: " The quick fox. ".into(),
            }),
            "{cmds:?}"
        );
        // The brain got BOB's instructions, the task, and (local brain) the profile.
        let body = server.last_request().json();
        let system = body["messages"][0]["content"].as_str().unwrap().to_string();
        assert!(system.starts_with("You are BOB"));
        assert!(system.contains("correct the spelling"));
        assert!(system.contains("Dávid"));
        assert_eq!(body["messages"][1]["content"], " teh quick fox ");
        h.native(NativeEvent::ReplaceResult {
            ok: true,
            method: Some("accessibility".into()),
            error: None,
        });
        settle(30).await;
        assert!(matches!(
            &rec.take()[0],
            CoreCommand::MenuStatus {
                busy: false,
                error: false,
                ..
            }
        ));
        assert_eq!(h.stats().replaced, 1);
        settle(1300).await;
        assert_eq!(rec.take(), vec![CoreCommand::HideMenu]);
    }

    #[tokio::test]
    async fn secure_empty_and_oversized_selections_never_reach_the_brain() {
        let (h, rec, server, _root) = setup(&["x"], "local").await;
        h.native(NativeEvent::Selection {
            app_bundle_id: "a".into(),
            text: String::new(),
            secure: true,
            source: None,
            anchor: None,
        });
        h.native(selection("   "));
        h.native(selection(&"a".repeat(MAX_CHARS + 1)));
        settle(50).await;
        let cmds = rec.take();
        assert_eq!(cmds.len(), 3);
        assert!(cmds
            .iter()
            .all(|c| matches!(c, CoreCommand::MenuStatus { error: true, .. })));
        assert_eq!(server.request_count(), 0);
    }

    #[tokio::test]
    async fn a_failed_replacement_reports_why_and_nothing_else_happens() {
        let (h, rec, _server, _root) = setup(&["Better text."], "local").await;
        h.native(selection("bad text"));
        settle(30).await;
        h.native(NativeEvent::MenuChoice { index: 1 });
        settle(200).await;
        rec.take();
        h.native(NativeEvent::ReplaceResult {
            ok: false,
            method: None,
            error: Some("the selection changed, so nothing was replaced".into()),
        });
        settle(30).await;
        let cmds = rec.take();
        let CoreCommand::MenuStatus { text, error, .. } = &cmds[0] else {
            panic!("{cmds:?}")
        };
        assert!(*error);
        assert!(text.contains("the selection changed"));
        assert_eq!(h.stats().failed, 1);
    }

    #[tokio::test]
    async fn cancelling_the_menu_ignores_a_late_choice() {
        let (h, rec, server, _root) = setup(&["x"], "local").await;
        h.native(selection("some text"));
        h.native(NativeEvent::MenuCancel);
        h.native(NativeEvent::MenuChoice { index: 0 });
        settle(100).await;
        assert_eq!(rec.take().len(), 1); // the menu only
        assert_eq!(server.request_count(), 0);
    }

    #[tokio::test]
    async fn secrets_are_never_sent_to_a_cloud_brain() {
        let (h, rec, server, _root) = setup(&["ok"], "cloud").await;
        let key = format!("sk-{}", "Ab1".repeat(12));
        h.native(selection(&format!("my key is {key}")));
        settle(30).await;
        h.native(NativeEvent::MenuChoice { index: 0 });
        settle(200).await;
        assert_eq!(
            server.request_count(),
            0,
            "SA-9: secret sent to a cloud brain"
        );
        let cmds = rec.take();
        assert!(cmds.iter().any(|c| matches!(
            c,
            CoreCommand::MenuStatus { error: true, text, .. } if text.contains("password or key")
        )));
    }

    #[tokio::test]
    async fn unchanged_output_does_not_touch_the_document() {
        let (h, rec, _server, _root) = setup(&["\"All good.\""], "local").await;
        h.native(selection("All good."));
        settle(30).await;
        h.native(NativeEvent::MenuChoice { index: 0 });
        settle(200).await;
        let cmds = rec.take();
        assert!(!cmds
            .iter()
            .any(|c| matches!(c, CoreCommand::ReplaceSelection { .. })));
        assert!(cmds.iter().any(|c| matches!(
            c,
            CoreCommand::MenuStatus { text, .. } if text.contains("nothing to change")
        )));
    }
}
