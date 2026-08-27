use crate::domain::library::{
    roots_overlap, ContentRoot, GameFavorite, GameId, LibraryGameDetail, LibraryMetadataMatchState,
    LibraryPage, LibraryQuery, LibrarySnapshot, LibrarySummary, ScanIssue, ScanIssueKind,
    ScanIssuePage, ScanProgress, ScanStatus, ScanSummary,
};
use crate::domain::metadata::MetadataProviderId;
use crate::domain::system::{SystemCatalog, SystemId};
use crate::error::AppError;
use crate::repositories::library::LibraryRepository;
use crate::repositories::metadata::MetadataRepository;
use crate::services::library_scanner::{
    ScanEventSink, ScanService, LIBRARY_SCAN_COMPLETED_EVENT, LIBRARY_SCAN_PROGRESS_EVENT,
};
use crate::services::metadata_evidence::{evidence_is_current, MetadataEvidenceService};
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::Emitter;

const WATCHER_DEBOUNCE: Duration = Duration::from_millis(250);

#[derive(Clone)]
pub struct LibraryApplicationService {
    repository: LibraryRepository,
    metadata_repository: MetadataRepository,
    evidence: MetadataEvidenceService,
    coordinator: Arc<ScanCoordinator>,
    status: Arc<Mutex<ScanStatus>>,
    watcher: Option<Arc<LibraryWatcher>>,
    watcher_issues: Arc<Mutex<Vec<ScanIssue>>>,
}

impl LibraryApplicationService {
    pub async fn initialize(
        pool: sqlx::SqlitePool,
        catalog: SystemCatalog,
        managed_root: PathBuf,
        frontend_sink: Arc<dyn ScanEventSink>,
    ) -> Result<Self, AppError> {
        let managed_root = ensure_managed_root(&managed_root, &catalog)?;
        let repository = LibraryRepository::new(pool.clone());
        let metadata_repository = MetadataRepository::new(pool);
        let evidence = MetadataEvidenceService::new(repository.clone());
        repository.upsert_managed_root(&managed_root).await?;
        repository.recover_interrupted_scan_runs().await?;

        let status = Arc::new(Mutex::new(
            repository.latest_scan_status().await?.unwrap_or_default(),
        ));
        let watcher_issues = Arc::new(Mutex::new(Vec::new()));
        let sink: Arc<dyn ScanEventSink> = Arc::new(StatusAndFrontendSink {
            status: status.clone(),
            frontend: frontend_sink,
        });
        let scanner = ScanService::new(repository.clone(), catalog, sink);
        let coordinator = Arc::new(ScanCoordinator::new(scanner));

        let watcher = match LibraryWatcher::new(coordinator.clone(), watcher_issues.clone()) {
            Ok(watcher) => {
                let watcher = Arc::new(watcher);
                watcher.refresh_roots(&repository.get_content_roots().await?);
                Some(watcher)
            }
            Err(error) => {
                add_watcher_issue(&watcher_issues, error.to_string());
                tracing::warn!(error = %error, "library filesystem watcher could not start");
                None
            }
        };

        Ok(Self {
            repository,
            metadata_repository,
            evidence,
            coordinator,
            status,
            watcher,
            watcher_issues,
        })
    }

    pub async fn get_content_roots(&self) -> Result<Vec<ContentRoot>, AppError> {
        self.repository.get_content_roots().await
    }

    pub async fn add_external_content_root(
        &self,
        path: &str,
        system_hint: Option<SystemId>,
    ) -> Result<ContentRoot, AppError> {
        let path = normalize_configured_path(path, "external content root")?;
        let existing = self.repository.find_content_root_by_path(&path).await?;
        self.ensure_no_enabled_overlap(&path, existing.as_ref().map(|root| root.id))
            .await?;
        let root = self
            .repository
            .upsert_external_root(&path, system_hint)
            .await?;
        self.refresh_watcher().await;
        Ok(root)
    }

    pub async fn remove_external_content_root(
        &self,
        root_id: crate::domain::library::ContentRootId,
    ) -> Result<(), AppError> {
        self.repository.remove_external_root(root_id).await?;
        self.refresh_watcher().await;
        Ok(())
    }

