//! Tauri command surface — mirrors the old Electron IPC API.

use crate::state::AppState;
use crate::vault::store::*;
use crate::vault::embed::EmbeddingConfig;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State};

fn with_vault<T>(
    state: &State<'_, Mutex<AppState>>,
    f: impl FnOnce(&Vault) -> Result<T, String>,
) -> Result<T, String> {
    let guard = state.lock().map_err(|e| e.to_string())?;
    let vault = guard
        .vault
        .as_ref()
        .ok_or_else(|| "Database is not open. Unlock or create a vault first.".to_string())?;
    f(vault)
}

fn vault_root(app: &AppHandle) -> PathBuf {
    if let Ok(dir) = std::env::var("PROMPTSACK_DATA_DIR") {
        return PathBuf::from(dir);
    }
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("vault")
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultStatus {
    pub open: bool,
    pub path: String,
    pub exists: bool,
    pub session_unlocked: bool,
    pub keychain_backend: String,
    pub storage: String,
    pub shell: String,
}

#[tauri::command]
pub fn vault_status(app: AppHandle, state: State<'_, Mutex<AppState>>) -> Result<VaultStatus, String> {
    let root = vault_root(&app);
    let guard = state.lock().map_err(|e| e.to_string())?;
    Ok(VaultStatus {
        open: guard.vault.is_some(),
        path: root.display().to_string(),
        exists: root.join("index.mdb").exists(),
        session_unlocked: guard
            .vault
            .as_ref()
            .map(|v| v.is_session_unlocked())
            .unwrap_or(false),
        keychain_backend: "keyring".into(),
        storage: "markdown+mongreldb-rust".into(),
        shell: "tauri".into(),
    })
}

#[tauri::command]
pub fn vault_open(
    app: AppHandle,
    state: State<'_, Mutex<AppState>>,
    passphrase: String,
) -> Result<serde_json::Value, String> {
    let root = vault_root(&app);
    let mut guard = state.lock().map_err(|e| e.to_string())?;
    // drop previous
    guard.vault = None;
    let vault = Vault::open_or_create(&root, &passphrase)?;
    let auto = vault.try_auto_unlock();
    let path = vault.root().display().to_string();
    guard.vault = Some(vault);
    Ok(serde_json::json!({
        "ok": true,
        "path": path,
        "autoUnlocked": auto,
        "keychainBackend": "keyring",
        "storage": "markdown+mongreldb-rust",
        "shell": "tauri",
    }))
}

#[tauri::command]
pub fn vault_close(state: State<'_, Mutex<AppState>>) -> Result<serde_json::Value, String> {
    let mut guard = state.lock().map_err(|e| e.to_string())?;
    guard.vault = None;
    Ok(serde_json::json!({ "ok": true }))
}

#[tauri::command]
pub fn session_unlock(
    state: State<'_, Mutex<AppState>>,
    password: String,
    remember: Option<bool>,
) -> Result<serde_json::Value, String> {
    with_vault(&state, |v| {
        let ok = v.unlock_session(&password);
        if ok {
            if remember == Some(false) {
                v.set_remember_lock(false)?;
            } else {
                v.set_remember_lock(true)?;
            }
        }
        Ok(serde_json::json!({ "ok": ok, "keychainBackend": "keyring" }))
    })
}

#[tauri::command]
pub fn session_lock(
    state: State<'_, Mutex<AppState>>,
    forget_keychain: Option<bool>,
) -> Result<serde_json::Value, String> {
    with_vault(&state, |v| {
        v.lock_session(forget_keychain.unwrap_or(false));
        Ok(serde_json::json!({ "ok": true }))
    })
}

#[tauri::command]
pub fn session_status(state: State<'_, Mutex<AppState>>) -> Result<serde_json::Value, String> {
    let guard = state.lock().map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "unlocked": guard.vault.as_ref().map(|v| v.is_session_unlocked()).unwrap_or(false),
        "keychainBackend": "keyring",
    }))
}

#[tauri::command]
pub fn prompts_list(
    state: State<'_, Mutex<AppState>>,
    filter: Option<ListFilter>,
    sort: Option<String>,
) -> Result<Vec<Prompt>, String> {
    with_vault(&state, |v| {
        v.list_prompts(filter.unwrap_or(ListFilter::All), sort.as_deref().unwrap_or("newest"))
    })
}

#[tauri::command]
pub fn prompts_get(state: State<'_, Mutex<AppState>>, id: String) -> Result<Option<Prompt>, String> {
    with_vault(&state, |v| Ok(v.get_prompt(&id)))
}

