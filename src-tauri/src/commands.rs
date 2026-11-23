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
    pub recent_apps: Vec<RecentApp>,
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
        recent_apps: ghost.recent_apps(),
        memory_count: ghost.memory().phrases.len(),
        p50_ms: stats.percentile(50.0),
        p95_ms: stats.percentile(95.0),
        stats,
    })
}

#[tauri::command]
pub async fn ghost_set_enabled(ghost: State<'_, GhostHandle>, enabled: bool) -> Result<(), String> {
    ghost.set_enabled(enabled);
    Ok(())
}

#[tauri::command]
pub async fn ghost_set_quiet(
    ghost: State<'_, GhostHandle>,
    bundle_id: String,
    name: String,
    quiet: bool,
) -> Result<(), String> {
    ghost.set_quiet(&bundle_id, &name, quiet);
    Ok(())
}

/// The remembered phrases (shown only when the user opens the list).
#[tauri::command]
pub async fn ghost_memory(ghost: State<'_, GhostHandle>) -> Result<Memory, String> {
    Ok(ghost.memory())
}

#[tauri::command]
pub async fn ghost_memory_clear(ghost: State<'_, GhostHandle>) -> Result<(), String> {
    ghost.clear_memory();
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
