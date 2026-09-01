// `LaunchFailure` is the normalized launch contract: a stable code, a fixed safe message, and
// typed context. Boxing it to satisfy the large-error lint would obscure that contract for no
// behavioural gain, and a launch failure is never on a hot path.
#![allow(clippy::result_large_err)]

use crate::adapters::game_process::{ProcessExit, SpawnedGame};
use crate::adapters::runtime_lock::RuntimeMutationLock;
use crate::adapters::runtime_paths::RuntimePaths;
use crate::adapters::runtime_process::{
    clear_process_record, make_launching_record, make_running_record, read_process_record,
    write_process_record,
};
use crate::application::runtime_manager::{RuntimeManager, VerifiedLaunchRuntime};
use crate::domain::bios::BiosRequirementStatusState;
use crate::domain::core::CoreId;
use crate::domain::launch::{
    ExitedGameSession, GameLaunchOverride, GameLaunchStateChanged, LaunchContentOption,
    LaunchErrorCode, LaunchFailure, LaunchResponse, LaunchState, PlaySession, PlaySessionId,
    PlaySessionOutcome, RunningGameSession,
};
use crate::domain::library::{
    ContentRootAvailability, ContentUnit, ContentUnitId, GameAvailability, GameId,
};
use crate::domain::runtime::{ManagedProcessPhase, RuntimeError, RuntimeStatus, SafeIdentifier};
use crate::domain::system::SystemCatalog;
use crate::error::AppError;
use crate::repositories::launch::{running_session, LaunchRepository, NewPlaySession};
use crate::repositories::library::LibraryRepository;
use crate::services::bios::BiosService;
use crate::services::retroarch::{LaunchPreparation, RetroArchService};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// The runtime capabilities a launch depends on.
///
/// Production uses `RuntimeManager` directly; the boundary exists so launch lifecycle tests can
/// drive a synthetic verified runtime while still exercising the real process record and the real
/// OS mutation lock.
pub trait LaunchRuntime: Send + Sync {
    fn verified_launch_runtime(&self) -> Result<VerifiedLaunchRuntime, RuntimeError>;
    fn status(&self) -> Result<RuntimeStatus, RuntimeError>;
    fn lock_for_launch(&self) -> Result<RuntimeMutationLock, RuntimeError>;
    fn ensure_no_active_game(&self) -> Result<(), RuntimeError>;
    fn runtime_paths(&self) -> RuntimePaths;
}

impl LaunchRuntime for RuntimeManager {
    fn verified_launch_runtime(&self) -> Result<VerifiedLaunchRuntime, RuntimeError> {
        RuntimeManager::verified_launch_runtime(self)
    }

    fn status(&self) -> Result<RuntimeStatus, RuntimeError> {
        RuntimeManager::status(self)
    }

    fn lock_for_launch(&self) -> Result<RuntimeMutationLock, RuntimeError> {
        RuntimeManager::lock_for_launch(self)
    }

    fn ensure_no_active_game(&self) -> Result<(), RuntimeError> {
        RuntimeManager::ensure_no_active_game(self)
    }

    fn runtime_paths(&self) -> RuntimePaths {
        self.paths().clone()
    }
}

pub trait LaunchEventSink: Send + Sync {
    fn publish(&self, event: GameLaunchStateChanged);
}

#[derive(Debug, Clone, Copy)]
pub struct LaunchConfig {
    /// How long a freshly started child is watched for an immediate exit.
    ///
    /// An emulator that dies during startup should be reported as an early launch exit rather than
    /// as a successful launch that vanishes a moment later.
    pub settle_window: Duration,
    pub settle_poll_interval: Duration,
    /// How often an adopted process from a previous application run is re-checked.
    pub adoption_poll_interval: Duration,
}

impl Default for LaunchConfig {
    fn default() -> Self {
        Self {
            settle_window: Duration::from_millis(400),
            settle_poll_interval: Duration::from_millis(20),
            adoption_poll_interval: Duration::from_secs(5),
        }
    }
}

#[derive(Debug, Default)]
struct ActiveState {
    running: Option<RunningGameSession>,
    /// A durable process record exists whose identity could not be established. A launch must be
    /// refused, but no honest running session can be described.
    blocked: bool,
}

/// Orchestrates one controlled managed RetroArch launch.
///
/// It owns the ordering, the in-process launch serialization, the durable process-record
/// transitions, play-session persistence, asynchronous monitoring, and restart reconciliation. It
/// never builds command lines, configuration files, or environments; that is RetroArchService.
#[derive(Clone)]
pub struct LaunchApplicationService {
    library: LibraryRepository,
    launch: LaunchRepository,
    catalog: SystemCatalog,
    bios: BiosService,
    runtime: Arc<dyn LaunchRuntime>,
    retroarch: Arc<RetroArchService>,
    events: Arc<dyn LaunchEventSink>,
    config: LaunchConfig,
    // Serializes the launch sequence in this process. The durable record and the OS mutation lock
    // cover a second or crashed application process.
    sequence: Arc<tokio::sync::Mutex<()>>,
    active: Arc<Mutex<ActiveState>>,
    launch_counter: Arc<AtomicU64>,
}

