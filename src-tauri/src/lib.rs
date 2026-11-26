//! BOB core (Tauri 2): the app shell, the bridge to the Swift helper and the engines.
//!
//! - `native_ipc`, `protocol`, `supervisor`: the helper (Accessibility, overlays, hotkeys, microphone,
//!   speech recognition) over a private Unix socket.
//! - `brain`: Bring Your Own Brain (registry, providers, the built-in llama-server, downloads).
//! - `ghost`, `selection`, `dictation`: the three features; `context`: personal context for prompts.
//! - `activity` (orb state), `shell` (menu bar, general settings, pause), `quit_key`.
//!
//! Privacy rule for every module: log metadata only, never typed text, selections, transcripts or keys.

pub mod activity;
pub mod app_info;
pub mod app_settings;
pub mod audit;
pub mod bench;
pub mod brain;
pub mod commands;
pub mod context;
pub mod dictation;
pub mod fsutil;
pub mod ghost;
pub mod logging;
pub mod mcp;
pub mod native_ipc;
pub mod paths;
pub mod pause;
pub mod protocol;
mod quit_key;
pub mod selection;
pub mod shell;
#[cfg(test)]
mod sim;
pub mod supervisor;
pub mod system;
#[cfg(test)]
pub(crate) mod testkit;
pub mod usage;

use app_info::AppInfo;
use native_ipc::{Bridge, BridgeStatus};
use protocol::{CoreCommand, MicStatus, NativeEvent};
use std::sync::Mutex;
use supervisor::{Backoff, Supervisor};
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem, Submenu},
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

/// Start hidden only after the welcome tour has been seen once (a first launch always shows the window).
/// The tour flag lives in the web view, so the core uses its own marker: any saved settings file.
fn commands_onboarded(app: &tauri::AppHandle) -> bool {
    layout_of(app)
        .map(|l| l.config.join(ghost::engine::SETTINGS_FILE).exists())
        .unwrap_or(false)
}

/// The folders of the running profile (`paths`): the app's own, or those under `BOB_HOME`.
fn layout_of(app: &tauri::AppHandle) -> tauri::Result<paths::Layout> {
    Ok(paths::layout(
        app.path().app_config_dir()?,
        app.path().app_data_dir()?,
    ))
}

/// Appended to the tray tooltip and the window title when a separate profile (`BOB_HOME`) runs.
pub const SEPARATE_HINT: &str = " (separate profile)";

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

