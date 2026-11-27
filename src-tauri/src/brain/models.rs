//! Model catalog and verified downloads (requirement BR-9).
//!
//! Downloads go to `<models dir>/<file>.part`, resume with an HTTP `Range` request, hash every byte with
//! SHA-256 (including bytes from an earlier partial download) and are only renamed into place when size
//! and hash both match. A file that fails verification is deleted, never loaded (property P3).
//!
//! Every catalog entry is Apache-2.0 licensed (checked on the Hugging Face model cards, 2026-11-22).
//! Models are downloaded by the user on first run; the repository never contains model files.

use futures_util::StreamExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModelEntry {
    pub id: &'static str,
    pub name: &'static str,
    /// What the model is recommended for, shown in the UI.
    pub role: &'static str,
    pub file: &'static str,
    pub url: &'static str,
    pub size: u64,
    pub sha256: &'static str,
    pub license: &'static str,
    /// Minimum RAM in GB for a good experience alongside other apps.
    pub min_ram_gb: u32,
}

/// Curated catalog. Sizes and hashes come from the Hugging Face API (`/api/models/<repo>/tree/main`,
/// LFS oid = SHA-256), read 2026-11-22.
pub const CATALOG: &[ModelEntry] = &[
    ModelEntry {
        id: "qwen2.5-1.5b-instruct-q4km",
        name: "Qwen2.5 1.5B Instruct (Q4_K_M)",
        role: "Default: best Ghost Text in our benchmark, about 1.4 GB of memory",
        file: "qwen2.5-1.5b-instruct-q4_k_m.gguf",
        url: "https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct-GGUF/resolve/main/qwen2.5-1.5b-instruct-q4_k_m.gguf",
        size: 1_117_320_736,
        sha256: "6a1a2eb6d15622bf3c96857206351ba97e1af16c30d7a74ee38970e434e9407e",
        license: "Apache-2.0",
        min_ram_gb: 16,
    },
    ModelEntry {
        id: "qwen3-4b-instruct-2507-q4km",
        name: "Qwen3 4B Instruct 2507 (Q4_K_M)",
        role: "Larger model for Selection Actions; no Ghost Text gain, about 3.1 GB of memory",
        file: "Qwen3-4B-Instruct-2507-Q4_K_M.gguf",
        url: "https://huggingface.co/unsloth/Qwen3-4B-Instruct-2507-GGUF/resolve/main/Qwen3-4B-Instruct-2507-Q4_K_M.gguf",
        size: 2_497_281_120,
        sha256: "3605803b982cb64aead44f6c1b2ae36e3acdb41d8e46c8a94c6533bc4c67e597",
        license: "Apache-2.0",
        min_ram_gb: 16,
    },
    ModelEntry {
        id: "qwen3-1.7b-base-q4km",
        name: "Qwen3 1.7B Base (Q4_K_M, experimental)",
        role: "Experimental base model; slightly below the default in our benchmark",
        file: "Qwen3-1.7B-Base.Q4_K_M.gguf",
        url: "https://huggingface.co/mradermacher/Qwen3-1.7B-Base-GGUF/resolve/main/Qwen3-1.7B-Base.Q4_K_M.gguf",
        size: 1_107_409_024,
        sha256: "8b5d946a169e62dd49444be8858c8a7f06067573de990b2bc19380d3a260b5fa",
        license: "Apache-2.0",
        min_ram_gb: 16,
    },
];

/// Speech-to-text models for Voice Dictation (whisper.cpp, run inside the Swift helper). Pinned to one
/// revision of `ggerganov/whisper.cpp` (MIT); sizes and SHA-256 from the Hugging Face API, read 2026-11-23.
/// Measured in docs/PERFORMANCE.md.
pub const SPEECH_CATALOG: &[ModelEntry] = &[
    ModelEntry {
        id: "whisper-large-v3-turbo-q5",
        name: "Whisper large-v3 turbo (Q5_0)",
        role: "Default: accurate in many languages, about 0.3 s for a short phrase on Apple Silicon, about 0.9 GB of memory",
        file: "ggml-large-v3-turbo-q5_0.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-large-v3-turbo-q5_0.bin",
        size: 574_041_195,
        sha256: "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2",
        license: "MIT",
        min_ram_gb: 16,
    },
    ModelEntry {
        id: "whisper-base",
        name: "Whisper base",
        role: "Light and very fast; good for English, weak for other languages, about 0.45 GB of memory",
        file: "ggml-base.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-base.bin",
        size: 147_951_465,
        sha256: "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe",
        license: "MIT",
        min_ram_gb: 8,
    },
];

