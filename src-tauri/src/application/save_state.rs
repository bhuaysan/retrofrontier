//! The Save-State application service.
//!
//! It owns four things: the reconciliation that turns a controlled launch's filesystem delta into
//! proved provenance, the bounded projection Game Detail lists, the controlled load, and the safe
//! delete. It owns no launch pipeline of its own — a save-state load goes through the same
//! `LaunchApplicationService` an ordinary launch does — and no filesystem safety of its own, which
//! belongs to `crate::services::save_state_fs`.
//!
//! ## The one rule everything here serves
//!
//! > RetroFrontier never derives trusted Save-State provenance from a filename. A managed Save
//! > State exists because a controlled launch proves its provenance and RetroFrontier subsequently
//! > verifies the exact physical state content.
//!
//! And for the destructive half:
//!
//! > A `SaveStateId` never directly authorizes a path. It identifies the expected domain object;
//! > the backend must prove the exact current filesystem target again before acting.
//!
//! ## Why the launch dependency is set late
//!
//! The launch service requires a `SaveStateLifecycle` — a launch with no durable baseline is a
//! launch whose save states could never be attributed — while only *loading* a save state needs
//! the launch service. The cycle is therefore broken on this side: the port is attached after both
//! services exist, and until it is, `is_managed_session_active` answers **true**, so an
//! unattached service refuses every mutation instead of performing an unguarded one.

use crate::application::launch::{
    BaselineRequest, LaunchApplicationService, SaveStateLaunchPlan, SaveStateLifecycle,
};
use crate::application::runtime_manager::{AuthenticatedCoreBinary, RuntimeManager};
use crate::domain::launch::{LaunchResponse, PlaySessionId};
use crate::domain::library::{ContentUnitAvailability, GameAvailability, GameId, UnixTimestamp};
use crate::domain::runtime::{RelativePath, RuntimeError, SafeIdentifier, Sha256Digest};
use crate::domain::save_state::{
    save_state_thumbnail_reference, DeleteSaveStateResponse, LaunchStateBaseline,
    LoadSaveStateResponse, SaveState, SaveStateCapabilities, SaveStateError, SaveStateFileIdentity,
    SaveStateId, SaveStateLoadability, SaveStateProvenance, SaveStateSlot,
    SaveStateThumbnailIdentity, SaveStateView,
};
use crate::error::AppError;
use crate::repositories::launch::LaunchRepository;
use crate::repositories::library::LibraryRepository;
use crate::repositories::save_state::{NewSaveState, RefreshedSaveState, SaveStateRepository};
use crate::services::save_state_fs::{
    delete_verified_managed_file, hash_managed_file, managed_file_matches_size,
    parse_state_candidate, snapshot_state_tree, state_tree_delta, thumbnail_relative_path,
    PollingStabilityProbe, StabilityProbe, StateCandidate, StateTreeSnapshot, VerifiedStateFile,
};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

/// The runtime facts a Save State needs. Production uses `RuntimeManager`; the boundary exists so
/// the service can be tested without an installed runtime tree.
pub trait SaveStateRuntime: Send + Sync {
    /// The authoritative lookup an actual load must pass.
    fn locate_authenticated_core_binary(
        &self,
        component_id: &SafeIdentifier,
        binary_sha256: Sha256Digest,
    ) -> Result<AuthenticatedCoreBinary, RuntimeError>;

    /// The cheap capability snapshot the listing may use. Never an authorization.
    fn declares_authenticated_core_binary(
        &self,
        component_id: &SafeIdentifier,
        binary_sha256: Sha256Digest,
    ) -> bool;
}

impl SaveStateRuntime for RuntimeManager {
    fn locate_authenticated_core_binary(
        &self,
        component_id: &SafeIdentifier,
        binary_sha256: Sha256Digest,
    ) -> Result<AuthenticatedCoreBinary, RuntimeError> {
        RuntimeManager::locate_authenticated_core_binary(self, component_id, binary_sha256)
    }

    fn declares_authenticated_core_binary(
        &self,
        component_id: &SafeIdentifier,
        binary_sha256: Sha256Digest,
    ) -> bool {
        RuntimeManager::declares_authenticated_core_binary(self, component_id, binary_sha256)
    }
}

