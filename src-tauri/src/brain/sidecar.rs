//! Lifecycle of the built-in `llama-server` sidecar (requirement BR-8).
//!
//! Security (`privacy-security` steering): bound to `127.0.0.1` only, a random per-launch API key passed
//! through `--api-key-file` (a key on the command line would be visible to every user via `ps`), and the
//! bundled web UI disabled. Flags were checked against the pinned build (`llama-server --help`, b12633).

use crate::supervisor::Backoff;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::process::{Child, Command};
use tokio::sync::{watch, Mutex};

/// Pinned llama.cpp build. Keep in sync with `scripts/versions.env`.
pub const LLAMA_BUILD: &str = "b12633";

#[derive(Debug, Clone, PartialEq)]
pub struct SidecarConfig {
    pub binary: PathBuf,
    pub model: PathBuf,
    /// Context size in tokens. Ghost prompts are short; 4096 keeps RAM low on 16 GB Macs.
    pub ctx_size: u32,
    /// One slot: Ghost Text pins slot 0 so its prompt cache survives between keystrokes.
    pub parallel: u32,
    /// Directory for the key file (per-user, 0700).
    pub runtime_dir: PathBuf,
}

impl SidecarConfig {
    pub fn new(binary: PathBuf, model: PathBuf, runtime_dir: PathBuf) -> Self {
        Self {
            binary,
            model,
            ctx_size: 4096,
            parallel: 1,
            runtime_dir,
        }
    }
}

/// Where the sidecar is reachable once healthy.
#[derive(Clone, PartialEq, Eq)]
pub struct Endpoint {
    pub base_url: String,
    pub api_key: String,
}

impl std::fmt::Debug for Endpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Endpoint")
            .field("base_url", &self.base_url)
            .field("api_key", &"<redacted>")
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum SidecarState {
    Stopped,
    Starting,
    Ready { base_url: String },
    Failed { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SidecarError {
    #[error("llama-server not found (run `make sidecars` or set BOB_LLAMA_SERVER)")]
    BinaryNotFound,
    #[error("model file not found: {0}")]
    ModelNotFound(String),
    #[error("could not start llama-server: {0}")]
    Spawn(String),
    #[error("llama-server did not become healthy within {0:?}")]
    HealthTimeout(Duration),
    #[error("llama-server exited during startup")]
    ExitedEarly,
    #[error("{0}")]
    Io(String),
}

/// Locate `llama-server`: `$BOB_LLAMA_SERVER`, next to the app executable (bundled), then the dev
/// location `src-tauri/binaries/llama/llama-server` created by `make sidecars`.
pub fn locate_binary() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("BOB_LLAMA_SERVER").map(PathBuf::from) {
        return p.is_file().then_some(p);
    }
    let exe = std::env::current_exe().ok()?;
    let mut candidates = vec![];
    if let Some(dir) = exe.parent() {
        candidates.push(dir.join("../Resources/llama/llama-server"));
        candidates.push(dir.join("llama/llama-server"));
    }
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("binaries/llama/llama-server"));
    candidates.into_iter().find(|p| p.is_file())
}

/// 32 random bytes as hex.
pub fn generate_api_key() -> String {
    hex::encode(rand::random::<[u8; 32]>())
}

/// Write the key to `<dir>/llama.key` with mode 0600 inside a 0700 directory.
pub fn write_key_file(dir: &Path, key: &str) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    let path = dir.join("llama.key");
    let tmp = dir.join("llama.key.tmp");
    std::fs::write(&tmp, format!("{key}\n"))?;
    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
    std::fs::rename(&tmp, &path)?;
    Ok(path)
}

