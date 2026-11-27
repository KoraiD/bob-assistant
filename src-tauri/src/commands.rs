//! Tauri IPC commands for the Brain panel and Ghost Text. Thin wrappers around [`BrainService`] and the
//! Ghost engine; errors become user-facing strings. Streaming results are delivered as events, keyed by
//! a request id.

use crate::brain::client::TestReport;
use crate::brain::registry::Role;
use crate::brain::service::{BrainService, ModelInfo, Snapshot};
use crate::brain::{BrainConfig, ChatRequest, CompletionRequest};
use crate::ghost::engine::{GhostHandle, GhostStats, RecentApp};
use crate::ghost::memory::Memory;
use crate::ghost::settings::QuietApp;
use futures_util::StreamExt;
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, State};

pub type Brains = Arc<BrainService>;

/// Emitted after any change to brains, roles or the built-in sidecar.
pub const BRAINS_CHANGED: &str = "brains-changed";
/// Streamed tokens of a "Try it" request.
pub const BRAIN_TOKEN: &str = "brain-token";
/// Model download progress.
pub const MODEL_PROGRESS: &str = "model-progress";

static NEXT_REQUEST: AtomicU64 = AtomicU64::new(1);

fn changed(app: &AppHandle) {
    let _ = app.emit(BRAINS_CHANGED, ());
}

#[tauri::command]
pub async fn brain_snapshot(brains: State<'_, Brains>) -> Result<Snapshot, String> {
    Ok(brains.snapshot().await)
}

#[tauri::command]
pub async fn brain_upsert(
    app: AppHandle,
    brains: State<'_, Brains>,
    config: BrainConfig,
    api_key: Option<String>,
) -> Result<(), String> {
    crate::audit::record(
        "brain.saved",
        serde_json::json!({
            "brain": config.id,
            "kind": format!("{:?}", config.kind),
            "cloud": config.is_cloud(),
            "key": match api_key.as_deref() { None => "unchanged", Some("") => "removed", Some(_) => "set" },
        }),
    );
    brains
        .upsert(config, api_key)
        .await
        .map_err(|e| e.to_string())?;
    changed(&app);
    Ok(())
}

#[tauri::command]
pub async fn brain_remove(
    app: AppHandle,
    brains: State<'_, Brains>,
    id: String,
) -> Result<(), String> {
    brains.remove(&id).await.map_err(|e| e.to_string())?;
    crate::audit::record("brain.removed", serde_json::json!({ "brain": id }));
    changed(&app);
    Ok(())
}

#[tauri::command]
pub async fn brain_set_role(
    app: AppHandle,
    brains: State<'_, Brains>,
    role: Role,
    id: Option<String>,
) -> Result<(), String> {
    brains
        .set_role(role, id.as_deref())
        .await
        .map_err(|e| e.to_string())?;
    crate::audit::record(
        "brain.role",
        serde_json::json!({ "role": role, "brain": id }),
    );
    changed(&app);
    Ok(())
}

#[tauri::command]
pub async fn brain_test(
    app: AppHandle,
    brains: State<'_, Brains>,
    id: String,
) -> Result<TestReport, String> {
    let r = brains.test(&id).await.map_err(|e| e.to_string())?;
    changed(&app);
    Ok(r)
}

#[derive(Clone, Serialize)]
pub struct TokenEvent {
    pub request_id: u64,
    pub text: Option<String>,
    pub first_token_ms: Option<u64>,
    pub done: bool,
    pub error: Option<String>,
    /// Ghost mode, on the final event: what Ghost Text would actually show (the polished chip).
    pub polished: Option<String>,
}

impl TokenEvent {
    fn token(request_id: u64, text: String, ms: u64) -> Self {
        Self {
            request_id,
            text: Some(text),
            first_token_ms: Some(ms),
            done: false,
            error: None,
            polished: None,
        }
    }
    fn end(
        request_id: u64,
        first: Option<u64>,
        error: Option<String>,
        polished: Option<String>,
    ) -> Self {
        Self {
            request_id,
            text: None,
            first_token_ms: first,
            done: true,
            error,
            polished,
        }
    }
}

