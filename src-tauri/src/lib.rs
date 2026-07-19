mod commands;
mod state;
mod vault;

use state::AppState;
use std::sync::Mutex;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(Mutex::new(AppState::default()))
        .invoke_handler(tauri::generate_handler![
            commands::vault_status,
            commands::vault_open,
            commands::vault_close,
            commands::session_unlock,
            commands::session_lock,
            commands::session_status,
            commands::prompts_list,
            commands::prompts_get,
            commands::prompts_create,
            commands::prompts_update,
            commands::prompts_delete,
            commands::prompts_search,
            commands::prompts_bulk_move,
            commands::prompts_bulk_delete,
            commands::prompts_copy_body,
            commands::folders_list,
            commands::folders_create,
            commands::folders_rename,
            commands::folders_set_locked,
            commands::folders_delete,
            commands::folders_export,
            commands::smart_folders_list,
            commands::smart_folders_create,
            commands::smart_folders_delete,
            commands::tags_list,
            commands::tags_rename,
            commands::tags_delete,
            commands::data_export_all,
            commands::data_export_prompts,
            commands::data_import,
            commands::data_export_to_file,
            commands::data_import_from_file,
            commands::settings_get,
            commands::settings_set_embedding,
            commands::settings_clear_embedding_api_key,
            commands::settings_set_remember_lock,
            commands::app_version,
        ])
        .setup(|app| {
            // Ensure app data dir exists
            if let Ok(dir) = app.path().app_data_dir() {
                let _ = std::fs::create_dir_all(dir);
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running PromptSack");
}
