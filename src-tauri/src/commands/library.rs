use crate::application::AppState;
use crate::domain::library::{
    ContentRoot, ContentRootId, LibrarySnapshot, ScanIssue, ScanStatus, ScanSummary,
};
use crate::domain::system::SystemId;
use crate::error::AppError;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddExternalContentRootRequest {
    pub path: String,
    pub system_hint: Option<SystemId>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentRootRequest {
    pub root_id: ContentRootId,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetContentRootEnabledRequest {
    pub root_id: ContentRootId,
    pub enabled: bool,
}

#[tauri::command]
pub async fn get_content_roots(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ContentRoot>, AppError> {
    let result = state.library().get_content_roots().await;
    log_result(&result);
    result
}

#[tauri::command]
pub async fn add_external_content_root(
    request: AddExternalContentRootRequest,
    state: tauri::State<'_, AppState>,
) -> Result<ContentRoot, AppError> {
    let result = state
        .library()
        .add_external_content_root(&request.path, request.system_hint)
        .await;
    log_result(&result);
    result
}

#[tauri::command]
pub async fn remove_external_content_root(
    request: ContentRootRequest,
    state: tauri::State<'_, AppState>,
) -> Result<(), AppError> {
    let result = state
        .library()
        .remove_external_content_root(request.root_id)
        .await;
    log_result(&result);
    result
}

#[tauri::command]
pub async fn set_content_root_enabled(
    request: SetContentRootEnabledRequest,
    state: tauri::State<'_, AppState>,
) -> Result<ContentRoot, AppError> {
    let result = state
        .library()
        .set_content_root_enabled(request.root_id, request.enabled)
        .await;
    log_result(&result);
    result
}

#[tauri::command]
pub async fn rescan_library(state: tauri::State<'_, AppState>) -> Result<ScanSummary, AppError> {
    let result = state.library().rescan_library().await;
    log_result(&result);
    result
}

#[tauri::command]
pub fn get_scan_status(state: tauri::State<'_, AppState>) -> ScanStatus {
    state.library().get_scan_status()
}

#[tauri::command]
pub async fn get_scan_issues(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ScanIssue>, AppError> {
    let result = state.library().get_scan_issues().await;
    log_result(&result);
    result
}

#[tauri::command]
pub async fn get_library_snapshot(
    state: tauri::State<'_, AppState>,
) -> Result<LibrarySnapshot, AppError> {
    let result = state.library().get_library_snapshot().await;
    log_result(&result);
    result
}

fn log_result<T>(result: &Result<T, AppError>) {
    if let Err(error) = result {
        error.log();
    }
}
