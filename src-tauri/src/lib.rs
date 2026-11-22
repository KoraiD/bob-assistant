//! BOB core. Orchestrates engines, brains, settings and the UI.
//!
//! Current scope: app metadata, tray (with permission status), and the bridge to the Swift helper.
//! Later tasks add the Brain router and the Ghost/Selection/Dictation engines.

pub mod app_info;
pub mod native_ipc;
pub mod protocol;
pub mod supervisor;

use app_info::AppInfo;
use native_ipc::{Bridge, BridgeStatus};
use protocol::{CoreCommand, MicStatus, NativeEvent};
use std::sync::Mutex;
use supervisor::{Backoff, Supervisor};
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Emitter, Manager, State, WindowEvent,
};

/// Tauri event carrying a [`BridgeStatus`] snapshot to the UI.
pub const STATUS_EVENT: &str = "native-status";

/// Keeps the helper process supervised for the lifetime of the app.
struct HelperGuard(#[allow(dead_code)] Mutex<Option<Supervisor>>);

/// IPC: static metadata about this build.
#[tauri::command]
fn app_info() -> AppInfo {
    AppInfo::current()
}

/// IPC: current helper connection and permission status.
#[tauri::command]
fn native_status(bridge: State<'_, Bridge>) -> BridgeStatus {
    bridge.status()
}

/// IPC: ask the helper to show the macOS permission prompts (Accessibility, Microphone).
#[tauri::command]
fn request_permissions(bridge: State<'_, Bridge>) -> Result<(), String> {
    bridge
        .send(&CoreCommand::RequestPermissions)
        .map_err(|e| format!("native helper is not connected: {e}"))
}

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// Human-readable permission labels for the tray (NB-6).
pub fn accessibility_label(status: &BridgeStatus) -> String {
    let v = match (status.connected, status.accessibility) {
        (false, _) => "helper not running",
        (true, None) => "checking...",
        (true, Some(true)) => "granted",
        (true, Some(false)) => "needed",
    };
    format!("Accessibility: {v}")
}

pub fn microphone_label(status: &BridgeStatus) -> String {
    let v = match (status.connected, status.microphone) {
        (false, _) => "helper not running",
        (true, None) => "checking...",
        (true, Some(MicStatus::Granted)) => "granted",
        (true, Some(MicStatus::Denied)) => "denied",
        (true, Some(MicStatus::Undetermined)) => "not asked yet",
    };
    format!("Microphone: {v}")
}

fn setup_tray(app: &tauri::App) -> tauri::Result<(MenuItem<tauri::Wry>, MenuItem<tauri::Wry>)> {
    let initial = BridgeStatus::default();
    let ax = MenuItem::with_id(
        app,
        "ax",
        accessibility_label(&initial),
        false,
        None::<&str>,
    )?;
    let mic = MenuItem::with_id(app, "mic", microphone_label(&initial), false, None::<&str>)?;
    let open = MenuItem::with_id(app, "open", "Open BOB", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit BOB", true, Some("Cmd+Q"))?;
    let menu = Menu::with_items(app, &[&ax, &mic, &open, &quit])?;

    let mut builder = TrayIconBuilder::with_id("bob-tray")
        .tooltip("BOB")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main_window(app),
            "quit" => app.exit(0),
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok((ax, mic))
}

/// Bind the socket, supervise the helper and forward status changes to the tray and UI.
fn start_native_bridge(
    app: &tauri::App,
    bridge: Bridge,
    ax_item: MenuItem<tauri::Wry>,
    mic_item: MenuItem<tauri::Wry>,
) {
    let socket = native_ipc::default_socket_path();
    let listener = match native_ipc::bind_socket(&socket) {
        Ok(l) => l,
        Err(e) => {
            log::error!("cannot bind native socket {}: {e}", socket.display());
            return;
        }
    };

    let handle = app.handle().clone();
    let serve_bridge = bridge.clone();
    std::thread::spawn(move || {
        serve_bridge.run(listener, |event, status| {
            // Only status-relevant events reach the UI; typed text never leaves the engine path.
            if matches!(
                event,
                NativeEvent::Hello { .. } | NativeEvent::Permissions { .. }
            ) {
                let _ = ax_item.set_text(accessibility_label(status));
                let _ = mic_item.set_text(microphone_label(status));
                let _ = handle.emit(STATUS_EVENT, status.clone());
            }
        });
    });

    let guard = match supervisor::locate_helper() {
        Some(path) => {
            log::info!("starting native helper");
            Some(Supervisor::start(path, &socket, Backoff::default()))
        }
        None => {
            log::warn!(
                "native helper not found; build it with `make native` or set BOB_NATIVE_PATH"
            );
            None
        }
    };
    app.manage(HelperGuard(Mutex::new(guard)));
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(Bridge::new())
        .invoke_handler(tauri::generate_handler![
            app_info,
            native_status,
            request_permissions
        ])
        .setup(|app| {
            let (ax, mic) = setup_tray(app)?;
            let bridge = app.state::<Bridge>().inner().clone();
            start_native_bridge(app, bridge, ax, mic);
            Ok(())
        })
        // Closing the window hides it; BOB keeps running from the menu bar.
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running BOB");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_labels_reflect_connection_and_permissions() {
        let mut st = BridgeStatus::default();
        assert_eq!(
            accessibility_label(&st),
            "Accessibility: helper not running"
        );
        st.connected = true;
        assert_eq!(accessibility_label(&st), "Accessibility: checking...");
        assert_eq!(microphone_label(&st), "Microphone: checking...");
        st.accessibility = Some(false);
        st.microphone = Some(MicStatus::Undetermined);
        assert_eq!(accessibility_label(&st), "Accessibility: needed");
        assert_eq!(microphone_label(&st), "Microphone: not asked yet");
        st.accessibility = Some(true);
        st.microphone = Some(MicStatus::Granted);
        assert_eq!(accessibility_label(&st), "Accessibility: granted");
        assert_eq!(microphone_label(&st), "Microphone: granted");
        st.microphone = Some(MicStatus::Denied);
        assert_eq!(microphone_label(&st), "Microphone: denied");
    }
}
