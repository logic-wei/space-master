pub mod catalog;
pub mod commands;
pub mod diagnostics;
pub mod fsutil;
pub mod model;
pub mod remove;
pub mod safety;
pub mod scan;
pub mod simctl;
pub mod state;

use tauri::Manager;

use state::AppState;

pub fn run() {
    tauri::Builder::default()
        // Registered before anything else, as this plugin requires: the second process
        // has to be turned away before it starts scanning or writing a ledger.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            // Raise the window that is already open, so a second launch looks like it
            // did something rather than nothing.
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .setup(|app| {
            let home = std::env::home_dir()
                .ok_or("$HOME is not set")?
                .canonicalize()?;
            // Resolved once at startup and handed to the Guard for R11. `None` only
            // if the platform has no data directory, in which case there is nothing
            // of ours to protect.
            let app_data_dir = app.path().app_data_dir().ok();
            // Before the state, so a panic in the setup below is also recorded.
            if let Some(dir) = &app_data_dir {
                diagnostics::install_panic_log(dir);
            }
            app.manage(AppState::new(home, app_data_dir)?);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::volume::get_volume_info,
            commands::scan::run_quick_scan,
            commands::scan::run_dev_scan,
            commands::scan::run_xcode_scan,
            commands::scan::cancel_scan,
            commands::orphans::run_orphan_scan,
            commands::orphans::reveal_orphan,
            commands::privacy::get_privacy_status,
            commands::privacy::open_privacy_settings,
            commands::clean::preview_clean,
            commands::clean::execute_clean,
            commands::clean::unfinished_batches,
            commands::ledger::ledger_history,
            commands::simulators::run_simulator_scan,
            commands::simulators::delete_simulators,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
