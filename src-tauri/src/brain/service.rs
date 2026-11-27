//! Application-level Brain service: the registry, Keychain secrets, the built-in sidecar and model
//! downloads behind one async API. Tauri commands in `lib.rs` are thin wrappers around it, so all the
//! behavior here is testable without the app.

use super::client::{HttpBrain, TestReport};
use super::models::{self, DownloadError, ModelEntry, Progress};
use super::registry::{Registry, RegistryError, Role};
use super::secrets::SecretStore;
use super::sidecar::{self, Sidecar, SidecarConfig, SidecarError, SidecarState};
use super::{BrainConfig, BrainError, ProviderKind};
use crate::supervisor::Backoff;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ServiceError {
    #[error(transparent)]
    Registry(#[from] RegistryError),
    #[error(transparent)]
    Brain(#[from] BrainError),
    #[error(transparent)]
    Sidecar(#[from] SidecarError),
    #[error(transparent)]
    Download(#[from] DownloadError),
    #[error("{0}")]
    Secret(String),
    #[error("unknown model {0}")]
    UnknownModel(String),
    #[error("model {0} is not downloaded yet")]
    ModelMissing(String),
    #[error("the file of model {0} changed after it was downloaded, so BOB deleted it; download it again")]
    ModelDamaged(String),
    #[error("the built-in brain is not running (choose a model in the Brain panel)")]
    BuiltinNotRunning,
}

/// Catalog entry plus local install state, for the UI.
#[derive(Debug, Clone, Serialize)]
pub struct ModelInfo {
    #[serde(flatten)]
    pub entry: ModelEntry,
    pub installed: bool,
    pub active: bool,
}

/// Everything the Brain panel renders.
#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    pub brains: Vec<BrainView>,
    pub roles: super::registry::Roles,
    pub builtin: SidecarState,
    pub builtin_model: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BrainView {
    #[serde(flatten)]
    pub config: BrainConfig,
    pub is_cloud: bool,
    /// Effective personal-context permission (PC-6), after applying the default.
    pub shares_context: bool,
}

pub struct BrainService {
    registry_path: PathBuf,
    models_dir: PathBuf,
    runtime_dir: PathBuf,
    registry: Mutex<Registry>,
    secrets: Box<dyn SecretStore>,
    sidecar: Mutex<Option<Sidecar>>,
    builtin_model: Mutex<Option<String>>,
    downloading: Mutex<Vec<(String, Arc<AtomicBool>)>>,
    http: reqwest::Client,
}

impl BrainService {
    pub fn new(
        config_dir: PathBuf,
        data_dir: PathBuf,
        runtime_dir: PathBuf,
        secrets: Box<dyn SecretStore>,
    ) -> Self {
        let registry_path = config_dir.join("brains.json");
        let registry = Registry::load(&registry_path).unwrap_or_default();
        Self {
            registry_path,
            models_dir: models::models_dir(&data_dir),
            runtime_dir,
            registry: Mutex::new(registry),
            secrets,
            sidecar: Mutex::new(None),
            builtin_model: Mutex::new(None),
            downloading: Mutex::new(vec![]),
            http: reqwest::Client::builder()
                .no_proxy()
                .build()
                .unwrap_or_default(),
        }
    }

    pub async fn snapshot(&self) -> Snapshot {
        let reg = self.registry.lock().await.clone();
        let builtin = match self.sidecar.lock().await.as_ref() {
            Some(s) => s.state(),
            None => SidecarState::Stopped,
        };
        Snapshot {
            brains: reg
                .brains
                .iter()
                .map(|b| BrainView {
                    is_cloud: b.kind != ProviderKind::Builtin && b.is_cloud(),
                    shares_context: b.shares_context(),
                    config: b.clone(),
                })
                .collect(),
            roles: reg.roles,
            builtin,
            builtin_model: self.builtin_model.lock().await.clone(),
        }
    }

    async fn persist(&self, reg: &Registry) -> Result<(), ServiceError> {
        reg.save(&self.registry_path)?;
        Ok(())
    }

    /// Add or update a brain. `api_key`: `Some("")` deletes the stored key, `Some(k)` stores it,
    /// `None` leaves it unchanged. Keys go to the Keychain only (BR-6).
    pub async fn upsert(
        &self,
        mut config: BrainConfig,
        api_key: Option<String>,
    ) -> Result<(), ServiceError> {
        match api_key.as_deref() {
            Some("") => {
                self.secrets
                    .delete(&config.id)
                    .map_err(|e| ServiceError::Secret(e.to_string()))?;
                config.has_api_key = false;
            }
            Some(k) => {
                self.secrets
                    .set(&config.id, k.trim())
                    .map_err(|e| ServiceError::Secret(e.to_string()))?;
                config.has_api_key = true;
            }
            None => {
                config.has_api_key = self.secrets.get(&config.id).ok().flatten().is_some();
            }
        }
        let mut reg = self.registry.lock().await;
        let mut next = reg.clone();
        if next.get(&config.id).is_some() {
            next.update(config)?;
        } else {
            next.add(config)?;
        }
        self.persist(&next).await?;
        *reg = next;
        Ok(())
    }

    pub async fn remove(&self, id: &str) -> Result<(), ServiceError> {
        let mut reg = self.registry.lock().await;
        let mut next = reg.clone();
        next.remove(id)?;
        self.persist(&next).await?;
        *reg = next;
        drop(reg);
        let _ = self.secrets.delete(id);
        Ok(())
    }

    pub async fn set_role(&self, role: Role, id: Option<&str>) -> Result<(), ServiceError> {
        let mut reg = self.registry.lock().await;
        let mut next = reg.clone();
        next.set_role(role, id)?;
        self.persist(&next).await?;
        *reg = next;
        Ok(())
    }

    /// Allow or forbid personal context for one brain (PC-6). Persisted with the brain.
    pub async fn set_share_context(&self, id: &str, share: bool) -> Result<(), ServiceError> {
        let mut reg = self.registry.lock().await;
        let mut next = reg.clone();
        let mut config = next
            .get(id)
            .cloned()
            .ok_or_else(|| RegistryError::NotFound(id.into()))?;
        config.share_context = Some(share);
        next.update(config)?;
        self.persist(&next).await?;
        *reg = next;
        Ok(())
    }

    /// Evaluate a short prompt on the built-in brain so its weights are back in memory (after the Mac
    /// woke from sleep, the first suggestion otherwise pays for paging them in). Returns whether it ran;
    /// does nothing when the built-in brain is not running.
    pub async fn warm_up_builtin(&self) -> bool {
        let ep = match self.sidecar.lock().await.as_ref() {
            Some(s) => s.endpoint().await,
            None => None,
        };
        let Some(ep) = ep else { return false };
        let http = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap_or_default();
        sidecar::warm_up(&http, &ep).await;
        true
    }

    /// A ready-to-use client for `id`. The built-in brain resolves to the running sidecar.
    pub async fn resolve(&self, id: &str) -> Result<HttpBrain, ServiceError> {
        let config = self
            .registry
            .lock()
            .await
            .get(id)
            .cloned()
            .ok_or_else(|| RegistryError::NotFound(id.into()))?;
        if config.kind == ProviderKind::Builtin {
            let ep = match self.sidecar.lock().await.as_ref() {
                Some(s) => s.endpoint().await,
                None => None,
            }
            .ok_or(ServiceError::BuiltinNotRunning)?;
            return Ok(HttpBrain::new(
                BrainConfig {
                    endpoint: ep.base_url,
                    ..config
                },
                Some(ep.api_key),
            )?);
        }
        let key = self
            .secrets
            .get(id)
            .map_err(|e| ServiceError::Secret(e.to_string()))?;
        Ok(HttpBrain::new(config, key)?)
    }

    pub async fn resolve_role(&self, role: Role) -> Result<HttpBrain, ServiceError> {
        let id = self
            .registry
            .lock()
            .await
            .for_role(role)
            .map(|b| b.id.clone())
            .ok_or_else(|| RegistryError::NotFound(format!("{role:?} brain")))?;
        self.resolve(&id).await
    }

    /// Connection test. Capabilities proven by the test are saved, so roles follow reality.
    pub async fn test(&self, id: &str) -> Result<TestReport, ServiceError> {
        let brain = self.resolve(id).await?;
        let report = brain.test().await;
        if report.reachable && report.error.is_none() {
            let mut reg = self.registry.lock().await;
            if let Some(mut cfg) = reg.get(id).cloned() {
                if cfg.capabilities != report.capabilities {
                    cfg.capabilities = report.capabilities;
                    let mut next = reg.clone();
                    next.update(cfg)?;
                    self.persist(&next).await?;
                    *reg = next;
                }
            }
        }
        Ok(report)
    }

    // ---- models + built-in sidecar ----

    pub async fn models(&self) -> Vec<ModelInfo> {
        let active = self.builtin_model.lock().await.clone();
        models::CATALOG
            .iter()
            .map(|m| ModelInfo {
                entry: m.clone(),
                installed: models::installed_path(&self.models_dir, m).is_file(),
                active: active.as_deref() == Some(m.id),
            })
            .collect()
    }

    /// Speech models (Voice Dictation) with their install state.
    pub fn speech_models(&self) -> Vec<ModelInfo> {
        models::SPEECH_CATALOG
            .iter()
            .map(|m| ModelInfo {
                entry: m.clone(),
                installed: models::installed_path(&self.models_dir, m).is_file(),
                active: false,
            })
            .collect()
    }

    /// Download a catalog model (language or speech). Only one download per model runs at a time.
    pub async fn download(
        &self,
        id: &str,
        on_progress: impl FnMut(Progress),
    ) -> Result<PathBuf, ServiceError> {
        let entry = models::find_any(id).ok_or_else(|| ServiceError::UnknownModel(id.into()))?;
        let flag = Arc::new(AtomicBool::new(false));
        {
            let mut d = self.downloading.lock().await;
            if d.iter().any(|(m, _)| m == id) {
                return Err(ServiceError::Download(DownloadError::Network(
                    "already downloading".into(),
                )));
            }
            d.push((id.to_string(), flag.clone()));
        }
        let dest = models::installed_path(&self.models_dir, entry);
        let res = models::download(&self.http, &entry.into(), &dest, on_progress, || {
            flag.load(Ordering::SeqCst)
        })
        .await;
        self.downloading.lock().await.retain(|(m, _)| m != id);
        Ok(res?)
    }

    pub async fn cancel_download(&self, id: &str) {
        for (m, f) in self.downloading.lock().await.iter() {
            if m == id {
                f.store(true, Ordering::SeqCst);
            }
        }
    }

    /// (Re)start the built-in sidecar with `model_id`.
    pub async fn start_builtin(
        &self,
        model_id: &str,
        health_timeout: Duration,
    ) -> Result<(), ServiceError> {
        let entry =
            models::find(model_id).ok_or_else(|| ServiceError::UnknownModel(model_id.into()))?;
        let model = models::installed_path(&self.models_dir, entry);
        if !model.is_file() {
            return Err(ServiceError::ModelMissing(model_id.into()));
        }
        // Hashed once per launch before llama-server parses it (code review SEC-13).
        if !models::verified(&model, entry.sha256).await? {
            return Err(ServiceError::ModelDamaged(model_id.into()));
        }
        let binary = sidecar::locate_binary().ok_or(SidecarError::BinaryNotFound)?;
        self.stop_builtin().await;
        let cfg = SidecarConfig::new(binary, model, self.runtime_dir.clone());
        let sc = Sidecar::start(cfg, health_timeout, Backoff::default())?;
        *self.sidecar.lock().await = Some(sc);
        *self.builtin_model.lock().await = Some(model_id.to_string());
        Ok(())
    }

    pub async fn stop_builtin(&self) {
        if let Some(mut s) = self.sidecar.lock().await.take() {
            s.stop().await;
        }
    }

    /// Wait for the built-in brain to become ready (used after start).
    pub async fn builtin_ready(&self, timeout: Duration) -> Result<(), ServiceError> {
        let guard = self.sidecar.lock().await;
        let s = guard.as_ref().ok_or(ServiceError::BuiltinNotRunning)?;
        s.ready(timeout).await?;
        Ok(())
    }

    /// First installed catalog model, in catalog order (the default model comes first).
    pub fn first_installed_model(&self) -> Option<&'static str> {
        models::CATALOG
            .iter()
            .find(|m| models::installed_path(&self.models_dir, m).is_file())
            .map(|m| m.id)
    }

    pub fn models_dir(&self) -> &std::path::Path {
        &self.models_dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brain::registry::BUILTIN_ID;
    use crate::brain::secrets::MemoryStore;
    use crate::brain::testserver::{Response, TestServer};
    use crate::brain::Capabilities;

    fn service(tag: &str) -> (BrainService, PathBuf) {
        let root = std::env::temp_dir().join(format!("bob-svc-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let svc = BrainService::new(
            root.join("config"),
            root.join("data"),
            root.join("rt"),
            Box::new(MemoryStore::default()),
        );
        (svc, root)
    }

    fn cloud(endpoint: &str) -> BrainConfig {
        BrainConfig {
            id: "cloud".into(),
            name: "Cloud".into(),
            kind: ProviderKind::OpenAiCompatible,
            endpoint: endpoint.into(),
            model: Some("gpt-x".into()),
            capabilities: ProviderKind::OpenAiCompatible.default_capabilities(),
            has_api_key: false,
            share_context: None,
        }
    }

    #[tokio::test]
    async fn api_keys_go_to_the_secret_store_never_to_disk() {
        let (svc, root) = service("keys");
        // A key-shaped fake built at runtime, so no secret-looking literal lives in the repository
        // (the Kiroween guard's secret scan would rightly flag one).
        let fake_key = format!("sk-{}", "x".repeat(32));
        svc.upsert(cloud("https://api.example.com"), Some(fake_key.clone()))
            .await
            .unwrap();
        let file = std::fs::read_to_string(root.join("config/brains.json")).unwrap();
        assert!(!file.contains(&fake_key), "key leaked into brains.json");
        assert!(file.contains("\"has_api_key\": true"));
        assert_eq!(svc.secrets.get("cloud").unwrap(), Some(fake_key));
        // Empty string deletes the key.
        svc.upsert(cloud("https://api.example.com"), Some(String::new()))
            .await
            .unwrap();
        assert_eq!(svc.secrets.get("cloud").unwrap(), None);
        // Removing the brain deletes its key too.
        svc.upsert(cloud("https://api.example.com"), Some("k".into()))
            .await
            .unwrap();
        svc.remove("cloud").await.unwrap();
        assert_eq!(svc.secrets.get("cloud").unwrap(), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// PC-6: cloud brains get no personal context unless the user allows it; local brains do by default.
    #[tokio::test]
    async fn personal_context_is_off_for_cloud_brains_until_allowed() {
        let (svc, root) = service("share");
        svc.upsert(cloud("https://api.example.com"), None)
            .await
            .unwrap();
        let view = |snap: &Snapshot, id: &str| {
            snap.brains
                .iter()
                .find(|b| b.config.id == id)
                .unwrap()
                .shares_context
        };
        let snap = svc.snapshot().await;
        assert!(!view(&snap, "cloud"));
        assert!(view(&snap, BUILTIN_ID));
        svc.set_share_context("cloud", true).await.unwrap();
        assert!(view(&svc.snapshot().await, "cloud"));
        svc.set_share_context(BUILTIN_ID, false).await.unwrap();
        assert!(!view(&svc.snapshot().await, BUILTIN_ID));
        // Persisted with the brain.
        let again = BrainService::new(
            root.join("config"),
            root.join("data"),
            root.join("rt"),
            Box::new(MemoryStore::default()),
        );
        assert!(view(&again.snapshot().await, "cloud"));
        assert!(svc.set_share_context("missing", true).await.is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn snapshot_labels_cloud_brains_and_persists_across_restarts() {
        let (svc, root) = service("persist");
        svc.upsert(cloud("https://api.example.com"), None)
            .await
            .unwrap();
        let snap = svc.snapshot().await;
        let c = snap.brains.iter().find(|b| b.config.id == "cloud").unwrap();
        assert!(c.is_cloud);
        assert!(
            !snap
                .brains
                .iter()
                .find(|b| b.config.id == BUILTIN_ID)
                .unwrap()
                .is_cloud
        );
        assert_eq!(snap.builtin, SidecarState::Stopped);
        let again = BrainService::new(
            root.join("config"),
            root.join("data"),
            root.join("rt"),
            Box::new(MemoryStore::default()),
        );
        assert!(again
            .snapshot()
            .await
            .brains
            .iter()
            .any(|b| b.config.id == "cloud"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn the_builtin_brain_needs_a_running_sidecar() {
        let (svc, root) = service("builtin");
        assert_eq!(
            svc.resolve(BUILTIN_ID).await.err(),
            Some(ServiceError::BuiltinNotRunning)
        );
        assert_eq!(
            svc.start_builtin("qwen2.5-1.5b-instruct-q4km", Duration::from_secs(1))
                .await
                .err(),
            Some(ServiceError::ModelMissing(
                "qwen2.5-1.5b-instruct-q4km".into()
            ))
        );
        assert_eq!(
            svc.start_builtin("nope", Duration::from_secs(1))
                .await
                .err(),
            Some(ServiceError::UnknownModel("nope".into()))
        );
        assert!(svc.models().await.iter().all(|m| !m.installed && !m.active));
        let _ = std::fs::remove_dir_all(&root);
    }
    /// Code review SEC-13: a model file swapped after its download is never handed to llama-server.
    #[tokio::test]
    async fn a_model_file_that_changed_after_download_is_refused_and_deleted() {
        let (svc, root) = service("damaged");
        let id = "qwen2.5-1.5b-instruct-q4km";
        let model = models::installed_path(svc.models_dir(), models::find(id).unwrap());
        std::fs::create_dir_all(model.parent().unwrap()).unwrap();
        std::fs::write(&model, b"GGUF not the pinned model").unwrap();
        assert_eq!(
            svc.start_builtin(id, Duration::from_secs(1)).await.err(),
            Some(ServiceError::ModelDamaged(id.into()))
        );
        assert!(!model.exists(), "the damaged file is deleted");
        assert!(
            svc.sidecar.lock().await.is_none(),
            "no model server started"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn a_connection_test_saves_the_proven_capabilities() {
        let server = TestServer::start(|req| match req.path.as_str() {
            "/v1/models" => Response::json(200, serde_json::json!({"data":[{"id":"gpt-x"}]})),
            "/v1/completions" => {
                Response::sse(&[r#"{"choices":[{"text":"x"}]}"#, "[DONE]"], Duration::ZERO)
            }
            "/v1/chat/completions" => Response::sse(
                &[r#"{"choices":[{"delta":{"content":"hi"}}]}"#, "[DONE]"],
                Duration::ZERO,
            ),
            _ => Response::new(404),
        })
        .await;
        let (svc, root) = service("captest");
        svc.upsert(cloud(&server.base), Some("k".into()))
            .await
            .unwrap();
        assert!(
            !svc.snapshot()
                .await
                .brains
                .iter()
                .find(|b| b.config.id == "cloud")
                .unwrap()
                .config
                .capabilities
                .ghost
        );
        let report = svc.test("cloud").await.unwrap();
        assert_eq!(
            report.capabilities,
            Capabilities {
                ghost: true,
                chat: true
            }
        );
        assert!(
            svc.snapshot()
                .await
                .brains
                .iter()
                .find(|b| b.config.id == "cloud")
                .unwrap()
                .config
                .capabilities
                .ghost
        );
        // Now the brain may serve Ghost Text.
        svc.set_role(Role::Ghost, Some("cloud")).await.unwrap();
        assert_eq!(
            svc.resolve_role(Role::Ghost).await.unwrap().config.id,
            "cloud"
        );
        assert_eq!(
            server.last_request().header("authorization"),
            Some("Bearer k")
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
