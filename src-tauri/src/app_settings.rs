//! General app settings (Settings panel): Dock icon, launch at login.
//!
//! Launch at login is not stored by BOB: macOS owns it (`SMAppService`, System Settings > General > Login
//! Items), so BOB reads the live status every time and the switch can never disagree with System Settings.

use serde::{Deserialize, Serialize};
use std::path::Path;

pub const FILE: &str = "app-settings.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    /// Show BOB in the Dock and the app switcher. Off: menu bar only (window still opens from the tray).
    pub show_in_dock: bool,
    /// Show the window when BOB starts. Off: start quietly in the menu bar (handy as a login item).
    pub open_window_at_launch: bool,
    /// How much goes into ~/Library/Logs/BOB/bob.log (Settings > Logs).
    pub log_level: crate::logging::Level,
    /// Let AI agents use BOB through MCP (Settings > AI agents). Off by default.
    pub mcp_enabled: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            show_in_dock: true,
            open_window_at_launch: true,
            log_level: crate::logging::Level::Info,
            mcp_enabled: false,
        }
    }
}

impl AppSettings {
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        crate::fsutil::write_private_file(
            &tmp,
            serde_json::to_string_pretty(self).map_err(std::io::Error::other)?,
        )?;
        std::fs::rename(tmp, path)
    }
}

/// Launch-at-login state as macOS reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LoginItem {
    Enabled,
    Disabled,
    /// Registered, but the user must allow it in System Settings > Login Items.
    RequiresApproval,
    /// Not available (for example a development binary outside an app bundle).
    Unavailable,
}

/// Label and file name of the fallback LaunchAgent.
pub const AGENT_LABEL: &str = "dev.bob.assistant.login";

/// The LaunchAgent plist that opens `app` (an `.app` bundle or an executable) at login. Used when macOS
/// refuses the modern login item (`SMAppService` reports "not found" for unsigned or moved builds).
pub fn launch_agent_plist(app: &Path) -> String {
    fn esc(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    }
    let target = esc(&app.display().to_string());
    let args = if app.extension().is_some_and(|e| e == "app") {
        format!("<string>/usr/bin/open</string>\n        <string>{target}</string>")
    } else {
        format!("<string>{target}</string>")
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{AGENT_LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        {args}
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>ProcessType</key>
    <string>Interactive</string>
</dict>
</plist>
"#
    )
}

/// The `.app` bundle containing `exe`, if any (`…/BOB.app/Contents/MacOS/bob`).
pub fn bundle_of(exe: &Path) -> Option<std::path::PathBuf> {
    exe.ancestors()
        .find(|p| p.extension().is_some_and(|e| e == "app"))
        .map(Path::to_path_buf)
}

#[cfg(target_os = "macos")]
mod login {
    use super::{bundle_of, launch_agent_plist, LoginItem, AGENT_LABEL};
    use objc2_service_management::{SMAppService, SMAppServiceStatus};
    use std::path::PathBuf;

    fn agent_path() -> Option<PathBuf> {
        let home = std::env::var_os("HOME")?;
        Some(
            PathBuf::from(home)
                .join("Library/LaunchAgents")
                .join(format!("{AGENT_LABEL}.plist")),
        )
    }

    fn service_status() -> SMAppServiceStatus {
        // SAFETY: plain Objective-C calls on the shared main-app service object.
        unsafe { SMAppService::mainAppService().status() }
    }

    pub fn status() -> LoginItem {
        match service_status() {
            SMAppServiceStatus::Enabled => return LoginItem::Enabled,
            SMAppServiceStatus::RequiresApproval => return LoginItem::RequiresApproval,
            _ => {}
        }
        if agent_path().is_some_and(|p| p.is_file()) {
            LoginItem::Enabled
        } else if agent_path().is_some() {
            LoginItem::Disabled
        } else {
            LoginItem::Unavailable
        }
    }

    /// Log what macOS says at startup (helps when the switch does not stick).
    pub fn describe() -> String {
        let s = service_status();
        let name = match s {
            SMAppServiceStatus::Enabled => "enabled",
            SMAppServiceStatus::RequiresApproval => "requires approval",
            SMAppServiceStatus::NotRegistered => "not registered",
            SMAppServiceStatus::NotFound => "not found",
            _ => "unknown",
        };
        let agent = agent_path().is_some_and(|p| p.is_file());
        format!(
            "login item: SMAppService {name}, LaunchAgent {}",
            if agent { "present" } else { "absent" }
        )
    }

    fn write_agent() -> Result<(), String> {
        let path = agent_path().ok_or("no home folder")?;
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let target = bundle_of(&exe).unwrap_or(exe);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, launch_agent_plist(&target)).map_err(|e| e.to_string())
    }

    fn remove_agent() {
        if let Some(p) = agent_path() {
            let _ = std::fs::remove_file(p);
        }
    }

    pub fn set(on: bool) -> Result<LoginItem, String> {
        // SAFETY: as above; errors come back as NSError.
        let service = unsafe { SMAppService::mainAppService() };
        if on {
            let modern = unsafe { service.registerAndReturnError() };
            match (modern, service_status()) {
                (Ok(()), SMAppServiceStatus::Enabled | SMAppServiceStatus::RequiresApproval) => {
                    remove_agent();
                }
                (result, _) => {
                    if let Err(e) = result {
                        log::info!(
                            "login item: SMAppService refused ({}), using a LaunchAgent",
                            e.localizedDescription()
                        );
                    }
                    write_agent()?;
                }
            }
        } else {
            if matches!(
                service_status(),
                SMAppServiceStatus::Enabled | SMAppServiceStatus::RequiresApproval
            ) {
                unsafe { service.unregisterAndReturnError() }
                    .map_err(|e| e.localizedDescription().to_string())?;
            }
            remove_agent();
        }
        Ok(status())
    }
}

