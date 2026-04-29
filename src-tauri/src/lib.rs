mod commands;
mod conversion;
mod paths;
mod progress;

use std::collections::HashMap;
use std::sync::Mutex;
use tauri::{Manager, WindowEvent};
use tauri_plugin_shell::process::CommandChild;

/// Application-wide state: tracks running conversions so they can be cancelled.
pub struct AppState {
    pub jobs: Mutex<HashMap<String, CommandChild>>,
}

impl AppState {
    fn new() -> Self {
        Self {
            jobs: Mutex::new(HashMap::new()),
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::start_conversion,
            commands::cancel_conversion,
            commands::open_output_folder,
            commands::reveal_file_in_finder,
            commands::ensure_output_dir,
        ])
        .setup(|app| {
            // Lazy-show the main window after a brief tick so the DOM is rendered
            // before we paint, avoiding a flash of unstyled content.
            if let Some(window) = app.get_webview_window("main") {
                let w = window.clone();
                tauri::async_runtime::spawn(async move {
                    // Give the WebView a tick to paint initial CSS before showing.
                    tokio::time::sleep(std::time::Duration::from_millis(40)).await;
                    let _ = w.show();
                    let _ = w.set_focus();
                });
            }
            // Make sure the output directory exists at startup.
            let _ = paths::ensure_output_directory();
            Ok(())
        })
        .on_window_event(|window, event| {
            // When the user closes the window, kill any in-flight subprocesses
            // so we don't leave orphaned yt-dlp/ffmpeg processes behind.
            if matches!(event, WindowEvent::CloseRequested { .. }) {
                if let Some(state) = window.try_state::<AppState>() {
                    if let Ok(mut jobs) = state.jobs.lock() {
                        for (_id, child) in jobs.drain() {
                            let _ = child.kill();
                        }
                    }
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running RetroTube");
}