/// The launch capabilities a Save State needs, attached after construction.
#[async_trait::async_trait]
pub trait SaveStateLaunchPort: Send + Sync {
    /// Whether a managed RetroArch session is launching, running, or of uncertain identity.
    fn is_managed_session_active(&self) -> bool;
    async fn launch_save_state(&self, plan: SaveStateLaunchPlan) -> LaunchResponse;
}

#[async_trait::async_trait]
impl SaveStateLaunchPort for LaunchApplicationService {
    fn is_managed_session_active(&self) -> bool {
        LaunchApplicationService::is_managed_session_active(self)
    }

    async fn launch_save_state(&self, plan: SaveStateLaunchPlan) -> LaunchResponse {
        LaunchApplicationService::launch_save_state(self, plan).await
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SaveStateConfig {
    /// How many times one baseline may reconcile without reaching a deterministic outcome before
    /// it is dropped. Bounded so a permanently indeterminate baseline cannot leak forever.
    pub max_reconciliation_attempts: u32,
    /// How many files a pre-launch state tree may contain. A larger tree fails the launch closed
    /// rather than recording a baseline RetroFrontier cannot reason about.
    pub max_baseline_entries: usize,
}

impl Default for SaveStateConfig {
    fn default() -> Self {
        Self {
            max_reconciliation_attempts: 3,
            max_baseline_entries: 20_000,
        }
    }
}

#[derive(Clone)]
pub struct SaveStateApplicationService {
    save_states: SaveStateRepository,
    library: LibraryRepository,
    sessions: LaunchRepository,
    runtime: Arc<dyn SaveStateRuntime>,
    stability: Arc<dyn StabilityProbe>,
    states_root: PathBuf,
    config: SaveStateConfig,
    launch: Arc<OnceLock<Arc<dyn SaveStateLaunchPort>>>,
}

impl SaveStateApplicationService {
    pub fn new(
        save_states: SaveStateRepository,
        library: LibraryRepository,
        sessions: LaunchRepository,
        runtime: Arc<dyn SaveStateRuntime>,
        states_root: impl Into<PathBuf>,
        config: SaveStateConfig,
    ) -> Self {
        Self {
            save_states,
            library,
            sessions,
            runtime,
            stability: Arc::new(PollingStabilityProbe::default()),
            states_root: states_root.into(),
            config,
            launch: Arc::new(OnceLock::new()),
        }
    }

    /// Replace the stability probe. Tests use this to make both outcomes reachable without
    /// sleeping; production always keeps the polling probe.
    #[cfg(test)]
    pub fn with_stability_probe(mut self, probe: Arc<dyn StabilityProbe>) -> Self {
        self.stability = probe;
        self
    }

    /// Complete the cycle once both services exist. Calling it twice is a no-op.
    pub fn attach_launch(&self, launch: Arc<dyn SaveStateLaunchPort>) {
        let _ = self.launch.set(launch);
    }

    /// Fails closed: with no launch port attached, every mutation is refused rather than performed
    /// without the one guard that keeps it away from a running emulator.
    fn managed_session_active(&self) -> bool {
        self.launch
            .get()
            .map(|launch| launch.is_managed_session_active())
            .unwrap_or(true)
    }

    // ------------------------------------------------------------------ listing

    /// The bounded Save-State projection Game Detail renders.
    ///
    /// Only `available`, proved states appear, most recently updated first. Each row is
    /// re-checked cheaply — managed-root containment, no-follow regular file, size — and a state
    /// whose file is provably gone or the wrong size transitions to `missing` and is dropped from
    /// the result. A same-size tamper survives that check and is caught by the full digest
    /// verification an actual load or delete performs.
    pub async fn list_save_states(&self, game_id: GameId) -> Result<Vec<SaveStateView>, AppError> {
        let states = self.save_states.save_states_for_game(game_id).await?;
        if states.is_empty() {
            return Ok(Vec::new());
        }
        let blocked = self.managed_session_active();
        let labels = self.content_unit_labels(game_id).await?;
        // One lookup per distinct binary rather than one per state: the same core usually produced
        // every state of a game.
        let mut core_available: BTreeMap<(SafeIdentifier, Sha256Digest), bool> = BTreeMap::new();

        let mut views = Vec::with_capacity(states.len());
        for state in states {
            if let Err(error) = managed_file_matches_size(
                &self.states_root,
                &state.state.relative_path,
                state.state.size_bytes,
            ) {
                tracing::info!(
                    save_state_id = %state.id,
                    game_id = %state.provenance.game_id,
                    slot = state.slot.get(),
                    error = %error,
                    "a registered save-state file no longer matches; the state is now missing"
                );
                self.save_states.mark_missing(state.id).await?;
                continue;
            }

            let key = (
                state.provenance.core_component_id.clone(),
                state.provenance.core_binary_sha256,
            );
            let core_present = *core_available.entry(key).or_insert_with(|| {
                self.runtime.declares_authenticated_core_binary(
                    &state.provenance.core_component_id,
                    state.provenance.core_binary_sha256,
                )
            });

            // `loadable` and `deletable` are independent, and both are snapshots. A blocked
            // session refuses *both* mutations, because M9 performs no Save-State load or delete
            // while a managed game is launching, running, or uncertain.
            let loadability = if blocked {
                SaveStateLoadability::TemporarilyBlocked
            } else if !core_present {
                SaveStateLoadability::CoreUnavailable
            } else {
                SaveStateLoadability::Ready
            };
            views.push(self.view(state, &labels, loadability, !blocked));
        }
        Ok(views)
    }

    /// A disc label appears exactly when it disambiguates: a single-unit game needs none.
    async fn content_unit_labels(
        &self,
        game_id: GameId,
    ) -> Result<BTreeMap<i64, String>, AppError> {
        let units = self.library.game_content_units(game_id).await?;
        if units.len() < 2 {
            return Ok(BTreeMap::new());
        }
        Ok(units
            .into_iter()
            .map(|unit| (unit.id.0, unit.local_title))
            .collect())
    }

    fn view(
        &self,
        state: SaveState,
        labels: &BTreeMap<i64, String>,
        loadability: SaveStateLoadability,
        deletable: bool,
    ) -> SaveStateView {
        SaveStateView {
            id: state.id,
            game_id: state.provenance.game_id,
            content_unit_id: state.provenance.content_unit_id,
            slot: state.slot,
            core_id: state.provenance.core_id.clone(),
            core_display_version: state.provenance.core_display_version.clone(),
            core_source_revision: state.provenance.core_source_revision.clone(),
            content_unit_label: labels.get(&state.provenance.content_unit_id.0).cloned(),
            created_at: state.created_at,
            updated_at: state.updated_at,
            // An opaque protocol reference, never a filesystem path. The protocol handler
            // re-verifies the thumbnail before any byte leaves Rust.
            thumbnail_ref: state
                .thumbnail
                .as_ref()
                .map(|_| save_state_thumbnail_reference(state.id)),
            capabilities: SaveStateCapabilities {
                loadability,
                deletable,
            },
        }
    }

    // ------------------------------------------------------------------ load

    /// Load one Save State through the shared managed launch pipeline.
    ///
    /// Nothing but the `SaveStateId` comes from the caller. Every fact is resolved from durable
    /// provenance and then re-proved against the current filesystem and the current trust state
    /// before a process is created.
    pub async fn load_save_state(&self, id: SaveStateId) -> LoadSaveStateResponse {
        match self.prepare_load(id).await {
            Ok(plan) => {
                let Some(launch) = self.launch.get() else {
                    return LoadSaveStateResponse::refused(SaveStateError::LaunchFailed);
                };
                match launch.launch_save_state(plan).await {
                    LaunchResponse::Started {
                        session,
                        diagnostics,
                    } => {
                        tracing::info!(
                            save_state_id = %id,
                            play_session_id = %session.session_id,
                            game_id = %session.game_id,
                            "a save state was loaded"
                        );
                        LoadSaveStateResponse::Started {
                            session,
                            diagnostics,
                        }
                    }
                    LaunchResponse::Failed { error } => {
                        // The launch pipeline's own verdict. It says nothing about the Save State
                        // itself, so no provenance and no digest is touched here.
                        tracing::info!(
                            save_state_id = %id,
                            code = error.code.as_str(),
                            "a save-state launch was not started"
                        );
                        LoadSaveStateResponse::LaunchFailed { error }
                    }
                    // Unreachable: the content unit is recorded provenance, so the pipeline is
                    // never asked to choose. Reporting it as unavailable is the honest fallback.
                    LaunchResponse::ContentSelectionRequired { .. } => {
                        LoadSaveStateResponse::refused(SaveStateError::Unavailable)
                    }
                }
            }
            Err(code) => {
                tracing::info!(
                    save_state_id = %id,
                    code = code.as_str(),
                    "a save-state load was refused"
                );
                LoadSaveStateResponse::refused(code)
            }
        }
    }

    /// Re-prove everything a load depends on, in order.
    async fn prepare_load(&self, id: SaveStateId) -> Result<SaveStateLaunchPlan, SaveStateError> {
        let state = self.verified_state(id).await?;

        // The game and its exact recorded content unit must still be there and usable. A
        // save-state load never substitutes another unit — a Disc 1 state is never Disc 2.
        let game = self
            .library
            .game(state.provenance.game_id)
            .await
            .map_err(storage)?
            .ok_or(SaveStateError::Unavailable)?;
        if game.availability != GameAvailability::Available {
            return Err(SaveStateError::Unavailable);
        }
        let unit = self
            .library
            .game_content_units(state.provenance.game_id)
            .await
            .map_err(storage)?
            .into_iter()
            .find(|unit| unit.id == state.provenance.content_unit_id)
            .ok_or(SaveStateError::Unavailable)?;
        if unit.availability != ContentUnitAvailability::Available {
            return Err(SaveStateError::Unavailable);
        }

        // The exact historical core binary, in a currently installed, authenticated, allowed
        // installation. A revoked, blocked, or below-floor component never satisfies this, and
        // there is deliberately no fallback to the game's current core.
        let core = self
            .runtime
            .locate_authenticated_core_binary(
                &state.provenance.core_component_id,
                state.provenance.core_binary_sha256,
            )
            .map_err(|error| {
                tracing::info!(
                    save_state_id = %id,
                    core_id = %state.provenance.core_id,
                    core_component_id = %state.provenance.core_component_id,
                    error = %error,
                    "the exact historical core binary is unavailable"
                );
                SaveStateError::CoreUnavailable
            })?;

        // Last, because it is the most likely to have changed while the checks above ran, and
        // because the launch pipeline re-checks it under the mutation lock anyway.
        if self.managed_session_active() {
            return Err(SaveStateError::TemporarilyBlocked);
        }

        Ok(SaveStateLaunchPlan {
            save_state_id: state.id,
            game_id: state.provenance.game_id,
            content_unit_id: state.provenance.content_unit_id,
            core,
            slot: state.slot,
        })
    }

    // ------------------------------------------------------------------ delete

    /// Delete one Save State, after re-proving the exact current filesystem target.
    ///
    /// `deletable` is independent of `loadable`: a state whose historical core is gone is still
    /// safely deletable, because deleting needs only the file, not the emulator.
    pub async fn delete_save_state(&self, id: SaveStateId) -> DeleteSaveStateResponse {
        match self.delete_verified(id).await {
            Ok(()) => DeleteSaveStateResponse::Deleted { save_state_id: id },
            Err(code) => {
                tracing::info!(
                    save_state_id = %id,
                    code = code.as_str(),
                    "a save-state delete was refused"
                );
                DeleteSaveStateResponse::failed(code)
            }
        }
    }

    async fn delete_verified(&self, id: SaveStateId) -> Result<(), SaveStateError> {
        if self.managed_session_active() {
            return Err(SaveStateError::TemporarilyBlocked);
        }
        let state = self.verified_state(id).await?;

        // The filesystem delete is the irreversible primary action, so it happens once every
        // check has passed, and the lifecycle row is persisted afterwards. There is deliberately
        // no attempt to "roll back" by recreating guessed bytes.
        delete_verified_managed_file(
            &self.states_root,
            &state.state.relative_path,
            state.state.size_bytes,
            state.state.sha256,
        )?;

        // A verified thumbnail goes with its state. One that can no longer be verified is left
        // exactly where it is: safe deletion of the state must not be sacrificed because
        // thumbnail cleanup could not be proved.
        let thumbnail_removed = match state.thumbnail.as_ref() {
            None => true,
            Some(thumbnail) => match delete_verified_managed_file(
                &self.states_root,
                &thumbnail.relative_path,
                thumbnail.size_bytes,
                thumbnail.sha256,
            ) {
                Ok(()) => true,
                Err(error) => {
                    tracing::warn!(
                        save_state_id = %id,
                        error = %error,
                        "the save state was deleted but its thumbnail could not be verified, so \
                         it was left untouched"
                    );
                    false
                }
            },
        };

        // If this fails, the row still says `available` while its file is gone. The next listing
        // or reconciliation re-verifies and converges on the physical truth, which is why the
        // ordering is safe.
        self.save_states
            .mark_deleted(id, thumbnail_removed)
            .await
            .map_err(|error| {
                tracing::warn!(
                    save_state_id = %id,
                    error = %error,
                    "the save state was deleted from disk but its lifecycle could not be persisted"
                );
                SaveStateError::DeleteFailed
            })?;
        tracing::info!(
            save_state_id = %id,
            game_id = %state.provenance.game_id,
            content_unit_id = %state.provenance.content_unit_id,
            core_id = %state.provenance.core_id,
            slot = state.slot.get(),
            thumbnail_removed,
            "a save state was deleted"
        );
        Ok(())
    }

    /// The shared re-proof both mutations start from.
    ///
    /// A digest or size mismatch means the *registered identity* is gone, not that the new bytes
    /// are corrupt — so the row leaves `available`, the untrusted file is left exactly as it is,
    /// and the old `SaveStateId` can no longer load or delete anything.
    async fn verified_state(&self, id: SaveStateId) -> Result<SaveState, SaveStateError> {
        let state = self
            .save_states
            .save_state(id)
            .await
            .map_err(storage)?
            .ok_or(SaveStateError::NotFound)?;
        if !state.status.is_available() {
            return Err(SaveStateError::Unavailable);
        }
        match crate::services::save_state_fs::verify_managed_file(
            &self.states_root,
            &state.state.relative_path,
            state.state.size_bytes,
            state.state.sha256,
        ) {
            Ok(_) => Ok(state),
            Err(error) => {
                tracing::info!(
                    save_state_id = %id,
                    error = %error,
                    "the registered save-state identity no longer matches; the file was left \
                     untouched"
                );
                if let Err(error) = self.save_states.mark_missing(id).await {
                    error.log();
                }
                Err(error)
            }
        }
    }

    // ------------------------------------------------------------------ reconciliation

    /// Reconcile every persisted baseline whose session has already certainly ended.
    ///
    /// This is what makes a baseline survive a RetroFrontier crash mid-session: the process is
    /// adopted by the launch service, and once it is proven gone the baseline is still here.
    pub async fn reconcile_on_startup(&self) -> Result<usize, AppError> {
        let pending = self.save_states.baselines_awaiting_reconciliation().await?;
        for session_id in &pending {
            self.reconcile(*session_id).await;
        }
        if !pending.is_empty() {
            tracing::info!(
                sessions = pending.len(),
                "save-state baselines were reconciled after a restart"
            );
        }
        Ok(pending.len())
    }

    /// Reconcile one session. Retryable and idempotent.
    async fn reconcile(&self, session_id: PlaySessionId) {
        if let Err(error) = self.reconcile_session_inner(session_id).await {
            tracing::warn!(
                play_session_id = %session_id,
                code = error.as_str(),
                "save-state reconciliation did not complete"
            );
        }
    }

    async fn reconcile_session_inner(
        &self,
        session_id: PlaySessionId,
    ) -> Result<(), SaveStateError> {
        // No baseline means either this session was already reconciled or it never had one.
        // Either way there is nothing to attribute, which is what makes a replay a no-op.
        let Some(baseline) = self
            .save_states
            .baseline(session_id)
            .await
            .map_err(storage)?
        else {
            return Ok(());
        };

        // **The fail-closed gate.** A session is closed only after its process end was certainly
        // observed — positively reaped, or independently proven absent. While it is open the
        // process may be alive or of uncertain identity, and M9 performs no attribution and no
        // destructive reconciliation in either case. The baseline is kept, so the retry after the
        // end is observed still has its "before".
        let session = self
            .sessions
            .session(session_id)
            .await
            .map_err(storage)?
            .ok_or(SaveStateError::ReconciliationFailed)?;
        if session.outcome.is_open() {
            return Ok(());
        }

        let observed = self.observe_delta(&baseline);

        let mut indeterminate = observed.indeterminate;
        for candidate in &observed.candidates {
            if let Err(error) = self.persist_candidate(&baseline, candidate).await {
                // One bad candidate must not discard the others: each is an independent proof.
                tracing::warn!(
                    play_session_id = %session_id,
                    code = error.as_str(),
                    "one save-state candidate could not be persisted"
                );
                indeterminate = true;
            }
        }

        // Absence is only provable from a complete enumeration.
        if observed.snapshot.is_complete() {
            self.mark_absent_states_missing(&observed.snapshot).await?;
        } else {
            indeterminate = true;
        }

        if indeterminate {
            let attempts = self
                .save_states
                .increment_baseline_attempts(session_id)
                .await
                .map_err(storage)?;
            if attempts < self.config.max_reconciliation_attempts {
                // Keep the baseline: the next startup reconciliation tries again.
                return Ok(());
            }
            // Bounded, so a permanently indeterminate baseline cannot leak forever. Nothing extra
            // is registered and nothing is deleted; what was proved stays proved.
            self.save_states
                .delete_baseline(session_id)
                .await
                .map_err(storage)?;
            return Err(SaveStateError::ReconciliationFailed);
        }

        self.save_states
            .delete_baseline(session_id)
            .await
            .map_err(storage)?;
        tracing::info!(
            play_session_id = %session_id,
            game_id = %baseline.provenance.game_id,
            content_unit_id = %baseline.provenance.content_unit_id,
            core_id = %baseline.provenance.core_id,
            registered = observed.candidates.len(),
            "save-state reconciliation completed"
        );
        Ok(())
    }

    /// The filesystem half: snapshot, delta, supported candidates, stability, digests, thumbnails.
    fn observe_delta(&self, baseline: &LaunchStateBaseline) -> ObservedDelta {
        let snapshot = snapshot_state_tree(&self.states_root);
        let delta = state_tree_delta(&baseline.entries, &snapshot);
        let changed: std::collections::BTreeSet<&RelativePath> = delta.iter().collect();

        let mut candidates = Vec::new();
        let mut indeterminate = false;
        for relative_path in &delta {
            // Only supported numbered slots are states. A thumbnail in the delta is picked up
            // through the state it belongs to, never on its own, and everything else is ignored.
            let StateCandidate::ManagedSlot(slot) = parse_state_candidate(relative_path) else {
                continue;
            };
            // A pathname existing is not evidence RetroArch finished writing it.
            if !self.stability.is_stable(&self.states_root, relative_path) {
                tracing::info!(
                    relative_path = %relative_path,
                    "a save-state candidate was not stable after the process ended; it was left \
                     unregistered and untouched"
                );
                indeterminate = true;
                continue;
            }
            let Ok(state) = hash_managed_file(&self.states_root, relative_path) else {
                indeterminate = true;
                continue;
            };

            // A thumbnail is associated only when this session's own delta proves it: the file
            // RetroArch wrote beside the state it just saved. A pre-existing image, a thumbnail
            // belonging to another state, and anything under `screenshots/` can never qualify —
            // the adapter is only ever given the states root.
            let thumbnail = thumbnail_relative_path(relative_path)
                .filter(|path| changed.contains(path))
                .filter(|path| self.stability.is_stable(&self.states_root, path))
                .and_then(|path| hash_managed_file(&self.states_root, &path).ok());

            candidates.push(ProvedCandidate {
                slot,
                state,
                thumbnail,
            });
        }

        ObservedDelta {
            snapshot,
            candidates,
            indeterminate,
        }
    }

    async fn persist_candidate(
        &self,
        baseline: &LaunchStateBaseline,
        candidate: &ProvedCandidate,
    ) -> Result<(), SaveStateError> {
        let provenance = baseline.provenance.clone();
        let new = NewSaveState {
            provenance: provenance.clone(),
            slot: candidate.slot,
            state: SaveStateFileIdentity {
                relative_path: candidate.state.relative_path.clone(),
                sha256: candidate.state.sha256,
                size_bytes: candidate.state.size_bytes,
            },
            thumbnail: candidate
                .thumbnail
                .as_ref()
                .map(|thumbnail| SaveStateThumbnailIdentity {
                    relative_path: thumbnail.relative_path.clone(),
                    sha256: thumbnail.sha256,
                    size_bytes: thumbnail.size_bytes,
                }),
        };

        let existing = self
            .save_states
            .available_state_at_path(&candidate.state.relative_path)
            .await
            .map_err(storage)?;

        match existing {
            // Already reconciled. A replay must change nothing at all, `updated_at` included.
            Some(existing)
                if existing.state.sha256 == candidate.state.sha256
                    && existing.provenance.play_session_id == provenance.play_session_id =>
            {
                Ok(())
            }
            // The same core binary overwrote its own slot. The object keeps its identity and its
            // immutable core provenance and moves onto the newly proved content: this change is
            // *explained* by a controlled launch, which is exactly what distinguishes it from the
            // unexplained change that invalidates a registered identity.
            Some(existing)
                if existing.provenance.core_binary_sha256 == provenance.core_binary_sha256 =>
            {
                self.save_states
                    .refresh_state(
                        existing.id,
                        &RefreshedSaveState {
                            play_session_id: provenance.play_session_id,
                            state: new.state.clone(),
                            thumbnail: new.thumbnail.clone(),
                        },
                    )
                    .await
                    .map_err(storage)?;
                tracing::info!(
                    save_state_id = %existing.id,
                    play_session_id = %provenance.play_session_id,
                    slot = candidate.slot.get(),
                    "a save state was updated in place by the same core binary"
                );
                Ok(())
            }
            // A *different* core binary now occupies the same physical path. The old object's
            // provenance is never rewritten: it becomes history and a new object is created with
            // its own immutable provenance and digest.
            Some(existing) => {
                self.save_states
                    .mark_superseded(existing.id)
                    .await
                    .map_err(storage)?;
                let registered = self
                    .save_states
                    .register_state(&new)
                    .await
                    .map_err(storage)?;
                tracing::info!(
                    superseded_save_state_id = %existing.id,
                    save_state_id = %registered.id,
                    slot = candidate.slot.get(),
                    "a save state was superseded by one from a different core binary"
                );
                Ok(())
            }
            None => {
                let registered = self
                    .save_states
                    .register_state(&new)
                    .await
                    .map_err(storage)?;
                tracing::info!(
                    save_state_id = %registered.id,
                    play_session_id = %provenance.play_session_id,
                    game_id = %provenance.game_id,
                    content_unit_id = %provenance.content_unit_id,
                    core_id = %provenance.core_id,
                    core_component_id = %provenance.core_component_id,
                    runtime_release_id = %provenance.originating_runtime_release_id,
                    slot = candidate.slot.get(),
                    thumbnail = registered.thumbnail.is_some(),
                    "a save state was registered"
                );
                Ok(())
            }
        }
    }

    /// Every registered state the completely enumerated tree no longer contains is provably gone.
    ///
    /// Nothing looks for a similarly named replacement: a new file at the same path is not
    /// automatically the same Save State, and needs its own provable attribution.
    async fn mark_absent_states_missing(
        &self,
        snapshot: &StateTreeSnapshot,
    ) -> Result<(), SaveStateError> {
        for state in self.save_states.available_states().await.map_err(storage)? {
            if snapshot.contains(&state.state.relative_path) {
                continue;
            }
            if self
                .save_states
                .mark_missing(state.id)
                .await
                .map_err(storage)?
            {
                tracing::info!(
                    save_state_id = %state.id,
                    game_id = %state.provenance.game_id,
                    slot = state.slot.get(),
                    "a registered save-state file is gone; the state is now missing"
                );
            }
        }
        Ok(())
    }

    /// The verified thumbnail bytes of one Save State, for the native media protocol.
    ///
    /// Resolved from durable state by identity, never from a caller-supplied path, and re-verified
    /// in full before any byte leaves Rust.
    pub async fn verified_thumbnail(
        &self,
        id: SaveStateId,
    ) -> Result<(PathBuf, u64), SaveStateError> {
        let state = self
            .save_states
            .save_state(id)
            .await
            .map_err(storage)?
            .ok_or(SaveStateError::NotFound)?;
        if !state.status.is_available() {
            return Err(SaveStateError::Unavailable);
        }
        let thumbnail = state.thumbnail.ok_or(SaveStateError::NotFound)?;
        crate::services::save_state_fs::verify_managed_file(
            &self.states_root,
            &thumbnail.relative_path,
            thumbnail.size_bytes,
            thumbnail.sha256,
        )?;
        Ok((
            self.states_root.join(thumbnail.relative_path.to_path_buf()),
            thumbnail.size_bytes,
        ))
    }
}

/// The save-state side of one managed launch.
#[async_trait::async_trait]
impl SaveStateLifecycle for SaveStateApplicationService {
    async fn capture_baseline(&self, request: BaselineRequest) -> Result<(), SaveStateError> {
        let snapshot = snapshot_state_tree(&self.states_root);
        // An enumeration that could not describe the whole tree cannot be a baseline: every file
        // it missed would look new afterwards and be attributed to this session.
        if !snapshot.is_complete() {
            tracing::warn!(
                play_session_id = %request.session_id,
                "the save-state tree could not be completely enumerated before the launch"
            );
            return Err(SaveStateError::ReconciliationFailed);
        }
        if snapshot.len() > self.config.max_baseline_entries {
            tracing::warn!(
                play_session_id = %request.session_id,
                entries = snapshot.len(),
                "the save-state tree is larger than RetroFrontier will baseline"
            );
            return Err(SaveStateError::ReconciliationFailed);
        }

        let baseline = LaunchStateBaseline {
            provenance: SaveStateProvenance {
                game_id: request.game_id,
                content_unit_id: request.content_unit_id,
                play_session_id: request.session_id,
                core_id: request.core_id,
                core_component_id: request.core_component_id,
                core_binary_sha256: request.core_binary_sha256,
                core_display_version: request.core_display_version,
                core_source_revision: request.core_source_revision,
                originating_runtime_release_id: request.runtime_release_id,
            },
            runtime_installation_id: request.runtime_installation_id,
            captured_at: now_timestamp(),
            attempts: 0,
            entries: snapshot.to_baseline_entries(),
        };
        self.save_states
            .put_baseline(request.session_id, &baseline)
            .await
            .map_err(|error| {
                error.log();
                SaveStateError::ReconciliationFailed
            })?;
        tracing::debug!(
            play_session_id = %request.session_id,
            entries = baseline.entries.len(),
            "a durable save-state baseline was captured before the launch"
        );
        Ok(())
    }

    async fn discard_baseline(&self, session_id: PlaySessionId) {
        if let Err(error) = self.save_states.delete_baseline(session_id).await {
            error.log();
        }
    }

    async fn reconcile_session(&self, session_id: PlaySessionId) {
        self.reconcile(session_id).await;
    }
}

/// One candidate whose exact content the filesystem phase proved.
#[derive(Debug, Clone)]
struct ProvedCandidate {
    slot: SaveStateSlot,
    state: VerifiedStateFile,
    thumbnail: Option<VerifiedStateFile>,
}

/// What one filesystem observation established.
struct ObservedDelta {
    snapshot: StateTreeSnapshot,
    candidates: Vec<ProvedCandidate>,
    /// Something could not be established with certainty, so the baseline is kept for a retry.
    indeterminate: bool,
}

fn storage(error: AppError) -> SaveStateError {
    error.log();
    SaveStateError::ReconciliationFailed
}

fn now_timestamp() -> UnixTimestamp {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or_default()
}