/// A language model for the built-in brain (never a speech model).
pub fn find(id: &str) -> Option<&'static ModelEntry> {
    CATALOG.iter().find(|m| m.id == id)
}

pub fn find_speech(id: &str) -> Option<&'static ModelEntry> {
    SPEECH_CATALOG.iter().find(|m| m.id == id)
}

/// Any downloadable model, language or speech.
pub fn find_any(id: &str) -> Option<&'static ModelEntry> {
    find(id).or_else(|| find_speech(id))
}

/// `~/Library/Application Support/BOB/models` (macOS) or the platform equivalent.
pub fn models_dir(app_data: &Path) -> PathBuf {
    app_data.join("models")
}

pub fn installed_path(dir: &Path, m: &ModelEntry) -> PathBuf {
    dir.join(m.file)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Progress {
    pub downloaded: u64,
    pub total: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DownloadError {
    #[error("download failed: {0}")]
    Network(String),
    #[error("server answered HTTP {0}")]
    Http(u16),
    #[error("downloaded file is {got} bytes, expected {expected}")]
    SizeMismatch { got: u64, expected: u64 },
    #[error("downloaded file failed SHA-256 verification; it was deleted")]
    HashMismatch,
    #[error("disk error: {0}")]
    Io(String),
    #[error("download cancelled")]
    Cancelled,
}

fn io(e: std::io::Error) -> DownloadError {
    DownloadError::Io(e.to_string())
}

/// Hash an existing partial file so a resumed download is verified end to end.
async fn hash_existing(path: &Path, hasher: &mut Sha256) -> Result<u64, DownloadError> {
    let Ok(mut f) = tokio::fs::File::open(path).await else {
        return Ok(0);
    };
    let mut buf = vec![0u8; 1 << 20];
    let mut total = 0u64;
    loop {
        let n = f.read(&mut buf).await.map_err(io)?;
        if n == 0 {
            return Ok(total);
        }
        hasher.update(&buf[..n]);
        total += n as u64;
    }
}

/// Spec for one download (decoupled from [`ModelEntry`] so tests can use any URL and content).
#[derive(Debug, Clone)]
pub struct DownloadSpec {
    pub url: String,
    pub size: u64,
    pub sha256: String,
}

impl From<&ModelEntry> for DownloadSpec {
    fn from(m: &ModelEntry) -> Self {
        Self {
            url: m.url.into(),
            size: m.size,
            sha256: m.sha256.into(),
        }
    }
}

/// Download `spec` to `dest`, resuming `<dest>.part` if present. `on_progress` is called at most every
/// 4 MiB; `cancelled` is polled between chunks.
pub async fn download(
    http: &reqwest::Client,
    spec: &DownloadSpec,
    dest: &Path,
    mut on_progress: impl FnMut(Progress),
    cancelled: impl Fn() -> bool,
) -> Result<PathBuf, DownloadError> {
    if let Some(dir) = dest.parent() {
        tokio::fs::create_dir_all(dir).await.map_err(io)?;
    }
    let part = dest.with_extension(format!(
        "{}.part",
        dest.extension().and_then(|e| e.to_str()).unwrap_or("bin")
    ));

    let mut hasher = Sha256::new();
    let mut have = hash_existing(&part, &mut hasher).await?;
    if have > spec.size {
        tokio::fs::remove_file(&part).await.map_err(io)?;
        hasher = Sha256::new();
        have = 0;
    }

    let mut req = http.get(&spec.url);
    if have > 0 {
        req = req.header("range", format!("bytes={have}-"));
    }
    let resp = req
        .send()
        .await
        .map_err(|e| DownloadError::Network(e.to_string()))?;
    let status = resp.status().as_u16();
    let mut file = match status {
        206 if have > 0 => tokio::fs::OpenOptions::new()
            .append(true)
            .open(&part)
            .await
            .map_err(io)?,
        200 => {
            // Server ignored the range (or this is a fresh start): begin again.
            hasher = Sha256::new();
            have = 0;
            tokio::fs::File::create(&part).await.map_err(io)?
        }
        // The partial file already holds everything.
        416 if have == spec.size => tokio::fs::OpenOptions::new()
            .append(true)
            .open(&part)
            .await
            .map_err(io)?,
        s => return Err(DownloadError::Http(s)),
    };

    let mut stream = resp.bytes_stream();
    let mut last_report = 0u64;
    on_progress(Progress {
        downloaded: have,
        total: spec.size,
    });
    while let Some(chunk) = stream.next().await {
        if cancelled() {
            file.flush().await.map_err(io)?;
            return Err(DownloadError::Cancelled);
        }
        let bytes = chunk.map_err(|e| DownloadError::Network(e.to_string()))?;
        if have + bytes.len() as u64 > spec.size {
            drop(file);
            let _ = tokio::fs::remove_file(&part).await;
            return Err(DownloadError::SizeMismatch {
                got: have + bytes.len() as u64,
                expected: spec.size,
            });
        }
        file.write_all(&bytes).await.map_err(io)?;
        hasher.update(&bytes);
        have += bytes.len() as u64;
        if have - last_report >= 4 << 20 {
            last_report = have;
            on_progress(Progress {
                downloaded: have,
                total: spec.size,
            });
        }
    }
    file.flush().await.map_err(io)?;
    drop(file);
    on_progress(Progress {
        downloaded: have,
        total: spec.size,
    });

    if have != spec.size {
        // Keep the partial file: the next attempt resumes it.
        return Err(DownloadError::SizeMismatch {
            got: have,
            expected: spec.size,
        });
    }
    if hex::encode(hasher.finalize()) != spec.sha256.to_ascii_lowercase() {
        let _ = tokio::fs::remove_file(&part).await;
        return Err(DownloadError::HashMismatch);
    }
    tokio::fs::rename(&part, dest).await.map_err(io)?;
    // Just hashed: no second pass before the first load.
    remember(dest);
    Ok(dest.to_path_buf())
}

/// Verify an installed file's SHA-256 (no caching; see [`verified`] for the check before loading).
pub async fn verify(path: &Path, sha256: &str) -> Result<bool, DownloadError> {
    let mut hasher = Sha256::new();
    hash_existing(path, &mut hasher).await?;
    Ok(hex::encode(hasher.finalize()) == sha256.to_ascii_lowercase())
}

// ---- Verification before loading (code review SEC-13) ----
//
// Any process of the user can write the models folder, so a file is hashed again before BOB hands it to
// llama-server or to the helper's whisper, once per launch. The result is kept in memory only (a stamp
// file could be forged) and is tied to the file's identity: device, inode, size, mtime and ctime. Writing,
// replacing or `touch`ing the file changes ctime, which the owner cannot set back, so it is hashed again.
// What remains is a change between the last check and the moment the other program opens the file.

/// The identity of a file's current contents, as far as the file system tells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Stamp {
    dev: u64,
    ino: u64,
    len: u64,
    mtime: (i64, i64),
    ctime: (i64, i64),
}

fn stamp(path: &Path) -> Option<Stamp> {
    use std::os::unix::fs::MetadataExt;
    // `symlink_metadata`: a link swapped in for the model is not followed (it is not a file).
    let m = std::fs::symlink_metadata(path).ok()?;
    m.is_file().then(|| Stamp {
        dev: m.dev(),
        ino: m.ino(),
        len: m.len(),
        mtime: (m.mtime(), m.mtime_nsec()),
        ctime: (m.ctime(), m.ctime_nsec()),
    })
}

/// What is known about a model file right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Integrity {
    /// Hashed this launch and unchanged since.
    Verified,
    /// Not hashed yet (or changed since): call [`verified`] or [`verify_in_background`].
    Unchecked,
    /// Not there (or not a regular file).
    Missing,
}

