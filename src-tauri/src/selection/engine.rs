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
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Save (sanitized) settings. The caller pushes the hotkey to the helper.
    pub fn set_settings(&self, s: SelectionSettings) -> std::io::Result<SelectionSettings> {
        let s = s.sanitized();
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
        self.shared
            .stats
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Voice edit (proposal 1): apply a spoken `instruction` to the selected `text` and replace it.
    pub fn voice_edit(&self, text: String, instruction: String) {
        let _ = self.tx.send(Input::VoiceEdit { text, instruction });
    }
}

/// The action for a spoken instruction. The instruction may name a language ("in German"), so the
/// "answer in" rule is left out.
pub fn voice_action(instruction: &str) -> Action {
    let instruction: String = instruction
        .trim()
        .chars()
        .take(actions::MAX_INSTRUCTION_CHARS)
        .collect();
    Action {
        label: "Voice edit".into(),
        progress: "Applying your instruction".into(),
        instruction: format!(
            "Task (spoken by the user): {instruction}\nApply this instruction to the user's text. If the \
             instruction is not about editing the text, make the smallest sensible edit, or return the text unchanged."
        ),
        sets_language: true,
        summary: false,
    }
}

enum Input {
    Native(NativeEvent),
    VoiceEdit {
        text: String,
        instruction: String,
    },
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
    Menu {
        capture: Capture,
        menu: Vec<Action>,
    },
    Running {
        capture: Capture,
        action: Action,
        voice: bool,
    },
    Replacing {
        voice: bool,
    },
    /// The result is shown in the panel, waiting for Replace, Copy or Discard (proposal 6).
    Preview {
        capture: Capture,
        text: String,
        voice: bool,
    },
}

/// The preview's choices, in order.
pub const PREVIEW_ITEMS: [&str; 3] = ["Replace selection", "Copy", "Discard"];

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
        f(&mut self
            .shared
            .stats
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner));
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
        crate::usage::record(|d| d.rewrite_failures += 1);
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
            Input::Native(NativeEvent::MenuChoice { index }) => {
                if matches!(self.state, State::Preview { .. }) {
                    self.on_preview_choice(index as usize)
                } else {
                    self.on_choice(index as usize)
                }
            }
            Input::Native(NativeEvent::MenuCancel) => {
                self.cancel();
                self.generation += 1;
                self.state = State::Idle;
            }
            Input::Native(NativeEvent::ReplaceResult { ok, error, .. }) => {
                let State::Replacing { voice } = self.state else {
                    return;
                };
                if ok {
                    self.stats(|s| s.replaced += 1);
                    crate::usage::record(|d| {
                        if voice {
                            d.voice_edits += 1
                        } else {
                            d.rewrites += 1
                        }
                    });
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
            Input::VoiceEdit { text, instruction } => self.on_voice_edit(text, instruction),
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
                let State::Running { capture, voice, .. } =
                    std::mem::replace(&mut self.state, State::Idle)
                else {
                    return;
                };
                match result {
                    Err(e) => self.fail(&e),
                    Ok(raw) => {
                        let style = self.context.settings();
                        let text = crate::context::typography::finish(
                            &clean::clean(&raw, &capture.text),
                            style.spelling,
                            style.quotes,
                            None,
                        );
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
                        if self.settings().preview {
                            self.sink.send(CoreCommand::ShowMenu {
                                title: "Preview: ⏎ replaces, Esc discards".into(),
                                items: PREVIEW_ITEMS.iter().map(|s| s.to_string()).collect(),
                                anchor: None,
                                preview: Some(text.clone()),
                            });
                            self.state = State::Preview {
                                capture,
                                text,
                                voice,
                            };
                            return;
                        }
                        self.state = State::Replacing { voice };
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
            preview: None,
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
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    fn on_choice(&mut self, index: usize) {
        let State::Menu { capture, menu } = std::mem::replace(&mut self.state, State::Idle) else {
            return;
        };
        let Some(action) = menu.get(index).cloned() else {
            return;
        };
        self.start(capture, action, false);
    }

    fn on_preview_choice(&mut self, index: usize) {
        let State::Preview {
            capture,
            text,
            voice,
        } = std::mem::replace(&mut self.state, State::Idle)
        else {
            return;
        };
        match index {
            0 => {
                self.state = State::Replacing { voice };
                self.sink.send(CoreCommand::ReplaceSelection {
                    expected: capture.text,
                    text,
                });
            }
            1 => {
                self.sink.send(CoreCommand::CopyText { text });
                self.status(
                    "Copied. The selection was not changed.",
                    false,
                    false,
                    Some(DONE_HIDE),
                );
            }
            _ => self.status("Discarded. Nothing changed.", false, false, Some(DONE_HIDE)),
        }
    }

    fn on_voice_edit(&mut self, text: String, instruction: String) {
        self.cancel();
        self.generation += 1;
        self.state = State::Idle;
        if instruction.trim().is_empty() {
            return self.status(
                "I didn't catch an instruction, so nothing changed.",
                false,
                true,
                Some(ERROR_HIDE),
            );
        }
        if text.trim().is_empty() {
            return;
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
        self.start(Capture { text }, voice_action(&instruction), true);
    }

    fn start(&mut self, capture: Capture, action: Action, voice: bool) {
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
                run_checked(
                    &brains,
                    &context,
                    &job_action,
                    &text,
                    Some((generation, &tx)),
                ),
            )
            .await
            .unwrap_or_else(|_| Err("The brain took too long, so BOB stopped waiting.".into()));
            let _ = tx.send(Input::Finished {
                generation,
                result,
                ms: started.elapsed().as_millis() as u64,
            });
        }));
        self.state = State::Running {
            capture,
            action,
            voice,
        };
    }
}