/// Bind the socket, supervise the helper and forward status changes to the tray and UI.
/// The engines that consume helper events.
struct Engines {
    ghost: ghost::engine::GhostHandle,
    context: commands::Context,
    selection: selection::engine::SelectionHandle,
    dictation: dictation::engine::DictationHandle,
    pause: std::sync::Arc<pause::Pause>,
    brains: commands::Brains,
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
        pause,
        brains,
    } = engines;
    let socket = native_ipc::socket_path();
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
                    | NativeEvent::ClipboardText { .. }
            ) {
                context.observe(event);
            }
            // A (re)connected helper starts in Accessibility-only mode with the default hotkey: tell it
            // what it may capture and which hotkey the user chose.
            if matches!(event, NativeEvent::Hello { .. }) {
                commands::push_context_mode(&cmd_bridge, &context, &ghost);
                let _ = cmd_bridge.send(&selection.hotkey_command());
                let _ = cmd_bridge.send(&selection.action_hotkeys_command());
                let _ = cmd_bridge.send(&dictation.command());
            }
            // After sleep the model's weights may be paged out: warm the built-in brain up again so the
            // first suggestion is not slow (the helper warms the speech model itself).
            if matches!(event, NativeEvent::Woke) {
                let brains = brains.clone();
                tauri::async_runtime::spawn(async move {
                    let started = std::time::Instant::now();
                    if brains.warm_up_builtin().await {
                        log::info!(
                            "woke from sleep: built-in brain warmed up in {} ms",
                            started.elapsed().as_millis()
                        );
                    }
                });
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
            // While BOB is paused, the hotkeys only explain why nothing happens (Ghost is paused itself).
            if pause.is_paused()
                && matches!(
                    event,
                    NativeEvent::Selection { .. } | NativeEvent::DictationKey { pressed: true }
                )
            {
                let _ = cmd_bridge.send(&CoreCommand::Orb {
                    state: protocol::OrbState::Error,
                    text: Some("BOB is paused. Resume it from the menu bar.".into()),
                });
                return;
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
                    | NativeEvent::Typo { .. }
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
    // Mac convention: Settings… with Cmd+, right after About.
    let settings = MenuItem::with_id(app, "app-settings", "Settings…", true, Some("CmdOrCtrl+,"))?;
    let bob = Submenu::with_items(
        app,
        "BOB",
        true,
        &[
            &PredefinedMenuItem::about(app, Some("About BOB"), None)?,
            &PredefinedMenuItem::separator(app)?,
            &settings,
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

/// A panic in any thread is logged (with its location) before the default handler runs, so a crash in
/// an engine task leaves a trace in bob.log instead of a silently dead feature.
fn install_panic_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let at = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_default();
        log::error!(
            "panic at {at}: {}",
            info.payload()
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| info.payload().downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "(no message)".into())
        );
        audit::record("app.panic", serde_json::json!({"at": at}));
        default(info);
    }));
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // A separate profile (BOB_HOME) is decided before anything is written; a bad value never falls back to
    // the user's own folders.
    let profile = match paths::init() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("BOB: {e}");
            std::process::exit(2);
        }
    };
    // Logs go to stderr and ~/Library/Logs/BOB/bob.log. Default: info for BOB, warn for dependencies; the
    // level is a setting (applied once settings load) and RUST_LOG overrides it.
    // Privacy: log metadata only (sizes, timings, states), never typed text, prompts or keys.
    let log_dir = paths::logs_dir();
    logging::init(Some(&log_dir), logging::Level::Info);
    audit::init(&log_dir);
    install_panic_hook();
    if let paths::Profile::Separate(home) = &profile {
        log::info!(
            "separate profile: settings, logs, keys and sockets under {}",
            home.display()
        );
    }
    let separate = paths::is_separate();
    // One BOB per profile: a second copy would take over the helper socket and load a second model.
    let _instance = match native_ipc::acquire_instance_lock(&paths::runtime_dir()) {
        Ok(Some(lock)) => Some(lock),
        Ok(None) => {
            log::warn!("BOB is already running; this copy exits");
            return;
        }
        Err(e) => {
            log::warn!("cannot check for another running BOB ({e}); continuing");
            None
        }
    };
    let mut builder = tauri::Builder::default().menu(app_menu);
    // The window-state plugin keeps its file in the app's own config folder, so a separate profile skips it.
    if !separate {
        builder = builder.plugin(tauri_plugin_window_state::Builder::default().build());
    }
    builder
        .on_menu_event(|app, event| match event.id().as_ref() {
            "app-quit" => {
                log::info!("quit requested from the app menu");
                app.exit(0);
            }
            "app-settings" => shell::open_panel(app, "settings"),
            _ => {}
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
            commands::ghost_set_chip_offset,
            commands::ghost_set_option,
            commands::ghost_memory,
            commands::ghost_memory_clear,
            commands::ghost_memory_forget,
            commands::ghost_memory_add,
            commands::ghost_memory_edit,
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
            commands::dictation_last,
            commands::dictation_paste_last,
            commands::usage_get,
            commands::usage_clear,
            commands::system_info,
            commands::running_apps,
            shell::general_get,
            shell::general_set_dock,
            shell::general_set_open_at_launch,
            shell::general_set_login,
            shell::general_set_log_level,
            shell::mcp_set_enabled,
            shell::logs_tail,
            shell::audit_recent,
            shell::logs_open_folder,
            shell::pause_start,
            shell::pause_resume,
        ])
        .setup(|app| {
            // Brain service: registry in the config dir, models in the data dir, the sidecar's key and
            // PID files in the per-user 0700 runtime dir next to the helper socket.
            let layout = layout_of(app.handle())?;
            let config_dir = layout.config.clone();
            if let Err(e) = fsutil::private_dir(&config_dir) {
                log::warn!("cannot make the settings folder private: {e}");
            }
            let ghost_dir = config_dir.clone();
            usage::init(config_dir.join(usage::FILE));
            #[cfg(target_os = "macos")]
            log::info!("{}", app_settings::describe_login_item());
            let context: commands::Context =
                std::sync::Arc::new(context::ContextHub::load(config_dir.clone()));
            app.manage(context.clone());
            let brains: commands::Brains = std::sync::Arc::new(brain::service::BrainService::new(
                config_dir,
                layout.data.clone(),
                layout.runtime.join("llama"),
                Box::new(brain::secrets::KeychainStore),
            ));
            app.manage(brains.clone());
            commands::autostart_builtin(app.handle().clone(), brains.clone());

            // Live orb state for the window and the tray, observed from engine commands (UI-3).
            let handle = app.handle().clone();
            let activity = activity::Activity::new(move |v| {
                let _ = handle.emit(ORB_EVENT, v.clone());
                if let Some(tray) = handle.tray_by_id("bob-tray") {
                    let mut tip = tray_tooltip(v);
                    if paths::is_separate() {
                        tip.push_str(SEPARATE_HINT);
                    }
                    let _ = tray.set_tooltip(Some(tip));
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
                let dir = layout.config.clone();
                let ctx = context.clone();
                tauri::async_runtime::block_on(async move {
                    selection::engine::spawn(brains, ctx, sink, Some(dir))
                })
            };
            app.manage(selection.clone());
            let dictation = {
                let sink = observed(&activity);
                let brains = app.state::<commands::Brains>().inner().clone();
                let dir = layout.config.clone();
                let models_dir = brains.models_dir().to_path_buf();
                let ctx = context.clone();
                let sel = selection.clone();
                tauri::async_runtime::block_on(async move {
                    dictation::engine::spawn(brains, ctx, sink, Some(dir), models_dir, Some(sel))
                })
            };
            app.manage(dictation.clone());
            quit_on_signals(app.handle().clone());

            log_app_menu(app);
            let handle = app.handle().clone();
            quit_key::install(move || handle.exit(0));
            // General settings: Dock icon now; launch at login is read live from macOS.
            let general_path = layout.config.join(app_settings::FILE);
            let general = app_settings::AppSettings::load(&general_path);
            if std::env::var_os("RUST_LOG").is_none() {
                logging::set_level(general.log_level);
            }
            audit::record(
                "app.start",
                serde_json::json!({"version": env!("CARGO_PKG_VERSION"), "log_level": format!("{:?}", logging::level())}),
            );
            shell::apply_dock(app.handle(), general.show_in_dock, false);
            if paths::is_separate() {
                if let Some(w) = app.get_webview_window("main") {
                    let title = w.title().unwrap_or_else(|_| "BOB".into());
                    let _ = w.set_title(&format!("{title}{SEPARATE_HINT}"));
                }
            }
            if !general.open_window_at_launch && commands_onboarded(app.handle()) {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.hide();
                }
            }
            let mcp_on = general.mcp_enabled;
            app.manage(shell::General {
                path: general_path,
                settings: Mutex::new(general),
            });
            app.manage(shell::Mcp::default());
            let pause = std::sync::Arc::new(pause::Pause::default());
            app.manage(pause.clone());
            shell::build_tray(app, ghost.settings().enabled, dictation.settings().enabled)?;
            let (ax, mic) = {
                let items = app.state::<shell::TrayItems>();
                (items.ax.clone(), items.mic.clone())
            };
            let bridge = app.state::<Bridge>().inner().clone();
            let engines = Engines {
                ghost,
                context,
                selection,
                dictation,
                pause,
                brains: app.state::<commands::Brains>().inner().clone(),
            };
            start_native_bridge(app, bridge, engines, ax, mic);
            // AI agents (MCP): the control socket exists only while the user allows it.
            if mcp_on {
                shell::apply_mcp(app.handle(), true);
            }
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
                audit::record("app.exit", serde_json::json!({}));
                crate::mcp::stop_control(None);
                usage::flush();
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
    /// only at runtime, silently). Regression guard for a real slip.
    #[test]
    fn every_command_the_ui_invokes_is_registered() {
        let lib = include_str!("lib.rs");
        let start = lib.find("generate_handler![").expect("handler list");
        let handlers = &lib[start..start + lib[start..].find(']').expect("end of handler list")];
        let ui = [
            include_str!("../../src/lib/appInfo.ts"),
            include_str!("../../src/lib/nativeStatus.ts"),
            include_str!("../../src/lib/brains.ts"),
            include_str!("../../src/lib/dictation.ts"),
            include_str!("../../src/lib/usage.ts"),
            include_str!("../../src/lib/general.ts"),
            include_str!("../../src/lib/context.ts"),
            include_str!("../../src/lib/selection.ts"),
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
