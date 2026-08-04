mod cleaner;
mod commands;
mod model;
mod plugins;
mod scanner;
mod settings;
mod trash_backend;

use std::sync::atomic::Ordering;
use tauri::{Emitter, Manager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(commands::AppState::default())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::begin_scan,
            commands::cancel_scan,
            commands::clean_candidates
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let state = window.state::<commands::AppState>();
                if state.cleaning.load(Ordering::Relaxed) {
                    api.prevent_close();
                    let _ = window.emit("cleaning-close-blocked", ());
                } else if let Ok(inner) = state.inner.lock() {
                    if let Some((_, cancel)) = &inner.active_scan {
                        cancel.store(true, Ordering::Relaxed);
                    }
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