/// Run an action without the panel (AI agents over MCP): same brain, rules, cleaning and quote style as the
/// hotkey, with the whole-action deadline. Returns the rewritten text.
pub async fn rewrite(
    brains: &BrainService,
    context: &ContextHub,
    action: &Action,
    text: &str,
) -> Result<String, String> {
    let n = text.chars().count();
    if text.trim().is_empty() {
        return Err("The text is empty.".into());
    }
    if n > MAX_CHARS {
        return Err(format!(
            "The text is too long ({n} characters; the limit is {MAX_CHARS})."
        ));
    }
    let raw = tokio::time::timeout(TIMEOUT, run_checked(brains, context, action, text, None))
        .await
        .unwrap_or_else(|_| Err("The brain took too long, so BOB stopped waiting.".into()))?;
    let style = context.settings();
    let out = crate::context::typography::finish(
        &clean::clean(&raw, text),
        style.spelling,
        style.quotes,
        None,
    );
    if out.trim().is_empty() {
        return Err("The brain returned no text.".into());
    }
    Ok(out)
}

/// The language the result must be in, when BOB can tell: the per-app or global "answer in" language, or
/// else the text's own language (when clearly detected and one the user writes). `None` for Translate and
/// for short or mixed text.
pub fn expected_language(
    context: &ContextHub,
    action: &Action,
    text: &str,
) -> Option<&'static str> {
    use crate::context::languages;
    if action.sets_language {
        return None;
    }
    let s = context.settings();
    let v = context.visible();
    let mode_lang = s
        .app_mode(&v.app_bundle_id)
        .and_then(|m| languages::code_for(&m.language));
    if let Some(code) = mode_lang.or_else(|| languages::code_for(&s.sanitized_language())) {
        return Some(code);
    }
    let own = languages::detect_strong(text)?;
    (s.languages.is_empty() || s.languages.iter().any(|l| l == own)).then_some(own)
}

/// Run the action and check the result's language. Small models sometimes answer a Hungarian text in
/// English, or ignore "answer in English" (style benchmark, docs/PERFORMANCE.md): BOB then asks once more
/// with a firmer rule and keeps whichever answer is in the right language.
async fn run_checked(
    brains: &BrainService,
    context: &ContextHub,
    action: &Action,
    text: &str,
    previews: Option<(u64, &mpsc::UnboundedSender<Input>)>,
) -> Result<String, String> {
    use crate::context::languages;
    let first = run_action(brains, context, action, text, previews, None).await?;
    let Some(want) = expected_language(context, action, text) else {
        return Ok(first);
    };
    match languages::detect_strong(&first) {
        Some(got) if got != want => {
            let name = languages::find(want).map_or(want, |l| l.name);
            log::info!("selection: answer came back in {got}, expected {want}; asking again");
            let rule = format!(
                "Important: write your whole answer in {name}. If the text is in another language, translate it into {name}."
            );
            let second = run_action(brains, context, action, text, previews, Some(&rule)).await?;
            if languages::detect_strong(&second) == Some(want) {
                return Ok(second);
            }
            // Still wrong: translate the first answer. Small models do one thing at a time well, and
            // Translate is that one thing.
            let translate = actions::menu(&Default::default(), name)
                .into_iter()
                .find(|a| a.sets_language)
                .expect("the menu has Translate");
            let third = run_action(brains, context, &translate, &first, previews, None).await?;
            Ok(if languages::detect_strong(&third) == Some(want) {
                third
            } else {
                first
            })
        }
        _ => Ok(first),
    }
}