/// The "Try it" prompt as it may leave BOB (code review SEC-17): redacted for cloud brains like every other
/// path, and recorded in the activity trail (without text).
fn try_prompt(brain: &crate::brain::BrainConfig, prompt: String) -> String {
    let prompt = if brain.is_cloud() {
        crate::ghost::redact::scrub(&prompt)
    } else {
        prompt
    };
    crate::audit::sent("try_it", brain, prompt.chars().count(), false);
    prompt
}

/// "Try it": stream a short completion (`mode = "ghost"`) or chat answer (`"chat"`) from a brain.
/// The prompt is the user's own input in the panel and is never logged.
#[tauri::command]
pub async fn brain_try(
    app: AppHandle,
    brains: State<'_, Brains>,
    id: String,
    prompt: String,
    mode: String,
) -> Result<u64, String> {
    let brain = brains.resolve(&id).await.map_err(|e| e.to_string())?;
    let prompt = try_prompt(&brain.config, prompt);
    let request_id = NEXT_REQUEST.fetch_add(1, Ordering::SeqCst);
    tauri::async_runtime::spawn(async move {
        let emit = |e: TokenEvent| {
            let _ = app.emit(BRAIN_TOKEN, e);
        };
        let started = Instant::now();
        let ghost_mode = mode == "ghost";
        let stream = if ghost_mode {
            brain
                .complete_stream(&CompletionRequest {
                    max_tokens: 24,
                    ..CompletionRequest::ghost(prompt.clone())
                })
                .await
        } else {
            brain
                .chat_stream(&ChatRequest {
                    system: "You are BOB, a concise assistant. Answer in at most two sentences."
                        .into(),
                    user: prompt.clone(),
                    max_tokens: 120,
                    temperature: 0.4,
                })
                .await
        };
        let mut stream = match stream {
            Ok(s) => s,
            Err(e) => return emit(TokenEvent::end(request_id, None, Some(e.to_string()), None)),
        };
        let mut first = None;
        let mut raw = String::new();
        while let Some(item) = stream.next().await {
            match item {
                Ok(text) => {
                    let ms = *first.get_or_insert(started.elapsed().as_millis() as u64);
                    raw.push_str(&text);
                    emit(TokenEvent::token(request_id, text, ms));
                }
                Err(e) => {
                    return emit(TokenEvent::end(
                        request_id,
                        first,
                        Some(e.to_string()),
                        None,
                    ))
                }
            }
        }
        let polished = if ghost_mode {
            crate::ghost::chip::polish(&raw, &prompt)
        } else {
            None
        };
        emit(TokenEvent::end(request_id, first, None, polished));
    });
    Ok(request_id)
}

#[tauri::command]
pub async fn models_list(brains: State<'_, Brains>) -> Result<Vec<ModelInfo>, String> {
    Ok(brains.models().await)
}

#[derive(Clone, Serialize)]
pub struct ProgressEvent {
    pub id: String,
    pub downloaded: u64,
    pub total: u64,
    pub done: bool,
    pub error: Option<String>,
}

#[tauri::command]
pub async fn model_download(
    app: AppHandle,
    brains: State<'_, Brains>,
    id: String,
) -> Result<(), String> {
    let svc = brains.inner().clone();
    tauri::async_runtime::spawn(async move {
        let a = app.clone();
        let mid = id.clone();
        let res = svc
            .download(&id, move |p| {
                let _ = a.emit(
                    MODEL_PROGRESS,
                    ProgressEvent {
                        id: mid.clone(),
                        downloaded: p.downloaded,
                        total: p.total,
                        done: false,
                        error: None,
                    },
                );
            })
            .await;
        let error = res.err().map(|e| e.to_string());
        // A new speech model may be the one dictation waits for: tell the helper to load it.
        if error.is_none() && crate::brain::models::find_speech(&id).is_some() {
            push_dictation(&app);
        }
        let _ = app.emit(
            MODEL_PROGRESS,
            ProgressEvent {
                id,
                downloaded: 0,
                total: 0,
                done: true,
                error,
            },
        );
        changed(&app);
    });
    Ok(())
}