/// Command-line arguments for llama-server. Pure, so the security properties are unit-testable.
pub fn build_args(cfg: &SidecarConfig, port: u16, key_file: &Path) -> Vec<String> {
    vec![
        "--model".into(),
        cfg.model.display().to_string(),
        "--host".into(),
        "127.0.0.1".into(),
        "--port".into(),
        port.to_string(),
        "--api-key-file".into(),
        key_file.display().to_string(),
        "--no-webui".into(),
        "--ctx-size".into(),
        cfg.ctx_size.to_string(),
        "--parallel".into(),
        cfg.parallel.to_string(),
        "--n-gpu-layers".into(),
        "99".into(),
        "--flash-attn".into(),
        "auto".into(),
    ]
}

/// Name of the PID file recording the running sidecar, so a crashed BOB session cannot leave an
/// orphaned llama-server (holding a model in memory) behind.
pub const PID_FILE: &str = "llama.pid";

/// Kill a llama-server left over from a previous session. Only kills a process whose executable name is
/// `llama-server`, so a recycled PID belonging to another program is never touched.
pub fn kill_stale(runtime_dir: &Path) -> bool {
    let pid_path = runtime_dir.join(PID_FILE);
    let Ok(text) = std::fs::read_to_string(&pid_path) else {
        return false;
    };
    let _ = std::fs::remove_file(&pid_path);
    let Ok(pid) = text.trim().parse::<u32>() else {
        return false;
    };
    let comm = std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "comm="])
        .output();
    let is_llama = comm
        .ok()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .trim()
                .ends_with("llama-server")
        })
        .unwrap_or(false);
    if is_llama {
        let _ = std::process::Command::new("kill")
            .arg(pid.to_string())
            .status();
    }
    is_llama
}

/// Ask the OS for a free loopback port.
pub fn free_port() -> std::io::Result<u16> {
    Ok(std::net::TcpListener::bind("127.0.0.1:0")?
        .local_addr()?
        .port())
}

/// Poll `/health` until it answers 200 (503 means "loading model"). `/health` needs no API key.
pub async fn wait_healthy(
    http: &reqwest::Client,
    base: &str,
    timeout: Duration,
) -> Result<(), SidecarError> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Ok(r) = http
            .get(format!("{base}/health"))
            .timeout(Duration::from_secs(2))
            .send()
            .await
        {
            if r.status().as_u16() == 200 {
                return Ok(());
            }
        }
        if Instant::now() >= deadline {
            return Err(SidecarError::HealthTimeout(timeout));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Evaluate a prompt into the cache without generating (`n_predict: 0`), so the first real Ghost
/// request after launch does not pay for loading weights into the GPU.
pub async fn warm_up(http: &reqwest::Client, ep: &Endpoint) {
    let _ = http
        .post(format!("{}/completion", ep.base_url))
        .bearer_auth(&ep.api_key)
        .json(&serde_json::json!({"prompt": "Hello", "n_predict": 0, "cache_prompt": true, "id_slot": 0}))
        .timeout(Duration::from_secs(60))
        .send()
        .await;
}

/// A supervised sidecar. Starts on [`Sidecar::start`], restarts on crash, stops on [`Sidecar::stop`]
/// or drop. The current state is observable for the UI.
pub struct Sidecar {
    state_rx: watch::Receiver<SidecarState>,
    endpoint: Arc<Mutex<Option<Endpoint>>>,
    stop_tx: Option<tokio::sync::oneshot::Sender<()>>,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl Sidecar {
    /// Validate paths, then launch the supervision task. Returns once the first start attempt has been
    /// issued; use [`Sidecar::ready`] to wait for health.
    pub fn start(
        cfg: SidecarConfig,
        health_timeout: Duration,
        backoff: Backoff,
    ) -> Result<Self, SidecarError> {
        if !cfg.binary.is_file() {
            return Err(SidecarError::BinaryNotFound);
        }
        if !cfg.model.is_file() {
            return Err(SidecarError::ModelNotFound(cfg.model.display().to_string()));
        }
        if kill_stale(&cfg.runtime_dir) {
            log::warn!("stopped a llama-server left over from a previous session");
        }
        let (state_tx, state_rx) = watch::channel(SidecarState::Starting);
        let endpoint = Arc::new(Mutex::new(None));
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
        let ep = endpoint.clone();
        let task = tokio::spawn(supervise(
            cfg,
            health_timeout,
            backoff,
            state_tx,
            ep,
            stop_rx,
        ));
        Ok(Self {
            state_rx,
            endpoint,
            stop_tx: Some(stop_tx),
            task: Some(task),
        })
    }

    pub fn state(&self) -> SidecarState {
        self.state_rx.borrow().clone()
    }

    pub fn subscribe(&self) -> watch::Receiver<SidecarState> {
        self.state_rx.clone()
    }

    pub async fn endpoint(&self) -> Option<Endpoint> {
        self.endpoint.lock().await.clone()
    }

    /// Wait until the sidecar is ready (or fails), up to `timeout`.
    pub async fn ready(&self, timeout: Duration) -> Result<Endpoint, SidecarError> {
        let mut rx = self.state_rx.clone();
        let wait = async {
            loop {
                match rx.borrow_and_update().clone() {
                    SidecarState::Ready { .. } => return Ok(()),
                    SidecarState::Failed { message } => return Err(SidecarError::Spawn(message)),
                    SidecarState::Stopped => return Err(SidecarError::ExitedEarly),
                    SidecarState::Starting => {}
                }
                if rx.changed().await.is_err() {
                    return Err(SidecarError::ExitedEarly);
                }
            }
        };
        tokio::time::timeout(timeout, wait)
            .await
            .map_err(|_| SidecarError::HealthTimeout(timeout))??;
        self.endpoint().await.ok_or(SidecarError::ExitedEarly)
    }

    pub async fn stop(&mut self) {
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(());
        }
        if let Some(t) = self.task.take() {
            let _ = t.await;
        }
    }
}

impl Drop for Sidecar {
    fn drop(&mut self) {
        // The child is spawned with kill_on_drop, so aborting the task also kills llama-server.
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(());
        }
    }
}

