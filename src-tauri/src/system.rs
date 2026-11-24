//! This Mac: chip and memory for the model recommendation (welcome tour), and the running apps for the
//! quiet-apps picker. Read on demand; nothing is stored or sent anywhere.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Recommendation {
    /// Built-in brain models to download, most important first.
    pub brain_models: Vec<&'static str>,
    pub speech_model: &'static str,
    /// One plain sentence explaining the choice.
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SystemInfo {
    /// For example "Apple M3".
    pub chip: String,
    pub ram_gb: u32,
    pub apple_silicon: bool,
    pub recommendation: Recommendation,
}

/// What to download for a Mac with `ram_gb` of memory. Based on the measurements in docs/PERFORMANCE.md:
/// the 1.5B brain uses about 1.4 GB, the 4B about 3.1 GB, whisper turbo about 0.9 GB and base about 0.45 GB.
pub fn recommend(ram_gb: u32, apple_silicon: bool) -> Recommendation {
    const GHOST: &str = "qwen2.5-1.5b-instruct-q4km";
    const LARGE: &str = "qwen3-4b-instruct-2507-q4km";
    const TURBO: &str = "whisper-large-v3-turbo-q5";
    const BASE: &str = "whisper-base";
    if !apple_silicon {
        return Recommendation {
            brain_models: vec![GHOST],
            speech_model: BASE,
            note: "Intel Macs run local models slowly: the small models keep BOB responsive, or connect a brain \
                   on another computer."
                .into(),
        };
    }
    match ram_gb {
        0..=11 => Recommendation {
            brain_models: vec![GHOST],
            speech_model: BASE,
            note: format!(
                "With {ram_gb} GB of memory, the small brain and the light speech model leave room for your other apps."
            ),
        },
        12..=23 => Recommendation {
            brain_models: vec![GHOST],
            speech_model: TURBO,
            note: format!(
                "With {ram_gb} GB of memory, BOB's default brain and the accurate speech model run comfortably together."
            ),
        },
        _ => Recommendation {
            brain_models: vec![GHOST, LARGE],
            speech_model: TURBO,
            note: format!(
                "With {ram_gb} GB of memory you can also add the larger brain for better Selection Actions."
            ),
        },
    }
}

fn sysctl(name: &str) -> Option<String> {
    let out = std::process::Command::new("/usr/sbin/sysctl")
        .args(["-n", name])
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

pub fn info() -> SystemInfo {
    let ram_gb = sysctl("hw.memsize")
        .and_then(|s| s.parse::<u64>().ok())
        .map(|b| ((b as f64) / 1024f64.powi(3)).round() as u32)
        .unwrap_or(0);
    let chip = sysctl("machdep.cpu.brand_string").unwrap_or_else(|| "this Mac".into());
    let apple_silicon = cfg!(target_arch = "aarch64");
    SystemInfo {
        recommendation: recommend(ram_gb, apple_silicon),
        chip,
        ram_gb,
        apple_silicon,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RunningApp {
    pub bundle_id: String,
    pub name: String,
}

/// Apps with a Dock icon that are running now (sorted by name), for the quiet-apps picker.
#[cfg(target_os = "macos")]
pub fn running_apps() -> Vec<RunningApp> {
    use objc2_app_kit::{NSApplicationActivationPolicy, NSWorkspace};
    // `runningApplications` is documented as thread safe.
    let apps = NSWorkspace::sharedWorkspace().runningApplications();
    let mut v: Vec<RunningApp> = apps
        .iter()
        .filter(|a| a.activationPolicy() == NSApplicationActivationPolicy::Regular)
        .filter_map(|a| {
            let bundle_id = a.bundleIdentifier()?.to_string();
            let name = a
                .localizedName()
                .map(|n| n.to_string())
                .unwrap_or_else(|| bundle_id.clone());
            Some(RunningApp { bundle_id, name })
        })
        .filter(|a| a.bundle_id != "dev.bob.assistant")
        .collect();
    v.sort_by_key(|a| a.name.to_lowercase());
    v.dedup_by(|a, b| a.bundle_id == b.bundle_id);
    v
}

#[cfg(not(target_os = "macos"))]
pub fn running_apps() -> Vec<RunningApp> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recommendations_follow_memory_and_chip() {
        assert_eq!(recommend(8, true).speech_model, "whisper-base");
        let m3 = recommend(16, true);
        assert_eq!(m3.speech_model, "whisper-large-v3-turbo-q5");
        assert_eq!(m3.brain_models, vec!["qwen2.5-1.5b-instruct-q4km"]);
        assert!(m3.note.contains("16 GB"));
        assert_eq!(recommend(32, true).brain_models.len(), 2);
        assert_eq!(recommend(64, false).speech_model, "whisper-base");
        for r in [recommend(8, true), recommend(16, true), recommend(36, true)] {
            for id in &r.brain_models {
                assert!(crate::brain::models::find(id).is_some(), "{id}");
            }
            assert!(crate::brain::models::find_speech(r.speech_model).is_some());
        }
    }

    #[test]
    fn this_mac_is_readable() {
        let i = info();
        assert!(i.ram_gb > 0);
        assert!(!i.chip.is_empty());
        let _ = running_apps();
    }
}
