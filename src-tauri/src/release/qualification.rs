//! The M7.5 real-runtime qualification harness.
//!
//! Every test here is `#[ignore]`d: they need a real published qualification repository, real
//! upstream RetroArch and core binaries, a graphical session, and content the operator legally
//! owns. None of that belongs in CI, and none of it is a substitute for the deterministic
//! round-trip test next door.
//!
//! What they are for is running the *real* pipeline on a real machine while producing inspectable
//! evidence. They compose the same application services the Tauri commands call — no shortcut, no
//! direct `Command::new("retroarch")`, no hand-built `runtime/versions` tree — so a pass here means
//! the architecture M2 and M7 built actually works with real software.
//!
//! Run them with the qualification environment configured, for example:
//!
//! ```text
//! RETROFRONTIER_RUNTIME_SOURCE=qualification \
//! RETROFRONTIER_RUNTIME_TUF_ROOT=<repo>/metadata/root.json \
//! RETROFRONTIER_RUNTIME_METADATA_URL=file://<repo>/metadata/ \
//! RETROFRONTIER_RUNTIME_TARGETS_URL=file://<repo>/repository-targets/ \
//! RETROFRONTIER_RUNTIME_MANIFEST_TARGET=rf-runtime-linux-x86_64-002.manifest.json \
//! RETROFRONTIER_QUALIFICATION_APP_DATA=$HOME/.local/share/com.retrofrontier.desktop \
//!   cargo test --features release-tools --lib qualification -- --ignored --nocapture --test-threads=1
//! ```

use crate::adapters::runtime_archive::LinuxRuntimeArchiveExtractor;
use crate::adapters::runtime_paths::RuntimePaths;
use crate::adapters::runtime_process::LinuxManagedProcessInspector;
use crate::adapters::runtime_release_source::configure_release_source;
use crate::application::runtime_manager::{RetentionPolicy, StructuralSmokeValidator};
use crate::application::{RuntimeApplicationService, RuntimeManager};
use crate::domain::runtime::RuntimeState;
use std::path::PathBuf;
use std::sync::Arc;

/// The application data directory the harness operates on.
///
/// Deliberately explicit: a qualification run must never guess a path and then mutate whatever it
/// happens to find there.
const APP_DATA_VARIABLE: &str = "RETROFRONTIER_QUALIFICATION_APP_DATA";

fn app_data_directory() -> PathBuf {
    PathBuf::from(
        std::env::var(APP_DATA_VARIABLE)
            .unwrap_or_else(|_| panic!("{APP_DATA_VARIABLE} must name the application data root")),
    )
}

/// Compose the runtime service exactly as `initialize_state` does, including the real process
/// inspector, so a live managed RetroArch still blocks mutation during qualification.
fn runtime_service() -> RuntimeApplicationService {
    let paths = RuntimePaths::new(app_data_directory());
    paths.prepare().expect("managed runtime roots prepare");
    let configured = configure_release_source(paths.trust_datastore())
        .expect("the qualification release source configuration is valid");
    let manager = RuntimeManager::new(
        paths,
        configured
            .as_ref()
            .map(|source| source.source.clone())
            .expect("a qualification release source must be configured"),
        Arc::new(LinuxRuntimeArchiveExtractor),
        Arc::new(LinuxManagedProcessInspector),
        Arc::new(StructuralSmokeValidator),
        RetentionPolicy::default(),
    )
    .expect("runtime manager composes");
    RuntimeApplicationService::new(manager).with_release_source(configured)
}

/// Install the approved real managed release through the same service the IPC command calls.
#[tokio::test]
#[ignore = "needs a published qualification repository and real upstream artefacts"]
async fn install_the_real_managed_runtime() {
    let service = runtime_service();

    let before = service
        .get_install_state()
        .expect("install state is readable");
    println!(
        "before: state={:?} source={:?} release_target={:?}",
        before.status.state, before.source_origin, before.release_target
    );

    let response = service.install_runtime().await;
    println!(
        "install: installed={} error={:?}",
        response.installed, response.error
    );
    assert!(
        response.installed,
        "installation failed: {:?}",
        response.error
    );
    assert_eq!(response.status.state, RuntimeState::Ready);

    let after = service
        .get_install_state()
        .expect("install state is readable");
    println!(
        "after: state={:?} release={:?} installation={:?}",
        after.status.state, after.status.release_id, after.status.installation_id
    );

    let launch = service
        .manager()
        .verified_launch_runtime()
        .expect("the installed runtime is launchable");
    println!("app_run: {}", launch.app_run_path.display());
    for (id, core) in &launch.cores {
        println!(
            "core: {id} -> {} systems={:?} present={}",
            core.core_path.display(),
            core.systems
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>(),
            core.core_path.is_file()
        );
    }
    for (id, path) in &launch.support_assets {
        println!(
            "support: {id} -> {} present={}",
            path.display(),
            path.is_dir()
        );
    }
}