#[tauri::command]
pub async fn model_cancel(brains: State<'_, Brains>, id: String) -> Result<(), String> {
    brains.cancel_download(&id).await;
    Ok(())
}

/// Start (or switch) the built-in brain's model. Returns when it is ready or failed.
#[tauri::command]
pub async fn builtin_start(
    app: AppHandle,
    brains: State<'_, Brains>,
    model_id: String,
) -> Result<(), String> {
    brains
        .start_builtin(&model_id, Duration::from_secs(120))
        .await
        .map_err(|e| e.to_string())?;
    changed(&app);
    let r = brains
        .builtin_ready(Duration::from_secs(150))
        .await
        .map_err(|e| e.to_string());
    changed(&app);
    r
}

#[tauri::command]
pub async fn builtin_stop(app: AppHandle, brains: State<'_, Brains>) -> Result<(), String> {
    brains.stop_builtin().await;
    changed(&app);
    Ok(())
}

/// Ghost Text status for the UI: settings, recent apps and latency statistics. Memory contents are
/// fetched separately (`ghost_memory`) and only when the user opens them.
#[derive(Serialize)]
pub struct GhostStatus {
    pub enabled: bool,
    pub quiet_apps: Vec<QuietApp>,
    pub chip_offsets: Vec<crate::ghost::settings::ChipOffset>,
    pub recent_apps: Vec<RecentApp>,
    pub typo_fixes: bool,
    pub mid_sentence: bool,
    pub memory_count: usize,
    pub stats: GhostStats,
    pub p50_ms: Option<u64>,
    pub p95_ms: Option<u64>,
}

#[tauri::command]
pub async fn ghost_status(ghost: State<'_, GhostHandle>) -> Result<GhostStatus, String> {
    let stats = ghost.stats();
    let settings = ghost.settings();
    Ok(GhostStatus {
        enabled: settings.enabled,
        quiet_apps: settings.quiet_apps,
        chip_offsets: settings.chip_offsets,
        recent_apps: ghost.recent_apps(),
        typo_fixes: settings.typo_fixes,
        mid_sentence: settings.mid_sentence,
        memory_count: ghost.memory().phrases.len(),
        p50_ms: stats.percentile(50.0),
        p95_ms: stats.percentile(95.0),
        stats,
    })
}

#[tauri::command]
pub async fn ghost_set_enabled(ghost: State<'_, GhostHandle>, enabled: bool) -> Result<(), String> {
    crate::audit::record("settings.ghost", serde_json::json!({ "enabled": enabled }));
    ghost.set_enabled(enabled);
    Ok(())
}

/// `option`: "typo_fixes" or "mid_sentence".
#[tauri::command]
pub async fn ghost_set_option(
    ghost: State<'_, GhostHandle>,
    option: String,
    on: bool,
) -> Result<(), String> {
    match option.as_str() {
        "typo_fixes" => ghost.set_option(Some(on), None),
        "mid_sentence" => ghost.set_option(None, Some(on)),
        other => return Err(format!("unknown option {other:?}")),
    }
    crate::audit::record("settings.ghost", serde_json::json!({ option: on }));
    Ok(())
}

/// Nudge the chip in one app; 0, 0 removes the nudge.
#[tauri::command]
pub async fn ghost_set_chip_offset(
    ghost: State<'_, GhostHandle>,
    bundle_id: String,
    name: String,
    dx: i32,
    dy: i32,
) -> Result<(), String> {
    ghost.set_chip_offset(&bundle_id, &name, dx, dy);
    crate::audit::record(
        "settings.chip_offset",
        serde_json::json!({ "app": bundle_id, "dx": dx, "dy": dy }),
    );
    Ok(())
}

#[tauri::command]
pub async fn ghost_set_quiet(
    ghost: State<'_, GhostHandle>,
    context: State<'_, Context>,
    bridge: State<'_, crate::native_ipc::Bridge>,
    bundle_id: String,
    name: String,
    quiet: bool,
) -> Result<(), String> {
    ghost.set_quiet(&bundle_id, &name, quiet);
    crate::audit::record(
        "settings.quiet_app",
        serde_json::json!({ "app": bundle_id, "quiet": quiet }),
    );
    // Quiet apps are never captured for screen context either.
    push_context_mode(&bridge, &context, &ghost);
    Ok(())
}

