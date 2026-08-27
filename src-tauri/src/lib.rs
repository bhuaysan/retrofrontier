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

use adapters::credentials::{
    developer_credentials_from_environment, CredentialVault, InMemoryCredentialVault,
    KeyringCredentialVault,
};
use adapters::database::Database;
use adapters::http::ReqwestHttpClient;
use adapters::metadata_paths::MetadataPaths;
use adapters::runtime_lock::ApplicationInstanceLock;
use adapters::runtime_paths::RuntimePaths;
use adapters::screenscraper::ScreenScraperProvider;
use application::metadata::CREDENTIAL_VAULT_SERVICE;
use application::AppState;
use application::{
    LibraryApplicationService, MetadataApplicationService, MetadataConfig, MetadataWorker,
    ProviderCredentialState, RuntimeApplicationService, RuntimeManager, SystemsApplicationService,
    TauriMetadataStateEventSink, TauriScanEventSink,
};
use domain::system::SystemCatalog;
use repositories::settings::SettingsRepository;
use services::bios::BiosService;
use services::media_delivery::{
    app_error_status, cover_response, parse_cover_route, protocol_error_response,
    CachedCoverDelivery, CACHED_COVER_PROTOCOL,
};
use services::metadata_provider::MetadataProvider;
use services::metadata_queue::{RandomJitter, SystemClock};
use std::sync::Arc;
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

    let metadata = initialize_metadata(app, &app_data_dir, database.pool().clone())?;
    let media_delivery = Arc::new(CachedCoverDelivery::new(
        database.pool().clone(),
        MetadataPaths::new(&app_data_dir),
    ));
    let metadata_worker = Arc::new(MetadataWorker::new(metadata.clone()));
    metadata_worker.start();

    Ok(AppState::new(
        settings,
        runtime_service.clone(),
        SystemsApplicationService::new(catalog, bios, runtime_service),
        instance_lock,
        library,
        metadata,
        media_delivery,
        metadata_worker,
    ))
}

/// Composes the metadata provider, credential boundary, and application service.
///
/// Development credentials come from an ignored local `.env` or the process environment; release
/// builds receive them through build-time injection. A build without credentials still starts: the
/// provider then reports that credentials are unavailable and the local library is unaffected.
fn initialize_metadata(
    app: &tauri::AppHandle,
    app_data_dir: &std::path::Path,
    pool: sqlx::SqlitePool,
) -> Result<Arc<MetadataApplicationService>, error::AppError> {
    #[cfg(debug_assertions)]
    adapters::credentials::load_development_environment_file(std::path::Path::new(".env"));

    let developer = developer_credentials_from_environment();
    match developer.as_ref() {
        // Only the origin is logged. The values themselves are never rendered anywhere.
        Some((_, origin)) => tracing::info!(?origin, "metadata provider credentials configured"),
        None => tracing::info!(
            "no metadata provider credentials are configured; metadata enrichment stays idle"
        ),
    }
    let credentials = Arc::new(ProviderCredentialState::new(
        developer.map(|(credentials, _)| credentials),
    ));

    let http = Arc::new(ReqwestHttpClient::new().map_err(|source| {
        error::AppError::Metadata(format!("provider HTTP client is unavailable: {source}"))
    })?);
    let provider: Arc<dyn MetadataProvider> = Arc::new(
        ScreenScraperProvider::new(http, credentials.clone()).map_err(|failure| {
            error::AppError::Metadata(format!("provider adapter is unavailable: {failure:?}"))
        })?,
    );

    // A host without a usable credential vault keeps working: personal credentials then live for
    // the session only, which the spike accepts as the fallback.
    let vault: Arc<dyn CredentialVault> =
        match keyring::Entry::new(CREDENTIAL_VAULT_SERVICE, "probe") {
            Ok(_) => Arc::new(KeyringCredentialVault::new(CREDENTIAL_VAULT_SERVICE)),
            Err(_) => {
                tracing::warn!(
                "no operating-system credential vault is available; optional provider credentials \
                 will not be persisted"
            );
                Arc::new(InMemoryCredentialVault::new())
            }
        };

    let service = tauri::async_runtime::block_on(MetadataApplicationService::initialize(
        pool,
        provider,
        vault,
        credentials,
        MetadataPaths::new(app_data_dir),
        Arc::new(SystemClock),
        Arc::new(RandomJitter),
        MetadataConfig::default(),
    ))?;
    Ok(Arc::new(service.with_event_sink(Arc::new(
        TauriMetadataStateEventSink::new(app.clone()),
    ))))
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
        .register_asynchronous_uri_scheme_protocol(
            CACHED_COVER_PROTOCOL,
            |_context, request, responder| {
                let app = _context.app_handle().clone();
                tauri::async_runtime::spawn(async move {
                    let response = if request.method().as_str() != "GET" {
                        protocol_error_response(405)
                    } else if let Some(game_id) = parse_cover_route(request.uri().path()) {
                        let delivery = app
                            .try_state::<AppState>()
                            .map(|state| state.media_delivery().clone());
                        match delivery {
                            Some(delivery) => match delivery.load_cover(game_id).await {
                                Ok(cover) => cover_response(cover),
                                Err(error) => protocol_error_response(app_error_status(error)),
                            },
                            None => protocol_error_response(503),
                        }
                    } else {
                        protocol_error_response(404)
                    };
                    responder.respond(response);
                });
            },
        )
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
            commands::library::get_scan_issue_page,
            commands::library::get_library_snapshot,
            commands::library::query_library,
            commands::library::get_library_summary,
            commands::library::get_library_game_detail,
            commands::library::set_game_favorite,
            commands::metadata::get_game_metadata,
            commands::metadata::request_game_metadata,
            commands::metadata::refresh_game_metadata,
            commands::metadata::get_metadata_provider_status,
            commands::metadata::select_game_metadata_candidate,
            commands::metadata::clear_game_metadata_candidate,
            commands::metadata::set_metadata_provider_credentials,
            commands::metadata::clear_metadata_provider_credentials,
            commands::metadata::get_metadata_provider_account
        ])
        .run(tauri::generate_context!())
        .expect("error while running RetroFrontier");
}
