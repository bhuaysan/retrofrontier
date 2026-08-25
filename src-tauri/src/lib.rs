// Runtime adapters and domain operations intentionally outnumber the M2 IPC surface; later
// milestones will expose more of this application-owned API without making the adapters public.
#[allow(dead_code)]
mod adapters;
#[allow(dead_code)]
mod application;
mod commands;
#[allow(dead_code)]
mod domain;
mod error;
mod logging;
mod repositories;

use adapters::database::Database;
use adapters::runtime_lock::ApplicationInstanceLock;
use adapters::runtime_paths::RuntimePaths;
use application::AppState;
use application::{RuntimeApplicationService, RuntimeManager};
use repositories::settings::SettingsRepository;
use tauri::Manager;

fn initialize_state(app: &tauri::AppHandle) -> Result<AppState, error::AppError> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|source| error::AppError::PathResolution(source.to_string()))?;
    let database_path = app_data_dir.join("database").join("retrofrontier.sqlite3");
    let database = tauri::async_runtime::block_on(Database::open(database_path))?;
    let settings = SettingsRepository::new(database.pool().clone());
    tauri::async_runtime::block_on(settings.set("foundation.ready", "true"))?;
    let runtime = RuntimeManager::for_app(RuntimePaths::new(&app_data_dir))
        .map_err(error::AppError::Runtime)?;
    let instance_lock = ApplicationInstanceLock::acquire(&runtime.paths().application_lock())
        .map_err(error::AppError::Runtime)?;
    let runtime_status = runtime
        .startup_reconcile()
        .map_err(error::AppError::Runtime)?;
    tracing::info!(state = ?runtime_status.state, "managed runtime reconciled");

    Ok(AppState::new(
        settings,
        RuntimeApplicationService::new(runtime),
        instance_lock,
    ))
}

pub fn run() {
    logging::init();
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        platform = std::env::consts::OS,
        architecture = std::env::consts::ARCH,
        "starting RetroFrontier"
    );

    tauri::Builder::default()
        .setup(|app| {
            let state = initialize_state(app.handle()).map_err(|source| {
                source.log();
                Box::new(source) as Box<dyn std::error::Error>
            })?;
            app.manage(state);
            tracing::info!("application storage initialized");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info::get_app_info,
            commands::runtime::get_runtime_status
        ])
        .run(tauri::generate_context!())
        .expect("error while running RetroFrontier");
}
