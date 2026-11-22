pub mod commands;
pub mod core;
pub mod plugins;

use core::desktop::{self, DesktopSettingsState};
use core::profiles::{self, ProfileSessionState};
use core::scheduler::{self, ScheduleFileLock};
use tauri::{Manager, WindowEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(ProfileSessionState::default())
        .manage(DesktopSettingsState::default())
        .manage(ScheduleFileLock::default())
        .setup(|app| {
            let handle = app.handle();
            let _ = profiles::ensure_initialized(handle);
            if let Err(err) = plugins::reload_installed_plugins(handle) {
                eprintln!("Warning: {err}");
            }

            let settings = desktop::load_settings(handle);
            if let Ok(mut guard) = app.state::<DesktopSettingsState>().0.lock() {
                *guard = settings.clone();
            }

            if let Err(err) = desktop::setup_tray(handle) {
                eprintln!("Warning: {err}");
            }
            if let Err(err) =
                desktop::apply_global_shortcut(handle, settings.global_shortcut.as_deref())
            {
                eprintln!("Warning: {err}");
            }

            scheduler::spawn_scheduler_worker(handle.clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event
                && window.label() == "main"
            {
                let minimize_to_tray = window
                    .app_handle()
                    .state::<DesktopSettingsState>()
                    .0
                    .lock()
                    .map(|g| g.minimize_to_tray)
                    .unwrap_or(true);

                if minimize_to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::list_profiles,
            commands::create_profile,
            commands::rename_profile,
            commands::unlock_profile,
            commands::lock_profile,
            commands::set_profile_password,
            commands::delete_profile,
            commands::get_platforms,
            commands::save_credentials,
            commands::verify_platform,
            commands::unlink_platform,
            commands::start_platform_oauth,
            commands::broadcast_post,
            commands::get_history,
            commands::clear_history,
            commands::recall_post,
            commands::get_desktop_settings,
            commands::update_desktop_settings,
            commands::open_data_folder,
            commands::clear_media_cache,
            commands::unlink_all_profile_accounts,
            commands::factory_reset,
            commands::schedule_post,
            commands::list_scheduled_posts,
            commands::cancel_scheduled_post,
            commands::publish_scheduled_now,
            commands::retry_scheduled_post,
            commands::export_profile_backup,
            commands::inspect_profile_backup,
            commands::preview_backup_restore,
            commands::import_profile_backup,
            plugins::get_registry_plugins,
            plugins::get_plugin_manifest,
            plugins::install_registry_plugin,
            plugins::install_local_plugin,
            plugins::set_plugin_enabled,
            plugins::uninstall_plugin,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
