//! Where BOB keeps its files and sockets, decided in one place.
//!
//! Normally: settings in the app's config folder, models in its data folder, logs in `~/Library/Logs/BOB`,
//! and the sockets, the single-instance lock and the model server's key in `$TMPDIR/bob-<uid>` (0700).
//!
//! With `BOB_HOME=/absolute/folder`, everything BOB writes moves under that folder (`config/`, `logs/`,
//! `run/`) and API keys use their own Keychain service, so a second BOB can run next to yours without
//! touching your settings, memory, logs, keys or sockets. Only downloaded models are shared, so a test run
//! does not download them again. The Test Bench uses this (docs: Development). A value that is not safe to
//! use stops BOB with a message instead of falling back to your real data.

use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

pub const ENV: &str = "BOB_HOME";
/// Keychain service for API keys.
pub const KEYCHAIN_SERVICE: &str = "dev.bob.assistant";
/// Keychain service for API keys of a separate profile (`BOB_HOME`).
pub const KEYCHAIN_SERVICE_SEPARATE: &str = "dev.bob.assistant.separate-profile";
/// Longest socket name BOB uses inside the runtime folder.
const LONGEST_SOCKET: &str = "control.sock";
/// `sun_path` holds 104 bytes on macOS, including the final NUL.
const MAX_SOCKET_PATH: usize = 103;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Profile {
    /// The user's own BOB.
    Default,
    /// A separate profile under this folder (`BOB_HOME`).
    Separate(PathBuf),
}

/// The folders BOB uses for one profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    /// Settings, memory, profile notes, usage (private, 0700).
    pub config: PathBuf,
    /// Parent of `models/` (always the user's, so models are shared).
    pub data: PathBuf,
    pub logs: PathBuf,
    /// Sockets, the single-instance lock and the model server's key and PID file (private, 0700).
    pub runtime: PathBuf,
}

impl Layout {
    /// The folders for `profile`, given the folders the user's own BOB uses.
    pub fn for_profile(profile: &Profile, own: Layout) -> Layout {
        match profile {
            Profile::Default => own,
            Profile::Separate(home) => Layout {
                config: home.join("config"),
                data: own.data,
                logs: home.join("logs"),
                runtime: home.join("run"),
            },
        }
    }
}

/// Check a `BOB_HOME` value. `user_home` is the user's home folder, which must never be used as is.
pub fn validate(value: &OsStr, user_home: Option<&Path>) -> Result<PathBuf, String> {
    let path = PathBuf::from(value);
    let shown = path.display();
    if !path.is_absolute() {
        return Err(format!("{ENV} must be an absolute path (got {shown})."));
    }
    // Checked on the raw value: `Path::components` quietly drops inner `.` parts.
    if value
        .as_encoded_bytes()
        .split(|b| *b == b'/')
        .any(|part| part == b"." || part == b"..")
        || path.components().any(|c| c == Component::ParentDir)
    {
        return Err(format!("{ENV} must not contain . or .. (got {shown})."));
    }
    if path.parent().is_none() || user_home.is_some_and(|h| h == path) {
        return Err(format!(
            "{ENV} must be a folder of its own, not / or your home folder (got {shown})."
        ));
    }
    let socket = path.join("run").join(LONGEST_SOCKET);
    let len = socket.as_os_str().len();
    if len > MAX_SOCKET_PATH {
        return Err(format!(
            "{ENV} is too long for a socket path ({len} of at most {MAX_SOCKET_PATH} bytes with run/{LONGEST_SOCKET}); use a shorter folder such as /tmp/bob-test."
        ));
    }
    Ok(path)
}

/// The profile from the environment (`Default` when `BOB_HOME` is unset).
pub fn profile_from_env() -> Result<Profile, String> {
    match std::env::var_os(ENV) {
        None => Ok(Profile::Default),
        Some(v) if v.is_empty() => Ok(Profile::Default),
        Some(v) => {
            let home = std::env::var_os("HOME").map(PathBuf::from);
            validate(&v, home.as_deref()).map(Profile::Separate)
        }
    }
}

fn cell() -> &'static OnceLock<Profile> {
    static P: OnceLock<Profile> = OnceLock::new();
    &P
}

/// Decide the profile once, at startup, and create its private folders. The app stops on an error; other
/// callers (the `bob-mcp` bridge, tests) get the profile lazily through [`profile`].
pub fn init() -> Result<Profile, String> {
    let p = profile_from_env()?;
    if let Profile::Separate(home) = &p {
        let own = own_layout(None, None);
        let layout = Layout::for_profile(&p, own);
        for dir in [home, &layout.config, &layout.logs, &layout.runtime] {
            crate::fsutil::private_dir(dir)
                .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        }
    }
    Ok(cell().get_or_init(|| p).clone())
}

/// The profile in use. Without [`init`], an invalid `BOB_HOME` counts as unset.
pub fn profile() -> &'static Profile {
    cell().get_or_init(|| profile_from_env().unwrap_or(Profile::Default))
}

