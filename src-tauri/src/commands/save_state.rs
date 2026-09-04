use crate::application::AppState;
use crate::domain::library::GameId;
use crate::domain::save_state::{
    DeleteSaveStateResponse, LoadSaveStateResponse, SaveStateId, SaveStateView,
};
use crate::error::AppError;
use serde::Deserialize;

/// The Save-State listing request.
///
/// `deny_unknown_fields` is not decoration: it means a field React should never send — a path, a
/// slot, a digest — is rejected outright rather than silently ignored, so the contract cannot rot
/// into one that accepts more than it documents.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListSaveStatesRequest {
    pub game_id: GameId,
}

/// The load and delete request.
///
/// **A `SaveStateId` and nothing else.** There is deliberately no field for a state path, a
/// thumbnail path, a core path, a runtime path, a digest, a requested slot, or a requested
/// `CoreId`: the backend resolves every one of those from durable provenance and re-proves them
/// against the current filesystem and the current trust state.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveStateRequest {
    pub save_state_id: SaveStateId,
}

/// The load request.
///
/// A `SaveStateId`, plus `activeGamepadId` — the frontend's own confirmed identity of the one
/// controller RetroFrontier currently accepts (`Gamepad.id`, via the browser Gamepad API; see
/// ADR-014), or absent when none is connected or supported. It is used for nothing but gating
/// which save-state hotkeys, if any, this launch may derive (MEDIUM-2); everything else about the
/// load is still resolved from durable provenance and re-proved independently.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LoadSaveStateRequest {
    pub save_state_id: SaveStateId,
    #[serde(default)]
    pub active_gamepad_id: Option<String>,
}

/// The bounded Save-State projection Game Detail renders.
///
/// Only `available`, proved states appear, ordered by the backend. A state whose registered file
/// is provably gone transitions to `missing` here and is simply absent from the result.
#[tauri::command]
pub async fn list_save_states(
    state: tauri::State<'_, AppState>,
    request: ListSaveStatesRequest,
) -> Result<Vec<SaveStateView>, AppError> {
    state.save_states().list_save_states(request.game_id).await
}

/// Load one Save State through the shared managed launch pipeline.
///
/// Every anticipated problem is a tagged response, not an IPC error, so React acts on a stable
/// code instead of parsing text.
#[tauri::command]
pub async fn load_save_state(
    state: tauri::State<'_, AppState>,
    request: LoadSaveStateRequest,
) -> Result<LoadSaveStateResponse, AppError> {
    Ok(state
        .save_states()
        .load_save_state(request.save_state_id, request.active_gamepad_id)
        .await)
}

/// Delete one Save State, after re-proving the exact current filesystem target.
///
/// The UI confirmation is a courtesy to the user, not the security boundary: this command fails
/// closed on its own if anything about the target no longer matches what was registered.
#[tauri::command]
pub async fn delete_save_state(
    state: tauri::State<'_, AppState>,
    request: SaveStateRequest,
) -> Result<DeleteSaveStateResponse, AppError> {
    Ok(state
        .save_states()
        .delete_save_state(request.save_state_id)
        .await)
}

#[cfg(test)]
mod tests {
    use super::{ListSaveStatesRequest, LoadSaveStateRequest, SaveStateRequest};

    /// The load request accepts the identity, and optionally the confirmed active-controller
    /// identity (MEDIUM-2) — nothing else.
    #[test]
    fn a_load_request_carries_only_a_save_state_id_and_an_optional_active_gamepad_id() {
        let request: LoadSaveStateRequest =
            serde_json::from_value(serde_json::json!({ "saveStateId": 42 })).unwrap();
        assert_eq!(request.save_state_id.0, 42);
        assert_eq!(request.active_gamepad_id, None);

        let request: LoadSaveStateRequest = serde_json::from_value(serde_json::json!({
            "saveStateId": 42,
            "activeGamepadId": "Sony Interactive Entertainment DualSense Wireless Controller",
        }))
        .unwrap();
        assert_eq!(
            request.active_gamepad_id.as_deref(),
            Some("Sony Interactive Entertainment DualSense Wireless Controller")
        );

        for smuggled in [
            serde_json::json!({ "saveStateId": 42, "statePath": "/etc/passwd" }),
            serde_json::json!({ "saveStateId": 42, "corePath": "/tmp/core.so" }),
            serde_json::json!({ "saveStateId": 42, "slot": 3 }),
        ] {
            assert!(
                serde_json::from_value::<LoadSaveStateRequest>(smuggled.clone()).is_err(),
                "{smuggled} must be refused"
            );
        }
        assert!(serde_json::from_value::<LoadSaveStateRequest>(serde_json::json!({})).is_err());
    }

    /// The IPC surface accepts identities and nothing else.
    #[test]
    fn a_save_state_request_carries_only_a_save_state_id() {
        let request: SaveStateRequest =
            serde_json::from_value(serde_json::json!({ "saveStateId": 42 })).unwrap();
        assert_eq!(request.save_state_id.0, 42);

        // Every field React must never supply is refused rather than ignored, so a caller cannot
        // smuggle a path, a digest, a slot, or a core choice past the boundary.
        for smuggled in [
            serde_json::json!({ "saveStateId": 42, "statePath": "/etc/passwd" }),
            serde_json::json!({ "saveStateId": 42, "relativePath": "../escape.state1" }),
            serde_json::json!({ "saveStateId": 42, "thumbnailPath": "/tmp/x.png" }),
            serde_json::json!({ "saveStateId": 42, "corePath": "/tmp/core.so" }),
            serde_json::json!({ "saveStateId": 42, "runtimePath": "/tmp/runtime" }),
            serde_json::json!({ "saveStateId": 42, "sha256": "a" }),
            serde_json::json!({ "saveStateId": 42, "slot": 3 }),
            serde_json::json!({ "saveStateId": 42, "coreId": "nestopia" }),
        ] {
            assert!(
                serde_json::from_value::<SaveStateRequest>(smuggled.clone()).is_err(),
                "{smuggled} must be refused"
            );
        }
        // And the identity itself is required.
        assert!(serde_json::from_value::<SaveStateRequest>(serde_json::json!({})).is_err());
    }

    #[test]
    fn a_listing_request_carries_only_a_game_id() {
        let request: ListSaveStatesRequest =
            serde_json::from_value(serde_json::json!({ "gameId": 7 })).unwrap();
        assert_eq!(request.game_id.0, 7);

        assert!(serde_json::from_value::<ListSaveStatesRequest>(
            serde_json::json!({ "gameId": 7, "statesRoot": "/tmp" })
        )
        .is_err());
        assert!(serde_json::from_value::<ListSaveStatesRequest>(serde_json::json!({})).is_err());
    }
}