impl LaunchApplicationService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        library: LibraryRepository,
        launch: LaunchRepository,
        catalog: SystemCatalog,
        bios: BiosService,
        runtime: Arc<dyn LaunchRuntime>,
        retroarch: Arc<RetroArchService>,
        events: Arc<dyn LaunchEventSink>,
        config: LaunchConfig,
    ) -> Self {
        Self {
            library,
            launch,
            catalog,
            bios,
            runtime,
            retroarch,
            events,
            config,
            sequence: Arc::new(tokio::sync::Mutex::new(())),
            active: Arc::new(Mutex::new(ActiveState::default())),
            launch_counter: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn get_launch_state(&self) -> LaunchState {
        let active = self.active.lock().expect("launch state lock");
        LaunchState {
            running: active.running.clone(),
            blocked: active.blocked,
        }
    }

    pub async fn core_override(
        &self,
        game_id: GameId,
    ) -> Result<Option<GameLaunchOverride>, AppError> {
        self.launch.core_override(game_id).await
    }

    /// Record a user-owned per-game core choice.
    ///
    /// The M7 contract admits an override only when the core is statically approved for the
    /// game's system, maps to an authenticated managed runtime component, is currently verified as
    /// installed, and is approved for that system by the authenticated release. That is exactly
    /// what the launch resolver decides, so this reuses it rather than restating the rules: an
    /// override can never become an escape hatch out of approved policy, and the two cannot drift
    /// apart.
    ///
    /// The launch path still revalidates. Persistence validation only keeps invalid state from
    /// being stored; a stored override can go stale afterwards, and only the launch may decide
    /// whether it is still good.
    pub async fn set_core_override(
        &self,
        game_id: GameId,
        core_id: &CoreId,
    ) -> Result<GameLaunchOverride, AppError> {
        let game = self
            .library
            .game(game_id)
            .await?
            .ok_or_else(|| AppError::Library("the requested game does not exist".to_owned()))?;
        let runtime = self.runtime.verified_launch_runtime().map_err(|error| {
            tracing::warn!(error = %error, "a core override needs a verified managed runtime");
            AppError::Library(
                "the managed runtime is not verified, so no core can be selected".to_owned(),
            )
        })?;
        RetroArchService::resolve_core(&self.catalog, game.system_id, Some(core_id), &runtime)
            .map_err(|failure| {
                AppError::Library(
                    match failure.code {
                        LaunchErrorCode::CoreNotInstalled => {
                            "that core is not installed in the verified managed runtime"
                        }
                        LaunchErrorCode::CorePolicyUnresolved => {
                            "no core policy is resolved for this system yet"
                        }
                        _ => "that core is not approved for this system",
                    }
                    .to_owned(),
                )
            })?;
        self.launch.set_core_override(game_id, core_id).await
    }

    pub async fn clear_core_override(&self, game_id: GameId) -> Result<(), AppError> {
        self.launch.clear_core_override(game_id).await
    }

    /// Reconcile launch state after a RetroFrontier restart.
    ///
    /// Runs after `RuntimeManager::startup_reconcile`, so the durable record has already been
    /// cleared when its process was proven dead. SQLite state never overrides that verdict.
    pub async fn reconcile_on_startup(&self) -> Result<LaunchState, AppError> {
        let paths = self.runtime.runtime_paths();
        let record = read_process_record(&paths);
        match record {
            Ok(None) => {
                // Nothing survived, so every open session was interrupted by the restart.
                let interrupted = self.launch.interrupt_open_sessions().await?;
                if interrupted > 0 {
                    tracing::info!(
                        interrupted,
                        "open play sessions were closed after a restart"
                    );
                }
                self.set_active(None, false);
            }
            Ok(Some(record)) => {
                // The record survived RuntimeManager's own reconciliation, so its process is
                // either live or of uncertain identity. Either way nothing is closed here.
                let session = self
                    .launch
                    .session(PlaySessionId(record.play_session_id))
                    .await?
                    .filter(|session| session.outcome.is_open());
                match (record.phase, session) {
                    (ManagedProcessPhase::Running, Some(session)) => {
                        let running = running_session(&session);
                        let game_id = running.game_id;
                        self.set_active(Some(running), false);
                        self.watch_until_absent(
                            None,
                            session.id,
                            Some(game_id),
                            PlaySessionOutcome::Interrupted,
                        );
                    }
                    // The record survives but no honest running session can be described — a
                    // pre-spawn `Launching` record, or one whose session is already closed. Stay
                    // blocked and let the liveness boundary release the record and the session
                    // once the child is proven gone.
                    (_, session) => {
                        self.set_active(None, true);
                        if let Some(session) = session {
                            self.watch_until_absent(
                                None,
                                session.id,
                                None,
                                PlaySessionOutcome::Interrupted,
                            );
                        }
                    }
                }
            }
            Err(_) => {
                // An unreadable or unsupported record is uncertainty: refuse launches, close
                // nothing, and delete nothing.
                self.set_active(None, true);
            }
        }
        Ok(self.get_launch_state())
    }

    /// The semantic launch entry point.
    ///
    /// Every anticipated problem is a normalized response rather than an error, so React can act
    /// on a stable code instead of parsing text.
    pub async fn launch_game(
        &self,
        game_id: GameId,
        content_unit_id: Option<ContentUnitId>,
    ) -> LaunchResponse {
        let Ok(_sequence) = self.sequence.try_lock() else {
            return LaunchResponse::failed(LaunchErrorCode::GameAlreadyRunning);
        };
        {
            let active = self.active.lock().expect("launch state lock");
            if active.running.is_some() || active.blocked {
                return LaunchResponse::failed(LaunchErrorCode::GameAlreadyRunning);
            }
        }

        match self.launch_locked(game_id, content_unit_id).await {
            Ok(response) => response,
            Err(failure) => LaunchResponse::failure(failure),
        }
    }

    async fn launch_locked(
        &self,
        game_id: GameId,
        content_unit_id: Option<ContentUnitId>,
    ) -> Result<LaunchResponse, LaunchFailure> {
        // ADR-011 serializes launch and runtime mutation under this lock, so an activation cannot
        // interleave with the window between verification and spawn.
        let runtime_lock = self
            .runtime
            .lock_for_launch()
            .map_err(|_| self.runtime_not_ready())?;
        self.runtime
            .ensure_no_active_game()
            .map_err(|_| LaunchFailure::new(LaunchErrorCode::GameAlreadyRunning))?;

        let game = self
            .library
            .game(game_id)
            .await
            .map_err(internal)?
            .ok_or_else(|| LaunchFailure::new(LaunchErrorCode::GameNotFound))?;
        if game.availability != GameAvailability::Available {
            return Err(LaunchFailure::new(LaunchErrorCode::GameUnavailable));
        }

        let units = self
            .library
            .game_content_units(game_id)
            .await
            .map_err(internal)?;
        let launchable = self.launchable_units(&units).await?;
        let unit = match content_unit_id {
            Some(requested) => launchable
                .iter()
                .find(|unit| unit.id == requested)
                .cloned()
                // A unit that belongs to another game, or is not launchable, is refused without
                // confirming whether it exists elsewhere.
                .ok_or_else(|| LaunchFailure::new(LaunchErrorCode::ContentUnavailable))?,
            None => match launchable.as_slice() {
                [] => return Err(LaunchFailure::new(LaunchErrorCode::ContentUnavailable)),
                [only] => only.clone(),
                many => {
                    return Ok(LaunchResponse::ContentSelectionRequired {
                        options: many.iter().map(content_option).collect(),
                    })
                }
            },
        };

        let root = self
            .library
            .content_root(unit.root_id)
            .await
            .map_err(internal)?
            .ok_or_else(|| LaunchFailure::new(LaunchErrorCode::ContentUnavailable))?;
        let content_path =
            RetroArchService::resolve_content_target(std::path::Path::new(&root.path), &unit)?;

        let launch_runtime = self
            .runtime
            .verified_launch_runtime()
            .map_err(|_| self.runtime_not_ready())?;
        let core_override = self
            .launch
            .core_override(game_id)
            .await
            .map_err(internal)?
            .map(|value| value.core_id);
        let core = RetroArchService::resolve_core(
            &self.catalog,
            game.system_id,
            core_override.as_ref(),
            &launch_runtime,
        )?;

        let bios_files = self.validate_bios(game.system_id)?;
        // The managed controller profiles come from the same verified runtime as the core, so a
        // release that does not carry them cannot start a game with an unusable controller.
        let controller_profiles_root =
            RetroArchService::resolve_controller_profiles(&launch_runtime)?;
        let context = self.retroarch.prepare(LaunchPreparation {
            app_run_path: &launch_runtime.app_run_path,
            core: &core,
            content_path: &content_path,
            bios_files: &bios_files,
            controller_profiles_root: &controller_profiles_root,
        })?;

        let session = self
            .launch
            .start_session(&NewPlaySession {
                game_id,
                content_unit_id: unit.id,
                core_id: core.core_id.clone(),
                runtime_installation_id: launch_runtime.installation_id.to_string(),
                runtime_release_id: launch_runtime.release_id.to_string(),
            })
            .await
            .map_err(|error| {
                tracing::warn!(error = %error, "the play session could not be persisted");
                LaunchFailure::new(LaunchErrorCode::SessionPersistenceFailed)
            })?;

        let paths = self.runtime.runtime_paths();
        let launch_id = self.next_launch_id()?;
        // The conservative pre-spawn record closes the crash window between exec and persisting a
        // PID; without it a live managed RetroArch could become invisible to RuntimeManager.
        let launching = make_launching_record(
            launch_id,
            session.id.0,
            launch_runtime.installation_id.clone(),
            &launch_runtime.app_run_path,
        )
        .and_then(|record| write_process_record(&paths, &record).map(|()| record));
        let launching = match launching {
            Ok(record) => record,
            Err(error) => {
                tracing::warn!(error = %error, "the managed process record could not be written");
                self.close_session(session.id, PlaySessionOutcome::FailedToStart, None)
                    .await;
                return Err(LaunchFailure::new(LaunchErrorCode::ProcessIdentityFailed));
            }
        };

        let mut child = match self.retroarch.spawn(&context) {
            Ok(child) => child,
            Err(failure) => {
                // The spawn never reached a running child, so no managed process exists.
                clear_record_after_proven_death(&paths);
                self.close_session(session.id, PlaySessionOutcome::FailedToStart, None)
                    .await;
                return Err(failure);
            }
        };

        // Complete the record with strong process identity. Failing here means a live child could
        // not be made visible to the safety checks, so it is stopped rather than left running.
        let running_record = make_running_record(&launching, child.pid())
            .and_then(|record| write_process_record(&paths, &record).map(|()| record));
        if let Err(error) = running_record {
            tracing::warn!(error = %error, "managed process identity could not be established");
            return match child.terminate() {
                // The child was positively observed and reaped, so nothing managed survives: the
                // pre-spawn record may go and the session honestly closes as a failed start.
                //
                // Whether it ended on its own comes from that same reaping observation. Asking
                // separately first raced the child: a game that exits instantly can be gone before
                // identity capture even reads `/proc`, and a lost race reported an ordinary early
                // exit as a failure to establish identity.
                Ok(termination) => {
                    let exit = termination.exit;
                    clear_record_after_proven_death(&paths);
                    self.close_session(session.id, PlaySessionOutcome::FailedToStart, exit.code())
                        .await;
                    Err(LaunchFailure::new(if termination.exited_on_its_own {
                        LaunchErrorCode::ProcessExitedDuringLaunch
                    } else {
                        LaunchErrorCode::ProcessIdentityFailed
                    })
                    .with_exit_code(exit.code()))
                }
                // Termination failed, so the child may still be alive. The pre-spawn `Launching`
                // record stays in place — it keeps runtime mutation and every further launch
                // blocked — and the session stays open rather than claiming a closed failed start
                // underneath a possibly live process. The liveness boundary releases both once it
                // has proved the child gone.
                Err(error) => {
                    tracing::warn!(error = %error, "the managed child could not be terminated");
                    self.set_active(None, true);
                    self.publish(None);
                    self.watch_until_absent(
                        Some(child),
                        session.id,
                        None,
                        PlaySessionOutcome::FailedToStart,
                    );
                    Err(LaunchFailure::new(LaunchErrorCode::ProcessIdentityFailed))
                }
            };
        }

        if let Some(exit) = self.settle(&mut child).await {
            // `settle` only returns an exit it positively reaped.
            clear_record_after_proven_death(&paths);
            self.close_session(session.id, PlaySessionOutcome::FailedToStart, exit.code())
                .await;
            return Err(
                LaunchFailure::new(LaunchErrorCode::ProcessExitedDuringLaunch)
                    .with_exit_code(exit.code()),
            );
        }

        // The durable record now protects the runtime, so the mutation lock can be released.
        drop(runtime_lock);
        let running = running_session(&session);
        self.set_active(Some(running.clone()), false);
        self.publish(None);
        self.monitor(child, session.id, running.game_id);

        Ok(LaunchResponse::Started {
            session: running,
            diagnostics: context.diagnostics,
        })
    }

    /// Watch a freshly started child for an immediate exit.
    ///
    /// Returns the exit when the child died inside the launch window, so a startup failure is
    /// reported as an early launch exit instead of a successful launch.
    async fn settle(&self, child: &mut SpawnedGame) -> Option<ProcessExit> {
        let deadline = std::time::Instant::now() + self.config.settle_window;
        loop {
            match child.try_exit() {
                Ok(Some(exit)) => return Some(exit),
                Ok(None) => {}
                Err(_) => return None,
            }
            if std::time::Instant::now() >= deadline {
                return None;
            }
            tokio::time::sleep(self.config.settle_poll_interval).await;
        }
    }

    /// Wait for the managed child on a blocking task and close everything down afterwards.
    ///
    /// RetroArch exiting must never terminate RetroFrontier, and the Tauri UI thread is never
    /// blocked while a game runs. A wait that fails is not evidence of death, so the durable
    /// record survives it and the decision moves to `watch_until_absent`.
    fn monitor(&self, child: SpawnedGame, session_id: PlaySessionId, game_id: GameId) {
        let service = self.clone();
        tokio::spawn(async move {
            // The handle comes back from the blocking task so an unobserved exit can keep being
            // re-checked instead of being assumed.
            let waited = tokio::task::spawn_blocking(move || {
                let mut child = child;
                let exit = child.wait();
                (child, exit)
            })
            .await;
            let (child, exit) = match waited {
                Ok((child, Ok(exit))) => (Some(child), Some(exit)),
                Ok((child, Err(error))) => {
                    tracing::warn!(error = %error, "the managed child could not be reaped");
                    (Some(child), None)
                }
                // The blocking task itself failed, so the child handle went with it.
                Err(error) => {
                    tracing::warn!(error = %error, "the managed process monitor stopped");
                    (None, None)
                }
            };
            let Some(exit) = exit else {
                // Death is unproven. Block, keep the durable record, and let the liveness
                // boundary decide when the process may be declared gone.
                service.set_active(None, true);
                service.publish(None);
                service.watch_until_absent(
                    child,
                    session_id,
                    Some(game_id),
                    PlaySessionOutcome::Interrupted,
                );
                return;
            };
            let outcome = if exit.is_clean() {
                PlaySessionOutcome::Completed
            } else {
                // A non-zero exit after a successful start is a crash of the emulator, not
                // evidence that the managed runtime is corrupt.
                PlaySessionOutcome::Crashed
            };
            let exit_code = exit.code();
            // The child was positively observed and reaped.
            clear_record_after_proven_death(&service.runtime.runtime_paths());
            service.close_session(session_id, outcome, exit_code).await;
            service.set_active(None, false);
            service.publish(Some(ExitedGameSession {
                session_id,
                game_id,
                outcome,
                exit_code,
            }));
        });
    }

    /// Poll until the managed child is *proven* gone, then finalize the session.
    ///
    /// This is the single way out of process uncertainty. It covers a process adopted from a
    /// previous application run — which RetroFrontier cannot `wait()` on, so no exit code is
    /// available — and a child of this run whose exit or termination could not be observed.
    ///
    /// The durable record is deleted here only after the child has been positively reaped;
    /// otherwise the verdict belongs to `ManagedProcessInspector`, which deletes the record itself
    /// once it has independently proved absence. Runtime mutation and any further launch stay
    /// blocked for the whole interval.
    fn watch_until_absent(
        &self,
        mut child: Option<SpawnedGame>,
        session_id: PlaySessionId,
        game_id: Option<GameId>,
        outcome: PlaySessionOutcome,
    ) {
        let service = self.clone();
        tokio::spawn(async move {
            loop {
                if let Some(child) = child.as_mut() {
                    if matches!(child.try_exit(), Ok(Some(_))) {
                        clear_record_after_proven_death(&service.runtime.runtime_paths());
                        break;
                    }
                }
                // `ensure_no_active_game` succeeds only once the inspector has proved the record's
                // process absent, and it is the component that clears the record in that case.
                if service.runtime.ensure_no_active_game().is_ok() {
                    break;
                }
                tokio::time::sleep(service.config.adoption_poll_interval).await;
            }
            service.close_session(session_id, outcome, None).await;
            service.set_active(None, false);
            service.publish(game_id.map(|game_id| ExitedGameSession {
                session_id,
                game_id,
                outcome,
                exit_code: None,
            }));
        });
    }

    /// The launchable content units of one game, in a deterministic order.
    ///
    /// Availability, membership role, and root state are all checked, so no unit is offered that
    /// the launch step would then refuse.
    async fn launchable_units(
        &self,
        units: &[ContentUnit],
    ) -> Result<Vec<ContentUnit>, LaunchFailure> {
        let mut launchable = Vec::new();
        for unit in units {
            let Some(root) = self
                .library
                .content_root(unit.root_id)
                .await
                .map_err(internal)?
            else {
                continue;
            };
            if !root.enabled
                || matches!(
                    root.availability,
                    ContentRootAvailability::Unavailable | ContentRootAvailability::Unsafe
                )
            {
                continue;
            }
            if RetroArchService::resolve_content_target(std::path::Path::new(&root.path), unit)
                .is_ok()
            {
                launchable.push(unit.clone());
            }
        }
        Ok(launchable)
    }

    /// Validate the BIOS prerequisites of one system before anything is spawned.
    fn validate_bios(
        &self,
        system_id: crate::domain::system::SystemId,
    ) -> Result<Vec<(String, std::path::PathBuf)>, LaunchFailure> {
        let discovery = self.bios.discover(None).map_err(|error| {
            tracing::warn!(error = %error, "BIOS discovery failed before launch");
            LaunchFailure::new(LaunchErrorCode::InternalLaunchFailure)
        })?;
        let mut missing = Vec::new();
        let mut invalid = Vec::new();
        let mut uncovered = Vec::new();
        for requirement in discovery
            .requirements
            .iter()
            .filter(|requirement| requirement.system_id == system_id && requirement.required)
        {
            match requirement.state {
                BiosRequirementStatusState::PresentValid => {}
                BiosRequirementStatusState::Missing
                | BiosRequirementStatusState::OptionalMissing => {
                    missing.push(requirement.requirement_id.clone())
                }
                BiosRequirementStatusState::PresentInvalid => {
                    invalid.push(requirement.requirement_id.clone())
                }
                BiosRequirementStatusState::NotCoveredByCatalog => {
                    uncovered.push(requirement.requirement_id.clone())
                }
            }
        }
        // Reported in order of how actionable they are for the user.
        if !missing.is_empty() {
            return Err(LaunchFailure::new(LaunchErrorCode::BiosMissing)
                .with_system(system_id)
                .with_bios_requirements(missing));
        }
        if !invalid.is_empty() {
            return Err(LaunchFailure::new(LaunchErrorCode::BiosInvalid)
                .with_system(system_id)
                .with_bios_requirements(invalid));
        }
        if !uncovered.is_empty() {
            return Err(LaunchFailure::new(LaunchErrorCode::BiosNotCoveredByCatalog)
                .with_system(system_id)
                .with_bios_requirements(uncovered));
        }
        Ok(self.bios.validated_files(&discovery, system_id))
    }

    fn runtime_not_ready(&self) -> LaunchFailure {
        let failure = LaunchFailure::new(LaunchErrorCode::RuntimeNotReady);
        match self.runtime.status() {
            Ok(status) => failure.with_runtime_state(status.state),
            Err(_) => failure,
        }
    }

    fn next_launch_id(&self) -> Result<SafeIdentifier, LaunchFailure> {
        let counter = self.launch_counter.fetch_add(1, Ordering::Relaxed);
        SafeIdentifier::new(format!("launch-{}-{counter}", std::process::id()))
            .map_err(|_| LaunchFailure::new(LaunchErrorCode::InternalLaunchFailure))
    }

    async fn close_session(
        &self,
        session_id: PlaySessionId,
        outcome: PlaySessionOutcome,
        exit_code: Option<i64>,
    ) {
        if let Err(error) = self
            .launch
            .complete_session(session_id, outcome, exit_code)
            .await
        {
            tracing::warn!(error = %error, "the play session could not be closed");
        }
    }

    fn set_active(&self, running: Option<RunningGameSession>, blocked: bool) {
        let mut active = self.active.lock().expect("launch state lock");
        active.running = running;
        active.blocked = blocked;
    }

    fn publish(&self, exited: Option<ExitedGameSession>) {
        self.events.publish(GameLaunchStateChanged {
            state: self.get_launch_state(),
            exited,
        });
    }
}

/// Delete the durable managed-process record.
///
/// SAFETY INVARIANT (ADR-011): every caller must already hold proof that no managed child
/// survives — either no process was ever created, or its exit was positively observed and reaped.
/// Where death cannot be proven the record is left in place and `watch_until_absent` defers to
/// `ManagedProcessInspector`, the only other component allowed to delete it.
fn clear_record_after_proven_death(paths: &RuntimePaths) {
    if let Err(error) = clear_process_record(paths) {
        tracing::warn!(error = %error, "the managed process record could not be cleared");
    }
}

fn content_option(unit: &ContentUnit) -> LaunchContentOption {
    LaunchContentOption {
        content_unit_id: unit.id,
        kind: unit.kind,
        local_title: unit.local_title.clone(),
        file_count: unit.files.len() as u64,
        availability: unit.availability,
    }
}

fn internal(error: AppError) -> LaunchFailure {
    error.log();
    LaunchFailure::new(LaunchErrorCode::InternalLaunchFailure)
}

/// The event sink used by the desktop application.
pub struct TauriLaunchEventSink {
    app: tauri::AppHandle,
}

impl TauriLaunchEventSink {
    pub fn new(app: tauri::AppHandle) -> Self {
        Self { app }
    }
}

impl LaunchEventSink for TauriLaunchEventSink {
    fn publish(&self, event: GameLaunchStateChanged) {
        use tauri::Emitter;
        if let Err(error) = self.app.emit("game-launch-state-changed", event) {
            tracing::warn!(error = %error, "the launch state event could not be delivered");
        }
    }
}

/// Session history is product data. This helper exists so callers cannot mistake it for the
/// process-safety authority.
pub fn is_process_authority(_session: &PlaySession) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::{LaunchApplicationService, LaunchConfig, LaunchEventSink, LaunchRuntime};
    use crate::adapters::database::Database;
    use crate::adapters::game_process::{
        GameProcessLauncher, LinuxGameProcessLauncher, ProcessFaults, SpawnRequest, SpawnedGame,
    };
    use crate::adapters::runtime_lock::RuntimeMutationLock;
    use crate::adapters::runtime_paths::RuntimePaths;
    use crate::adapters::runtime_process::{
        read_process_record, LinuxManagedProcessInspector, ManagedProcessInspector,
    };
    use crate::application::runtime_manager::{ManagedCoreComponent, VerifiedLaunchRuntime};
    use crate::domain::core::CoreId;
    use crate::domain::launch::{
        GameLaunchStateChanged, HostPrerequisite, LaunchErrorCode, LaunchResponse, PlaySessionId,
        PlaySessionOutcome,
    };
    use crate::domain::library::{ContentUnitId, GameId};
    use crate::domain::runtime::{
        ManagedProcessPhase, RuntimeError, RuntimeState, RuntimeStatus, SafeIdentifier,
    };
    use crate::domain::system::{SystemCatalog, SystemId};
    use crate::repositories::launch::LaunchRepository;
    use crate::repositories::library::LibraryRepository;
    use crate::services::bios::BiosService;
    use crate::services::retroarch::RetroArchService;
    use crate::services::retroarch_host::HostPrerequisiteInspector;
    use crate::services::retroarch_paths::LaunchPaths;
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};
    use tempfile::TempDir;

    const TEST_TIME: i64 = 1_756_000_000_000;

    /// A synthetic verified runtime over real runtime paths.
    ///
    /// The process record and the OS mutation lock are the real ones, so the launch lifecycle is
    /// exercised against genuine process-safety behaviour rather than a stub.
    struct TestRuntime {
        paths: RuntimePaths,
        launch: Mutex<Option<VerifiedLaunchRuntime>>,
        state: Mutex<RuntimeState>,
    }

    impl LaunchRuntime for TestRuntime {
        fn verified_launch_runtime(&self) -> Result<VerifiedLaunchRuntime, RuntimeError> {
            self.launch
                .lock()
                .unwrap()
                .clone()
                .ok_or_else(|| RuntimeError::InstalledTree("not ready".to_owned()))
        }

        fn status(&self) -> Result<RuntimeStatus, RuntimeError> {
            Ok(RuntimeStatus {
                state: *self.state.lock().unwrap(),
                installation_id: Some("install-1".to_owned()),
                release_id: Some("release-1".to_owned()),
                can_rollback: false,
                repair_required: false,
            })
        }

        fn lock_for_launch(&self) -> Result<RuntimeMutationLock, RuntimeError> {
            RuntimeMutationLock::acquire(&self.paths.mutation_lock())
        }

        fn ensure_no_active_game(&self) -> Result<(), RuntimeError> {
            LinuxManagedProcessInspector.ensure_no_active_game(&self.paths)
        }

        fn runtime_paths(&self) -> RuntimePaths {
            self.paths.clone()
        }
    }

    #[derive(Default)]
    struct RecordingEvents {
        events: Mutex<Vec<GameLaunchStateChanged>>,
    }

    impl LaunchEventSink for RecordingEvents {
        fn publish(&self, event: GameLaunchStateChanged) {
            self.events.lock().unwrap().push(event);
        }
    }

    struct StaticHost {
        missing: Vec<HostPrerequisite>,
    }

    impl HostPrerequisiteInspector for StaticHost {
        fn inspect(&self, _environment: &BTreeMap<String, String>) -> Vec<HostPrerequisite> {
            self.missing.clone()
        }
    }

    /// A launcher that arms fault injection on every child it starts, and can run one side
    /// effect the moment a child exists. Both exist so uncertain-death handling can be proved
    /// without depending on the OS `waitpid`/`kill` calls failing by chance.
    #[derive(Default)]
    struct FaultyLauncher {
        faults: ProcessFaults,
        on_spawn: Mutex<Option<Box<dyn FnOnce() + Send>>>,
    }

    impl FaultyLauncher {
        fn on_spawn(&self, effect: impl FnOnce() + Send + 'static) {
            *self.on_spawn.lock().unwrap() = Some(Box::new(effect));
        }
    }

    impl GameProcessLauncher for FaultyLauncher {
        fn spawn(&self, request: &SpawnRequest) -> Result<SpawnedGame, std::io::Error> {
            let mut child = LinuxGameProcessLauncher.spawn(request)?;
            child.inject_faults(self.faults.clone());
            if let Some(effect) = self.on_spawn.lock().unwrap().take() {
                effect();
            }
            Ok(child)
        }
    }

    struct Harness {
        _app_data: TempDir,
        _content: TempDir,
        bios_root: PathBuf,
        app_run: PathBuf,
        /// The synthetic runtime written by `set_app_run_until_stopped` creates this file when it
        /// starts, and ends when `stop_file` appears.
        started_file: PathBuf,
        stop_file: PathBuf,
        runtime: Arc<TestRuntime>,
        library_repository: LibraryRepository,
        launch_repository: LaunchRepository,
        catalog: SystemCatalog,
        bios: BiosService,
        retroarch: Arc<RetroArchService>,
        config: LaunchConfig,
        service: LaunchApplicationService,
        events: Arc<RecordingEvents>,
    }

    impl Harness {
        async fn build(system: SystemId, missing_host: Vec<HostPrerequisite>) -> Self {
            Self::build_with(system, missing_host, Arc::new(LinuxGameProcessLauncher)).await
        }

        async fn build_with(
            system: SystemId,
            missing_host: Vec<HostPrerequisite>,
            launcher: Arc<dyn GameProcessLauncher>,
        ) -> Self {
            let app_data = tempfile::tempdir().unwrap();
            let content = tempfile::tempdir().unwrap();
            let bios_root = app_data.path().join("user-bios");
            fs::create_dir_all(&bios_root).unwrap();

            let database = Database::open(app_data.path().join("database/launch.sqlite3"))
                .await
                .unwrap();
            let pool = database.pool().clone();
            seed_library(&pool, system, content.path()).await;

            let runtime_paths = RuntimePaths::new(app_data.path());
            runtime_paths.prepare().unwrap();
            let component = default_component(system);
            let (launch_runtime, app_run) = synthetic_runtime(
                &runtime_paths,
                component,
                system,
                system == SystemId::NintendoGameCube,
            );

            let runtime = Arc::new(TestRuntime {
                paths: runtime_paths,
                launch: Mutex::new(Some(launch_runtime)),
                state: Mutex::new(RuntimeState::Ready),
            });
            let catalog = SystemCatalog::v1();
            let bios = BiosService::from_catalog(&bios_root, &catalog).unwrap();
            let retroarch = Arc::new(RetroArchService::new(
                LaunchPaths::new(app_data.path()),
                launcher,
                Arc::new(StaticHost {
                    missing: missing_host,
                }),
            ));
            let events = Arc::new(RecordingEvents::default());
            let launch_repository = LaunchRepository::new(pool.clone());
            let library_repository = LibraryRepository::new(pool);
            let config = LaunchConfig {
                settle_window: Duration::from_millis(120),
                settle_poll_interval: Duration::from_millis(10),
                adoption_poll_interval: Duration::from_millis(50),
            };
            let service = LaunchApplicationService::new(
                library_repository.clone(),
                launch_repository.clone(),
                catalog.clone(),
                bios.clone(),
                runtime.clone(),
                retroarch.clone(),
                events.clone(),
                config,
            );

            Self {
                started_file: app_data.path().join("synthetic-runtime-started"),
                stop_file: app_data.path().join("stop-the-synthetic-runtime"),
                _app_data: app_data,
                _content: content,
                bios_root,
                app_run,
                runtime,
                library_repository,
                launch_repository,
                catalog,
                bios,
                retroarch,
                config,
                service,
                events,
            }
        }

        /// A fresh application service over the same durable state: a RetroFrontier restart.
        fn rebuild(&self) -> LaunchApplicationService {
            LaunchApplicationService::new(
                self.library_repository.clone(),
                self.launch_repository.clone(),
                self.catalog.clone(),
                self.bios.clone(),
                self.runtime.clone(),
                self.retroarch.clone(),
                Arc::new(RecordingEvents::default()),
                self.config,
            )
        }

        fn set_app_run(&self, script: &str) {
            fs::write(&self.app_run, format!("#!/bin/sh\n{script}\n")).unwrap();
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&self.app_run, fs::Permissions::from_mode(0o755)).unwrap();
        }

        /// A synthetic runtime that announces itself and then stays alive until `stop` is called.
        fn set_app_run_until_stopped(&self) {
            self.set_app_run(&format!(
                ": > '{}'\nwhile [ ! -f '{}' ]; do sleep 0.02; done",
                self.started_file.display(),
                self.stop_file.display()
            ));
        }

        /// Block until the synthetic runtime is executing its script rather than still starting
        /// up. Until then the interpreter has not finished reading the AppRun, and moving it
        /// would kill the child instead of leaving it alive.
        fn await_started(&self) {
            for _ in 0..1000 {
                if self.started_file.exists() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            panic!("the synthetic runtime never started");
        }

        fn stop(&self) {
            fs::write(&self.stop_file, b"stop").unwrap();
        }

        async fn await_blocked(&self, service: &LaunchApplicationService) {
            let deadline = Instant::now() + Duration::from_secs(10);
            while !service.get_launch_state().blocked && Instant::now() < deadline {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            assert!(
                service.get_launch_state().blocked,
                "the launch never blocked"
            );
        }

        async fn await_unblocked(&self, service: &LaunchApplicationService) {
            let deadline = Instant::now() + Duration::from_secs(10);
            while service.get_launch_state().blocked && Instant::now() < deadline {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            assert!(
                !service.get_launch_state().blocked,
                "the launch never became available again"
            );
        }

        /// Edit the verified runtime the way a re-verification would, so the persistence contract
        /// can be exercised against a runtime that no longer satisfies it.
        fn edit_launch_runtime(&self, edit: impl FnOnce(&mut VerifiedLaunchRuntime)) {
            let mut runtime = self.runtime.launch.lock().unwrap();
            edit(runtime.as_mut().expect("a verified launch runtime"));
        }

        fn core_path(&self, component: &str) -> PathBuf {
            self.runtime
                .verified_launch_runtime()
                .unwrap()
                .cores
                .get(&SafeIdentifier::new(component).unwrap())
                .unwrap()
                .core_path
                .clone()
        }

        fn write_bios(&self, filename: &str, contents: &[u8]) {
            fs::write(self.bios_root.join(filename), contents).unwrap();
        }

        async fn await_outcome(&self, session: PlaySessionId) -> PlaySessionOutcome {
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                let outcome = self
                    .launch_repository
                    .session(session)
                    .await
                    .unwrap()
                    .unwrap()
                    .outcome;
                if !outcome.is_open() || Instant::now() >= deadline {
                    return outcome;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }
    }

    fn default_component(system: SystemId) -> &'static str {
        match system {
            SystemId::Nes => "nestopia",
            SystemId::Snes => "bsnes-mercury-balanced",
            SystemId::PlayStation => "beetle-psx",
            SystemId::NintendoGameCube => "dolphin",
            _ => "unresolved-core",
        }
    }

    fn synthetic_runtime(
        paths: &RuntimePaths,
        component: &str,
        system: SystemId,
        with_support: bool,
    ) -> (VerifiedLaunchRuntime, PathBuf) {
        let installation_id = SafeIdentifier::new("install-1").unwrap();
        let installation = paths.version_path(&installation_id);
        let app_run = installation.join("runtime/app/AppRun");
        fs::create_dir_all(app_run.parent().unwrap()).unwrap();
        fs::write(&app_run, "#!/bin/sh\nsleep 5\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&app_run, fs::Permissions::from_mode(0o755)).unwrap();
        let core_path = installation.join(format!("cores/{component}/core.so"));
        fs::create_dir_all(core_path.parent().unwrap()).unwrap();
        fs::write(&core_path, b"synthetic core").unwrap();

        let mut support_assets = BTreeMap::new();
        // The managed controller profiles are part of every real release, so a synthetic launchable
        // runtime carries them too.
        let profiles = installation.join("runtime/support/joypad-autoconfig");
        fs::create_dir_all(profiles.join("udev")).unwrap();
        support_assets.insert(
            SafeIdentifier::new(crate::services::retroarch::JOYPAD_AUTOCONFIG_COMPONENT).unwrap(),
            profiles,
        );
        if with_support {
            let sys = installation.join("runtime/support/dolphin/Sys");
            fs::create_dir_all(&sys).unwrap();
            support_assets.insert(SafeIdentifier::new("dolphin-sys").unwrap(), sys);
        }

        (
            VerifiedLaunchRuntime {
                status: RuntimeStatus {
                    state: RuntimeState::Ready,
                    installation_id: Some("install-1".to_owned()),
                    release_id: Some("release-1".to_owned()),
                    can_rollback: false,
                    repair_required: false,
                },
                installation_id,
                release_id: SafeIdentifier::new("release-1").unwrap(),
                app_run_path: app_run.clone(),
                cores: BTreeMap::from([(
                    SafeIdentifier::new(component).unwrap(),
                    ManagedCoreComponent {
                        component_id: SafeIdentifier::new(component).unwrap(),
                        core_path,
                        systems: vec![SafeIdentifier::new(system.as_str()).unwrap()],
                    },
                )]),
                support_assets,
            },
            app_run,
        )
    }

    /// Two games: game 1 has one launchable unit, game 2 has two, game 3 is unavailable.
    async fn seed_library(pool: &sqlx::SqlitePool, system: SystemId, content_root: &Path) {
        let extension = match system {
            SystemId::Nes => "nes",
            SystemId::Snes => "sfc",
            SystemId::PlayStation => "chd",
            _ => "rvz",
        };
        let folder = "Games";
        fs::create_dir_all(content_root.join(folder)).unwrap();
        for name in ["one", "two-a", "two-b", "gone"] {
            fs::write(
                content_root
                    .join(folder)
                    .join(format!("{name}.{extension}")),
                b"synthetic content",
            )
            .unwrap();
        }
        fs::remove_file(content_root.join(folder).join(format!("gone.{extension}"))).unwrap();

        sqlx::query(
            "INSERT INTO content_roots (id, path, kind, enabled, availability, created_at, \
             updated_at) VALUES (1, ?, 'managed', 1, 'available', ?, ?)",
        )
        .bind(content_root.to_str().unwrap())
        .bind(TEST_TIME)
        .bind(TEST_TIME)
        .execute(pool)
        .await
        .unwrap();

        let mut unit_id = 0_i64;
        let mut file_id = 0_i64;
        for (game_id, title, availability, members) in [
            (1_i64, "One", "available", vec!["one"]),
            (2, "Two", "available", vec!["two-a", "two-b"]),
            (3, "Gone", "unavailable", vec!["gone"]),
        ] {
            sqlx::query(
                "INSERT INTO games (id, system_id, local_title, availability, created_at, \
                 updated_at) VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(game_id)
            .bind(system.as_str())
            .bind(title)
            .bind(availability)
            .bind(TEST_TIME)
            .bind(TEST_TIME)
            .execute(pool)
            .await
            .unwrap();

            for name in members {
                unit_id += 1;
                file_id += 1;
                let relative = format!("{folder}/{name}.{extension}");
                let unit_availability = if availability == "available" {
                    "available"
                } else {
                    "missing"
                };
                sqlx::query(
                    "INSERT INTO content_units (id, game_id, root_id, system_id, kind, \
                     local_title, primary_relative_path, fingerprint, availability, created_at, \
                     updated_at) VALUES (?, ?, 1, ?, 'single_file', ?, ?, NULL, ?, ?, ?)",
                )
                .bind(unit_id)
                .bind(game_id)
                .bind(system.as_str())
                .bind(name)
                .bind(&relative)
                .bind(unit_availability)
                .bind(TEST_TIME)
                .bind(TEST_TIME)
                .execute(pool)
                .await
                .unwrap();
                sqlx::query(
                    "INSERT INTO content_files (id, root_id, relative_path, size_bytes, \
                     modified_at, availability, created_at, updated_at) \
                     VALUES (?, 1, ?, 17, ?, ?, ?, ?)",
                )
                .bind(file_id)
                .bind(&relative)
                .bind(TEST_TIME)
                .bind(if availability == "available" {
                    "available"
                } else {
                    "missing"
                })
                .bind(TEST_TIME)
                .bind(TEST_TIME)
                .execute(pool)
                .await
                .unwrap();
                sqlx::query(
                    "INSERT INTO content_unit_files (content_unit_id, content_file_id, ordinal, \
                     role) VALUES (?, ?, 0, 'standalone')",
                )
                .bind(unit_id)
                .bind(file_id)
                .execute(pool)
                .await
                .unwrap();
            }
        }
    }

    fn failure_code(response: &LaunchResponse) -> LaunchErrorCode {
        match response {
            LaunchResponse::Failed { error } => error.code,
            other => panic!("expected a failure, got {other:?}"),
        }
    }

    fn started_session(response: &LaunchResponse) -> PlaySessionId {
        match response {
            LaunchResponse::Started { session, .. } => session.session_id,
            other => panic!("expected a started launch, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_missing_or_unavailable_game_is_refused_before_anything_is_started() {
        let harness = Harness::build(SystemId::Nes, Vec::new()).await;

        assert_eq!(
            failure_code(&harness.service.launch_game(GameId(99), None).await),
            LaunchErrorCode::GameNotFound
        );
        assert_eq!(
            failure_code(&harness.service.launch_game(GameId(3), None).await),
            LaunchErrorCode::GameUnavailable
        );
        assert!(harness
            .launch_repository
            .open_sessions()
            .await
            .unwrap()
            .is_empty());
        assert!(read_process_record(&harness.runtime.paths)
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn one_launchable_unit_is_selected_automatically_and_several_require_a_choice() {
        let harness = Harness::build(SystemId::Nes, Vec::new()).await;
        harness.set_app_run("sleep 5");

        let selection = harness.service.launch_game(GameId(2), None).await;
        let options = match &selection {
            LaunchResponse::ContentSelectionRequired { options } => options.clone(),
            other => panic!("expected a content selection, got {other:?}"),
        };
        assert_eq!(options.len(), 2);
        assert!(harness
            .launch_repository
            .open_sessions()
            .await
            .unwrap()
            .is_empty());

        let started = harness.service.launch_game(GameId(1), None).await;
        assert!(matches!(started, LaunchResponse::Started { .. }));
    }

    /// Two unrelated launch harnesses share one test process, so whenever any of them spawns a
    /// child the fork copies the whole descriptor table — including this harness's mutation-lock
    /// descriptor — into that child. `flock` belongs to the open file description, so a stale
    /// duplicate would keep this harness's lock held after its owner released it and the next
    /// launch would report `RuntimeNotReady` instead of its real domain outcome.
    ///
    /// The duplicate is created directly here so the condition is deterministic rather than a
    /// matter of fork/exec timing in a parallel test.
    #[tokio::test]
    async fn a_descriptor_inherited_by_a_parallel_test_child_cannot_strand_the_mutation_lock() {
        let harness = Harness::build(SystemId::Nes, Vec::new()).await;
        let inherited = {
            let owner = harness.runtime.lock_for_launch().unwrap();
            owner.duplicate_descriptor()
        };

        // Both launches have to take and release the very same lock the drop above released.
        assert_eq!(
            failure_code(&harness.service.launch_game(GameId(99), None).await),
            LaunchErrorCode::GameNotFound
        );
        assert_eq!(
            failure_code(&harness.service.launch_game(GameId(3), None).await),
            LaunchErrorCode::GameUnavailable
        );
        drop(inherited);
    }

    #[tokio::test]
    async fn a_foreign_or_unlaunchable_content_unit_is_never_started() {
        let harness = Harness::build(SystemId::Nes, Vec::new()).await;

        // Unit 2 belongs to game 2, not game 1.
        assert_eq!(
            failure_code(
                &harness
                    .service
                    .launch_game(GameId(1), Some(ContentUnitId(2)))
                    .await
            ),
            LaunchErrorCode::ContentUnavailable
        );
        // Game 3's unit exists but its content is missing.
        assert_eq!(
            failure_code(
                &harness
                    .service
                    .launch_game(GameId(3), Some(ContentUnitId(4)))
                    .await
            ),
            LaunchErrorCode::GameUnavailable
        );
        assert_eq!(
            failure_code(
                &harness
                    .service
                    .launch_game(GameId(1), Some(ContentUnitId(404)))
                    .await
            ),
            LaunchErrorCode::ContentUnavailable
        );
    }

    #[tokio::test]
    async fn a_runtime_that_is_not_ready_or_lacks_the_core_cannot_launch() {
        let harness = Harness::build(SystemId::Nes, Vec::new()).await;

        *harness.runtime.launch.lock().unwrap() = None;
        *harness.runtime.state.lock().unwrap() = RuntimeState::Broken;
        let failure = harness.service.launch_game(GameId(1), None).await;
        assert_eq!(failure_code(&failure), LaunchErrorCode::RuntimeNotReady);
        if let LaunchResponse::Failed { error } = &failure {
            assert_eq!(error.context.runtime_state, Some(RuntimeState::Broken));
        }

        // Ready, but the approved core is not part of the installed runtime.
        let mut without_core = harness
            .runtime
            .launch
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_else(|| {
                let paths = harness.runtime.paths.clone();
                synthetic_runtime(&paths, "nestopia", SystemId::Nes, false).0
            });
        without_core.cores.clear();
        *harness.runtime.launch.lock().unwrap() = Some(without_core);
        *harness.runtime.state.lock().unwrap() = RuntimeState::Ready;
        assert_eq!(
            failure_code(&harness.service.launch_game(GameId(1), None).await),
            LaunchErrorCode::CoreNotInstalled
        );
    }

    #[tokio::test]
    async fn an_unresolved_system_reports_unresolved_core_policy() {
        let harness = Harness::build(SystemId::Nintendo64, Vec::new()).await;

        assert_eq!(
            failure_code(&harness.service.launch_game(GameId(1), None).await),
            LaunchErrorCode::CorePolicyUnresolved
        );
    }

    #[tokio::test]
    async fn a_valid_override_is_used_and_an_invalid_one_never_falls_back() {
        let harness = Harness::build(SystemId::Nes, Vec::new()).await;
        harness.set_app_run("sleep 5");
        let nestopia = CoreId::new("nestopia").unwrap();

        harness
            .service
            .set_core_override(GameId(1), &nestopia)
            .await
            .unwrap();
        let started = harness.service.launch_game(GameId(1), None).await;
        let session = started_session(&started);
        assert_eq!(
            harness
                .launch_repository
                .session(session)
                .await
                .unwrap()
                .unwrap()
                .core_id,
            nestopia
        );

        // A core approved for another system may not be stored at all.
        assert!(harness
            .service
            .set_core_override(GameId(1), &CoreId::new("beetle-psx").unwrap())
            .await
            .is_err());

        // A directly persisted unapproved override is refused at launch instead of falling back.
        harness
            .launch_repository
            .set_core_override(GameId(1), &CoreId::new("beetle-psx").unwrap())
            .await
            .unwrap();
        let harness2 = Harness::build(SystemId::Nes, Vec::new()).await;
        harness2
            .launch_repository
            .set_core_override(GameId(1), &CoreId::new("beetle-psx").unwrap())
            .await
            .unwrap();
        assert_eq!(
            failure_code(&harness2.service.launch_game(GameId(1), None).await),
            LaunchErrorCode::CoreNotApproved
        );
    }

    #[tokio::test]
    async fn required_bios_is_validated_before_the_process_is_spawned() {
        let harness = Harness::build(SystemId::PlayStation, Vec::new()).await;
        harness.set_app_run("sleep 5");

        assert_eq!(
            failure_code(&harness.service.launch_game(GameId(1), None).await),
            LaunchErrorCode::BiosMissing
        );

        harness.write_bios("scph5501.bin", b"not the documented dump");
        assert_eq!(
            failure_code(&harness.service.launch_game(GameId(1), None).await),
            LaunchErrorCode::BiosInvalid
        );
        assert!(read_process_record(&harness.runtime.paths)
            .unwrap()
            .is_none());
        assert!(harness
            .launch_repository
            .open_sessions()
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn a_missing_display_session_blocks_the_launch() {
        let harness = Harness::build(SystemId::Nes, vec![HostPrerequisite::DisplaySession]).await;

        let failure = harness.service.launch_game(GameId(1), None).await;

        assert_eq!(
            failure_code(&failure),
            LaunchErrorCode::HostPrerequisiteMissing
        );
        assert!(harness
            .launch_repository
            .open_sessions()
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn a_degraded_host_launches_with_visible_diagnostics() {
        let harness = Harness::build(
            SystemId::Nes,
            vec![
                HostPrerequisite::AudioService,
                HostPrerequisite::InputDevices,
            ],
        )
        .await;
        harness.set_app_run("sleep 5");

        let started = harness.service.launch_game(GameId(1), None).await;

        match &started {
            LaunchResponse::Started { diagnostics, .. } => assert_eq!(diagnostics.len(), 2),
            other => panic!("expected a started launch, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_running_game_blocks_a_second_launch_and_every_runtime_mutation() {
        let harness = Harness::build(SystemId::Nes, Vec::new()).await;
        harness.set_app_run("sleep 5");

        let started = harness.service.launch_game(GameId(1), None).await;
        let session = started_session(&started);

        let record = read_process_record(&harness.runtime.paths)
            .unwrap()
            .unwrap();
        assert_eq!(record.phase, ManagedProcessPhase::Running);
        assert_eq!(record.play_session_id, session.0);
        assert!(record.pid.is_some());
        assert!(matches!(
            harness.runtime.ensure_no_active_game(),
            Err(RuntimeError::GameActive)
        ));

        assert_eq!(
            failure_code(&harness.service.launch_game(GameId(1), None).await),
            LaunchErrorCode::GameAlreadyRunning
        );
        assert!(harness.service.get_launch_state().running.is_some());
    }

    #[tokio::test]
    async fn a_clean_exit_closes_the_session_and_returns_a_stable_state() {
        let harness = Harness::build(SystemId::Nes, Vec::new()).await;
        harness.set_app_run("sleep 0.4; exit 0");

        let started = harness.service.launch_game(GameId(1), None).await;
        let session = started_session(&started);

        assert_eq!(
            harness.await_outcome(session).await,
            PlaySessionOutcome::Completed
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        while harness.service.get_launch_state().running.is_some() && Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(harness.service.get_launch_state().running.is_none());
        assert!(read_process_record(&harness.runtime.paths)
            .unwrap()
            .is_none());
        harness.runtime.ensure_no_active_game().unwrap();
        assert!(harness
            .events
            .events
            .lock()
            .unwrap()
            .iter()
            .any(|event| event.exited.is_some()));
    }

    #[tokio::test]
    async fn a_non_zero_exit_after_a_successful_start_is_recorded_as_a_crash() {
        let harness = Harness::build(SystemId::Nes, Vec::new()).await;
        harness.set_app_run("sleep 0.4; exit 4");

        let started = harness.service.launch_game(GameId(1), None).await;
        let session = started_session(&started);

        assert_eq!(
            harness.await_outcome(session).await,
            PlaySessionOutcome::Crashed
        );
        assert_eq!(
            harness
                .launch_repository
                .session(session)
                .await
                .unwrap()
                .unwrap()
                .exit_code,
            Some(4)
        );
    }

    #[tokio::test]
    async fn an_immediate_exit_is_reported_as_an_early_launch_exit() {
        let harness = Harness::build(SystemId::Nes, Vec::new()).await;
        harness.set_app_run("exit 9");

        let failure = harness.service.launch_game(GameId(1), None).await;

        assert_eq!(
            failure_code(&failure),
            LaunchErrorCode::ProcessExitedDuringLaunch
        );
        if let LaunchResponse::Failed { error } = &failure {
            assert_eq!(error.context.exit_code, Some(9));
        }
        let sessions = harness.launch_repository.open_sessions().await.unwrap();
        assert!(sessions.is_empty());
        assert_eq!(
            harness
                .launch_repository
                .session(PlaySessionId(1))
                .await
                .unwrap()
                .unwrap()
                .outcome,
            PlaySessionOutcome::FailedToStart
        );
        assert!(read_process_record(&harness.runtime.paths)
            .unwrap()
            .is_none());
        harness.runtime.ensure_no_active_game().unwrap();
        assert!(harness.service.get_launch_state().running.is_none());
    }

    #[tokio::test]
    async fn a_spawn_failure_leaves_no_record_and_no_open_session() {
        let harness = Harness::build(SystemId::Nes, Vec::new()).await;
        fs::write(&harness.app_run, "not an executable").unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&harness.app_run, fs::Permissions::from_mode(0o644)).unwrap();

        let failure = harness.service.launch_game(GameId(1), None).await;

        assert_eq!(failure_code(&failure), LaunchErrorCode::SpawnFailed);
        assert!(harness
            .launch_repository
            .open_sessions()
            .await
            .unwrap()
            .is_empty());
        assert_eq!(
            harness
                .launch_repository
                .session(PlaySessionId(1))
                .await
                .unwrap()
                .unwrap()
                .outcome,
            PlaySessionOutcome::FailedToStart
        );
        assert!(read_process_record(&harness.runtime.paths)
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn a_restart_with_no_surviving_process_interrupts_open_sessions() {
        let harness = Harness::build(SystemId::Nes, Vec::new()).await;
        harness.set_app_run("sleep 5");
        let started = harness.service.launch_game(GameId(1), None).await;
        let session = started_session(&started);

        // A restart: the record is gone because RuntimeManager proved the process dead.
        fs::remove_file(harness.runtime.paths.game_process_record()).unwrap();
        let restarted = harness.rebuild();

        let state = restarted.reconcile_on_startup().await.unwrap();

        assert!(state.running.is_none());
        assert!(!state.blocked);
        assert_eq!(
            harness
                .launch_repository
                .session(session)
                .await
                .unwrap()
                .unwrap()
                .outcome,
            PlaySessionOutcome::Interrupted
        );
    }

    #[tokio::test]
    async fn a_restart_with_a_live_child_keeps_the_session_running_and_mutation_blocked() {
        let harness = Harness::build(SystemId::Nes, Vec::new()).await;
        harness.set_app_run("sleep 5");
        let started = harness.service.launch_game(GameId(1), None).await;
        let session = started_session(&started);

        let restarted = harness.rebuild();
        let state = restarted.reconcile_on_startup().await.unwrap();

        assert_eq!(
            state.running.map(|running| running.session_id),
            Some(session)
        );
        assert!(!state.blocked);
        assert_eq!(
            harness
                .launch_repository
                .session(session)
                .await
                .unwrap()
                .unwrap()
                .outcome,
            PlaySessionOutcome::Running
        );
        assert!(matches!(
            harness.runtime.ensure_no_active_game(),
            Err(RuntimeError::GameActive)
        ));
        assert_eq!(
            failure_code(&restarted.launch_game(GameId(1), None).await),
            LaunchErrorCode::GameAlreadyRunning
        );
    }

    #[tokio::test]
    async fn an_uncertain_process_record_blocks_launches_without_deleting_anything() {
        let harness = Harness::build(SystemId::Nes, Vec::new()).await;
        // A record this binary cannot interpret is uncertainty, not proof of absence.
        fs::write(
            harness.runtime.paths.game_process_record(),
            serde_json::to_vec(&serde_json::json!({
                "schema_version": 99,
                "phase": "running",
                "launch_id": "launch-1",
                "play_session_id": 1,
                "boot_id": "unknown",
                "installation_id": "install-1",
                "expected_apprun_path": "/tmp/install-1/AppRun"
            }))
            .unwrap(),
        )
        .unwrap();

        let state = harness.service.reconcile_on_startup().await.unwrap();

        assert!(state.blocked);
        assert!(state.running.is_none());
        assert!(harness.runtime.paths.game_process_record().exists());
        assert_eq!(
            failure_code(&harness.service.launch_game(GameId(1), None).await),
            LaunchErrorCode::GameAlreadyRunning
        );
    }

    #[tokio::test]
    async fn simultaneous_launch_requests_start_at_most_one_game() {
        let harness = Harness::build(SystemId::Nes, Vec::new()).await;
        harness.set_app_run("sleep 5");
        let first = harness.service.clone();
        let second = harness.service.clone();

        let (left, right) = tokio::join!(
            async move { first.launch_game(GameId(1), None).await },
            async move { second.launch_game(GameId(1), None).await }
        );

        let responses = [left, right];
        let started = responses
            .iter()
            .filter(|response| matches!(response, LaunchResponse::Started { .. }))
            .count();
        assert_eq!(started, 1);
        let rejected = responses
            .iter()
            .find(|response| matches!(response, LaunchResponse::Failed { .. }))
            .unwrap();
        assert_eq!(failure_code(rejected), LaunchErrorCode::GameAlreadyRunning);
        assert_eq!(
            harness
                .launch_repository
                .open_sessions()
                .await
                .unwrap()
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn a_monitor_that_cannot_observe_the_exit_keeps_the_record_until_absence_is_proven() {
        let launcher = Arc::new(FaultyLauncher::default());
        // Neither waiting nor polling can observe this child, so its death is never provable from
        // the process handle alone.
        launcher.faults.fail_wait(true);
        launcher.faults.fail_try_exit(true);
        let harness = Harness::build_with(SystemId::Nes, Vec::new(), launcher.clone()).await;
        harness.set_app_run_until_stopped();

        let session = started_session(&harness.service.launch_game(GameId(1), None).await);
        harness.await_blocked(&harness.service).await;

        // Uncertainty is fail-closed: the durable record survives, runtime mutation stays
        // blocked, a second launch is refused, and the session is not closed.
        let record = read_process_record(&harness.runtime.paths)
            .unwrap()
            .unwrap();
        assert_eq!(record.phase, ManagedProcessPhase::Running);
        assert_eq!(record.play_session_id, session.0);
        assert!(matches!(
            harness.runtime.ensure_no_active_game(),
            Err(RuntimeError::GameActive)
        ));
        assert_eq!(
            failure_code(&harness.service.launch_game(GameId(1), None).await),
            LaunchErrorCode::GameAlreadyRunning
        );
        assert_eq!(
            harness
                .launch_repository
                .session(session)
                .await
                .unwrap()
                .unwrap()
                .outcome,
            PlaySessionOutcome::Running
        );

        // Only proven absence releases the record, the session, and the next launch.
        harness.stop();
        assert_eq!(
            harness.await_outcome(session).await,
            PlaySessionOutcome::Interrupted
        );
        harness.await_unblocked(&harness.service).await;
        assert!(read_process_record(&harness.runtime.paths)
            .unwrap()
            .is_none());
        harness.runtime.ensure_no_active_game().unwrap();

        launcher.faults.fail_wait(false);
        launcher.faults.fail_try_exit(false);
        harness.set_app_run("sleep 5");
        assert!(matches!(
            harness.service.launch_game(GameId(1), None).await,
            LaunchResponse::Started { .. }
        ));
    }

    #[tokio::test]
    async fn a_child_that_cannot_be_terminated_keeps_the_launching_record_and_the_open_session() {
        let launcher = Arc::new(FaultyLauncher::default());
        launcher.faults.fail_terminate(true);
        let harness = Harness::build_with(SystemId::Nes, Vec::new(), launcher.clone()).await;
        harness.set_app_run_until_stopped();

        // Break the Running-record transition the moment the child exists: the AppRun no longer
        // resolves inside the managed installation, so its identity cannot be durably recorded.
        let app_directory = harness.app_run.parent().unwrap().to_path_buf();
        let hidden = app_directory.with_file_name("app-hidden");
        let (from, to) = (app_directory.clone(), hidden.clone());
        let started = harness.started_file.clone();
        launcher.on_spawn(move || {
            for _ in 0..1000 {
                if started.exists() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            fs::rename(&from, &to).unwrap();
        });

        let failure = harness.service.launch_game(GameId(1), None).await;
        assert_eq!(
            failure_code(&failure),
            LaunchErrorCode::ProcessIdentityFailed
        );

        // The child could not be proven dead, so the pre-spawn record stays exactly as written.
        let record = read_process_record(&harness.runtime.paths)
            .unwrap()
            .unwrap();
        assert_eq!(record.phase, ManagedProcessPhase::Launching);
        assert!(record.pid.is_none());
        assert!(matches!(
            harness.runtime.ensure_no_active_game(),
            Err(RuntimeError::GameActive)
        ));
        assert!(harness.service.get_launch_state().blocked);
        assert_eq!(
            failure_code(&harness.service.launch_game(GameId(1), None).await),
            LaunchErrorCode::GameAlreadyRunning
        );
        // No closed failed-to-start session is claimed while the child may still be alive.
        assert_eq!(
            harness
                .launch_repository
                .session(PlaySessionId(1))
                .await
                .unwrap()
                .unwrap()
                .outcome,
            PlaySessionOutcome::Running
        );

        // This is exactly the durable state a restart reconciles from: a surviving pre-spawn
        // record naming an open session, which `reconcile_on_startup` reads as blocked.
        assert_eq!(record.play_session_id, 1);
        assert!(!harness
            .launch_repository
            .open_sessions()
            .await
            .unwrap()
            .is_empty());

        // Once the child is proven gone, the record and the session both reconcile.
        harness.stop();
        assert_eq!(
            harness.await_outcome(PlaySessionId(1)).await,
            PlaySessionOutcome::FailedToStart
        );
        harness.await_unblocked(&harness.service).await;
        assert!(read_process_record(&harness.runtime.paths)
            .unwrap()
            .is_none());

        // And launching is available again.
        fs::rename(&hidden, &app_directory).unwrap();
        launcher.faults.fail_terminate(false);
        harness.set_app_run("sleep 5");
        assert!(matches!(
            harness.service.launch_game(GameId(1), None).await,
            LaunchResponse::Started { .. }
        ));
    }

    #[tokio::test]
    async fn only_a_core_that_satisfies_the_whole_launch_contract_can_be_persisted() {
        let harness = Harness::build(SystemId::Nes, Vec::new()).await;
        let nestopia = CoreId::new("nestopia").unwrap();

        // Statically approved, authenticated, and verified installed: this is the contract.
        assert_eq!(
            harness
                .service
                .set_core_override(GameId(1), &nestopia)
                .await
                .unwrap()
                .core_id,
            nestopia
        );

        // Approved by the catalog, but the authenticated release does not approve it for this
        // system.
        harness.edit_launch_runtime(|runtime| {
            for core in runtime.cores.values_mut() {
                core.systems.clear();
            }
        });
        assert!(harness
            .service
            .set_core_override(GameId(1), &nestopia)
            .await
            .is_err());
        harness.edit_launch_runtime(|runtime| {
            for core in runtime.cores.values_mut() {
                core.systems = vec![SafeIdentifier::new(SystemId::Nes.as_str()).unwrap()];
            }
        });

        // Approved and authenticated, but not actually installed.
        let core_path = harness.core_path("nestopia");
        fs::remove_file(&core_path).unwrap();
        assert!(harness
            .service
            .set_core_override(GameId(1), &nestopia)
            .await
            .is_err());
        fs::write(&core_path, b"synthetic core").unwrap();

        // Not resolvable to an authenticated managed component at all.
        harness.edit_launch_runtime(|runtime| runtime.cores.clear());
        assert!(harness
            .service
            .set_core_override(GameId(1), &nestopia)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn an_unapproved_core_or_an_unresolved_system_can_never_be_persisted() {
        let approved = Harness::build(SystemId::Nes, Vec::new()).await;
        // A core approved only for another system.
        assert!(approved
            .service
            .set_core_override(GameId(1), &CoreId::new("beetle-psx").unwrap())
            .await
            .is_err());
        // A core no catalog definition claims.
        assert!(approved
            .service
            .set_core_override(GameId(1), &CoreId::new("not-a-core").unwrap())
            .await
            .is_err());

        // A system whose core policy is still unresolved approves nothing.
        let unresolved = Harness::build(SystemId::Nintendo64, Vec::new()).await;
        assert!(unresolved
            .service
            .set_core_override(GameId(1), &CoreId::new("nestopia").unwrap())
            .await
            .is_err());
    }

    #[tokio::test]
    async fn a_persisted_override_is_still_revalidated_at_launch() {
        let harness = Harness::build(SystemId::Nes, Vec::new()).await;
        harness.set_app_run("sleep 5");
        let nestopia = CoreId::new("nestopia").unwrap();
        harness
            .service
            .set_core_override(GameId(1), &nestopia)
            .await
            .unwrap();

        // Validating at persistence time does not make the stored value trustworthy later: the
        // runtime can change underneath it.
        fs::remove_file(harness.core_path("nestopia")).unwrap();

        assert_eq!(
            failure_code(&harness.service.launch_game(GameId(1), None).await),
            LaunchErrorCode::CoreNotInstalled
        );
        assert_eq!(
            harness
                .service
                .core_override(GameId(1))
                .await
                .unwrap()
                .unwrap()
                .core_id,
            nestopia
        );
    }
}