pub fn is_separate() -> bool {
    matches!(profile(), Profile::Separate(_))
}

/// The folders the user's own BOB uses. `config` and `data` come from Tauri's path resolver (known only once
/// the app runs); before that, placeholders under the logs folder are never written.
fn own_layout(config: Option<PathBuf>, data: Option<PathBuf>) -> Layout {
    let logs = crate::logging::default_dir();
    Layout {
        config: config.unwrap_or_else(|| logs.join("unused-config")),
        data: data.unwrap_or_else(|| logs.join("unused-data")),
        logs,
        runtime: std::env::temp_dir().join(format!("bob-{}", current_uid())),
    }
}

/// The folders for the running profile; `config` and `data` are the app's own (Tauri's path resolver).
pub fn layout(config: PathBuf, data: PathBuf) -> Layout {
    Layout::for_profile(profile(), own_layout(Some(config), Some(data)))
}

pub fn logs_dir() -> PathBuf {
    Layout::for_profile(profile(), own_layout(None, None)).logs
}

pub fn runtime_dir() -> PathBuf {
    Layout::for_profile(profile(), own_layout(None, None)).runtime
}

pub fn keychain_service() -> &'static str {
    if is_separate() {
        KEYCHAIN_SERVICE_SEPARATE
    } else {
        KEYCHAIN_SERVICE
    }
}

/// The current user's id, found without `unsafe`: the home folder is owned by the user on macOS. Falls back
/// to 0 only if `HOME` is unset or unreadable (the runtime folder is still 0700).
fn current_uid() -> u32 {
    use std::os::unix::fs::MetadataExt;
    std::env::var_os("HOME")
        .and_then(|h| std::fs::metadata(h).ok())
        .map(|m| m.uid())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn own() -> Layout {
        Layout {
            config: "/Users/u/Library/Application Support/dev.bob.assistant".into(),
            data: "/Users/u/Library/Application Support/dev.bob.assistant".into(),
            logs: "/Users/u/Library/Logs/BOB".into(),
            runtime: "/var/folders/x/T/bob-501".into(),
        }
    }

    #[test]
    fn the_default_profile_uses_the_users_folders() {
        assert_eq!(Layout::for_profile(&Profile::Default, own()), own());
    }

    #[test]
    fn a_separate_profile_moves_everything_but_the_models() {
        let l = Layout::for_profile(&Profile::Separate("/tmp/bob-test".into()), own());
        assert_eq!(l.config, PathBuf::from("/tmp/bob-test/config"));
        assert_eq!(l.logs, PathBuf::from("/tmp/bob-test/logs"));
        assert_eq!(l.runtime, PathBuf::from("/tmp/bob-test/run"));
        assert_eq!(l.data, own().data, "models are shared");
    }

    #[test]
    fn unsafe_values_are_refused_with_a_reason() {
        let home = Path::new("/Users/u");
        let ok = |v: &str| validate(OsStr::new(v), Some(home));
        assert_eq!(ok("/tmp/bob-test"), Ok(PathBuf::from("/tmp/bob-test")));
        assert!(ok("tmp/bob").unwrap_err().contains("absolute"));
        assert!(ok("/tmp/../Users/u").unwrap_err().contains(".."));
        assert!(ok("/tmp/./bob").unwrap_err().contains(".."));
        assert!(ok("/").unwrap_err().contains("home folder"));
        assert!(ok("/Users/u").unwrap_err().contains("home folder"));
        let long = format!("/tmp/{}", "x".repeat(100));
        assert!(ok(&long).unwrap_err().contains("too long"));
        // Right at the limit: run/control.sock fits in 103 bytes.
        let fits = format!(
            "/tmp/{}",
            "x".repeat(MAX_SOCKET_PATH - "/tmp/".len() - "/run/control.sock".len())
        );
        assert!(ok(&fits).is_ok(), "{}", fits.len());
    }

    #[test]
    fn a_separate_profile_keeps_its_keys_apart() {
        assert_ne!(KEYCHAIN_SERVICE, KEYCHAIN_SERVICE_SEPARATE);
        assert!(KEYCHAIN_SERVICE_SEPARATE.starts_with(KEYCHAIN_SERVICE));
    }

    #[test]
    fn the_users_runtime_folder_is_per_user_in_the_temp_folder() {
        let l = own_layout(None, None);
        assert!(l.runtime.starts_with(std::env::temp_dir()));
        assert!(l
            .runtime
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("bob-"));
        assert!(l.logs.ends_with("Library/Logs/BOB"));
    }

    #[test]
    fn private_folders_are_created_0700() {
        let home = std::env::temp_dir().join(format!("bob-home-{}", rand::random::<u32>()));
        let layout = Layout::for_profile(&Profile::Separate(home.clone()), own());
        for dir in [&home, &layout.config, &layout.logs, &layout.runtime] {
            crate::fsutil::private_dir(dir).unwrap();
            let mode = std::fs::metadata(dir).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o700, "{}", dir.display());
        }
        let _ = std::fs::remove_dir_all(&home);
    }
}