/// The remembered phrases (shown only when the user opens the list).
#[tauri::command]
pub async fn ghost_memory(ghost: State<'_, GhostHandle>) -> Result<Memory, String> {
    Ok(ghost.memory())
}

#[tauri::command]
pub async fn ghost_memory_forget(
    ghost: State<'_, GhostHandle>,
    text: String,
) -> Result<bool, String> {
    Ok(ghost.forget_phrase(&text))
}

/// Add a phrase by hand (Ghost card). False when it would not be kept (too short or secret-like).
#[tauri::command]
pub async fn ghost_memory_add(ghost: State<'_, GhostHandle>, text: String) -> Result<bool, String> {
    let before = ghost.memory();
    ghost.remember(&text);
    Ok(ghost.memory() != before)
}

#[tauri::command]
pub async fn ghost_memory_edit(
    ghost: State<'_, GhostHandle>,
    old: String,
    new: String,
) -> Result<bool, String> {
    Ok(ghost.edit_phrase(&old, &new))
}

#[tauri::command]
pub async fn ghost_memory_clear(ghost: State<'_, GhostHandle>) -> Result<(), String> {
    ghost.clear_memory();
    crate::audit::record("memory.cleared", serde_json::json!({}));
    Ok(())
}

/// At launch: start the built-in brain with the first installed model, if any.
pub fn autostart_builtin(app: AppHandle, brains: Brains) {
    tauri::async_runtime::spawn(async move {
        let Some(model) = brains.first_installed_model() else {
            log::info!(
                "built-in brain: no model installed yet (models dir: {})",
                brains.models_dir().display()
            );
            return;
        };
        log::info!("built-in brain: starting with model {model}");
        if let Err(e) = brains.start_builtin(model, Duration::from_secs(120)).await {
            log::warn!("built-in brain did not start: {e}");
        }
        changed(&app);
        match brains.builtin_ready(Duration::from_secs(150)).await {
            Ok(()) => log::info!("built-in brain: ready"),
            Err(e) => log::warn!("built-in brain: not ready: {e}"),
        }
        changed(&app);
    });
}

// ---- personal context (spec personal-context) ----

pub type Context = Arc<crate::context::ContextHub>;

/// Tell the helper what it may capture: the chosen scope, never the quiet apps (PC-4, PC-7).
pub fn push_context_mode(
    bridge: &crate::native_ipc::Bridge,
    context: &Context,
    ghost: &GhostHandle,
) {
    let exclude = ghost
        .settings()
        .quiet_apps
        .into_iter()
        .map(|a| a.bundle_id)
        .collect();
    let _ = bridge.send(&crate::protocol::CoreCommand::SetContextMode {
        scope: context.settings().scope,
        exclude,
        clipboard: context.settings().ghost.clipboard,
    });
}

/// Everything the Personalize tab shows. The profile is the user's own file; visible context is
/// summarized (sizes only).
#[derive(Serialize)]
pub struct ContextView {
    pub settings: crate::context::ContextSettings,
    pub profile: String,
    pub profile_path: Option<String>,
    pub template: &'static str,
    pub max_profile_bytes: usize,
    pub visible: crate::context::VisibleSummary,
    pub screen_recording: Option<bool>,
}

#[tauri::command]
pub async fn context_get(
    context: State<'_, Context>,
    bridge: State<'_, crate::native_ipc::Bridge>,
) -> Result<ContextView, String> {
    Ok(ContextView {
        settings: context.settings(),
        profile: context.profile(),
        profile_path: context.profile_path().map(|p| p.display().to_string()),
        template: crate::context::profile::TEMPLATE,
        max_profile_bytes: crate::context::profile::MAX_BYTES,
        visible: context.summary(),
        screen_recording: bridge.status().screen_recording,
    })
}

