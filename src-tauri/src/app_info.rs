//! Static application metadata exposed to the UI over Tauri IPC.
//!
//! Kept free of any Tauri types so it is trivially unit- and property-testable.

use serde::{Deserialize, Serialize};

/// Metadata about the running build. Mirrors `AppInfo` in `src/lib/appInfo.ts`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub platform: String,
    pub arch: String,
}

impl AppInfo {
    /// Build metadata from compile-time constants.
    pub fn current() -> Self {
        Self::new(
            env!("CARGO_PKG_VERSION"),
            std::env::consts::OS,
            std::env::consts::ARCH,
        )
    }

    /// Construct from explicit parts. `name` is always "BOB".
    pub fn new(version: &str, platform: &str, arch: &str) -> Self {
        Self {
            name: "BOB".to_string(),
            version: version.to_string(),
            platform: platform.to_string(),
            arch: arch.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn current_reports_bob_and_crate_version() {
        let info = AppInfo::current();
        assert_eq!(info.name, "BOB");
        assert_eq!(info.version, env!("CARGO_PKG_VERSION"));
        assert!(!info.platform.is_empty());
        assert!(!info.arch.is_empty());
    }

    #[test]
    fn serializes_with_the_field_names_the_ui_expects() {
        let json = serde_json::to_value(AppInfo::new("1.2.3", "macos", "aarch64")).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"name":"BOB","version":"1.2.3","platform":"macos","arch":"aarch64"})
        );
    }

    proptest! {
        /// Property: the IPC payload round-trips losslessly for any version/platform/arch text.
        #[test]
        fn json_roundtrip_is_lossless(v in ".{0,40}", p in ".{0,40}", a in ".{0,40}") {
            let info = AppInfo::new(&v, &p, &a);
            let s = serde_json::to_string(&info).unwrap();
            let back: AppInfo = serde_json::from_str(&s).unwrap();
            prop_assert_eq!(back, info);
        }

        /// Property: the product name is never influenced by inputs.
        #[test]
        fn name_is_always_bob(v in ".{0,40}", p in ".{0,40}", a in ".{0,40}") {
            prop_assert_eq!(AppInfo::new(&v, &p, &a).name, "BOB");
        }
    }
}
