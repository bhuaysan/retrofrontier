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
    /// The frontend's own confirmed identity of the one controller RetroFrontier currently accepts
    /// (`Gamepad.id`, via the browser Gamepad API; see ADR-014), or absent when none is connected
    /// or supported. Used for nothing but gating which save-state hotkeys, if any, this launch may
    /// derive (MEDIUM-2) — never a filesystem path, an executable, or anything else this request
    /// otherwise deliberately excludes.
    #[serde(default)]
    pub active_gamepad_id: Option<String>,
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
        .launch_game(
            request.game_id,
            request.content_unit_id,
            request.active_gamepad_id,
        )
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

#[cfg(test)]
mod tests {
    use super::LaunchGameRequest;

    /// `activeGamepadId` (MEDIUM-2) is optional and absent by default, exactly like
    /// `contentUnitId`.
    #[test]
    fn a_launch_request_defaults_active_gamepad_id_to_none_and_accepts_it_when_supplied() {
        let request: LaunchGameRequest =
            serde_json::from_value(serde_json::json!({ "gameId": 1 })).unwrap();
        assert_eq!(request.active_gamepad_id, None);

        let request: LaunchGameRequest = serde_json::from_value(serde_json::json!({
            "gameId": 1,
            "activeGamepadId": "Sony Interactive Entertainment DualSense Wireless Controller",
        }))
        .unwrap();
        assert_eq!(
            request.active_gamepad_id.as_deref(),
            Some("Sony Interactive Entertainment DualSense Wireless Controller")
        );
    }
}