type VerifiedFiles = std::sync::Mutex<std::collections::HashMap<PathBuf, Stamp>>;

fn verified_files() -> &'static VerifiedFiles {
    static FILES: std::sync::OnceLock<VerifiedFiles> = std::sync::OnceLock::new();
    FILES.get_or_init(Default::default)
}

fn lock_verified() -> std::sync::MutexGuard<'static, std::collections::HashMap<PathBuf, Stamp>> {
    verified_files()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Cheap (one `lstat`): whether `path` was verified this launch and has not changed since.
pub fn integrity(path: &Path) -> Integrity {
    let Some(now) = stamp(path) else {
        return Integrity::Missing;
    };
    if lock_verified().get(path) == Some(&now) {
        Integrity::Verified
    } else {
        Integrity::Unchecked
    }
}

/// Remember a file whose content BOB has just hashed itself (a finished download).
fn remember(path: &Path) {
    if let Some(s) = stamp(path) {
        lock_verified().insert(path.to_path_buf(), s);
    }
}

/// SHA-256 of a file, lowercase hex (blocking).
fn hash_file(path: &Path) -> std::io::Result<String> {
    use std::io::Read;
    let mut f = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            return Ok(hex::encode(hasher.finalize()));
        }
        hasher.update(&buf[..n]);
    }
}

