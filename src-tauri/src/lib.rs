//! BOB core. Orchestrates engines, brains, settings and the UI.
//!
//! Task 1 only wires the shell: app metadata, the tray icon and a hide-on-close
//! window so BOB can live in the menu bar. Later tasks add modules for the
//! native-helper IPC, the Brain router and the Ghost/Selection/Dictation engines.

pub mod app_info;

use app_info::AppInfo;
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Manager, WindowEvent,
};

/// IPC: static metadata about this build.
#[tauri::command]
fn app_info() -> AppInfo {
    AppInfo::current()
}

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open BOB", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit BOB", true, Some("Cmd+Q"))?;
    let menu = Menu::with_items(app, &[&open, &quit])?;

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
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![app_info])
        .setup(|app| {
            setup_tray(app)?;
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
