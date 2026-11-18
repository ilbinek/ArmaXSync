//! ArmaXSync desktop client (Tauri v2). The UI lives in `ui/apps/desktop`.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;

use std::collections::HashMap;
use std::sync::Mutex;

use tauri::{Emitter, Manager};

pub struct AppState {
    pub dirs: axs_client::DataDirs,
    pub syncs: Mutex<HashMap<String, axs_sync::CancellationToken>>,
}

fn forward_deep_links(app: &tauri::AppHandle, args: &[String]) {
    for a in args {
        if a.starts_with("armaxsync://") {
            let _ = app.emit("deep-link", a.clone());
        }
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("ARMAXSYNC_LOG")
                .unwrap_or_else(|_| "info".into()),
        )
        .init();
    let dirs = axs_client::DataDirs::resolve();
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
            forward_deep_links(app, &argv);
        }))
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState {
            dirs,
            syncs: Mutex::new(HashMap::new()),
        })
        .setup(|app| {
            #[cfg(any(windows, target_os = "linux"))]
            {
                use tauri_plugin_deep_link::DeepLinkExt;
                let _ = app.deep_link().register_all();
            }
            {
                use tauri_plugin_deep_link::DeepLinkExt;
                let handle = app.handle().clone();
                app.deep_link().on_open_url(move |ev| {
                    for u in ev.urls() {
                        let _ = handle.emit("deep-link", u.to_string());
                    }
                });
            }
            // Links passed on the command line of the first instance.
            let args: Vec<String> = std::env::args().collect();
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(1500));
                forward_deep_links(&handle, &args);
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_state,
            commands::save_config,
            commands::repo_resolve,
            commands::repo_save,
            commands::repo_remove,
            commands::repo_check,
            commands::repo_check_all,
            commands::repo_info,
            commands::sync_plan,
            commands::sync_start,
            commands::sync_cancel,
            commands::profile_get,
            commands::profile_save,
            commands::profile_remove,
            commands::profile_activate,
            commands::addons,
            commands::apply_modset,
            commands::launch_preview,
            commands::launch_game,
            commands::game_info,
            commands::extract_keys,
            commands::latest_rpt,
            commands::read_text_from,
            commands::server_helper,
            commands::import_legacy,
            commands::build_repo,
            commands::push_repo,
        ])
        .run(tauri::generate_context!())
        .expect("error while running ArmaXSync");
}
