use crate::application::AppState;
use crate::domain::AppInfo;
use crate::error::AppError;

#[tauri::command]
pub async fn get_app_info(state: tauri::State<'_, AppState>) -> Result<AppInfo, AppError> {
    let result = state.app_info().get_app_info().await;
    if let Err(error) = &result {
        error.log();
    }
    result
}
