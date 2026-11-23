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
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            show_in_dock: true,
            open_window_at_launch: true,
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
        std::fs::write(
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

#[cfg(target_os = "macos")]
mod login {
    use super::LoginItem;
    use objc2_service_management::{SMAppService, SMAppServiceStatus};

    pub fn status() -> LoginItem {
        // SAFETY: plain Objective-C calls on the shared main-app service object.
        let s = unsafe { SMAppService::mainAppService().status() };
        match s {
            SMAppServiceStatus::Enabled => LoginItem::Enabled,
            SMAppServiceStatus::RequiresApproval => LoginItem::RequiresApproval,
            SMAppServiceStatus::NotRegistered => LoginItem::Disabled,
            _ => LoginItem::Unavailable,
        }
    }

    pub fn set(on: bool) -> Result<LoginItem, String> {
        // SAFETY: as above; errors come back as NSError.
        let service = unsafe { SMAppService::mainAppService() };
        let result = unsafe {
            if on {
                service.registerAndReturnError()
            } else {
                service.unregisterAndReturnError()
            }
        };
        result.map_err(|e| e.localizedDescription().to_string())?;
        Ok(status())
    }
}

#[cfg(target_os = "macos")]
pub use login::{set as set_login_item, status as login_item};

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
        };
        s.save(&p).unwrap();
        assert_eq!(AppSettings::load(&p), s);
        std::fs::write(&p, "{broken").unwrap();
        assert_eq!(AppSettings::load(&p), AppSettings::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn login_item_status_is_readable() {
        // A test binary is not an app bundle: any status is fine, it must just not crash.
        let _ = login_item();
    }
}
