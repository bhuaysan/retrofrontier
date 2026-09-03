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
#[cfg(feature = "release-tools")]
pub mod release;
mod repositories;
mod services;

use adapters::credentials::{
    developer_credentials_from_environment, CredentialVault, InMemoryCredentialVault,
    KeyringCredentialVault,
};
use adapters::database::Database;
use adapters::game_process::LinuxGameProcessLauncher;
use adapters::http::ReqwestHttpClient;
use adapters::metadata_paths::MetadataPaths;
use adapters::runtime_lock::ApplicationInstanceLock;
use adapters::runtime_paths::RuntimePaths;
use adapters::runtime_release_source::configure_release_source;
use adapters::screenscraper::ScreenScraperProvider;
use application::metadata::CREDENTIAL_VAULT_SERVICE;
use application::AppState;
use application::{
    LaunchApplicationService, LaunchConfig, LibraryApplicationService, MetadataApplicationService,
    MetadataConfig, MetadataScrapeApplicationService, MetadataScrapeConfig, MetadataWorkSignal,
    MetadataWorker, ProviderCredentialState, RuntimeApplicationService, RuntimeManager,
    SaveStateApplicationService, SaveStateConfig, SystemsApplicationService, TauriLaunchEventSink,
    TauriMetadataStateEventSink, TauriScanEventSink,
};
use domain::system::SystemCatalog;
use repositories::settings::SettingsRepository;
use services::bios::BiosService;
use services::media_delivery::{
    app_error_status, cover_response, parse_cover_route, parse_save_state_thumbnail_route,
    protocol_error_response, CachedCoverDelivery, SaveStateThumbnailDelivery,
    CACHED_COVER_PROTOCOL,
};
use services::metadata_provider::MetadataProvider;
use services::metadata_queue::{RandomJitter, SystemClock};
use services::retroarch::RetroArchService;
use services::retroarch_host::LinuxHostPrerequisiteInspector;
use services::retroarch_paths::LaunchPaths;
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
    let runtime_paths = RuntimePaths::new(&app_data_dir);
    // Without a configured trusted release source M2's installer can never resolve a release, so
    // the runtime stays `NotInstalled` forever. Configuration failure is fatal on purpose: a
    // maintainer who mis-set the qualification environment must be told, not silently downgraded
    // to a build that reports the runtime as uninstallable.
    let release_source = configure_release_source(runtime_paths.trust_datastore())
        .map_err(error::AppError::Runtime)?;
    match release_source.as_ref() {
        Some(source) => tracing::info!(
            origin = ?source.origin,
            release_target = source.manifest_target_name.as_str(),
            "approved managed runtime release source configured"
        ),
        None => tracing::info!(
            "no approved managed runtime release source is configured; the managed runtime \
             cannot be installed by this build"
        ),
    }
    let runtime = RuntimeManager::for_app(
        runtime_paths,
        release_source.as_ref().map(|source| source.source.clone()),
    )
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
    let catalog_for_launch = catalog.clone();
    let bios_for_launch = bios.clone();
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

    let runtime_service =
        RuntimeApplicationService::new(runtime.clone()).with_release_source(release_source);

    // The launch subsystem is composed after the runtime has been reconciled, so restart
    // reconciliation sees the durable process record RuntimeManager has already judged.
    let launch_paths = LaunchPaths::new(&app_data_dir);
    launch_paths.prepare()?;
    let states_root = launch_paths.states_root().to_path_buf();

    // The two services are mutually dependent, so the cycle is broken by construction order:
    // save-states first (it needs no launch), then the launch service (which *requires* a
    // save-state lifecycle, because a launch with no durable baseline is a launch whose save
    // states could never be attributed), then the port is attached. Until it is, the save-state
    // service reports every managed session as active and refuses every mutation.
    let save_states = Arc::new(SaveStateApplicationService::new(
        repositories::save_state::SaveStateRepository::new(database.pool().clone()),
        repositories::library::LibraryRepository::new(database.pool().clone()),
        repositories::launch::LaunchRepository::new(database.pool().clone()),
        Arc::new(runtime.clone()),
        &states_root,
        SaveStateConfig::default(),
    ));
    let launch = LaunchApplicationService::new(
        repositories::library::LibraryRepository::new(database.pool().clone()),
        repositories::launch::LaunchRepository::new(database.pool().clone()),
        catalog_for_launch,
        bios_for_launch,
        Arc::new(runtime.clone()),
        Arc::new(RetroArchService::new(
            launch_paths,
            Arc::new(LinuxGameProcessLauncher),
            Arc::new(LinuxHostPrerequisiteInspector::default()),
        )),
        Arc::new(TauriLaunchEventSink::new(app.clone())),
        save_states.clone(),
        LaunchConfig::default(),
    );
    save_states.attach_launch(launch.clone_as_port());
    let launch_state = tauri::async_runtime::block_on(launch.reconcile_on_startup())?;
    if launch_state.running.is_some() || launch_state.blocked {
        tracing::info!(
            blocked = launch_state.blocked,
            "a managed game process survived the previous RetroFrontier run"
        );
    }
    // Save-state reconciliation runs *after* launch reconciliation, so a session adopted from a
    // previous run is still open here and is deliberately not attributed; it reconciles once its
    // process is proven gone. A quarantine file left by a crash mid-delete is inert either way.
    let swept = crate::services::save_state_fs::sweep_delete_quarantine(&states_root);
    if swept > 0 {
        tracing::info!(swept, "save-state delete quarantine files were removed");
    }
    match tauri::async_runtime::block_on(save_states.reconcile_on_startup()) {
        Ok(0) => {}
        Ok(sessions) => tracing::info!(sessions, "save-state baselines were reconciled"),
        // Reconciliation is retryable, so a failure here must not stop the application from
        // starting: nothing was attributed and nothing was destroyed.
        Err(error) => error.log(),
    }

    // One work signal is shared by the metadata service, the scrape orchestrator and the worker,
    // so explicitly requested work never waits out the worker's idle sleep.
    let work_signal = MetadataWorkSignal::new();
    let metadata = initialize_metadata(app, &app_data_dir, database.pool().clone(), work_signal)?;
    let media_delivery = Arc::new(CachedCoverDelivery::new(
        database.pool().clone(),
        MetadataPaths::new(&app_data_dir),
    ));
    let metadata_scrape = Arc::new(tauri::async_runtime::block_on(
        MetadataScrapeApplicationService::initialize(
            database.pool().clone(),
            Arc::new(SystemClock),
            metadata.work_signal(),
            metadata.provider_id(),
            MetadataScrapeConfig::default(),
        ),
    )?);
    let metadata_worker = Arc::new(MetadataWorker::new(
        metadata.clone(),
        metadata_scrape.clone(),
    ));
    metadata_worker.start();

    Ok(AppState::new(
        settings,
        runtime_service.clone(),
        SystemsApplicationService::new(catalog, bios, runtime_service),
        instance_lock,
        library,
        launch,
        metadata,
        metadata_scrape,
        media_delivery,
        save_states,
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
    signal: MetadataWorkSignal,
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
    Ok(Arc::new(
        service
            .with_event_sink(Arc::new(TauriMetadataStateEventSink::new(app.clone())))
            .with_work_signal(signal),
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
        .plugin(tauri_plugin_dialog::init())
        .register_asynchronous_uri_scheme_protocol(
            CACHED_COVER_PROTOCOL,
            |_context, request, responder| {
                let app = _context.app_handle().clone();
                tauri::async_runtime::spawn(async move {
                    let path = request.uri().path().to_owned();
                    let response = if request.method().as_str() != "GET" {
                        protocol_error_response(405)
                    } else if let Some(game_id) = parse_cover_route(&path) {
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
                    } else if let Some(save_state_id) = parse_save_state_thumbnail_route(&path) {
                        // Resolved from durable provenance by identity, then re-verified in full.
                        // The WebView never sends, and never receives, a filesystem path.
                        let delivery = app.try_state::<AppState>().map(|state| {
                            SaveStateThumbnailDelivery::new(state.save_states().clone())
                        });
                        match delivery {
                            Some(delivery) => match delivery.load_thumbnail(save_state_id).await {
                                Ok(thumbnail) => cover_response(thumbnail),
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
            commands::runtime::get_runtime_install_state,
            commands::runtime::install_runtime,
            commands::runtime::repair_runtime,
            commands::systems::get_systems,
            commands::systems::get_bios_status,
            commands::library::get_content_roots,
            commands::library::add_external_content_root,
            commands::library::open_managed_rom_folder,
            commands::library::remove_external_content_root,
            commands::library::set_content_root_enabled,
            commands::library::rescan_library,
            commands::library::get_scan_status,
            commands::library::get_scan_issues,
            commands::library::get_scan_issue_page,
            commands::library::get_library_snapshot,
            commands::library::query_library,
            commands::library::query_library_shelves,
            commands::library::get_library_summary,
            commands::library::get_library_game_detail,
            commands::library::set_game_favorite,
            commands::launch::launch_game,
            commands::launch::get_launch_state,
            commands::metadata::get_game_metadata,
            commands::metadata::request_game_metadata,
            commands::metadata::refresh_game_metadata,
            commands::metadata::get_metadata_provider_status,
            commands::metadata::select_game_metadata_candidate,
            commands::metadata::clear_game_metadata_candidate,
            commands::metadata::set_metadata_provider_credentials,
            commands::metadata::clear_metadata_provider_credentials,
            commands::metadata::get_metadata_provider_account,
            commands::metadata::preview_metadata_scrape,
            commands::metadata::get_metadata_scrape_status,
            commands::metadata::start_metadata_scrape,
            commands::metadata::stop_metadata_scrape,
            commands::save_state::list_save_states,
            commands::save_state::load_save_state,
            commands::save_state::delete_save_state
        ])
        .run(tauri::generate_context!())
        .expect("error while running RetroFrontier");
}