    pub async fn set_content_root_enabled(
        &self,
        root_id: crate::domain::library::ContentRootId,
        enabled: bool,
    ) -> Result<ContentRoot, AppError> {
        if enabled {
            let root = self
                .repository
                .content_root(root_id)
                .await?
                .ok_or(AppError::ContentRootInvalidOperation)?;
            self.ensure_no_enabled_overlap(&root.path, Some(root_id))
                .await?;
        }
        let root = self
            .repository
            .set_content_root_enabled(root_id, enabled)
            .await?;
        self.refresh_watcher().await;
        Ok(root)
    }

    pub async fn rescan_library(&self) -> Result<ScanSummary, AppError> {
        self.coordinator.request_scan().await
    }

    pub fn get_scan_status(&self) -> ScanStatus {
        self.status
            .lock()
            .expect("library scan status mutex is not poisoned")
            .clone()
    }

    pub async fn get_scan_issues(&self) -> Result<Vec<ScanIssue>, AppError> {
        let mut issues = self.repository.list_latest_scan_issues().await?;
        issues.extend(
            self.watcher_issues
                .lock()
                .expect("library watcher issue mutex is not poisoned")
                .iter()
                .cloned(),
        );
        issues.sort_by_key(|issue| issue.created_at);
        Ok(issues)
    }

    /// Bounded latest-run issue query for future M6 rendering. Transient watcher diagnostics stay
    /// on the legacy aggregate command above; persisted scan issues have stable database ordering.
    pub async fn get_scan_issue_page(
        &self,
        offset: u64,
        limit: u32,
    ) -> Result<ScanIssuePage, AppError> {
        self.repository
            .list_latest_scan_issues_page(offset, limit)
            .await
    }

    pub async fn query_library(&self, request: &LibraryQuery) -> Result<LibraryPage, AppError> {
        let mut page = self
            .repository
            .query_library(request, MetadataProviderId::ScreenScraper)
            .await?;
        self.validate_live_metadata_state(&mut page).await?;
        Ok(page)
    }

    pub async fn get_library_summary(&self) -> Result<LibrarySummary, AppError> {
        self.repository.get_library_summary().await
    }

    pub async fn get_library_game_detail(
        &self,
        game_id: GameId,
    ) -> Result<Option<LibraryGameDetail>, AppError> {
        self.repository.get_library_game_detail(game_id).await
    }

    pub async fn set_game_favorite(
        &self,
        game_id: GameId,
        favorite: bool,
    ) -> Result<GameFavorite, AppError> {
        self.repository.set_game_favorite(game_id, favorite).await?;
        Ok(GameFavorite { game_id, favorite })
    }

    pub async fn get_library_snapshot(&self) -> Result<LibrarySnapshot, AppError> {
        self.repository.get_library_snapshot().await
    }

    async fn ensure_no_enabled_overlap(
        &self,
        path: &str,
        ignored_root: Option<crate::domain::library::ContentRootId>,
    ) -> Result<(), AppError> {
        for root in self.repository.get_content_roots().await? {
            if !root.enabled || ignored_root == Some(root.id) {
                continue;
            }
            if roots_overlap(&root.path, path) {
                return Err(AppError::ContentRootOverlap);
            }
        }
        Ok(())
    }

    /// Applies M5's live-evidence read invariant to the bounded UI page.
    ///
    /// Match snapshots and current M4 evidence are each loaded in bulk. Stale items retain the
    /// list query's cached metadata and cover because staleness changes trust, not last-known-good
    /// data availability.
    async fn validate_live_metadata_state(&self, page: &mut LibraryPage) -> Result<(), AppError> {
        let game_ids: Vec<GameId> = page
            .items
            .iter()
            .filter(|item| item.metadata_match_state == LibraryMetadataMatchState::Matched)
            .map(|item| item.game_id)
            .collect();
        if game_ids.is_empty() {
            return Ok(());
        }

        let stored = self
            .metadata_repository
            .load_match_evidence_for_games(&game_ids, MetadataProviderId::ScreenScraper)
            .await?;
        let current = self.evidence.current_evidence_for_games(&game_ids).await?;

        for item in &mut page.items {
            if item.metadata_match_state != LibraryMetadataMatchState::Matched {
                continue;
            }
            let snapshot = stored.get(&item.game_id);
            let current_evidence = current.get(&item.game_id).and_then(Option::as_ref);
            if !evidence_is_current(
                snapshot.and_then(|match_evidence| match_evidence.evidence.as_ref()),
                snapshot.and_then(|match_evidence| match_evidence.match_type),
                current_evidence,
            ) {
                item.metadata_match_state = LibraryMetadataMatchState::Stale;
            }
        }
        Ok(())
    }

