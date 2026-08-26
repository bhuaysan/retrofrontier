// Runtime adapters and domain operations intentionally outnumber the M2/M3 IPC surface; later
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
mod services;

use adapters::database::Database;
use adapters::runtime_lock::ApplicationInstanceLock;
use adapters::runtime_paths::RuntimePaths;
use application::AppState;
use application::{
    LibraryApplicationService, RuntimeApplicationService, RuntimeManager,
    SystemsApplicationService, TauriScanEventSink,
};
use domain::system::SystemCatalog;
use repositories::settings::SettingsRepository;
use services::bios::BiosService;
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
    let catalog = SystemCatalog::v1();
    catalog
        .validate()
        .map_err(|source| error::AppError::Catalog(source.to_string()))?;
    let documents_dir = app
        .path()
        .document_dir()
        .map_err(|source| error::AppError::PathResolution(source.to_string()))?;
    let bios =
        BiosService::from_catalog(documents_dir.join("RetroFrontier").join("BIOS"), &catalog)
            .map_err(error::AppError::Bios)?;
    let library = tauri::async_runtime::block_on(LibraryApplicationService::initialize(
        database.pool().clone(),
        catalog.clone(),
        documents_dir.join("RetroFrontier").join("ROMs"),
        std::sync::Arc::new(TauriScanEventSink::new(app.clone())),
    ))?;
    let instance_lock = ApplicationInstanceLock::acquire(&runtime.paths().application_lock())
        .map_err(error::AppError::Runtime)?;
    let runtime_status = runtime
        .startup_reconcile()
        .map_err(error::AppError::Runtime)?;
    tracing::info!(state = ?runtime_status.state, "managed runtime reconciled");

    let runtime_service = RuntimeApplicationService::new(runtime.clone());

    Ok(AppState::new(
        settings,
        runtime_service.clone(),
        SystemsApplicationService::new(catalog, bios, runtime_service),
        instance_lock,
        library,
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
            commands::runtime::get_runtime_status,
            commands::systems::get_systems,
            commands::systems::get_bios_status,
            commands::library::get_content_roots,
            commands::library::add_external_content_root,
            commands::library::remove_external_content_root,
            commands::library::set_content_root_enabled,
            commands::library::rescan_library,
            commands::library::get_scan_status,
            commands::library::get_scan_issues,
            commands::library::get_library_snapshot
        ])
        .run(tauri::generate_context!())
        .expect("error while running RetroFrontier");
}
