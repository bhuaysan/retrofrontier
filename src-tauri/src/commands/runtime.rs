use crate::application::AppState;
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