/// Report the verified runtime without changing anything, for evidence collection between steps.
#[tokio::test]
#[ignore = "needs an installed real managed runtime"]
async fn report_the_verified_managed_runtime() {
    let service = runtime_service();
    let state = service
        .get_install_state()
        .expect("install state is readable");
    println!(
        "state={:?} source={:?} release={:?} installation={:?}",
        state.status.state,
        state.source_origin,
        state.status.release_id,
        state.status.installation_id
    );
    let snapshot = service
        .verified_runtime_snapshot()
        .expect("the verified snapshot is readable");
    println!(
        "verified cores: {:?}",
        snapshot
            .verified_core_ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------------------------------------
// Real launch qualification
// ---------------------------------------------------------------------------------------------

/// The managed library root the running application uses, so the harness sees the same games.
const LIBRARY_VARIABLE: &str = "RETROFRONTIER_QUALIFICATION_LIBRARY";
/// The game the launch qualification should start, by its library id.
const GAME_VARIABLE: &str = "RETROFRONTIER_QUALIFICATION_GAME_ID";
/// How long to leave the real emulator running before terminating it.
const HOLD_VARIABLE: &str = "RETROFRONTIER_QUALIFICATION_HOLD_SECONDS";

#[derive(Debug)]
struct PrintingLaunchEvents;

impl crate::application::launch::LaunchEventSink for PrintingLaunchEvents {
    fn publish(&self, event: crate::domain::launch::GameLaunchStateChanged) {
        println!("launch event: {event:?}");
    }
}

/// Rescan the managed library so qualification content the operator staged becomes launchable.
///
/// This runs the real scanner through the real application service; it never inserts a row by
/// hand, because a hand-written library entry would not prove that launch resolves real content.
#[tokio::test]
#[ignore = "mutates the operator's real library index"]
async fn rescan_the_managed_library() {
    use crate::adapters::database::Database;
    use crate::application::LibraryApplicationService;
    use crate::domain::system::SystemCatalog;
    use crate::services::library_scanner::ScanEventSink;

    #[derive(Debug)]
    struct SilentScanEvents;
    impl ScanEventSink for SilentScanEvents {
        fn progress(&self, _progress: crate::domain::library::ScanProgress) {}
        fn completed(&self, _summary: crate::domain::library::ScanSummary) {}
    }

    let app_data = app_data_directory();
    let library_root = PathBuf::from(
        std::env::var(LIBRARY_VARIABLE)
            .unwrap_or_else(|_| panic!("{LIBRARY_VARIABLE} must name the managed library root")),
    );
    let database = Database::open(app_data.join("database").join("retrofrontier.sqlite3"))
        .await
        .expect("the library database opens");
    let library = LibraryApplicationService::initialize(
        database.pool().clone(),
        SystemCatalog::v1(),
        library_root.join("ROMs"),
        Arc::new(SilentScanEvents),
    )
    .await
    .expect("the library service composes");
    let summary = library.rescan_library().await.expect("the scan completes");
    println!("scan summary: {summary:?}");
    for game in library
        .get_library_snapshot()
        .await
        .expect("the snapshot is readable")
        .games
    {
        println!("game: {:?}", game.game);
    }
}

/// Launch one real game through the whole M7 path and report what actually happened.
///
/// This composes `LaunchApplicationService` exactly as the composition root does, so the run goes
/// `launch_game -> LaunchApplicationService -> RetroArchService -> verified RuntimeManager runtime
/// -> LinuxGameProcessLauncher -> the authenticated managed AppRun`. RetroArch is never invoked
/// from a shell, and no path is supplied from outside the verified runtime boundary.
#[tokio::test]
#[ignore = "needs an installed real managed runtime, real content, and a graphical session"]
async fn launch_a_real_game_through_the_m7_path() {
    use crate::adapters::database::Database;
    use crate::adapters::game_process::LinuxGameProcessLauncher;
    use crate::application::{LaunchApplicationService, LaunchConfig};
    use crate::domain::library::GameId;
    use crate::domain::system::SystemCatalog;
    use crate::repositories::launch::LaunchRepository;
    use crate::repositories::library::LibraryRepository;
    use crate::services::bios::BiosService;
    use crate::services::retroarch::RetroArchService;
    use crate::services::retroarch_host::LinuxHostPrerequisiteInspector;
    use crate::services::retroarch_paths::LaunchPaths;

    let app_data = app_data_directory();
    let library_root = PathBuf::from(
        std::env::var(LIBRARY_VARIABLE)
            .unwrap_or_else(|_| panic!("{LIBRARY_VARIABLE} must name the managed library root")),
    );
    let game_id: i64 = std::env::var(GAME_VARIABLE)
        .unwrap_or_else(|_| panic!("{GAME_VARIABLE} must name the game to launch"))
        .parse()
        .expect("the game id is numeric");
    let hold_seconds: u64 = std::env::var(HOLD_VARIABLE)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(12);

    let service = runtime_service();
    let manager = service.manager().clone();
    assert_eq!(
        manager.status().expect("status").state,
        RuntimeState::Ready,
        "the managed runtime must be installed before launch qualification"
    );

    let database = Database::open(app_data.join("database").join("retrofrontier.sqlite3"))
        .await
        .expect("the library database opens");
    let catalog = SystemCatalog::v1();
    let bios = BiosService::from_catalog(library_root.join("BIOS"), &catalog)
        .expect("BIOS discovery composes");
    let launch_paths = LaunchPaths::new(&app_data);
    launch_paths.prepare().expect("launch paths prepare");

    let launch = LaunchApplicationService::new(
        LibraryRepository::new(database.pool().clone()),
        LaunchRepository::new(database.pool().clone()),
        catalog,
        bios,
        Arc::new(manager.clone()),
        Arc::new(RetroArchService::new(
            launch_paths,
            Arc::new(LinuxGameProcessLauncher),
            Arc::new(LinuxHostPrerequisiteInspector::default()),
        )),
        Arc::new(PrintingLaunchEvents),
        LaunchConfig::default(),
    );
    let reconciled = launch
        .reconcile_on_startup()
        .await
        .expect("restart reconciliation succeeds");
    println!(
        "reconciled: running={:?} blocked={}",
        reconciled.running, reconciled.blocked
    );

    let response = launch.launch_game(GameId(game_id), None).await;
    println!("launch response: {response:?}");

    let record_path = manager.paths().game_process_record();
    println!(
        "game-process.json: {}",
        std::fs::read_to_string(record_path).unwrap_or_else(|_| "<absent>".to_owned())
    );

    // A second launch must be refused while the first is alive.
    let second = launch.launch_game(GameId(game_id), None).await;
    println!("second launch response: {second:?}");

    // Runtime mutation must be refused while a managed process is alive.
    let blocked = manager.cleanup();
    println!("runtime mutation while running: {blocked:?}");

    tokio::time::sleep(std::time::Duration::from_secs(hold_seconds)).await;
    println!("state while running: {:?}", launch.get_launch_state());

    // Terminate the real emulator the way closing the window would, then let the monitor reap it.
    let record = std::fs::read_to_string(record_path).unwrap_or_default();
    if let Some(pid) = record
        .split('"')
        .zip(record.split('"').skip(1))
        .find(|(key, _)| *key == "pid")
        .and_then(|_| {
            record
                .split("\"pid\":")
                .nth(1)
                .and_then(|rest| {
                    rest.split(|c: char| !c.is_ascii_digit())
                        .find(|s| !s.is_empty())
                })
                .and_then(|value| value.parse::<i32>().ok())
        })
    {
        println!("terminating managed process {pid}");
        unsafe {
            libc_kill(pid, 15);
        }
    }

    for _ in 0..40 {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let state = launch.get_launch_state();
        if state.running.is_none() && !state.blocked {
            break;
        }
    }
    println!("state after exit: {:?}", launch.get_launch_state());
    println!(
        "game-process.json after exit: {}",
        std::fs::read_to_string(record_path).unwrap_or_else(|_| "<absent>".to_owned())
    );
    println!("runtime mutation after exit: {:?}", manager.cleanup());
}

extern "C" {
    #[link_name = "kill"]
    fn libc_kill(pid: i32, signal: i32) -> i32;
}

/// Report BIOS discovery and per-system readiness against the installed real runtime.
///
/// This is the PlayStation evidence: RetroFrontier must report a dump the approved core does not
/// look up as *not covered by the catalog*, not as a usable BIOS.
#[tokio::test]
#[ignore = "needs an installed real managed runtime and the operator's own BIOS directory"]
async fn report_bios_and_system_readiness() {
    use crate::application::SystemsApplicationService;
    use crate::domain::system::SystemCatalog;
    use crate::services::bios::BiosService;

    let library_root = PathBuf::from(
        std::env::var(LIBRARY_VARIABLE)
            .unwrap_or_else(|_| panic!("{LIBRARY_VARIABLE} must name the managed library root")),
    );
    let catalog = SystemCatalog::v1();
    let bios = BiosService::from_catalog(library_root.join("BIOS"), &catalog)
        .expect("BIOS discovery composes");
    let systems = SystemsApplicationService::new(catalog, bios.clone(), runtime_service());

    let discovery = systems
        .get_bios_status(None)
        .expect("BIOS status is readable");
    println!("bios discovery: {discovery:#?}");

    let response = systems.get_systems().expect("systems are readable");
    for system in &response.systems {
        println!("system: {system:?}");
    }
}

/// Report what a restarted RetroFrontier concludes about a managed process it did not fork.
///
/// Run this after killing the launch harness while the emulator is still alive: it is the real
/// crash-recovery case ADR-011 describes, and the honest answer must be "still busy".
#[tokio::test]
#[ignore = "needs a managed RetroArch process that survived a previous harness run"]
async fn reconcile_after_a_crash() {
    use crate::adapters::database::Database;
    use crate::adapters::game_process::LinuxGameProcessLauncher;
    use crate::application::{LaunchApplicationService, LaunchConfig};
    use crate::domain::system::SystemCatalog;
    use crate::repositories::launch::LaunchRepository;
    use crate::repositories::library::LibraryRepository;
    use crate::services::bios::BiosService;
    use crate::services::retroarch::RetroArchService;
    use crate::services::retroarch_host::LinuxHostPrerequisiteInspector;
    use crate::services::retroarch_paths::LaunchPaths;

    let app_data = app_data_directory();
    let library_root = PathBuf::from(
        std::env::var(LIBRARY_VARIABLE)
            .unwrap_or_else(|_| panic!("{LIBRARY_VARIABLE} must name the managed library root")),
    );
    let service = runtime_service();
    let manager = service.manager().clone();

    println!(
        "game-process.json at startup: {}",
        std::fs::read_to_string(manager.paths().game_process_record())
            .unwrap_or_else(|_| "<absent>".to_owned())
    );
    println!(
        "runtime startup reconcile: {:?}",
        manager.startup_reconcile()
    );
    println!("runtime mutation attempt: {:?}", manager.cleanup());

    let database = Database::open(app_data.join("database").join("retrofrontier.sqlite3"))
        .await
        .expect("the library database opens");
    let catalog = SystemCatalog::v1();
    let bios = BiosService::from_catalog(library_root.join("BIOS"), &catalog)
        .expect("BIOS discovery composes");
    let launch_paths = LaunchPaths::new(&app_data);
    launch_paths.prepare().expect("launch paths prepare");
    let launch = LaunchApplicationService::new(
        LibraryRepository::new(database.pool().clone()),
        LaunchRepository::new(database.pool().clone()),
        catalog,
        bios,
        Arc::new(manager.clone()),
        Arc::new(RetroArchService::new(
            launch_paths,
            Arc::new(LinuxGameProcessLauncher),
            Arc::new(LinuxHostPrerequisiteInspector::default()),
        )),
        Arc::new(PrintingLaunchEvents),
        LaunchConfig::default(),
    );
    let state = launch
        .reconcile_on_startup()
        .await
        .expect("restart reconciliation succeeds");
    println!(
        "restart state: running={:?} blocked={}",
        state.running, state.blocked
    );

    let refused = launch
        .launch_game(crate::domain::library::GameId(1), None)
        .await;
    println!("launch attempt after restart: {refused:?}");
}
