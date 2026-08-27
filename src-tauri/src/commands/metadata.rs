//! Metadata IPC surface.
//!
//! These commands deserialize their input, call one application service method, and return a typed
//! DTO. They contain no provider logic, no SQL, no filesystem access, and no retry behaviour.
//!
//! Nothing here can return an application credential, a personal password, a raw provider payload,
//! or an authenticated provider URL: the DTOs simply have no field for them.

use crate::adapters::credentials::SecretString;
use crate::application::AppState;
use crate::domain::library::GameId;
use crate::domain::metadata::{GameMetadataState, MetadataProviderStatus, UserAccountState};
use crate::error::AppError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameMetadataRequest {
    pub game_id: GameId,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectMetadataCandidateRequest {
    pub game_id: GameId,
    pub provider_game_id: String,
}

/// Write-only credential input.
///
/// Deliberately `Deserialize` only, so the same shape can never travel back to the frontend.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetProviderCredentialsRequest {
    pub username: String,
    pub password: String,
}

impl std::fmt::Debug for SetProviderCredentialsRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SetProviderCredentialsRequest")
            .field("username", &crate::adapters::credentials::REDACTED)
            .field("password", &crate::adapters::credentials::REDACTED)
            .finish()
    }
}

/// Readable state of the optional personal account. Has no password field by construction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderAccountStatus {
    pub configured: bool,
    pub state: UserAccountState,
    /// Account name when it can be read safely. Never the password.
    pub username: Option<String>,
}

#[tauri::command]
pub async fn get_game_metadata(
    request: GameMetadataRequest,
    state: tauri::State<'_, AppState>,
) -> Result<GameMetadataState, AppError> {
    let result = state.metadata().get_metadata_state(request.game_id).await;
    log_result(&result);
    result
}

#[tauri::command]
pub async fn request_game_metadata(
    request: GameMetadataRequest,
    state: tauri::State<'_, AppState>,
) -> Result<(), AppError> {
    let result = state.metadata().request_enrichment(request.game_id).await;
    log_result(&result);
    result
}

#[tauri::command]
pub async fn refresh_game_metadata(
    request: GameMetadataRequest,
    state: tauri::State<'_, AppState>,
) -> Result<(), AppError> {
    let result = state.metadata().request_refresh(request.game_id).await;
    log_result(&result);
    result
}

#[tauri::command]
pub async fn get_metadata_provider_status(
    state: tauri::State<'_, AppState>,
) -> Result<MetadataProviderStatus, AppError> {
    let result = state.metadata().provider_status().await;
    log_result(&result);
    result
}

#[tauri::command]
pub async fn select_game_metadata_candidate(
    request: SelectMetadataCandidateRequest,
    state: tauri::State<'_, AppState>,
) -> Result<(), AppError> {
    let result = state
        .metadata()
        .select_provider_candidate(request.game_id, &request.provider_game_id)
        .await;
    log_result(&result);
    result
}

#[tauri::command]
pub async fn clear_game_metadata_candidate(
    request: GameMetadataRequest,
    state: tauri::State<'_, AppState>,
) -> Result<(), AppError> {
    let result = state
        .metadata()
        .clear_provider_candidate(request.game_id)
        .await;
    log_result(&result);
    result
}

#[tauri::command]
pub async fn set_metadata_provider_credentials(
    request: SetProviderCredentialsRequest,
    state: tauri::State<'_, AppState>,
) -> Result<(), AppError> {
    let result = state
        .metadata()
        .set_user_credentials(&request.username, SecretString::new(request.password))
        .await;
    log_result(&result);
    result
}

#[tauri::command]
pub async fn clear_metadata_provider_credentials(
    state: tauri::State<'_, AppState>,
) -> Result<(), AppError> {
    let result = state.metadata().clear_user_credentials().await;
    log_result(&result);
    result
}

#[tauri::command]
pub async fn get_metadata_provider_account(
    state: tauri::State<'_, AppState>,
) -> Result<ProviderAccountStatus, AppError> {
    let result = state
        .metadata()
        .user_account_state()
        .await
        .map(|(state, username)| ProviderAccountStatus {
            configured: matches!(
                state,
                UserAccountState::Configured | UserAccountState::Invalid
            ),
            state,
            username,
        });
    log_result(&result);
    result
}

fn log_result<T>(result: &Result<T, AppError>) {
    if let Err(error) = result {
        error.log();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_input_is_write_only_and_redacted_in_debug_output() {
        let request: SetProviderCredentialsRequest =
            serde_json::from_str(r#"{"username":"fake-account","password":"fake-user-password"}"#)
                .expect("credential input should deserialize");

        assert_eq!(request.username, "fake-account");
        let rendered = format!("{request:?}");
        assert!(!rendered.contains("fake-account"));
        assert!(!rendered.contains("fake-user-password"));
    }

    #[test]
    fn the_readable_account_status_has_no_password_field() {
        let status = ProviderAccountStatus {
            configured: true,
            state: UserAccountState::Configured,
            username: Some("fake-account".to_owned()),
        };

        let serialized = serde_json::to_value(&status).expect("status should serialize");
        let keys: Vec<&String> = serialized
            .as_object()
            .expect("status is an object")
            .keys()
            .collect();

        assert_eq!(keys, vec!["configured", "state", "username"]);
        assert_eq!(serialized["state"], "configured");
    }

    #[test]
    fn an_unavailable_vault_is_reported_without_claiming_configuration() {
        let status = ProviderAccountStatus {
            configured: false,
            state: UserAccountState::VaultUnavailable,
            username: None,
        };

        let serialized = serde_json::to_value(&status).expect("status should serialize");
        assert_eq!(serialized["state"], "vaultUnavailable");
        assert_eq!(serialized["username"], serde_json::Value::Null);
    }
}