/// Hash `path` (blocking) and compare it with `sha256`. A file that does not match is deleted and never
/// loaded (P3); the user can download it again.
pub fn verify_blocking(path: &Path, sha256: &str) -> Result<bool, DownloadError> {
    if integrity(path) == Integrity::Verified {
        return Ok(true);
    }
    let before =
        stamp(path).ok_or_else(|| DownloadError::Io("the model file is missing".into()))?;
    let started = std::time::Instant::now();
    let ok = hash_file(path).map_err(io)? == sha256.to_ascii_lowercase();
    // A file that changed while it was read is not trusted either way: report it as not matching only
    // if the hash failed, and do not remember it.
    let unchanged = stamp(path) == Some(before);
    log::info!(
        "model check: {} bytes in {} ms, {}",
        before.len,
        started.elapsed().as_millis(),
        if !ok {
            "mismatch"
        } else if unchanged {
            "ok"
        } else {
            "changed while read"
        }
    );
    if !ok {
        crate::audit::record(
            "model.mismatch",
            serde_json::json!({ "bytes": before.len, "deleted": true }),
        );
        let _ = std::fs::remove_file(path);
        lock_verified().remove(path);
        return Ok(false);
    }
    if !unchanged {
        return Err(DownloadError::Io(
            "the model file changed while BOB checked it".into(),
        ));
    }
    lock_verified().insert(path.to_path_buf(), before);
    Ok(true)
}

/// Tests and the simulator use empty stand-in model files: treat one as verified.
#[cfg(test)]
pub fn assume_verified(path: &Path) {
    remember(path);
}

/// [`verify_blocking`] off the async runtime's threads.
pub async fn verified(path: &Path, sha256: &str) -> Result<bool, DownloadError> {
    if integrity(path) == Integrity::Verified {
        return Ok(true);
    }
    let (p, s) = (path.to_path_buf(), sha256.to_string());
    tokio::task::spawn_blocking(move || verify_blocking(&p, &s))
        .await
        .map_err(|e| DownloadError::Io(e.to_string()))?
}