async fn launch(cfg: &SidecarConfig) -> Result<(Child, Endpoint), SidecarError> {
    let key = generate_api_key();
    let key_file =
        write_key_file(&cfg.runtime_dir, &key).map_err(|e| SidecarError::Io(e.to_string()))?;
    let port = free_port().map_err(|e| SidecarError::Io(e.to_string()))?;
    let child = Command::new(&cfg.binary)
        .args(build_args(cfg, port, &key_file))
        // The prebuilt binary finds its dylibs through @rpath relative to itself; nothing else is inherited.
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| SidecarError::Spawn(e.to_string()))?;
    if let Some(pid) = child.id() {
        let _ = std::fs::write(cfg.runtime_dir.join(PID_FILE), pid.to_string());
    }
    Ok((
        child,
        Endpoint {
            base_url: format!("http://127.0.0.1:{port}"),
            api_key: key,
        },
    ))
}

async fn supervise(
    cfg: SidecarConfig,
    health_timeout: Duration,
    backoff: Backoff,
    state: watch::Sender<SidecarState>,
    endpoint: Arc<Mutex<Option<Endpoint>>>,
    mut stop: tokio::sync::oneshot::Receiver<()>,
) {
    let http = reqwest::Client::builder()
        .no_proxy()
        .build()
        .unwrap_or_default();
    let mut attempt = 0u32;
    loop {
        let _ = state.send(SidecarState::Starting);
        let (mut child, ep) = match launch(&cfg).await {
            Ok(x) => x,
            Err(e) => {
                let _ = state.send(SidecarState::Failed {
                    message: e.to_string(),
                });
                return;
            }
        };
        let started = Instant::now();
        let healthy = tokio::select! {
            r = wait_healthy(&http, &ep.base_url, health_timeout) => r,
            _ = child.wait() => Err(SidecarError::ExitedEarly),
            _ = &mut stop => {
                let _ = child.kill().await;
                let _ = std::fs::remove_file(cfg.runtime_dir.join(PID_FILE));
                let _ = state.send(SidecarState::Stopped);
                return;
            }
        };
        match healthy {
            Ok(()) => {
                warm_up(&http, &ep).await;
                *endpoint.lock().await = Some(ep.clone());
                let _ = state.send(SidecarState::Ready {
                    base_url: ep.base_url.clone(),
                });
                tokio::select! {
                    _ = child.wait() => {
                        log::warn!("llama-server exited; restarting");
                    }
                    _ = &mut stop => {
                        let _ = child.kill().await;
                        let _ = std::fs::remove_file(cfg.runtime_dir.join(PID_FILE));
                        *endpoint.lock().await = None;
                        let _ = state.send(SidecarState::Stopped);
                        return;
                    }
                }
                *endpoint.lock().await = None;
                if started.elapsed() > Duration::from_secs(30) {
                    attempt = 0;
                }
            }
            Err(e) => {
                let _ = child.kill().await;
                if attempt >= 5 {
                    let _ = state.send(SidecarState::Failed {
                        message: e.to_string(),
                    });
                    return;
                }
            }
        }
        let delay = backoff.delay(attempt);
        attempt = attempt.saturating_add(1);
        tokio::select! {
            _ = tokio::time::sleep(delay) => {}
            _ = &mut stop => { let _ = state.send(SidecarState::Stopped); return; }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brain::testserver::{Response, TestServer};
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn cfg(binary: PathBuf, model: PathBuf, rt: PathBuf) -> SidecarConfig {
        SidecarConfig::new(binary, model, rt)
    }

    #[test]
    fn args_bind_loopback_use_a_key_file_and_disable_the_web_ui() {
        let c = cfg(
            "/bin/llama-server".into(),
            "/m/model.gguf".into(),
            "/tmp/rt".into(),
        );
        let args = build_args(&c, 4242, Path::new("/tmp/rt/llama.key"));
        let joined = args.join(" ");
        assert!(joined.contains("--host 127.0.0.1"));
        assert!(joined.contains("--port 4242"));
        assert!(joined.contains("--api-key-file /tmp/rt/llama.key"));
        assert!(args.contains(&"--no-webui".to_string()));
        assert!(
            !args.iter().any(|a| a == "--api-key"),
            "the key must never be on the command line"
        );
        for forbidden in [
            "0.0.0.0",
            "--tools",
            "--agent",
            "--ui-mcp-proxy",
            "--mcp-servers-config",
        ] {
            assert!(
                !joined.contains(forbidden),
                "{forbidden} must not be enabled"
            );
        }
    }

    #[test]
    fn api_keys_are_random_and_long() {
        let a = generate_api_key();
        let b = generate_api_key();
        assert_eq!(a.len(), 64);
        assert_ne!(a, b);
    }

    #[test]
    fn key_file_is_private() {
        let dir = std::env::temp_dir().join(format!("bob-key-{}", std::process::id()));
        let p = write_key_file(&dir, "abc").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "abc\n");
        assert_eq!(
            std::fs::metadata(&p).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
            0o700
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn health_wait_tolerates_loading_then_succeeds() {
        let hits = Arc::new(AtomicUsize::new(0));
        let h = hits.clone();
        let server = TestServer::start(move |_| {
            if h.fetch_add(1, Ordering::SeqCst) < 3 {
                Response::json(
                    503,
                    serde_json::json!({"error":{"message":"Loading model"}}),
                )
            } else {
                Response::json(200, serde_json::json!({"status":"ok"}))
            }
        })
        .await;
        wait_healthy(
            &reqwest::Client::new(),
            &server.base,
            Duration::from_secs(5),
        )
        .await
        .unwrap();
        assert!(hits.load(Ordering::SeqCst) >= 4);
    }

    #[tokio::test]
    async fn health_wait_times_out() {
        let server = TestServer::start(|_| Response::new(503)).await;
        assert!(matches!(
            wait_healthy(
                &reqwest::Client::new(),
                &server.base,
                Duration::from_millis(300)
            )
            .await,
            Err(SidecarError::HealthTimeout(_))
        ));
    }

    #[test]
    fn start_validates_paths_before_spawning() {
        let rt = std::env::temp_dir();
        let missing = cfg(
            "/nonexistent/llama-server".into(),
            "/nonexistent/m.gguf".into(),
            rt.clone(),
        );
        assert_eq!(
            Sidecar::start(missing, Duration::from_secs(1), Backoff::default()).err(),
            Some(SidecarError::BinaryNotFound)
        );
        let no_model = cfg("/bin/sh".into(), "/nonexistent/m.gguf".into(), rt);
        assert!(matches!(
            Sidecar::start(no_model, Duration::from_secs(1), Backoff::default()).err(),
            Some(SidecarError::ModelNotFound(_))
        ));
    }

    /// A fake llama-server (python3 one-liner) that serves /health and records every launch, so restart
    /// and stop behavior are tested without a real model.
    ///
    /// `crash_after_ready`: exit 0.5 s after the first successful `/health`. Tying the crash to readiness
    /// (not to launch time) keeps the test valid on slow CI runners, where Python start-up alone can exceed
    /// a fixed timer (seen on GitHub Actions: every attempt died before becoming healthy).
    fn fake_server(dir: &Path, crash_after_ready: bool) -> PathBuf {
        let launches = dir.join("launches");
        let script = dir.join("fake-llama-server");
        let die = if crash_after_ready {
            "CRASH = True"
        } else {
            "CRASH = False"
        };
        std::fs::write(
            &script,
            format!(
                r#"#!/usr/bin/env python3
import sys, os, threading, http.server
args = sys.argv
port = int(args[args.index("--port") + 1])
assert args[args.index("--host") + 1] == "127.0.0.1"
key = open(args[args.index("--api-key-file") + 1]).read().strip()
open("{launches}", "a").write("x\n")
{die}
class H(http.server.BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def do_GET(self):
        global CRASH
        self.send_response(200 if self.path == "/health" else 404); self.end_headers(); self.wfile.write(b'{{"status":"ok"}}')
        if CRASH and self.path == "/health":
            CRASH = False
            threading.Timer(0.5, lambda: os._exit(1)).start()
    def do_POST(self):
        ok = self.headers.get("Authorization") == "Bearer " + key
        self.send_response(200 if ok else 401); self.end_headers(); self.wfile.write(b"{{}}")
http.server.HTTPServer(("127.0.0.1", port), H).serve_forever()
"#,
                launches = launches.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        script
    }

    fn launches(dir: &Path) -> usize {
        std::fs::read_to_string(dir.join("launches"))
            .map(|s| s.lines().count())
            .unwrap_or(0)
    }

    fn python_available() -> bool {
        std::process::Command::new("python3")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    #[tokio::test]
    async fn starts_reports_ready_and_stops_the_process() {
        if !python_available() {
            eprintln!("SKIPPED: python3 not available");
            return;
        }
        let dir = std::env::temp_dir().join(format!("bob-side-ok-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let model = dir.join("m.gguf");
        std::fs::write(&model, b"x").unwrap();
        let bin = fake_server(&dir, false);
        let mut s = Sidecar::start(
            cfg(bin, model, dir.join("rt")),
            Duration::from_secs(10),
            Backoff::default(),
        )
        .unwrap();
        let ep = s.ready(Duration::from_secs(15)).await.unwrap();
        assert!(ep.base_url.starts_with("http://127.0.0.1:"));
        assert!(matches!(s.state(), SidecarState::Ready { .. }));
        // The key in the endpoint is the one the server accepts.
        let ok = reqwest::Client::new()
            .post(format!("{}/completion", ep.base_url))
            .bearer_auth(&ep.api_key)
            .send()
            .await
            .unwrap();
        assert_eq!(ok.status().as_u16(), 200);
        let bad = reqwest::Client::new()
            .post(format!("{}/completion", ep.base_url))
            .bearer_auth("wrong")
            .send()
            .await
            .unwrap();
        assert_eq!(bad.status().as_u16(), 401);
        assert!(
            dir.join("rt").join(PID_FILE).exists(),
            "running sidecar records its pid"
        );
        s.stop().await;
        assert_eq!(s.state(), SidecarState::Stopped);
        assert!(
            !dir.join("rt").join(PID_FILE).exists(),
            "pid file removed on stop"
        );
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(
            reqwest::Client::new()
                .get(format!("{}/health", ep.base_url))
                .send()
                .await
                .is_err(),
            "process still running after stop"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn kill_stale_ignores_pids_that_are_not_llama_server() {
        let dir = std::env::temp_dir().join(format!("bob-stale-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // Our own PID: a real process that is not llama-server must survive.
        std::fs::write(dir.join(PID_FILE), std::process::id().to_string()).unwrap();
        assert!(!kill_stale(&dir));
        assert!(
            !dir.join(PID_FILE).exists(),
            "the stale pid file is always removed"
        );
        std::fs::write(dir.join(PID_FILE), "not a pid").unwrap();
        assert!(!kill_stale(&dir));
        assert!(!kill_stale(&dir.join("missing")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn restarts_after_a_crash() {
        if !python_available() {
            eprintln!("SKIPPED: python3 not available");
            return;
        }
        let dir = std::env::temp_dir().join(format!("bob-side-crash-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let model = dir.join("m.gguf");
        std::fs::write(&model, b"x").unwrap();
        let bin = fake_server(&dir, true);
        let fast = Backoff {
            base: Duration::from_millis(50),
            max: Duration::from_millis(200),
        };
        let mut s = Sidecar::start(
            cfg(bin, model, dir.join("rt")),
            Duration::from_secs(10),
            fast,
        )
        .unwrap();
        s.ready(Duration::from_secs(15)).await.unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        while launches(&dir) < 2 {
            assert!(
                Instant::now() < deadline,
                "sidecar was not restarted after it died"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        s.stop().await;
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Real llama-server + real model. Run manually after `make sidecars` and downloading a model:
    /// `BOB_TEST_MODEL=/path/to/model.gguf cargo test real_llama_server -- --ignored --nocapture`
    #[tokio::test]
    #[ignore = "needs the real llama-server binary and a GGUF model"]
    async fn real_llama_server_streams_a_ghost_completion() {
        use crate::brain::client::HttpBrain;
        use crate::brain::{registry, BrainConfig, CompletionRequest};
        use futures_util::StreamExt;
        let bin = locate_binary().expect("run `make sidecars` first");
        let model = PathBuf::from(std::env::var("BOB_TEST_MODEL").expect("set BOB_TEST_MODEL"));
        let rt = std::env::temp_dir().join(format!("bob-real-{}", std::process::id()));
        let mut s = Sidecar::start(
            cfg(bin, model, rt),
            Duration::from_secs(120),
            Backoff::default(),
        )
        .unwrap();
        let ep = s.ready(Duration::from_secs(180)).await.unwrap();
        let brain = HttpBrain::new(
            BrainConfig {
                endpoint: ep.base_url.clone(),
                ..registry::builtin_brain()
            },
            Some(ep.api_key.clone()),
        )
        .unwrap();
        for prompt in [
            "The quick brown fox jumps over the",
            "Thanks for your email. I will get back to you",
        ] {
            let t0 = Instant::now();
            let mut stream = brain
                .complete_stream(&CompletionRequest::ghost(prompt))
                .await
                .unwrap();
            let mut first = None;
            let mut text = String::new();
            while let Some(tok) = stream.next().await {
                first.get_or_insert(t0.elapsed());
                text.push_str(&tok.unwrap());
            }
            println!(
                "prompt={prompt:?} first_token={:?} total={:?} text={text:?}",
                first,
                t0.elapsed()
            );
            assert!(!text.is_empty());
        }
        s.stop().await;
    }
}
