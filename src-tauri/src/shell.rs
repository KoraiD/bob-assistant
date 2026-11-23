//! The Mac shell around BOB: the menu bar item (status, quick toggles, pause), general settings (Dock icon,
//! launch at login) and the "Pause BOB" commands.

use crate::app_settings::{self, AppSettings, LoginItem};
use crate::dictation::engine::DictationHandle;
use crate::ghost::engine::GhostHandle;
use crate::native_ipc::BridgeStatus;
use crate::pause::{Pause, PauseFor, PauseView};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, State};

/// Emitted when pause, Dock or quick-toggle state changes from outside the window (tray).
pub const SHELL_EVENT: &str = "shell-changed";
/// Emitted to open a panel in the window (payload: panel id), e.g. "settings" from Cmd+,.
pub const OPEN_PANEL_EVENT: &str = "open-panel";

/// Menu bar items whose text or check mark changes at runtime.
pub struct TrayItems {
    pub ax: MenuItem<tauri::Wry>,
    pub mic: MenuItem<tauri::Wry>,
    ghost: CheckMenuItem<tauri::Wry>,
    dictation: CheckMenuItem<tauri::Wry>,
    pause_hour: MenuItem<tauri::Wry>,
    pause_tomorrow: MenuItem<tauri::Wry>,
    resume: MenuItem<tauri::Wry>,
}

/// Saved general settings and where they live.
pub struct General {
    pub path: PathBuf,
    pub settings: Mutex<AppSettings>,
}

pub fn show_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

pub fn open_panel(app: &AppHandle, panel: &str) {
    show_main_window(app);
    let _ = app.emit(OPEN_PANEL_EVENT, panel.to_string());
}

/// Apply the Dock setting: Regular (Dock + app switcher) or Accessory (menu bar only). `reshow` brings the
/// window back after the switch (macOS hides it); only wanted when the user flips the setting live.
pub fn apply_dock(app: &AppHandle, show: bool, reshow: bool) {
    #[cfg(target_os = "macos")]
    {
        let policy = if show {
            tauri::ActivationPolicy::Regular
        } else {
            tauri::ActivationPolicy::Accessory
        };
        let _ = app.set_activation_policy(policy);
        if !show && reshow {
            show_main_window(app);
        }
    }
}