    async fn refresh_watcher(&self) {
        if let Some(watcher) = &self.watcher {
            if let Ok(roots) = self.repository.get_content_roots().await {
                watcher.refresh_roots(&roots);
            }
        }
    }
}

struct StatusAndFrontendSink {
    status: Arc<Mutex<ScanStatus>>,
    frontend: Arc<dyn ScanEventSink>,
}

impl ScanEventSink for StatusAndFrontendSink {
    fn progress(&self, progress: ScanProgress) {
        let mut status = self
            .status
            .lock()
            .expect("library scan status mutex is not poisoned");
        status.running = true;
        status.progress = Some(progress.clone());
        drop(status);
        self.frontend.progress(progress);
    }

    fn completed(&self, summary: ScanSummary) {
        let mut status = self
            .status
            .lock()
            .expect("library scan status mutex is not poisoned");
        status.running = false;
        status.progress = None;
        status.last_result = Some(summary.clone());
        drop(status);
        self.frontend.completed(summary);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScanRequest {
    Started,
    Queued,
}

#[derive(Debug, Default)]
struct ScanSchedule {
    running: bool,
    follow_up: bool,
}

impl ScanSchedule {
    fn request(&mut self) -> ScanRequest {
        if self.running {
            self.follow_up = true;
            ScanRequest::Queued
        } else {
            self.running = true;
            ScanRequest::Started
        }
    }

    fn finished(&mut self) -> bool {
        if self.follow_up {
            self.follow_up = false;
            true
        } else {
            self.running = false;
            false
        }
    }
}

#[derive(Clone)]
struct ScanCoordinator {
    scanner: ScanService,
    schedule: Arc<Mutex<ScanSchedule>>,
}

impl ScanCoordinator {
    fn new(scanner: ScanService) -> Self {
        Self {
            scanner,
            schedule: Arc::new(Mutex::new(ScanSchedule::default())),
        }
    }

    async fn request_scan(&self) -> Result<ScanSummary, AppError> {
        if self
            .schedule
            .lock()
            .expect("library scan schedule mutex is not poisoned")
            .request()
            == ScanRequest::Queued
        {
            return Ok(queued_summary());
        }

        loop {
            let result = self.scanner.scan_once().await;
            let follow_up = self
                .schedule
                .lock()
                .expect("library scan schedule mutex is not poisoned")
                .finished();
            if follow_up {
                if let Err(error) = &result {
                    tracing::warn!(error = %error, "library scan failed before its queued follow-up");
                }
                continue;
            }
            return result;
        }
    }
}

fn queued_summary() -> ScanSummary {
    ScanSummary {
        run_id: crate::domain::library::ScanRunId(0),
        state: crate::domain::library::ScanRunState::Running,
        counters: Default::default(),
        duration_ms: 0,
    }
}

pub struct TauriScanEventSink {
    app: tauri::AppHandle,
}

impl TauriScanEventSink {
    pub fn new(app: tauri::AppHandle) -> Self {
        Self { app }
    }
}

impl ScanEventSink for TauriScanEventSink {
    fn progress(&self, progress: ScanProgress) {
        if let Err(error) = self.app.emit(LIBRARY_SCAN_PROGRESS_EVENT, progress) {
            tracing::warn!(error = %error, "could not emit library scan progress");
        }
    }

    fn completed(&self, summary: ScanSummary) {
        if let Err(error) = self.app.emit(LIBRARY_SCAN_COMPLETED_EVENT, summary) {
            tracing::warn!(error = %error, "could not emit library scan completion");
        }
    }
}

struct LibraryWatcher {
    watcher: Mutex<RecommendedWatcher>,
    watched_paths: Mutex<BTreeSet<String>>,
    sender: Sender<WatcherMessage>,
    _worker: thread::JoinHandle<()>,
}

#[derive(Debug)]
enum WatcherMessage {
    Change,
    Failure(String),
}

impl LibraryWatcher {
    fn new(
        coordinator: Arc<ScanCoordinator>,
        issues: Arc<Mutex<Vec<ScanIssue>>>,
    ) -> Result<Self, AppError> {
        let (sender, receiver) = mpsc::channel();
        let callback_sender = sender.clone();
        let watcher = RecommendedWatcher::new(
            move |result: notify::Result<notify::Event>| match result {
                Ok(_) => {
                    let _ = callback_sender.send(WatcherMessage::Change);
                }
                Err(error) => {
                    let _ = callback_sender.send(WatcherMessage::Failure(error.to_string()));
                }
            },
            Config::default(),
        )
        .map_err(|error| AppError::Library(format!("filesystem watcher setup failed: {error}")))?;

        let worker = thread::Builder::new()
            .name("retrofrontier-library-watcher".to_owned())
            .spawn(move || watcher_worker(receiver, coordinator, issues))
            .map_err(|error| {
                AppError::Library(format!("filesystem watcher worker failed: {error}"))
            })?;

        Ok(Self {
            watcher: Mutex::new(watcher),
            watched_paths: Mutex::new(BTreeSet::new()),
            sender,
            _worker: worker,
        })
    }

    fn refresh_roots(&self, roots: &[ContentRoot]) {
        let desired: BTreeSet<_> = roots
            .iter()
            .filter(|root| root.enabled)
            .map(|root| root.path.clone())
            .collect();
        let mut watched = self
            .watched_paths
            .lock()
            .expect("library watcher paths mutex is not poisoned");
        let mut watcher = self
            .watcher
            .lock()
            .expect("library watcher mutex is not poisoned");

        let paths_to_watch: Vec<_> = desired.difference(&watched).cloned().collect();
        for path in paths_to_watch {
            if let Err(error) = watcher.watch(Path::new(&path), RecursiveMode::Recursive) {
                add_watcher_issue_from_path(&self.sender, &path, error.to_string());
            } else {
                watched.insert(path);
            }
        }
        for path in watched
            .clone()
            .difference(&desired)
            .cloned()
            .collect::<Vec<_>>()
        {
            if let Err(error) = watcher.unwatch(Path::new(&path)) {
                add_watcher_issue_from_path(&self.sender, &path, error.to_string());
            } else {
                watched.remove(&path);
            }
        }
    }
}

fn watcher_worker(
    receiver: Receiver<WatcherMessage>,
    coordinator: Arc<ScanCoordinator>,
    issues: Arc<Mutex<Vec<ScanIssue>>>,
) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            add_watcher_issue(&issues, error.to_string());
            return;
        }
    };

    loop {
        let first = match receiver.recv() {
            Ok(message) => message,
            Err(_) => return,
        };
        let mut changed = false;
        handle_watcher_message(first, &issues, &mut changed);
        loop {
            match receiver.recv_timeout(WATCHER_DEBOUNCE) {
                Ok(message) => handle_watcher_message(message, &issues, &mut changed),
                Err(mpsc::RecvTimeoutError::Timeout) => break,
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
        }
        if changed {
            if let Err(error) = runtime.block_on(coordinator.request_scan()) {
                tracing::warn!(error = %error, "watcher-triggered library scan failed");
            }
        }
    }
}