#[cfg(target_os = "macos")]
pub use login::{describe as describe_login_item, set as set_login_item, status as login_item};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_show_the_dock_icon_and_round_trip() {
        let dir = std::env::temp_dir().join(format!("bob-app-{}", rand::random::<u32>()));
        let p = dir.join(FILE);
        assert!(AppSettings::load(&p).show_in_dock);
        let s = AppSettings {
            show_in_dock: false,
            open_window_at_launch: false,
            log_level: crate::logging::Level::Debug,
            mcp_enabled: true,
        };
        s.save(&p).unwrap();
        assert_eq!(AppSettings::load(&p), s);
        std::fs::write(&p, "{broken").unwrap();
        assert_eq!(AppSettings::load(&p), AppSettings::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_launch_agent_opens_the_bundle_and_escapes_paths() {
        let p = launch_agent_plist(Path::new("/Applications/B&B <x>.app"));
        assert!(p.contains("<string>/usr/bin/open</string>"));
        assert!(p.contains("/Applications/B&amp;B &lt;x&gt;.app"));
        assert!(p.contains("<key>RunAtLoad</key>"));
        assert!(p.contains(AGENT_LABEL));
        let raw = launch_agent_plist(Path::new("/tmp/bob"));
        assert!(!raw.contains("/usr/bin/open"));
        assert_eq!(
            bundle_of(Path::new("/A/BOB.app/Contents/MacOS/bob")),
            Some(std::path::PathBuf::from("/A/BOB.app"))
        );
        assert_eq!(bundle_of(Path::new("/tmp/bob")), None);
    }

    /// Writes and removes the real LaunchAgent (`cargo test -- --ignored login_item_round_trip`).
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore]
    fn login_item_round_trip() {
        let before = login_item();
        assert_eq!(set_login_item(true).unwrap(), LoginItem::Enabled);
        assert_eq!(set_login_item(false).unwrap(), LoginItem::Disabled);
        if before == LoginItem::Enabled {
            set_login_item(true).unwrap();
        }
    }

    #[test]
    fn login_item_status_is_readable() {
        // A test binary is not an app bundle: any status is fine, it must just not crash.
        let _ = login_item();
    }
}