/// Start [`verify_blocking`] on its own thread unless one already runs for `path`; `done` gets the
/// result. For callers without an async runtime (the dictation configuration).
pub fn verify_in_background(
    path: PathBuf,
    sha256: &'static str,
    done: impl FnOnce(Result<bool, DownloadError>) + Send + 'static,
) {
    static RUNNING: std::sync::Mutex<Vec<PathBuf>> = std::sync::Mutex::new(Vec::new());
    {
        let mut running = RUNNING
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if running.contains(&path) {
            return;
        }
        running.push(path.clone());
    }
    let spawned = std::thread::Builder::new()
        .name("bob-model-check".into())
        .spawn({
            let path = path.clone();
            move || {
                let res = verify_blocking(&path, sha256);
                RUNNING
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .retain(|p| p != &path);
                done(res);
            }
        });
    if spawned.is_err() {
        RUNNING
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .retain(|p| p != &path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brain::testserver::{Response, TestServer};
    use std::sync::Arc;
    use std::time::Duration;

    fn sha(b: &[u8]) -> String {
        hex::encode(Sha256::digest(b))
    }

    fn content(n: usize) -> Vec<u8> {
        (0..n).map(|i| (i * 31 % 251) as u8).collect()
    }

    /// Serves `data`, honoring `Range: bytes=N-` with 206 responses.
    async fn range_server(data: Vec<u8>) -> TestServer {
        let data = Arc::new(data);
        TestServer::start(move |req| {
            if let Some(r) = req.header("range") {
                let start: usize = r
                    .trim_start_matches("bytes=")
                    .trim_end_matches('-')
                    .parse()
                    .unwrap();
                if start >= data.len() {
                    return Response::new(416);
                }
                return Response::new(206)
                    .header("content-length", &(data.len() - start).to_string())
                    .body(data[start..].to_vec());
            }
            Response::new(200)
                .header("content-length", &data.len().to_string())
                .body(data.to_vec())
        })
        .await
    }

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("bob-dl-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn catalog_entries_are_well_formed_and_permissively_licensed() {
        let mut ids = std::collections::HashSet::new();
        for m in CATALOG {
            assert!(ids.insert(m.id), "duplicate id {}", m.id);
            assert_eq!(m.sha256.len(), 64);
            assert!(m.sha256.chars().all(|c| c.is_ascii_hexdigit()));
            assert!(m.url.starts_with("https://huggingface.co/") && m.url.ends_with(m.file));
            assert!(m.file.ends_with(".gguf") && m.size > 100_000_000);
            assert_eq!(
                m.license, "Apache-2.0",
                "{} must be permissively licensed",
                m.id
            );
        }
        assert!(find("qwen2.5-1.5b-instruct-q4km").is_some());
        for m in SPEECH_CATALOG {
            assert!(ids.insert(m.id), "duplicate id {}", m.id);
            assert_eq!(m.sha256.len(), 64);
            assert!(m
                .url
                .starts_with("https://huggingface.co/ggerganov/whisper.cpp/resolve/"));
            assert!(m.url.ends_with(m.file) && m.file.ends_with(".bin"));
            assert_eq!(m.license, "MIT");
            // A speech model can never be started as the built-in language model.
            assert!(find(m.id).is_none() && find_any(m.id).is_some());
        }
    }

    #[tokio::test]
    async fn downloads_verifies_and_installs() {
        let data = content(300_000);
        let server = range_server(data.clone()).await;
        let dir = tmpdir("ok");
        let dest = dir.join("m.gguf");
        let spec = DownloadSpec {
            url: format!("{}/m.gguf", server.base),
            size: data.len() as u64,
            sha256: sha(&data),
        };
        let mut last = None;
        let path = download(
            &reqwest::Client::new(),
            &spec,
            &dest,
            |p| last = Some(p),
            || false,
        )
        .await
        .unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), data);
        assert_eq!(
            last,
            Some(Progress {
                downloaded: 300_000,
                total: 300_000
            })
        );
        assert!(!dir.join("m.gguf.part").exists());
        assert!(verify(&path, &spec.sha256).await.unwrap());
    }

    #[tokio::test]
    async fn resumes_a_partial_download_with_a_range_request() {
        let data = content(200_000);
        let server = range_server(data.clone()).await;
        let dir = tmpdir("resume");
        let dest = dir.join("m.gguf");
        std::fs::write(dir.join("m.gguf.part"), &data[..80_000]).unwrap();
        let spec = DownloadSpec {
            url: format!("{}/m.gguf", server.base),
            size: data.len() as u64,
            sha256: sha(&data),
        };
        download(&reqwest::Client::new(), &spec, &dest, |_| {}, || false)
            .await
            .unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), data);
        assert_eq!(server.last_request().header("range"), Some("bytes=80000-"));
    }

    #[tokio::test]
    async fn a_corrupt_partial_file_is_caught_by_the_end_to_end_hash() {
        // The first 1000 bytes on disk are wrong; the resumed tail is right. The hash covers both.
        let data = content(50_000);
        let server = range_server(data.clone()).await;
        let dir = tmpdir("corrupt-part");
        let dest = dir.join("m.gguf");
        std::fs::write(dir.join("m.gguf.part"), vec![0u8; 1000]).unwrap();
        let spec = DownloadSpec {
            url: format!("{}/m.gguf", server.base),
            size: data.len() as u64,
            sha256: sha(&data),
        };
        let err = download(&reqwest::Client::new(), &spec, &dest, |_| {}, || false)
            .await
            .unwrap_err();
        assert_eq!(err, DownloadError::HashMismatch);
        assert!(!dest.exists() && !dir.join("m.gguf.part").exists());
    }

    #[tokio::test]
    async fn a_truncated_download_keeps_the_part_file_for_resume() {
        let data = content(10_000);
        let short = data[..6_000].to_vec();
        let server = TestServer::start(move |_| Response::new(200).body(short.clone())).await;
        let dir = tmpdir("short");
        let dest = dir.join("m.gguf");
        let spec = DownloadSpec {
            url: server.base.clone(),
            size: data.len() as u64,
            sha256: sha(&data),
        };
        let err = download(&reqwest::Client::new(), &spec, &dest, |_| {}, || false)
            .await
            .unwrap_err();
        assert_eq!(
            err,
            DownloadError::SizeMismatch {
                got: 6_000,
                expected: 10_000
            }
        );
        assert!(!dest.exists());
        assert_eq!(
            std::fs::metadata(dir.join("m.gguf.part")).unwrap().len(),
            6_000
        );
    }

    #[tokio::test]
    async fn an_oversized_response_is_rejected_and_removed() {
        let big = content(20_000);
        let server = TestServer::start(move |_| Response::new(200).body(big.clone())).await;
        let dir = tmpdir("big");
        let dest = dir.join("m.gguf");
        let spec = DownloadSpec {
            url: server.base.clone(),
            size: 5_000,
            sha256: "0".repeat(64),
        };
        assert!(matches!(
            download(&reqwest::Client::new(), &spec, &dest, |_| {}, || false).await,
            Err(DownloadError::SizeMismatch { .. })
        ));
        assert!(!dest.exists() && !dir.join("m.gguf.part").exists());
    }

    #[tokio::test]
    async fn cancellation_stops_the_download_and_keeps_progress() {
        let server = TestServer::start(|_| {
            let mut r = Response::new(200);
            for _ in 0..20 {
                r = r.chunk_after(Duration::from_millis(10), vec![7u8; 1000]);
            }
            r
        })
        .await;
        let dir = tmpdir("cancel");
        let dest = dir.join("m.gguf");
        let spec = DownloadSpec {
            url: server.base.clone(),
            size: 20_000,
            sha256: "0".repeat(64),
        };
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let err = download(
            &reqwest::Client::new(),
            &spec,
            &dest,
            |_| {},
            || calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) >= 3,
        )
        .await
        .unwrap_err();
        assert_eq!(err, DownloadError::Cancelled);
        assert!(!dest.exists());
        assert!(dir.join("m.gguf.part").exists());
    }

    #[tokio::test]
    async fn http_errors_are_reported() {
        let server = TestServer::start(|_| Response::new(404)).await;
        let dir = tmpdir("404");
        let spec = DownloadSpec {
            url: server.base.clone(),
            size: 10,
            sha256: "0".repeat(64),
        };
        assert_eq!(
            download(
                &reqwest::Client::new(),
                &spec,
                &dir.join("m.gguf"),
                |_| {},
                || false
            )
            .await,
            Err(DownloadError::Http(404))
        );
    }

    /// Code review SEC-13: verified once, trusted while unchanged, checked again after any write.
    #[tokio::test]
    async fn a_model_is_verified_once_and_again_after_it_changes() {
        let dir = tmpdir("integrity");
        let path = dir.join("m.gguf");
        let good = content(4096);
        std::fs::write(&path, &good).unwrap();
        assert_eq!(integrity(&path), Integrity::Unchecked);
        assert!(verified(&path, &sha(&good)).await.unwrap());
        assert_eq!(integrity(&path), Integrity::Verified);
        // Same size, same content length, new bytes: the ctime changes, so it is no longer trusted.
        let mut bad = good.clone();
        bad[0] ^= 1;
        std::fs::write(&path, &bad).unwrap();
        assert_eq!(integrity(&path), Integrity::Unchecked);
        assert!(!verified(&path, &sha(&good)).await.unwrap());
        assert!(!path.exists(), "a file that does not match is deleted");
        assert_eq!(integrity(&path), Integrity::Missing);
        // A symlink in place of the model is not a model file.
        let target = dir.join("elsewhere.gguf");
        std::fs::write(&target, &good).unwrap();
        std::os::unix::fs::symlink(&target, &path).unwrap();
        assert_eq!(integrity(&path), Integrity::Missing);
        assert!(verified(&path, &sha(&good)).await.is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_finished_download_counts_as_verified() {
        let data = content(10_000);
        let body = data.clone();
        let server = TestServer::start(move |_| Response::new(200).body(body.clone())).await;
        let dir = tmpdir("dl-verified");
        let dest = dir.join("m.gguf");
        let spec = DownloadSpec {
            url: server.base.clone(),
            size: data.len() as u64,
            sha256: sha(&data),
        };
        download(&reqwest::Client::new(), &spec, &dest, |_| {}, || false)
            .await
            .unwrap();
        assert_eq!(integrity(&dest), Integrity::Verified);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn background_verification_reports_its_result() {
        let dir = std::env::temp_dir().join(format!("bob-dl-bg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("m.bin");
        let data = content(2048);
        std::fs::write(&path, &data).unwrap();
        // The background API takes a 'static hash, as the catalog's are.
        let hash: &'static str = Box::leak(sha(&data).into_boxed_str());
        let (tx, rx) = std::sync::mpsc::channel();
        verify_in_background(path.clone(), hash, move |r| tx.send(r).unwrap());
        assert_eq!(rx.recv_timeout(Duration::from_secs(10)).unwrap(), Ok(true));
        assert_eq!(integrity(&path), Integrity::Verified);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Measures the check on an installed catalog model (docs/PERFORMANCE.md). Read-only: it hashes
    /// without the delete-on-mismatch of `verify_blocking`. `BOB_MODEL_FILE=<path to a catalog file>`.
    #[test]
    #[ignore]
    fn real_model_check_time() {
        let Some(p) = std::env::var_os("BOB_MODEL_FILE").map(PathBuf::from) else {
            eprintln!("set BOB_MODEL_FILE to an installed catalog model");
            return;
        };
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        let entry = CATALOG
            .iter()
            .chain(SPEECH_CATALOG)
            .find(|m| m.file == name)
            .expect("not a catalog file");
        for run in 1..=3 {
            let t = std::time::Instant::now();
            assert_eq!(hash_file(&p).unwrap(), entry.sha256);
            eprintln!(
                "{} ({} bytes) run {run}: {} ms",
                entry.id,
                entry.size,
                t.elapsed().as_millis()
            );
        }
    }

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(24))]
        /// BR-9 / P3: whatever bytes the server sends, the file is installed if and only if they are
        /// exactly the expected content.
        #[test]
        fn only_verified_content_is_ever_installed(
            expected in proptest::collection::vec(proptest::prelude::any::<u8>(), 1..2000),
            served in proptest::collection::vec(proptest::prelude::any::<u8>(), 0..2000),
            serve_expected in proptest::prelude::any::<bool>(),
        ) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let body = if serve_expected { expected.clone() } else { served.clone() };
                let server = TestServer::start(move |_| Response::new(200).body(body.clone())).await;
                let dir = tmpdir(&format!("prop-{}", rand::random::<u64>()));
                let dest = dir.join("m.gguf");
                let spec = DownloadSpec { url: server.base.clone(), size: expected.len() as u64, sha256: sha(&expected) };
                let res = download(&reqwest::Client::new(), &spec, &dest, |_| {}, || false).await;
                let should_install = serve_expected || served == expected;
                assert_eq!(res.is_ok(), should_install);
                assert_eq!(dest.exists(), should_install);
                if dest.exists() { assert_eq!(std::fs::read(&dest).unwrap(), expected); }
                let _ = std::fs::remove_dir_all(&dir);
            });
        }
    }
}
