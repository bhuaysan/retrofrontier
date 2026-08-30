use crate::application::AppState;
use crate::application::{RuntimeInstallResponse, RuntimeInstallState};
use crate::domain::runtime::RuntimeStatus;
use crate::error::AppError;

#[tauri::command]
pub async fn get_runtime_status(
    state: tauri::State<'_, AppState>,
) -> Result<RuntimeStatus, AppError> {
    let result = state.runtime().get_runtime_status().await;
    if let Err(error) = &result {
        error.log();
    }
    result
}

/// Everything Settings needs to describe the managed runtime honestly, including whether this
/// build has an approved release source at all.
#[tauri::command]
pub async fn get_runtime_install_state(
    state: tauri::State<'_, AppState>,
) -> Result<RuntimeInstallState, AppError> {
    let result = state.runtime().get_install_state();
    if let Err(error) = &result {
        error.log();
    }
    result
}

/// Install the single approved managed release. Anticipated problems are normalized into the
/// response rather than raised as IPC errors, so React never parses an error string.
#[tauri::command]
pub async fn install_runtime(
    state: tauri::State<'_, AppState>,
) -> Result<RuntimeInstallResponse, AppError> {
    Ok(state.runtime().install_runtime().await)
}

/// Reconstruct the approved managed release into a fresh immutable installation.
#[tauri::command]
pub async fn repair_runtime(
    state: tauri::State<'_, AppState>,
) -> Result<RuntimeInstallResponse, AppError> {
    Ok(state.runtime().repair_runtime().await)
}