fn handle_watcher_message(
    message: WatcherMessage,
    issues: &Arc<Mutex<Vec<ScanIssue>>>,
    changed: &mut bool,
) {
    match message {
        WatcherMessage::Change => *changed = true,
        WatcherMessage::Failure(error) => add_watcher_issue(issues, error),
    }
}

fn add_watcher_issue(issues: &Arc<Mutex<Vec<ScanIssue>>>, detail: String) {
    issues
        .lock()
        .expect("library watcher issue mutex is not poisoned")
        .push(ScanIssue {
            id: None,
            scan_run_id: None,
            root_id: None,
            kind: ScanIssueKind::WatcherFailure,
            relative_path: None,
            related_path: None,
            detail: Some(detail),
            created_at: now_timestamp(),
        });
}

// Watcher setup errors happen before the worker has a convenient issue-store reference. They are
// sent to the worker, which records the typed issue without making manual scans unavailable.
fn add_watcher_issue_from_path(sender: &Sender<WatcherMessage>, path: &str, detail: String) {
    let _ = sender.send(WatcherMessage::Failure(format!("{path}: {detail}")));
}

fn ensure_managed_root(path: &Path, catalog: &SystemCatalog) -> Result<String, AppError> {
    if !path.is_absolute() {
        return Err(AppError::ContentRootInvalidPath);
    }
    ensure_directory(path, "managed ROM root")?;
    for system in catalog.systems() {
        ensure_directory(
            &path.join(&system.managed_rom_folder_name),
            "managed system ROM folder",
        )?;
    }
    let canonical = fs::canonicalize(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            AppError::ContentRootUnavailable
        } else {
            AppError::Storage(error)
        }
    })?;
    canonical
        .to_str()
        .map(str::to_owned)
        .ok_or_else(|| AppError::ContentRootInvalidPath)
}