#[tauri::command]
pub fn prompts_create(
    state: State<'_, Mutex<AppState>>,
    input: CreatePromptInput,
) -> Result<Prompt, String> {
    with_vault(&state, |v| v.create_prompt(input))
}

#[tauri::command]
pub fn prompts_update(
    state: State<'_, Mutex<AppState>>,
    id: String,
    patch: UpdatePromptInput,
) -> Result<Prompt, String> {
    with_vault(&state, |v| v.update_prompt(&id, patch))
}

#[tauri::command]
pub fn prompts_delete(state: State<'_, Mutex<AppState>>, id: String) -> Result<serde_json::Value, String> {
    with_vault(&state, |v| {
        v.delete_prompt(&id)?;
        Ok(serde_json::json!({ "ok": true }))
    })
}

#[tauri::command]
pub fn prompts_search(
    state: State<'_, Mutex<AppState>>,
    query: String,
) -> Result<Vec<SearchHit>, String> {
    with_vault(&state, |v| v.search(&query, 50))
}

#[tauri::command]
pub fn prompts_bulk_move(
    state: State<'_, Mutex<AppState>>,
    ids: Vec<String>,
    folder_id: Option<String>,
) -> Result<serde_json::Value, String> {
    with_vault(&state, |v| {
        let count = v.bulk_move(&ids, folder_id)?;
        Ok(serde_json::json!({ "count": count }))
    })
}

#[tauri::command]
pub fn prompts_bulk_delete(
    state: State<'_, Mutex<AppState>>,
    ids: Vec<String>,
) -> Result<serde_json::Value, String> {
    with_vault(&state, |v| {
        let count = v.bulk_delete(&ids)?;
        Ok(serde_json::json!({ "count": count }))
    })
}

#[tauri::command]
pub fn prompts_copy_body(
    app: AppHandle,
    state: State<'_, Mutex<AppState>>,
    id: String,
) -> Result<serde_json::Value, String> {
    let body = with_vault(&state, |v| {
        let p = v
            .get_prompt(&id)
            .ok_or_else(|| "Prompt not found".to_string())?;
        if p.content_encrypted {
            return Err("Unlock the session to copy locked content".into());
        }
        Ok(p.body)
    })?;
    use tauri_plugin_clipboard_manager::ClipboardExt;
    app.clipboard()
        .write_text(body.clone())
        .map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "ok": true, "length": body.len() }))
}

#[tauri::command]
pub fn folders_list(state: State<'_, Mutex<AppState>>) -> Result<Vec<Folder>, String> {
    with_vault(&state, |v| v.list_folders())
}

#[tauri::command]
pub fn folders_create(state: State<'_, Mutex<AppState>>, name: String) -> Result<Folder, String> {
    with_vault(&state, |v| v.create_folder(&name))
}

#[tauri::command]
pub fn folders_rename(
    state: State<'_, Mutex<AppState>>,
    id: String,
    name: String,
) -> Result<Folder, String> {
    with_vault(&state, |v| v.rename_folder(&id, &name))
}

#[tauri::command]
pub fn folders_set_locked(
    state: State<'_, Mutex<AppState>>,
    id: String,
    locked: bool,
    password: Option<String>,
) -> Result<Folder, String> {
    with_vault(&state, |v| v.set_folder_locked(&id, locked, password))
}

#[tauri::command]
pub fn folders_delete(
    state: State<'_, Mutex<AppState>>,
    id: String,
    mode: String,
) -> Result<serde_json::Value, String> {
    with_vault(&state, |v| {
        v.delete_folder(&id, &mode)?;
        Ok(serde_json::json!({ "ok": true }))
    })
}

#[tauri::command]
pub fn folders_export(
    state: State<'_, Mutex<AppState>>,
    id: String,
) -> Result<ExportPayload, String> {
    with_vault(&state, |v| v.export_folder(&id))
}

#[tauri::command]
pub fn smart_folders_list(state: State<'_, Mutex<AppState>>) -> Result<Vec<SmartFolder>, String> {
    with_vault(&state, |v| v.list_smart_folders())
}

#[tauri::command]
pub fn smart_folders_create(
    state: State<'_, Mutex<AppState>>,
    name: String,
    filter: ListFilter,
) -> Result<SmartFolder, String> {
    with_vault(&state, |v| v.create_smart_folder(&name, filter))
}

#[tauri::command]
pub fn smart_folders_delete(
    state: State<'_, Mutex<AppState>>,
    id: String,
) -> Result<serde_json::Value, String> {
    with_vault(&state, |v| {
        v.delete_smart_folder(&id)?;
        Ok(serde_json::json!({ "ok": true }))
    })
}