#[tauri::command]
pub async fn context_set_settings(
    context: State<'_, Context>,
    bridge: State<'_, crate::native_ipc::Bridge>,
    ghost: State<'_, GhostHandle>,
    settings: crate::context::ContextSettings,
) -> Result<(), String> {
    let wants_capture = settings.scope != crate::protocol::ContextScope::Accessibility;
    let old = context.settings();
    crate::audit::settings("settings.personalize", &old, &settings.clone().sanitized());
    context
        .set_settings(settings)
        .map_err(|e| format!("cannot save the settings: {e}"))?;
    push_context_mode(&bridge, &context, &ghost);
    // Ask for Screen Recording only when the user picked a capture scope and it is not granted yet.
    if wants_capture && bridge.status().screen_recording != Some(true) {
        let _ = bridge.send(&crate::protocol::CoreCommand::RequestScreenRecording);
    }
    Ok(())
}

#[tauri::command]
pub async fn profile_save(context: State<'_, Context>, text: String) -> Result<(), String> {
    context.set_profile(&text).map_err(|e| e.to_string())?;
    crate::audit::record("profile.saved", serde_json::json!({ "bytes": text.len() }));
    Ok(())
}

#[tauri::command]
pub async fn context_request_screen_recording(
    bridge: State<'_, crate::native_ipc::Bridge>,
) -> Result<(), String> {
    bridge
        .send(&crate::protocol::CoreCommand::RequestScreenRecording)
        .map_err(|e| format!("native helper is not connected: {e}"))
}

/// What BOB would add to prompts right now, so the user can see it (shown only on request).
#[derive(Serialize)]
pub struct ContextPreview {
    pub ghost_notes: String,
    pub chat_system: String,
}

#[tauri::command]
pub async fn context_preview(
    context: State<'_, Context>,
    ghost: State<'_, GhostHandle>,
) -> Result<ContextPreview, String> {
    let memory = ghost
        .memory()
        .most_used(crate::context::GHOST_MEMORY_PHRASES);
    Ok(ContextPreview {
        ghost_notes: context.ghost_preamble(&memory),
        chat_system: context.chat_system("(the action you pick, e.g. \"Fix grammar\")", true),
    })
}

#[tauri::command]
pub async fn brain_set_share_context(
    app: AppHandle,
    brains: State<'_, Brains>,
    id: String,
    share: bool,
) -> Result<(), String> {
    brains
        .set_share_context(&id, share)
        .await
        .map_err(|e| e.to_string())?;
    crate::audit::record(
        "brain.share_context",
        serde_json::json!({ "brain": id, "share": share }),
    );
    changed(&app);
    Ok(())
}

// ---- Selection Actions ----

#[derive(Serialize)]
pub struct SelectionView {
    pub settings: crate::selection::actions::SelectionSettings,
    /// Menu labels in order, as the user will see them.
    pub actions: Vec<String>,
    pub stats: crate::selection::engine::SelectionStats,
}

#[tauri::command]
pub async fn selection_get(
    selection: State<'_, crate::selection::engine::SelectionHandle>,
    context: State<'_, Context>,
) -> Result<SelectionView, String> {
    let settings = selection.settings();
    let actions = crate::selection::actions::menu(&settings, &context.settings().language)
        .into_iter()
        .map(|a| a.label)
        .collect();
    Ok(SelectionView {
        settings,
        actions,
        stats: selection.stats(),
    })
}

#[tauri::command]
pub async fn selection_set_settings(
    selection: State<'_, crate::selection::engine::SelectionHandle>,
    bridge: State<'_, crate::native_ipc::Bridge>,
    settings: crate::selection::actions::SelectionSettings,
) -> Result<(), String> {
    crate::audit::settings("settings.selection", &selection.settings(), &settings);
    selection
        .set_settings(settings)
        .map_err(|e| format!("cannot save the settings: {e}"))?;
    let _ = bridge.send(&selection.hotkey_command());
    let _ = bridge.send(&selection.action_hotkeys_command());
    Ok(())
}

// ---- Voice Dictation ----

/// Send the current dictation configuration to the helper (hotkey, model to load, language).
pub fn push_dictation(app: &AppHandle) {
    use tauri::Manager;
    if let (Some(d), Some(bridge)) = (
        app.try_state::<crate::dictation::engine::DictationHandle>(),
        app.try_state::<crate::native_ipc::Bridge>(),
    ) {
        let _ = bridge.send(&d.command());
    }
}