fn ensure_directory(path: &Path, _label: &str) -> Result<(), AppError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(AppError::ContentRootInvalidPath),
        Ok(metadata) if !metadata.is_dir() => Err(AppError::ContentRootNotDirectory),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(path).map_err(AppError::Storage)
        }
        Err(error) => Err(AppError::Storage(error)),
    }
}

fn normalize_configured_path(path: &str, _label: &str) -> Result<String, AppError> {
    let requested = PathBuf::from(path.trim());
    if !requested.is_absolute() {
        return Err(AppError::ContentRootInvalidPath);
    }
    if requested
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(AppError::ContentRootInvalidPath);
    }

    let normalized = match fs::symlink_metadata(&requested) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(AppError::ContentRootInvalidPath);
        }
        Ok(metadata) if !metadata.is_dir() => return Err(AppError::ContentRootNotDirectory),
        Ok(_) => fs::canonicalize(&requested).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                AppError::ContentRootUnavailable
            } else {
                AppError::Storage(error)
            }
        })?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => requested,
        Err(error) => return Err(AppError::Storage(error)),
    };
    normalized
        .to_str()
        .map(str::to_owned)
        .ok_or(AppError::ContentRootInvalidPath)
}

fn now_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{
        ensure_directory, ensure_managed_root, normalize_configured_path, ScanRequest, ScanSchedule,
    };
    use crate::adapters::database::Database;
    use crate::domain::library::{
        ContentRootAvailability, ContentRootId, ContentRootKind, LibraryQuery,
    };
    use crate::domain::system::{SystemCatalog, SystemId};
    use crate::error::AppError;
    use crate::services::library_scanner::NoopScanEventSink;
    use std::fs;
    use std::path::Path;
    use std::sync::Arc;
    use tempfile::tempdir;

    #[test]
    fn watcher_signals_coalesce_while_a_scan_is_running() {
        let mut schedule = ScanSchedule::default();
        assert_eq!(schedule.request(), ScanRequest::Started);
        assert_eq!(schedule.request(), ScanRequest::Queued);
        assert_eq!(schedule.request(), ScanRequest::Queued);
        assert!(schedule.finished());
        assert!(!schedule.finished());
    }

    #[test]
    fn no_scan_is_marked_overlapping_by_the_schedule() {
        let mut schedule = ScanSchedule::default();
        assert_eq!(schedule.request(), ScanRequest::Started);
        assert!(!schedule.finished());
        assert_eq!(schedule.request(), ScanRequest::Started);
        assert!(!schedule.finished());
    }

    #[test]
    fn managed_bootstrap_creates_every_catalog_folder() {
        let directory = tempdir().unwrap();
        let root = directory
            .path()
            .join("Documents")
            .join("RetroFrontier")
            .join("ROMs");
        let catalog = SystemCatalog::v1();

        let persisted_path = ensure_managed_root(&root, &catalog).unwrap();

        assert_eq!(persisted_path, root.to_str().unwrap());
        for system in catalog.systems() {
            let folder = root.join(&system.managed_rom_folder_name);
            assert!(folder.is_dir(), "missing managed folder {folder:?}");
        }
    }

    #[test]
    fn content_root_paths_return_granular_safe_errors() {
        let directory = tempdir().unwrap();
        let file = directory.path().join("not-a-directory");
        fs::write(&file, b"fixture").unwrap();

        assert!(matches!(
            normalize_configured_path("relative/root", "test root"),
            Err(AppError::ContentRootInvalidPath)
        ));
        assert!(matches!(
            normalize_configured_path(
                directory.path().join("../outside").to_str().unwrap(),
                "test root"
            ),
            Err(AppError::ContentRootInvalidPath)
        ));
        assert!(matches!(
            normalize_configured_path(file.to_str().unwrap(), "test root"),
            Err(AppError::ContentRootNotDirectory)
        ));
        assert!(matches!(
            ensure_directory(&file, "test root"),
            Err(AppError::ContentRootNotDirectory)
        ));
        assert!(matches!(
            ensure_managed_root(Path::new("relative/root"), &SystemCatalog::v1()),
            Err(AppError::ContentRootInvalidPath)
        ));
    }

    #[tokio::test]
    async fn root_lifecycle_persists_external_hints_without_touching_content() {
        let directory = tempdir().unwrap();
        let managed_root = directory.path().join("managed");
        let external_root = directory.path().join("external");
        fs::create_dir_all(&external_root).unwrap();
        let marker = external_root.join("user.nes");
        fs::write(&marker, b"user-owned").unwrap();
        let database = Database::open(directory.path().join("database.sqlite3"))
            .await
            .unwrap();
        let service = super::LibraryApplicationService::initialize(
            database.pool().clone(),
            SystemCatalog::v1(),
            managed_root,
            Arc::new(NoopScanEventSink),
        )
        .await
        .unwrap();

        let external = service
            .add_external_content_root(external_root.to_str().unwrap(), Some(SystemId::Nes))
            .await
            .unwrap();
        assert_eq!(external.system_hint, Some(SystemId::Nes));
        assert_eq!(fs::read(&marker).unwrap(), b"user-owned");
        assert_eq!(service.get_content_roots().await.unwrap().len(), 2);
        let duplicate = service
            .add_external_content_root(external_root.to_str().unwrap(), Some(SystemId::Nes))
            .await
            .unwrap();
        assert_eq!(duplicate.id, external.id);
        assert_eq!(service.get_content_roots().await.unwrap().len(), 2);

        let nested = external_root.join("nested");
        assert!(matches!(
            service
                .add_external_content_root(nested.to_str().unwrap(), None)
                .await,
            Err(AppError::ContentRootOverlap)
        ));

        let managed_id = service
            .get_content_roots()
            .await
            .unwrap()
            .into_iter()
            .find(|root| root.kind == ContentRootKind::Managed)
            .unwrap()
            .id;
        assert!(matches!(
            service.remove_external_content_root(managed_id).await,
            Err(AppError::ContentRootInvalidOperation)
        ));
        assert!(matches!(
            service
                .set_content_root_enabled(ContentRootId(99_999), false)
                .await,
            Err(AppError::ContentRootInvalidOperation)
        ));

        let summary = service.rescan_library().await.unwrap();
        assert_eq!(
            summary.state,
            crate::domain::library::ScanRunState::Completed
        );
        assert!(!service.get_scan_status().running);
        assert_eq!(service.get_library_snapshot().await.unwrap().games.len(), 1);
        let game_id = service
            .query_library(&LibraryQuery::default())
            .await
            .unwrap()
            .items
            .first()
            .unwrap()
            .game_id;
        service.set_game_favorite(game_id, true).await.unwrap();
        service.rescan_library().await.unwrap();
        assert!(
            service
                .query_library(&LibraryQuery::default())
                .await
                .unwrap()
                .items
                .first()
                .unwrap()
                .favorite
        );

        service
            .set_content_root_enabled(external.id, false)
            .await
            .unwrap();
        let disabled = service
            .get_content_roots()
            .await
            .unwrap()
            .into_iter()
            .find(|root| root.id == external.id)
            .unwrap();
        assert!(!disabled.enabled);
        assert_eq!(disabled.availability, ContentRootAvailability::Disabled);

        service
            .remove_external_content_root(external.id)
            .await
            .unwrap();
        let removed = service
            .get_content_roots()
            .await
            .unwrap()
            .into_iter()
            .find(|root| root.id == external.id)
            .unwrap();
        assert!(!removed.enabled);
        assert!(external_root.is_dir());
        assert_eq!(fs::read(&marker).unwrap(), b"user-owned");

        drop(service);
        let reloaded = super::LibraryApplicationService::initialize(
            database.pool().clone(),
            SystemCatalog::v1(),
            directory.path().join("managed-reloaded"),
            Arc::new(NoopScanEventSink),
        )
        .await
        .unwrap();
        let reloaded_external = reloaded
            .get_content_roots()
            .await
            .unwrap()
            .into_iter()
            .find(|root| root.id == external.id)
            .unwrap();
        assert_eq!(reloaded_external.path, external.path);
        assert_eq!(reloaded_external.system_hint, Some(SystemId::Nes));
        assert!(!reloaded_external.enabled);
    }
}
