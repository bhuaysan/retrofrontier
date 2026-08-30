use crate::application::AppState;
use crate::domain::launch::{LaunchResponse, LaunchState};
use crate::domain::library::{ContentUnitId, GameId};
use crate::error::AppError;
use serde::Deserialize;

/// The semantic launch request.
///
/// React supplies RetroFrontier identities only. There is deliberately no field for an executable,
/// core, BIOS, save, system, or content filesystem path.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchGameRequest {
    pub game_id: GameId,
    #[serde(default)]
    pub content_unit_id: Option<ContentUnitId>,
}

/// Every anticipated launch problem is a normalized `LaunchResponse`, not an IPC error, so React
/// can act on a stable code instead of parsing an error string.
#[tauri::command]
pub async fn launch_game(
    state: tauri::State<'_, AppState>,
    request: LaunchGameRequest,
) -> Result<LaunchResponse, AppError> {
    let response = state
        .launch()
        .launch_game(request.game_id, request.content_unit_id)
        .await;
    if let Some(code) = response.error_code() {
        tracing::info!(code = code.as_str(), "a launch request was not started");
    }
    Ok(response)
}

#[tauri::command]
pub async fn get_launch_state(state: tauri::State<'_, AppState>) -> Result<LaunchState, AppError> {
    Ok(state.launch().get_launch_state())
}