#[derive(Serialize)]
pub struct DictationView {
    pub settings: crate::dictation::settings::DictationSettings,
    pub models: Vec<ModelInfo>,
    pub stats: crate::dictation::engine::DictationStats,
    pub microphone: Option<crate::protocol::MicStatus>,
}

#[tauri::command]
pub async fn dictation_get(
    dictation: State<'_, crate::dictation::engine::DictationHandle>,
    brains: State<'_, Brains>,
    bridge: State<'_, crate::native_ipc::Bridge>,
) -> Result<DictationView, String> {
    Ok(DictationView {
        settings: dictation.settings(),
        models: brains.speech_models(),
        stats: dictation.stats(),
        microphone: bridge.status().microphone,
    })
}

#[tauri::command]
pub async fn dictation_set_settings(
    app: AppHandle,
    dictation: State<'_, crate::dictation::engine::DictationHandle>,
    settings: crate::dictation::settings::DictationSettings,
) -> Result<(), String> {
    crate::audit::settings("settings.dictation", &dictation.settings(), &settings);
    dictation
        .set_settings(settings)
        .map_err(|e| format!("cannot save the settings: {e}"))?;
    push_dictation(&app);
    Ok(())
}

/// The last dictation while it is still in memory (two minutes), for "Copy last dictation".
#[tauri::command]
pub async fn dictation_last(
    dictation: State<'_, crate::dictation::engine::DictationHandle>,
) -> Result<Option<String>, String> {
    Ok(dictation.last_dictation())
}

/// Type the last dictation again at the caret (menu bar). False when there is none.
#[tauri::command]
pub async fn dictation_paste_last(
    dictation: State<'_, crate::dictation::engine::DictationHandle>,
) -> Result<bool, String> {
    Ok(dictation.paste_last())
}

// ---- Statistics, this Mac ----

#[tauri::command]
pub async fn usage_get() -> Result<crate::usage::UsageView, String> {
    Ok(crate::usage::view())
}

#[tauri::command]
pub async fn usage_clear() -> Result<(), String> {
    crate::usage::clear();
    crate::audit::record("usage.cleared", serde_json::json!({}));
    Ok(())
}

#[tauri::command]
pub async fn system_info() -> Result<crate::system::SystemInfo, String> {
    tauri::async_runtime::spawn_blocking(crate::system::info)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn running_apps() -> Result<Vec<crate::system::RunningApp>, String> {
    Ok(crate::system::running_apps())
}

#[cfg(test)]
mod try_tests {
    use super::*;
    use crate::brain::{BrainConfig, ProviderKind};

    fn brain(id: &str, endpoint: &str) -> BrainConfig {
        BrainConfig {
            id: id.into(),
            name: id.into(),
            kind: ProviderKind::OpenAiCompatible,
            endpoint: endpoint.into(),
            model: None,
            capabilities: ProviderKind::OpenAiCompatible.default_capabilities(),
            has_api_key: false,
            share_context: None,
        }
    }

    /// Code review SEC-17: "Try it" redacts for cloud brains and is audited, like every other path.
    #[test]
    fn try_it_redacts_for_cloud_brains_and_is_audited() {
        // Built at run time: no key-shaped literal in the repository.
        let key = format!("ghp_{}", "a1B2".repeat(9));
        let text = format!("my token is {key}");
        let cloud = brain("try-cloud", "https://api.example.com/v1");
        let sent = try_prompt(&cloud, text.clone());
        assert!(!sent.contains(&key), "{sent}");
        let local = brain("try-local", "http://127.0.0.1:11434/v1");
        assert_eq!(try_prompt(&local, text.clone()), text);
        let trail = crate::audit::recent(Some("data.sent"), 50);
        for id in ["try-cloud", "try-local"] {
            assert!(
                trail
                    .iter()
                    .any(|e| e.detail["brain"] == id && e.detail["feature"] == "try_it"),
                "{id}"
            );
        }
        assert!(!format!("{trail:?}").contains("my token"));
    }
}