pub fn build_tray(app: &tauri::App, ghost_on: bool, dictation_on: bool) -> tauri::Result<()> {
    let initial = BridgeStatus::default();
    let ax = MenuItem::with_id(
        app,
        "ax",
        crate::accessibility_label(&initial),
        false,
        None::<&str>,
    )?;
    let mic = MenuItem::with_id(
        app,
        "mic",
        crate::microphone_label(&initial),
        false,
        None::<&str>,
    )?;
    let ghost = CheckMenuItem::with_id(app, "ghost", "Ghost Text", true, ghost_on, None::<&str>)?;
    let dictation = CheckMenuItem::with_id(
        app,
        "dictation",
        "Voice Dictation",
        true,
        dictation_on,
        None::<&str>,
    )?;
    let pause_hour = MenuItem::with_id(
        app,
        "pause-hour",
        "Pause BOB for 1 Hour",
        true,
        None::<&str>,
    )?;
    let pause_tomorrow = MenuItem::with_id(
        app,
        "pause-tomorrow",
        "Pause BOB Until Tomorrow",
        true,
        None::<&str>,
    )?;
    let resume = MenuItem::with_id(app, "resume", "Resume BOB", false, None::<&str>)?;
    let open = MenuItem::with_id(app, "open", "Open BOB", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit BOB", true, Some("Cmd+Q"))?;
    let sep = || PredefinedMenuItem::separator(app);
    let menu = Menu::with_items(
        app,
        &[
            &ax,
            &mic,
            &sep()?,
            &ghost,
            &dictation,
            &sep()?,
            &pause_hour,
            &pause_tomorrow,
            &resume,
            &sep()?,
            &open,
            &settings,
            &quit,
        ],
    )?;
    let mut builder = TrayIconBuilder::with_id("bob-tray")
        .tooltip("BOB")
        .menu(&menu)
        .on_menu_event(|app, event| on_tray_menu(app, event.id.as_ref()));
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    app.manage(TrayItems {
        ax,
        mic,
        ghost,
        dictation,
        pause_hour,
        pause_tomorrow,
        resume,
    });
    Ok(())
}

fn on_tray_menu(app: &AppHandle, id: &str) {
    match id {
        "open" => show_main_window(app),
        "settings" => open_panel(app, "settings"),
        "quit" => app.exit(0),
        "ghost" => {
            if let Some(g) = app.try_state::<GhostHandle>() {
                g.set_enabled(!g.settings().enabled);
            }
            changed(app);
        }
        "dictation" => {
            if let Some(d) = app.try_state::<DictationHandle>() {
                let mut s = d.settings();
                s.enabled = !s.enabled;
                let _ = d.set_settings(s);
                crate::commands::push_dictation(app);
            }
            changed(app);
        }
        "pause-hour" => {
            set_pause(app, Some(PauseFor::Hour));
        }
        "pause-tomorrow" => {
            set_pause(app, Some(PauseFor::UntilTomorrow));
        }
        "resume" => {
            set_pause(app, None);
        }
        _ => {}
    }
}

/// Bring the tray's check marks and pause items in line with the current state, and tell the window.
pub fn changed(app: &AppHandle) {
    if let Some(items) = app.try_state::<TrayItems>() {
        if let Some(g) = app.try_state::<GhostHandle>() {
            let _ = items.ghost.set_checked(g.settings().enabled);
        }
        if let Some(d) = app.try_state::<DictationHandle>() {
            let _ = items.dictation.set_checked(d.settings().enabled);
        }
        let paused = app.try_state::<Arc<Pause>>().is_some_and(|p| p.is_paused());
        let _ = items.pause_hour.set_enabled(!paused);
        let _ = items.pause_tomorrow.set_enabled(!paused);
        let _ = items.resume.set_enabled(paused);
    }
    let _ = app.emit(SHELL_EVENT, ());
}

/// Pause (`Some`) or resume (`None`) every feature. An expiring pause resumes by itself.
pub fn set_pause(app: &AppHandle, how: Option<PauseFor>) -> PauseView {
    let Some(pause) = app.try_state::<Arc<Pause>>().map(|p| p.inner().clone()) else {
        return PauseView {
            paused: false,
            until: None,
        };
    };
    let view = match how {
        Some(h) => pause.pause(h),
        None => pause.resume(),
    };
    if let Some(g) = app.try_state::<GhostHandle>() {
        g.set_paused(view.paused);
    }
    log::info!("BOB {}", if view.paused { "paused" } else { "resumed" });
    if let Some(until) = view.until {
        let (app, pause) = (app.clone(), pause.clone());
        std::thread::spawn(move || {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(until);
            std::thread::sleep(std::time::Duration::from_secs(
                until.saturating_sub(now) + 1,
            ));
            // Only if this pause is still the one in effect and has now expired.
            if !pause.is_paused() {
                if let Some(g) = app.try_state::<GhostHandle>() {
                    g.set_paused(false);
                }
                changed(&app);
            }
        });
    }
    changed(app);
    view
}

// ---- IPC ----

#[derive(Serialize)]
pub struct GeneralView {
    pub show_in_dock: bool,
    pub open_window_at_launch: bool,
    pub login_item: LoginItem,
    pub pause: PauseView,
}

fn login_item() -> LoginItem {
    #[cfg(target_os = "macos")]
    {
        app_settings::login_item()
    }
    #[cfg(not(target_os = "macos"))]
    {
        LoginItem::Unavailable
    }
}

/// The Settings view. Locks the settings once: a second `lock()` in the same expression would deadlock,
/// because the first guard lives until the end of the statement (this froze the app once).
pub fn general_view(general: &General, pause: &Pause, login_item: LoginItem) -> GeneralView {
    let s = general.settings.lock().expect("general").clone();
    GeneralView {
        show_in_dock: s.show_in_dock,
        open_window_at_launch: s.open_window_at_launch,
        login_item,
        pause: pause.view(),
    }
}

#[tauri::command]
pub fn general_get(general: State<'_, General>, pause: State<'_, Arc<Pause>>) -> GeneralView {
    general_view(&general, &pause, login_item())
}

#[tauri::command]
pub fn general_set_dock(
    app: AppHandle,
    general: State<'_, General>,
    show: bool,
) -> Result<(), String> {
    let s = {
        let mut s = general.settings.lock().expect("general");
        s.show_in_dock = show;
        s.clone()
    };
    s.save(&general.path)
        .map_err(|e| format!("cannot save the setting: {e}"))?;
    apply_dock(&app, show, true);
    Ok(())
}

#[tauri::command]
pub fn general_set_open_at_launch(general: State<'_, General>, open: bool) -> Result<(), String> {
    let s = {
        let mut s = general.settings.lock().expect("general");
        s.open_window_at_launch = open;
        s.clone()
    };
    s.save(&general.path)
        .map_err(|e| format!("cannot save the setting: {e}"))
}

#[tauri::command]
pub fn general_set_login(on: bool) -> Result<LoginItem, String> {
    #[cfg(target_os = "macos")]
    {
        app_settings::set_login_item(on)
            .map_err(|e| format!("macOS did not accept the login item: {e}"))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = on;
        Err("launch at login is only available on macOS".into())
    }
}

/// `how`: "hour", "tomorrow" or "indefinite".
#[tauri::command]
pub fn pause_start(app: AppHandle, how: String) -> Result<PauseView, String> {
    let how = match how.as_str() {
        "hour" => PauseFor::Hour,
        "tomorrow" => PauseFor::UntilTomorrow,
        "indefinite" => PauseFor::Indefinitely,
        other => return Err(format!("unknown pause {other:?}")),
    };
    Ok(set_pause(&app, Some(how)))
}

#[tauri::command]
pub fn pause_resume(app: AppHandle) -> PauseView {
    set_pause(&app, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression: `general_get` deadlocked on the main thread (double lock in one expression).
    #[test]
    fn the_settings_view_returns_promptly() {
        let general = General {
            path: std::env::temp_dir().join("bob-general-test.json"),
            settings: Mutex::new(AppSettings {
                show_in_dock: false,
                open_window_at_launch: true,
            }),
        };
        let pause = Pause::default();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let v = general_view(&general, &pause, LoginItem::Disabled);
            let _ = tx.send((v.show_in_dock, v.open_window_at_launch, v.pause.paused));
        });
        let got = rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("general_view deadlocked");
        assert_eq!(got, (false, true, false));
    }
}
