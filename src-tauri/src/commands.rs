//! Thin wrappers around the conversion engine, exposed to the WebView via `invoke`.

use crate::conversion::{self, ConversionRequest};
use crate::paths;
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

#[tauri::command]
pub async fn start_conversion(
    app: AppHandle,
    request: ConversionRequest,
) -> Result<String, String> {
    conversion::start(app, request).await
}

#[tauri::command]
pub fn cancel_conversion(app: AppHandle, job_id: String) -> Result<(), String> {
    conversion::cancel(&app, &job_id)
}

#[tauri::command]
pub fn open_output_folder(app: AppHandle) -> Result<(), String> {
    let dir = paths::ensure_output_directory().map_err(|e| e.to_string())?;
    app.opener()
        .open_path(dir.to_string_lossy().to_string(), None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn reveal_file_in_finder(app: AppHandle, path: String) -> Result<(), String> {
    if path.trim().is_empty() {
        return open_output_folder(app);
    }
    app.opener()
        .reveal_item_in_dir(&path)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn ensure_output_dir() -> Result<String, String> {
    let dir = paths::ensure_output_directory().map_err(|e| e.to_string())?;
    Ok(dir.to_string_lossy().to_string())
}