#[tauri::command]
pub fn tags_list(state: State<'_, Mutex<AppState>>) -> Result<Vec<TagInfo>, String> {
    with_vault(&state, |v| Ok(v.list_tags()))
}

#[tauri::command]
pub fn tags_rename(
    state: State<'_, Mutex<AppState>>,
    old_name: String,
    new_name: String,
) -> Result<serde_json::Value, String> {
    with_vault(&state, |v| {
        v.rename_tag(&old_name, &new_name)?;
        Ok(serde_json::json!({ "ok": true }))
    })
}

#[tauri::command]
pub fn tags_delete(state: State<'_, Mutex<AppState>>, name: String) -> Result<serde_json::Value, String> {
    with_vault(&state, |v| {
        v.delete_tag(&name)?;
        Ok(serde_json::json!({ "ok": true }))
    })
}

#[tauri::command]
pub fn data_export_all(state: State<'_, Mutex<AppState>>) -> Result<ExportPayload, String> {
    with_vault(&state, |v| v.export_all())
}

#[tauri::command]
pub fn data_export_prompts(
    state: State<'_, Mutex<AppState>>,
    ids: Vec<String>,
) -> Result<ExportPayload, String> {
    with_vault(&state, |v| v.export_prompts(&ids))
}

#[tauri::command]
pub fn data_import(
    state: State<'_, Mutex<AppState>>,
    payload: ExportPayload,
    mode: Option<String>,
) -> Result<serde_json::Value, String> {
    with_vault(&state, |v| {
        let (folders, prompts) = v.import_payload(payload, mode.as_deref().unwrap_or("merge"))?;
        Ok(serde_json::json!({ "folders": folders, "prompts": prompts }))
    })
}

#[tauri::command]
pub fn data_export_to_file(
    app: AppHandle,
    payload: ExportPayload,
) -> Result<serde_json::Value, String> {
    use tauri_plugin_dialog::DialogExt;
    let path = app
        .dialog()
        .file()
        .set_file_name(&format!(
            "promptsack-export-{}.json",
            chrono::Utc::now().format("%Y-%m-%d")
        ))
        .add_filter("JSON", &["json"])
        .blocking_save_file();
    let Some(path) = path else {
        return Ok(serde_json::json!({ "ok": false }));
    };
    let path = path.into_path().map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "ok": true, "path": path.display().to_string() }))
}

#[tauri::command]
pub fn data_import_from_file(
    app: AppHandle,
    state: State<'_, Mutex<AppState>>,
) -> Result<serde_json::Value, String> {
    use tauri_plugin_dialog::DialogExt;
    let path = app
        .dialog()
        .file()
        .add_filter("JSON", &["json"])
        .blocking_pick_file();
    let Some(path) = path else {
        return Ok(serde_json::json!({ "ok": false }));
    };
    let path = path.into_path().map_err(|e| e.to_string())?;
    let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let payload: ExportPayload = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    with_vault(&state, |v| {
        let (folders, prompts) = v.import_payload(payload, "merge")?;
        Ok(serde_json::json!({ "ok": true, "folders": folders, "prompts": prompts }))
    })
}

#[tauri::command]
pub fn settings_get(state: State<'_, Mutex<AppState>>) -> Result<serde_json::Value, String> {
    with_vault(&state, |v| v.get_settings())
}

#[tauri::command]
pub fn settings_set_embedding(
    state: State<'_, Mutex<AppState>>,
    config: EmbeddingConfig,
    api_key: Option<String>,
) -> Result<serde_json::Value, String> {
    with_vault(&state, |v| {
        let applied = v.set_embedding(config, api_key)?;
        Ok(serde_json::json!({ "ok": true, "config": applied, "keychainBackend": "keyring" }))
    })
}

#[tauri::command]
pub fn settings_clear_embedding_api_key(
    state: State<'_, Mutex<AppState>>,
) -> Result<serde_json::Value, String> {
    with_vault(&state, |v| {
        v.clear_embedding_api_key()?;
        Ok(serde_json::json!({ "ok": true }))
    })
}

#[tauri::command]
pub fn settings_set_remember_lock(
    state: State<'_, Mutex<AppState>>,
    remember: bool,
) -> Result<serde_json::Value, String> {
    with_vault(&state, |v| {
        v.set_remember_lock(remember)?;
        Ok(serde_json::json!({ "ok": true }))
    })
}

#[tauri::command]
pub fn app_version(app: AppHandle) -> String {
    app.package_info().version.to_string()
}
