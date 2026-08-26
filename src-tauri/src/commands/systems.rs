use crate::application::{AppState, SystemsResponse};
use crate::domain::bios::BiosDiscovery;
use crate::error::AppError;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BiosStatusRequest {
    /// Development/test-only escape hatch for an explicit local BIOS root. Release builds reject
    /// this field so production always uses the OS-resolved Documents/RetroFrontier/BIOS root.
    pub root_override: Option<String>,
}

#[tauri::command]
pub async fn get_systems(state: tauri::State<'_, AppState>) -> Result<SystemsResponse, AppError> {
    let result = state.systems().get_systems();
    if let Err(error) = &result {
        error.log();
    }
    result
}

#[tauri::command]
pub async fn get_bios_status(
    request: Option<BiosStatusRequest>,
    state: tauri::State<'_, AppState>,
) -> Result<BiosDiscovery, AppError> {
    let root_override = request
        .and_then(|request| request.root_override)
        .map(PathBuf::from);
    if root_override.is_some() && !cfg!(debug_assertions) {
        let error = AppError::BiosOverrideNotAllowed;
        error.log();
        return Err(error);
    }

    let result = state.systems().get_bios_status(root_override);
    if let Err(error) = &result {
        error.log();
    }
    result
}
