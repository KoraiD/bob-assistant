//! BOB core. Orchestrates engines, brains, settings and the UI.
//!
//! Current scope: app metadata, tray (with permission status), the bridge to the Swift helper and the
//! Brain layer (registry, providers, built-in llama-server sidecar). Later tasks add the
//! Ghost/Selection/Dictation engines.

pub mod activity;
pub mod app_info;
pub mod bench;
pub mod brain;
pub mod commands;
pub mod context;
pub mod dictation;
pub mod ghost;
pub mod native_ipc;
pub mod protocol;
mod quit_key;
pub mod selection;
pub mod supervisor;

use app_info::AppInfo;
use native_ipc::{Bridge, BridgeStatus};
use protocol::{CoreCommand, MicStatus, NativeEvent};
use std::sync::Mutex;
use supervisor::{Backoff, Supervisor};
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem, Submenu},
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

/// IPC: quit BOB the same way the tray item does (used by the in-window Cmd+Q fallback).
#[tauri::command]
fn app_quit(app: tauri::AppHandle) {
    log::info!("quit requested from the window");
    app.exit(0);
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

/// Emitted whenever the orb state changes (payload: `activity::OrbView`).
pub const ORB_EVENT: &str = "orb-state";

/// Tray tooltip for an orb state, in BOB's voice.
pub fn tray_tooltip(v: &activity::OrbView) -> String {
    let s = match v.state {
        activity::Orb::Idle => "BOB is floating nearby",
        activity::Orb::Listening => "BOB is listening...",
        activity::Orb::Thinking => "BOB is thinking...",
        activity::Orb::Haunting => "BOB is haunting your caret",
        activity::Orb::Error => "BOB needs a hand",
    };
    match &v.message {
        Some(m) if !m.is_empty() => format!("{s}: {m}"),
        _ => s.to_string(),
    }
}

/// IPC: the orb's current state (the UI then follows `orb-state` events).
#[tauri::command]
fn orb_get(activity: State<'_, std::sync::Arc<activity::Activity>>) -> activity::OrbView {
    activity.view()
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
/// The engines that consume helper events.
struct Engines {
    ghost: ghost::engine::GhostHandle,
    context: commands::Context,
    selection: selection::engine::SelectionHandle,
    dictation: dictation::engine::DictationHandle,
}

fn start_native_bridge(
    app: &tauri::App,
    bridge: Bridge,
    engines: Engines,
    ax_item: MenuItem<tauri::Wry>,
    mic_item: MenuItem<tauri::Wry>,
) {
    let Engines {
        ghost,
        context,
        selection,
        dictation,
    } = engines;
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
    let cmd_bridge = bridge.clone();
    std::thread::spawn(move || {
        serve_bridge.run(listener, |event, status| {
            // Surroundings and screen text feed the personal context (never the UI, never logs).
            if matches!(
                event,
                NativeEvent::Focus { .. }
                    | NativeEvent::ContextState { .. }
                    | NativeEvent::ScreenText { .. }
            ) {
                context.observe(event);
            }
            // A (re)connected helper starts in Accessibility-only mode with the default hotkey: tell it
            // what it may capture and which hotkey the user chose.
            if matches!(event, NativeEvent::Hello { .. }) {
                commands::push_context_mode(&cmd_bridge, &context, &ghost);
                let _ = cmd_bridge.send(&selection.hotkey_command());
                let _ = cmd_bridge.send(&dictation.command());
            }
            // Voice Dictation (focus and text events only tell it whether the field is secure and what
            // precedes the caret; transcripts never reach the UI or logs).
            if matches!(
                event,
                NativeEvent::DictationKey { .. }
                    | NativeEvent::Transcript { .. }
                    | NativeEvent::DictationError { .. }
                    | NativeEvent::DictationStatus { .. }
                    | NativeEvent::Focus { .. }
                    | NativeEvent::TextState { .. }
                    | NativeEvent::Hello { .. }
            ) {
                dictation.native(event.clone());
            }
            // Selection Actions (selected text never reaches the UI or logs).
            if matches!(
                event,
                NativeEvent::Selection { .. }
                    | NativeEvent::MenuChoice { .. }
                    | NativeEvent::MenuCancel
                    | NativeEvent::ReplaceResult { .. }
                    | NativeEvent::Hello { .. }
            ) {
                selection.native(event.clone());
            }
            // Text, focus and key events drive Ghost Text; they never go to the UI.
            if matches!(
                event,
                NativeEvent::TextState { .. }
                    | NativeEvent::Focus { .. }
                    | NativeEvent::Key { .. }
                    | NativeEvent::Hello { .. }
            ) {
                ghost.native(event.clone());
            }
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

/// BOB's macOS app menu. Replaces Tauri's default, which is titled after the Rust package ("bob") and
/// whose Quit uses `terminate:`. Here Quit is a regular item that calls `app.exit`, the path that stops
/// the model server. Edit items stay predefined so copy/paste keep working in BOB's text fields.
fn app_menu(app: &tauri::AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let quit = MenuItem::with_id(app, "app-quit", "Quit BOB", true, Some("CmdOrCtrl+Q"))?;
    let bob = Submenu::with_items(
        app,
        "BOB",
        true,
        &[
            &PredefinedMenuItem::about(app, Some("About BOB"), None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, Some("Hide BOB"))?,
            &PredefinedMenuItem::hide_others(app, None)?,
            &PredefinedMenuItem::show_all(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;
    let edit = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ],
    )?;
    let window = Submenu::with_items(
        app,
        "Window",
        true,
        &[
            &PredefinedMenuItem::minimize(app, None)?,
            &PredefinedMenuItem::close_window(app, None)?,
        ],
    )?;
    Menu::with_items(app, &[&bob, &edit, &window])
}

/// Debug aid: log the macOS app menu (titles and accelerators only).
fn log_app_menu(app: &tauri::App) {
    let Some(menu) = app.menu() else {
        log::debug!("app menu: none installed");
        return;
    };
    for item in menu.items().unwrap_or_default() {
        if let Some(sub) = item.as_submenu() {
            let names: Vec<String> = sub
                .items()
                .unwrap_or_default()
                .iter()
                .filter_map(|i| {
                    i.as_predefined_menuitem()
                        .and_then(|p| p.text().ok())
                        .or_else(|| i.as_menuitem().and_then(|m| m.text().ok()))
                })
                .collect();
            log::debug!(
                "app menu: {:?} -> {:?}",
                sub.text().unwrap_or_default(),
                names
            );
        }
    }
}

/// Route SIGTERM (logout, shutdown, `kill`) and SIGINT (Ctrl+C in a terminal) through the normal quit
/// path, so `RunEvent::Exit` runs and the model server is stopped instead of orphaned.
fn quit_on_signals(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        use tokio::signal::unix::{signal, SignalKind};
        let (Ok(mut term), Ok(mut int)) = (
            signal(SignalKind::terminate()),
            signal(SignalKind::interrupt()),
        ) else {
            log::warn!("cannot install signal handlers");
            return;
        };
        tokio::select! {
            _ = term.recv() => log::info!("SIGTERM received, quitting"),
            _ = int.recv() => log::info!("SIGINT received, quitting"),
        }
        app.exit(0);
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Logs go to stderr. Default: info for BOB, warn for dependencies; override with RUST_LOG.
    // Privacy: log metadata only (sizes, timings, states), never typed text, prompts or keys.
    let _ = env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("warn,bob_lib=info"),
    )
    .try_init();
    tauri::Builder::default()
        .menu(app_menu)
        .on_menu_event(|app, event| {
            if event.id().as_ref() == "app-quit" {
                log::info!("quit requested from the app menu");
                app.exit(0);
            }
        })
        .manage(Bridge::new())
        .invoke_handler(tauri::generate_handler![
            app_info,
            app_quit,
            orb_get,
            native_status,
            request_permissions,
            commands::brain_snapshot,
            commands::brain_upsert,
            commands::brain_remove,
            commands::brain_set_role,
            commands::brain_test,
            commands::brain_try,
            commands::models_list,
            commands::model_download,
            commands::model_cancel,
            commands::builtin_start,
            commands::builtin_stop,
            commands::ghost_status,
            commands::ghost_set_enabled,
            commands::ghost_set_quiet,
            commands::ghost_memory,
            commands::ghost_memory_clear,
            commands::context_get,
            commands::context_set_settings,
            commands::context_request_screen_recording,
            commands::context_preview,
            commands::profile_save,
            commands::brain_set_share_context,
            commands::selection_get,
            commands::selection_set_settings,
            commands::dictation_get,
            commands::dictation_set_settings,
        ])
        .setup(|app| {
            // Brain service: registry in the config dir, models in the data dir, the sidecar's key and
            // PID files in the per-user 0700 runtime dir next to the helper socket.
            let config_dir = app.path().app_config_dir()?;
            let ghost_dir = config_dir.clone();
            let context: commands::Context =
                std::sync::Arc::new(context::ContextHub::load(config_dir.clone()));
            app.manage(context.clone());
            let data_dir = app.path().app_data_dir()?;
            let runtime_dir = native_ipc::default_socket_path()
                .parent()
                .map(|p| p.join("llama"))
                .unwrap_or_else(|| std::env::temp_dir().join("bob-llama"));
            let brains: commands::Brains = std::sync::Arc::new(brain::service::BrainService::new(
                config_dir,
                data_dir,
                runtime_dir,
                Box::new(brain::secrets::KeychainStore),
            ));
            app.manage(brains.clone());
            commands::autostart_builtin(app.handle().clone(), brains.clone());

            // Live orb state for the window and the tray, observed from engine commands (UI-3).
            let handle = app.handle().clone();
            let activity = activity::Activity::new(move |v| {
                let _ = handle.emit(ORB_EVENT, v.clone());
                if let Some(tray) = handle.tray_by_id("bob-tray") {
                    let _ = tray.set_tooltip(Some(tray_tooltip(v)));
                }
            });
            app.manage(activity.clone());
            let observed = |activity: &std::sync::Arc<activity::Activity>| -> std::sync::Arc<dyn ghost::engine::Sink> {
                std::sync::Arc::new(activity::ObservedSink {
                    inner: std::sync::Arc::new(app.state::<Bridge>().inner().clone()),
                    activity: activity.clone(),
                })
            };

            // Ghost Text engine: native events in, chip/insert commands out over the bridge.
            let sink = observed(&activity);
            let ghost = tauri::async_runtime::block_on(async {
                ghost::engine::spawn_ctx(
                    brains,
                    sink,
                    ghost::engine::EngineConfig::default(),
                    Some(ghost_dir),
                    context.clone(),
                )
            });
            app.manage(ghost.clone());
            let selection = {
                let sink = observed(&activity);
                let brains = app.state::<commands::Brains>().inner().clone();
                let dir = app.path().app_config_dir()?;
                let ctx = context.clone();
                tauri::async_runtime::block_on(async move {
                    selection::engine::spawn(brains, ctx, sink, Some(dir))
                })
            };
            app.manage(selection.clone());
            let dictation = {
                let sink = observed(&activity);
                let brains = app.state::<commands::Brains>().inner().clone();
                let dir = app.path().app_config_dir()?;
                let models_dir = brains.models_dir().to_path_buf();
                let ctx = context.clone();
                tauri::async_runtime::block_on(async move {
                    dictation::engine::spawn(brains, ctx, sink, Some(dir), models_dir)
                })
            };
            app.manage(dictation.clone());
            quit_on_signals(app.handle().clone());

            log_app_menu(app);
            let handle = app.handle().clone();
            quit_key::install(move || handle.exit(0));
            let (ax, mic) = setup_tray(app)?;
            let bridge = app.state::<Bridge>().inner().clone();
            let engines = Engines {
                ghost,
                context,
                selection,
                dictation,
            };
            start_native_bridge(app, bridge, engines, ax, mic);
            Ok(())
        })
        // Closing the window hides it; BOB keeps running from the menu bar.
        .on_window_event(|window, event| {
            log::debug!("window event: {event:?}");
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building BOB")
        .run(|app, event| {
            if matches!(
                event,
                tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit
            ) {
                log::info!("run event: {event:?}");
            }
            // Stop the built-in model server before the process exits (it would otherwise keep a model
            // in memory). A crash is covered by the PID file check at the next start.
            if let tauri::RunEvent::Exit = event {
                if let Some(brains) = app.try_state::<commands::Brains>() {
                    let brains = brains.inner().clone();
                    tauri::async_runtime::block_on(brains.stop_builtin());
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every IPC command the UI calls must be registered in `generate_handler!` (a missing one fails
    /// only at runtime, silently). Regression guard for a real slip during Task 3.
    #[test]
    fn every_command_the_ui_invokes_is_registered() {
        let lib = include_str!("lib.rs");
        let start = lib.find("generate_handler![").expect("handler list");
        let handlers = &lib[start..start + lib[start..].find(']').expect("end of handler list")];
        let ui = [
            include_str!("../../src/lib/appInfo.ts"),
            include_str!("../../src/lib/nativeStatus.ts"),
            include_str!("../../src/lib/brains.ts"),
        ];
        let mut checked = 0;
        for src in ui {
            for part in src.split("invoke").skip(1) {
                // Only real calls: `invoke("name"` or `invoke<Type>("name"` (the generic is followed
                // directly by the argument list).
                let call = match part.strip_prefix('<') {
                    Some(g) => match g.find('>') {
                        Some(i) => &g[i + 1..],
                        None => continue,
                    },
                    None => part,
                };
                let Some(rest) = call.strip_prefix("(\"") else {
                    continue;
                };
                let name = &rest[..rest.find('"').expect("closing quote")];
                assert!(
                    handlers.contains(&format!("{name},"))
                        || handlers.contains(&format!("::{name},")),
                    "UI invokes `{name}` but it is not registered in generate_handler!"
                );
                checked += 1;
            }
        }
        assert!(
            checked >= 13,
            "expected to check all UI commands, found {checked}"
        );
    }

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