/// Run `action` on `text` with the selection brain, streaming previews when `previews` is set. `extra` is
/// appended to the system prompt (the language retry).
async fn run_action(
    brains: &BrainService,
    context: &ContextHub,
    action: &Action,
    text: &str,
    previews: Option<(u64, &mpsc::UnboundedSender<Input>)>,
    extra: Option<&str>,
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
    let mut system = context.chat_system_opts(
        &action.instruction,
        brain.config.shares_context(),
        !action.sets_language,
    );
    if let Some(rule) = extra {
        system.push_str("\n\n");
        system.push_str(rule);
    }
    crate::audit::sent(
        "selection",
        &brain.config,
        text.chars().count(),
        brain.config.shares_context(),
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
        if let Some((generation, tx)) = previews {
            if last_preview.elapsed() >= PREVIEW_EVERY {
                last_preview = Instant::now();
                let _ = tx.send(Input::Preview {
                    generation,
                    text: out.clone(),
                });
            }
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
    async fn preview_shows_the_result_and_waits_for_a_choice() {
        let (h, rec, _server, _root) = setup(&["The quick fox."], "local").await;
        h.set_settings(SelectionSettings {
            preview: true,
            ..Default::default()
        })
        .unwrap();
        let run = |h: &SelectionHandle| {
            h.native(selection("teh quick fox"));
            h.native(NativeEvent::MenuChoice { index: 0 });
        };
        run(&h);
        settle(300).await;
        let cmds = rec.take();
        assert!(!cmds
            .iter()
            .any(|c| matches!(c, CoreCommand::ReplaceSelection { .. })));
        assert!(cmds.iter().any(|c| matches!(
            c,
            CoreCommand::ShowMenu { preview: Some(p), items, .. } if p == "The quick fox." && items[0] == PREVIEW_ITEMS[0]
        )));
        // Replace.
        h.native(NativeEvent::MenuChoice { index: 0 });
        settle(30).await;
        assert!(rec.take().contains(&CoreCommand::ReplaceSelection {
            expected: "teh quick fox".into(),
            text: "The quick fox.".into()
        }));
        // Copy leaves the selection alone.
        h.native(NativeEvent::ReplaceResult {
            ok: true,
            method: None,
            error: None,
        });
        run(&h);
        settle(300).await;
        rec.take();
        h.native(NativeEvent::MenuChoice { index: 1 });
        settle(30).await;
        let cmds = rec.take();
        assert!(cmds.contains(&CoreCommand::CopyText {
            text: "The quick fox.".into()
        }));
        assert!(!cmds
            .iter()
            .any(|c| matches!(c, CoreCommand::ReplaceSelection { .. })));
        // Esc discards.
        run(&h);
        settle(300).await;
        h.native(NativeEvent::MenuCancel);
        h.native(NativeEvent::MenuChoice { index: 0 });
        settle(30).await;
        assert!(!rec
            .take()
            .iter()
            .any(|c| matches!(c, CoreCommand::ReplaceSelection { .. })));
    }

    /// The style benchmark found the local model answering Hungarian text in English; BOB asks once more.
    #[tokio::test]
    async fn an_answer_in_the_wrong_language_is_asked_again() {
        let server = TestServer::start(|req| {
            let system = req.json()["messages"][0]["content"]
                .as_str()
                .unwrap_or("")
                .to_string();
            let answer = if system.contains("Important: write your whole answer in Hungarian") {
                "Holnap nem tudok menni, mert beteg vagyok."
            } else {
                "I can't go tomorrow because I am sick."
            };
            let ev = serde_json::json!({"choices":[{"delta":{"content": answer}}]}).to_string();
            Response::sse(&[ev.as_str()], Duration::ZERO)
                .chunk_after(Duration::ZERO, "data: [DONE]\n\n")
        })
        .await;
        let root = std::env::temp_dir().join(format!("bob-sel-lang-{}", rand::random::<u32>()));
        let svc = Arc::new(BrainService::new(
            root.join("c"),
            root.join("d"),
            root.join("r"),
            Box::new(MemoryStore::default()),
        ));
        svc.upsert(
            BrainConfig {
                id: "chat".into(),
                name: "Chat".into(),
                kind: ProviderKind::OpenAiCompatible,
                endpoint: server.base.clone(),
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
        let ctx = ContextHub::in_memory(Default::default(), "");
        let fix = actions::menu(&Default::default(), "").remove(0);
        let out = rewrite(&svc, &ctx, &fix, "Holnap nem tudok menni mert beteg vagyok")
            .await
            .unwrap();
        assert_eq!(out, "Holnap nem tudok menni, mert beteg vagyok.");
        assert_eq!(server.request_count(), 2);
        // Text in the expected language: one request only.
        let _ = rewrite(&svc, &ctx, &fix, "I cant go tomorrow because I am sick").await;
        assert_eq!(server.request_count(), 3);
        // Translate decides its own language: never retried.
        let tr = actions::menu(&Default::default(), "")
            .into_iter()
            .find(|a| a.sets_language)
            .unwrap();
        assert_eq!(
            expected_language(&ctx, &tr, "Holnap nem tudok menni mert beteg vagyok"),
            None
        );
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
