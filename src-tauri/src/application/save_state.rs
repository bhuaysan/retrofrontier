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
    BaselineRequest, LaunchApplicationService, LaunchExclusionGuard, SaveStateLaunchPlan,
    SaveStateLifecycle,
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
    /// The same predicate, for a caller that already holds this port's exclusion guard.
    ///
    /// See `LaunchApplicationService::is_running_or_blocked` for why this must be a distinct,
    /// narrower check rather than a call to `is_managed_session_active`.
    fn is_running_or_blocked(&self) -> bool;
    async fn launch_save_state(&self, plan: SaveStateLaunchPlan) -> LaunchResponse;
    /// Enter the same in-process critical section a launch uses, or fail at once (HIGH-1).
    ///
    /// A Save-State delete holds this guard for its whole authorization-to-destructive-action
    /// window, so a managed launch cannot begin — win this same section — while a delete is
    /// deciding whether to destroy a file, and a delete cannot begin while a launch already owns
    /// the section. Neither side blocks: whichever loses `try_lock` fails closed immediately.
    fn try_enter_exclusion(&self) -> Option<LaunchExclusionGuard>;
}

#[async_trait::async_trait]
impl SaveStateLaunchPort for LaunchApplicationService {
    fn is_managed_session_active(&self) -> bool {
        LaunchApplicationService::is_managed_session_active(self)
    }

    fn is_running_or_blocked(&self) -> bool {
        LaunchApplicationService::is_running_or_blocked(self)
    }

    async fn launch_save_state(&self, plan: SaveStateLaunchPlan) -> LaunchResponse {
        LaunchApplicationService::launch_save_state(self, plan).await
    }

    fn try_enter_exclusion(&self) -> Option<LaunchExclusionGuard> {
        LaunchApplicationService::try_enter_exclusion(self)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SaveStateConfig {
    /// How many files a pre-launch state tree may contain. A larger tree fails the launch closed
    /// rather than recording a baseline RetroFrontier cannot reason about.
    ///
    /// There is deliberately no retry-count limit on how many times an indeterminate baseline may
    /// reconcile before it is dropped (MEDIUM-1): a baseline is the only proof a session's Save
    /// States exist, and a retry-count cutoff would discard that evidence for a state that merely
    /// hasn't finished settling yet. An indeterminate baseline is retained until reconciliation
    /// reaches a deterministic outcome, or `session_was_superseded` proves a later session has
    /// since written to the tree and made attribution impossible.
    pub max_baseline_entries: usize,
}

impl Default for SaveStateConfig {
    fn default() -> Self {
        Self {
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
    /// Test-only rendezvous point inside `delete_verified`'s critical section, used to prove
    /// HIGH-1's delete-vs-launch serialization deterministically instead of racing real timing.
    #[cfg(test)]
    delete_checkpoint: Arc<OnceLock<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>>,
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
            #[cfg(test)]
            delete_checkpoint: Arc::new(OnceLock::new()),
        }
    }

    /// Make `delete_verified` pause once it has entered its exclusion section and passed its
    /// first eligibility check, notifying `reached` and waiting for `resume`. Production never
    /// sets this; it exists so a test can deterministically interleave a concurrent launch attempt
    /// into that exact window instead of racing real thread timing.
    #[cfg(test)]
    pub fn with_delete_checkpoint(
        self,
        reached: Arc<tokio::sync::Notify>,
        resume: Arc<tokio::sync::Notify>,
    ) -> Self {
        let _ = self.delete_checkpoint.set((reached, resume));
        self
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
            // **The re-check is skipped entirely while a managed session is active**, for exactly
            // the reason the load and delete paths check for one first: a running RetroArch is
            // entitled to be mid-write on a registered state file, and a half-written file has the
            // wrong size. `missing` is a closed lifecycle that is never reopened, so concluding it
            // from a mid-write observation would cost the state its identity and its history — the
            // session that ends would then register a brand-new object at the same path. Nothing
            // is lost by waiting: the listing still renders, and the session's own reconciliation
            // records the new content on the existing object.
            if !blocked {
                if let Err(error) = managed_file_matches_size(
                    &self.states_root,
                    &state.state.relative_path,
                    state.state.size_bytes,
                ) {
                    if error.proves_absence_or_mismatch() {
                        tracing::info!(
                            save_state_id = %state.id,
                            game_id = %state.provenance.game_id,
                            slot = state.slot.get(),
                            error = %error,
                            "a registered save-state file no longer matches; the state is now \
                             missing"
                        );
                        self.save_states.mark_missing(state.id).await?;
                        continue;
                    }
                    // The file could not be examined at all. That is uncertainty, not absence, so
                    // the row keeps its lifecycle and the state is simply not offered right now.
                    tracing::warn!(
                        save_state_id = %state.id,
                        error = %error,
                        "a registered save-state file could not be examined; its lifecycle was \
                         left unchanged"
                    );
                    continue;
                }
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

    /// The library's own stored relative path to one baseline's exact recorded content unit
    /// (HIGH-2) — the same file RetroFrontier hands RetroArch as its content argument, and so the
    /// same basename RetroArch derives its state-file namespace from. `None` when the game or the
    /// exact content unit can no longer be resolved at all.
    async fn content_relative_path(&self, provenance: &SaveStateProvenance) -> Option<String> {
        let units = self
            .library
            .game_content_units(provenance.game_id)
            .await
            .ok()?;
        units
            .into_iter()
            .find(|unit| unit.id == provenance.content_unit_id)
            .map(|unit| unit.primary_relative_path)
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
    ///
    /// **The active-session check comes first, and that ordering is load-bearing.**
    /// `verified_state` marks a state `missing` when its registered digest no longer matches, and a
    /// *running* RetroArch is entitled to be rewriting exactly that file — the session that ends
    /// will reconcile it properly. Verifying first would therefore let a live emulator's ordinary
    /// mid-write content turn a perfectly good Save State into `missing`. The launch pipeline
    /// re-checks the same thing again under the runtime mutation lock, so this is a guard rather
    /// than the authority.
    async fn prepare_load(&self, id: SaveStateId) -> Result<SaveStateLaunchPlan, SaveStateError> {
        if self.managed_session_active() {
            return Err(SaveStateError::TemporarilyBlocked);
        }
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

        // HIGH-2: prove the registered file is exactly the file RetroArch would target for this
        // content and slot, before ever authorizing a load of it. RetroFrontier verifies file A
        // (the registered `state_relative_path`) but only ever hands RetroArch a *slot*; RetroArch
        // itself derives the path it actually opens from the content basename and the core's own
        // reported namespace. Registration (`observe_delta`) already restricts attribution to
        // exactly this content's basename, so an ordinarily-registered row can never fail this —
        // this is defense in depth against a row whose provenance was established some other way
        // (a direct database write, a future migration, a bug elsewhere) ever being loaded as if
        // it belonged to content it does not.
        if !crate::services::save_state_fs::state_basename_matches_content(
            &state.state.relative_path,
            &unit.primary_relative_path,
        ) {
            tracing::warn!(
                save_state_id = %id,
                "the registered save-state path does not match this content's own basename; the \
                 load was refused rather than risk loading the wrong file"
            );
            return Err(SaveStateError::UnsafeFilesystemTarget);
        }

        // A cheap early refusal only: the exact historical core binary must be located in a
        // currently installed, authenticated, allowed installation. A revoked, blocked, or
        // below-floor component never satisfies this. **This result is not carried forward.** It
        // exists purely so an obviously-doomed load fails fast with `CoreUnavailable` instead of
        // going all the way through the launch pipeline; it is not a durable authorization, and
        // the plan below carries only the immutable component id and binary digest needed to
        // *locate* the binary again. Trust policy can change between this check and the moment
        // the launch pipeline actually authorizes and spawns a process, so the decisive lookup is
        // redone from scratch inside `launch_locked`, under the runtime mutation lock, and this
        // call's result is discarded once it has answered "plausible" or "not now".
        if let Err(error) = self.runtime.locate_authenticated_core_binary(
            &state.provenance.core_component_id,
            state.provenance.core_binary_sha256,
        ) {
            tracing::info!(
                save_state_id = %id,
                core_id = %state.provenance.core_id,
                core_component_id = %state.provenance.core_component_id,
                error = %error,
                "the exact historical core binary is unavailable"
            );
            return Err(SaveStateError::CoreUnavailable);
        }

        // Again, because a session may have started while the checks above ran. The launch
        // pipeline re-checks it a third time under the runtime mutation lock, which is the one
        // that actually decides.
        if self.managed_session_active() {
            return Err(SaveStateError::TemporarilyBlocked);
        }

        Ok(SaveStateLaunchPlan {
            save_state_id: state.id,
            game_id: state.provenance.game_id,
            content_unit_id: state.provenance.content_unit_id,
            core_component_id: state.provenance.core_component_id.clone(),
            core_binary_sha256: state.provenance.core_binary_sha256,
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
        // HIGH-1: enter the exact same in-process critical section a launch uses, *before* doing
        // anything else, and hold it for the rest of this function. This is what makes "no managed
        // launch may begin while this delete is deciding whether to destroy a file" a structural
        // property of the whole authorization-to-destructive-action window, rather than a
        // point-in-time check a concurrently starting launch could slip past between it and the
        // actual filesystem delete below. A launch that already owns the section — including one
        // still only *starting* — refuses this delete immediately; this never blocks.
        let Some(launch) = self.launch.get() else {
            return Err(SaveStateError::TemporarilyBlocked);
        };
        let Some(_exclusion) = launch.try_enter_exclusion() else {
            return Err(SaveStateError::TemporarilyBlocked);
        };

        // First, for the same reason the load path checks first: a running RetroArch may
        // legitimately be rewriting this very file, and `verified_state` would otherwise mark a
        // perfectly good Save State `missing` on the strength of a mid-write digest.
        //
        // This deliberately calls `is_running_or_blocked`, not `managed_session_active`: the guard
        // above already holds the same in-process section `managed_session_active` would itself
        // try (and fail) to acquire, which would report every delete as blocked by itself.
        // Structurally holding the exclusion guard already proves no launch can be *starting*;
        // this narrower check still catches one that is already running or of uncertain identity.
        if launch.is_running_or_blocked() {
            return Err(SaveStateError::TemporarilyBlocked);
        }

        #[cfg(test)]
        if let Some((reached, resume)) = self.delete_checkpoint.get() {
            reached.notify_one();
            resume.notified().await;
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
                // Only a *proven* absence or a proven content mismatch closes the lifecycle.
                // `missing` can never be reopened, so an observation that merely failed — the
                // process is out of descriptors, the read errored, the tree is momentarily
                // unreadable — must not be allowed to retire a perfectly good save state. The
                // module's rule everywhere else is that only a complete, certain observation may
                // drive a destructive transition, and this is the same rule.
                if error.proves_absence_or_mismatch() {
                    tracing::info!(
                        save_state_id = %id,
                        error = %error,
                        "the registered save-state identity no longer matches; the file was left \
                         untouched"
                    );
                    if let Err(error) = self.save_states.mark_missing(id).await {
                        error.log();
                    }
                } else {
                    tracing::warn!(
                        save_state_id = %id,
                        error = %error,
                        "the registered save state could not be examined; its lifecycle was left \
                         unchanged"
                    );
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

        // **A baseline can only prove anything while its session is the last thing that touched
        // the state tree.** A baseline is deliberately retained when reconciliation is
        // indeterminate, and the retry happens at the next startup — by which time another session
        // may have run and written its own states. Those files are absent from *this* baseline too,
        // so the delta can no longer say whose they are: attributing them here would register
        // another game's save state under this session's game, content unit, and core binary, and
        // supersede the row that legitimately owns it.
        //
        // There is no way to disambiguate after the fact, so this fails closed: the baseline is
        // dropped, nothing is attributed, and nothing is marked missing. Whatever this session
        // wrote and did not get registered stays on disk as an unattributable file, which is
        // exactly what M9 does with every file whose provenance it cannot prove.
        if self
            .save_states
            .session_was_superseded(session_id)
            .await
            .map_err(storage)?
        {
            tracing::warn!(
                play_session_id = %session_id,
                game_id = %baseline.provenance.game_id,
                "a later play session has since written to the state tree, so this baseline can no \
                 longer attribute anything; it was dropped without attributing or removing anything"
            );
            self.save_states
                .delete_baseline(session_id)
                .await
                .map_err(storage)?;
            return Ok(());
        }

        // HIGH-2: resolve the exact content this baseline's session launched, so attribution below
        // can be restricted to states whose own basename is that content — never merely a
        // `.stateN` file found somewhere in the owned tree. `None` when the content unit can no
        // longer be resolved at all, which must make this round indeterminate rather than silently
        // attributing nothing and discarding the baseline: the content may simply be unavailable
        // right now, not gone forever.
        let content_relative_path = self.content_relative_path(&baseline.provenance).await;

        // The filesystem phase blocks — the stability probe sleeps between observations, and
        // hashing reads whole files. It runs from the process monitor and, on a failed launch,
        // from inside `launch_locked` while the sequence lock and the OS runtime-mutation lock are
        // held, so parking a runtime worker there would stall the application at exactly its
        // busiest moment.
        let service = self.clone();
        let for_observation = baseline.clone();
        let observed = tokio::task::spawn_blocking(move || {
            service.observe_delta(&for_observation, content_relative_path.as_deref())
        })
        .await
        .map_err(|error| {
            tracing::warn!(error = %error, "the save-state observation task stopped");
            SaveStateError::ReconciliationFailed
        })?;

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
            // MEDIUM-1: a baseline is the only proof this session's Save States exist, so it is
            // kept durably pending for as long as reconciliation stays indeterminate — there is no
            // retry-count cutoff that discards it. `attempts` is retained purely as a diagnostic
            // counter now, not as a deletion trigger: a permanently indeterminate baseline would
            // previously leak an unbounded number of *log lines*, and that is bounded here by
            // throttling the warning rather than by destroying the only attribution evidence for
            // states a temporarily unreadable tree, a slow filesystem, or a still-settling write
            // simply hasn't finished proving yet. The only things that ever end this baseline's
            // life are a deterministic reconciliation (below) and `session_was_superseded` proving
            // a later session has since written to the tree and made attribution impossible.
            let attempts = self
                .save_states
                .increment_baseline_attempts(session_id)
                .await
                .map_err(storage)?;
            if attempts == 1 || attempts.is_power_of_two() {
                tracing::warn!(
                    play_session_id = %session_id,
                    game_id = %baseline.provenance.game_id,
                    attempts,
                    "save-state reconciliation is still indeterminate; the baseline remains \
                     retained and will be retried"
                );
            }
            return Ok(());
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
    /// `content_relative_path` is `None` exactly when HIGH-2's content resolution above could not
    /// establish what this session's content even was — never a reason to attribute anything, and
    /// the caller marks the whole round indeterminate rather than deterministic-with-nothing-found
    /// in that case, so a temporarily unresolvable content unit gets retried rather than silently
    /// losing its baseline.
    fn observe_delta(
        &self,
        baseline: &LaunchStateBaseline,
        content_relative_path: Option<&str>,
    ) -> ObservedDelta {
        let snapshot = snapshot_state_tree(&self.states_root);
        let delta = state_tree_delta(&baseline.entries, &snapshot);
        let changed: std::collections::BTreeSet<&RelativePath> = delta.iter().collect();

        let mut candidates = Vec::new();
        let mut indeterminate = content_relative_path.is_none();
        for relative_path in &delta {
            // Only supported numbered slots are states. A thumbnail in the delta is picked up
            // through the state it belongs to, never on its own, and everything else is ignored.
            let StateCandidate::ManagedSlot(slot) = parse_state_candidate(relative_path) else {
                continue;
            };
            // HIGH-2: attribution requires the file's own basename to be the exact content this
            // session launched — never merely a `.stateN` file anywhere in the owned tree. A file
            // shaped exactly like a valid managed slot but under a foreign namespace (a different
            // basename, regardless of directory) is left completely unattributed and untouched.
            if !content_relative_path.is_some_and(|content| {
                crate::services::save_state_fs::state_basename_matches_content(
                    relative_path,
                    content,
                )
            }) {
                tracing::info!(
                    relative_path = %relative_path,
                    "a save-state candidate's basename is not this session's own content; it was \
                     left unattributed and untouched"
                );
                continue;
            }
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
            // The same game, the same content unit, and the same core binary overwrote its own
            // slot. The object keeps its identity and its immutable provenance and moves onto the
            // newly proved content: this change is *explained* by a controlled launch, which is
            // what distinguishes it from the unexplained change that invalidates a registered
            // identity.
            //
            // The game and content unit are part of the test, not just the binary. RetroArch's
            // state path is `<library name>/<content basename>.stateN`, so two different library
            // games whose ROMs share a basename — the same dump added from two content roots, or
            // two files both called `Tetris.nes` — collide on one path under one core. Refreshing
            // on the binary alone would move the first game's row onto the second game's bytes
            // while keeping the first game's ids: its detail page would list a state that is really
            // the other game's, and loading it would boot the wrong ROM. That is a false
            // attribution, so it takes the supersede-and-insert path below instead.
            Some(existing)
                if existing.provenance.core_binary_sha256 == provenance.core_binary_sha256
                    && existing.provenance.game_id == provenance.game_id
                    && existing.provenance.content_unit_id == provenance.content_unit_id =>
            {
                self.save_states
                    .refresh_state(
                        existing.id,
                        &RefreshedSaveState {
                            play_session_id: provenance.play_session_id,
                            state: new.state.clone(),
                            // MEDIUM-3: the state's bytes just changed, so the strict thumbnail
                            // rule applies to *this* version exactly as it does to a brand-new
                            // one — a thumbnail is exposed only when this controlled launch's own
                            // delta proved the relationship for it. Falling back to the previous
                            // version's thumbnail here would let newly written state bytes stay
                            // associated with an image that was only ever proved for the bytes
                            // they replaced. If no thumbnail was proved this time, the exposed
                            // thumbnail becomes `None` and the frontend renders the placeholder;
                            // the previous image simply becomes an untracked orphan file, exactly
                            // as a state's thumbnail already does whenever it fails its own
                            // re-verification during a delete.
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

    /// The verified thumbnail **bytes** of one Save State, for the native media protocol.
    ///
    /// Resolved from durable state by identity, never from a caller-supplied path, and re-verified
    /// in full before any byte leaves Rust. It deliberately returns the bytes rather than a path:
    /// handing a path back would force the caller to open it a second time, and a second open
    /// resolves the name afresh and follows symbolic links — the exact substitution window the
    /// delete path refuses to leave open.
    pub async fn verified_thumbnail(&self, id: SaveStateId) -> Result<Vec<u8>, SaveStateError> {
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
        crate::services::save_state_fs::read_verified_managed_file(
            &self.states_root,
            &thumbnail.relative_path,
            thumbnail.size_bytes,
            thumbnail.sha256,
        )
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

#[cfg(test)]
mod tests {
    use super::{
        SaveStateApplicationService, SaveStateConfig, SaveStateLaunchPort, SaveStateRuntime,
    };
    use crate::adapters::database::Database;
    use crate::adapters::runtime_integrity::sha256_bytes;
    use crate::application::launch::{BaselineRequest, SaveStateLaunchPlan, SaveStateLifecycle};
    use crate::application::runtime_manager::AuthenticatedCoreBinary;
    use crate::domain::core::CoreId;
    use crate::domain::launch::{
        LaunchErrorCode, LaunchFailure, LaunchResponse, PlaySessionId, PlaySessionOutcome,
    };
    use crate::domain::library::{ContentUnitId, GameId};
    use crate::domain::runtime::{RelativePath, RuntimeError, SafeIdentifier, Sha256Digest};
    use crate::domain::save_state::{
        DeleteSaveStateResponse, LoadSaveStateResponse, SaveStateError, SaveStateFileIdentity,
        SaveStateId, SaveStateLoadability, SaveStateProvenance, SaveStateSlot, SaveStateStatus,
    };
    use crate::repositories::launch::{LaunchRepository, NewPlaySession};
    use crate::repositories::library::LibraryRepository;
    use crate::repositories::save_state::{NewSaveState, SaveStateRepository};
    use crate::services::save_state_fs::StabilityProbe;
    use std::path::Path;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use tempfile::TempDir;

    const TEST_TIME: i64 = 1_756_900_000_000;
    const CORE_A: &[u8] = b"core binary A";
    const CORE_B: &[u8] = b"core binary B";

    // ---------------------------------------------------------------- doubles

    /// A runtime whose installed core binaries are whatever the test says they are.
    #[derive(Default)]
    struct StubRuntime {
        available: Mutex<Vec<Sha256Digest>>,
    }

    impl StubRuntime {
        fn with(binaries: &[&[u8]]) -> Arc<Self> {
            Arc::new(Self {
                available: Mutex::new(binaries.iter().map(|bytes| sha256_bytes(bytes)).collect()),
            })
        }

        fn remove(&self, bytes: &[u8]) {
            let digest = sha256_bytes(bytes);
            self.available.lock().unwrap().retain(|it| *it != digest);
        }
    }

    impl SaveStateRuntime for StubRuntime {
        fn locate_authenticated_core_binary(
            &self,
            component_id: &SafeIdentifier,
            binary_sha256: Sha256Digest,
        ) -> Result<AuthenticatedCoreBinary, RuntimeError> {
            if !self.available.lock().unwrap().contains(&binary_sha256) {
                return Err(RuntimeError::InstalledTree("absent".to_owned()));
            }
            Ok(AuthenticatedCoreBinary {
                component_id: component_id.clone(),
                core_path: std::path::PathBuf::from("/synthetic/cores/core.so"),
                binary_sha256,
                binary_size_bytes: 13,
                systems: vec![SafeIdentifier::new("nes").unwrap()],
                display_version: Some("1.53".to_owned()),
                source_revision: Some("deadbeef".to_owned()),
                installation_id: SafeIdentifier::new("install-1").unwrap(),
                release_id: SafeIdentifier::new("release-1").unwrap(),
            })
        }

        fn declares_authenticated_core_binary(
            &self,
            _component_id: &SafeIdentifier,
            binary_sha256: Sha256Digest,
        ) -> bool {
            self.available.lock().unwrap().contains(&binary_sha256)
        }
    }

    /// A launch port that records what it was asked to launch and can pretend a game is running.
    #[derive(Default)]
    struct StubLaunch {
        active: AtomicBool,
        plans: Mutex<Vec<SaveStateLaunchPlan>>,
        fail: AtomicBool,
        /// The same kind of in-process exclusion mutex `LaunchApplicationService` uses, so a test
        /// can hold it to prove a delete refuses while "a launch" (simulated here) owns the
        /// section, exactly as `try_enter_exclusion` requires.
        sequence: Arc<tokio::sync::Mutex<()>>,
    }

    #[async_trait::async_trait]
    impl SaveStateLaunchPort for StubLaunch {
        fn is_managed_session_active(&self) -> bool {
            self.active.load(Ordering::Relaxed)
        }

        fn is_running_or_blocked(&self) -> bool {
            self.active.load(Ordering::Relaxed)
        }

        async fn launch_save_state(&self, plan: SaveStateLaunchPlan) -> LaunchResponse {
            // Mirrors `LaunchApplicationService::launch()`'s own first move: contend for the same
            // in-process exclusion section a delete may be holding, so a save-state load racing a
            // delete is refused exactly as it would be in production, rather than the stub
            // silently letting it through.
            let Ok(_sequence) = self.sequence.try_lock() else {
                return LaunchResponse::failed(LaunchErrorCode::GameAlreadyRunning);
            };
            self.plans.lock().unwrap().push(plan);
            if self.fail.load(Ordering::Relaxed) {
                return LaunchResponse::failure(LaunchFailure::new(LaunchErrorCode::SpawnFailed));
            }
            LaunchResponse::Started {
                session: crate::domain::launch::RunningGameSession {
                    session_id: PlaySessionId(1),
                    game_id: GameId(1),
                    content_unit_id: ContentUnitId(1),
                    core_id: CoreId::new("nestopia").unwrap(),
                    started_at: TEST_TIME,
                },
                diagnostics: Vec::new(),
            }
        }

        fn try_enter_exclusion(&self) -> Option<crate::application::launch::LaunchExclusionGuard> {
            self.sequence
                .clone()
                .try_lock_owned()
                .ok()
                .map(crate::application::launch::LaunchExclusionGuard)
        }
    }

    /// A deterministic stability probe: stable unless a path was explicitly declared unstable.
    #[derive(Default)]
    struct ScriptedStability {
        unstable: Mutex<Vec<String>>,
        observations: AtomicUsize,
    }

    impl ScriptedStability {
        fn unstable(&self, relative_path: &str) {
            self.unstable.lock().unwrap().push(relative_path.to_owned());
        }

        /// Reverse an earlier `unstable` declaration, so a test can prove a baseline that was
        /// indeterminate for a while still reconciles normally once the condition resolves.
        fn make_stable(&self, relative_path: &str) {
            self.unstable
                .lock()
                .unwrap()
                .retain(|path| path != relative_path);
        }
    }

    impl StabilityProbe for ScriptedStability {
        fn is_stable(&self, _states_root: &Path, relative_path: &RelativePath) -> bool {
            self.observations.fetch_add(1, Ordering::Relaxed);
            !self
                .unstable
                .lock()
                .unwrap()
                .iter()
                .any(|path| path == relative_path.as_str())
        }
    }

    // ---------------------------------------------------------------- fixture

    struct Fixture {
        _directory: TempDir,
        states_root: std::path::PathBuf,
        service: SaveStateApplicationService,
        save_states: SaveStateRepository,
        sessions: LaunchRepository,
        runtime: Arc<StubRuntime>,
        launch: Arc<StubLaunch>,
        stability: Arc<ScriptedStability>,
        pool: sqlx::SqlitePool,
    }

    impl Fixture {
        async fn build() -> Self {
            Self::build_with(StubRuntime::with(&[CORE_A, CORE_B])).await
        }

        async fn build_with(runtime: Arc<StubRuntime>) -> Self {
            let directory = tempfile::tempdir().unwrap();
            let states_root = directory.path().join("states");
            std::fs::create_dir_all(&states_root).unwrap();
            let database = Database::open(directory.path().join("save-states.sqlite3"))
                .await
                .unwrap();
            let pool = database.pool().clone();
            seed(&pool).await;

            let stability = Arc::new(ScriptedStability::default());
            let launch = Arc::new(StubLaunch::default());
            let service = SaveStateApplicationService::new(
                SaveStateRepository::new(pool.clone()),
                LibraryRepository::new(pool.clone()),
                LaunchRepository::new(pool.clone()),
                runtime.clone(),
                &states_root,
                SaveStateConfig::default(),
            )
            .with_stability_probe(stability.clone());
            service.attach_launch(launch.clone());

            Self {
                _directory: directory,
                states_root,
                service,
                save_states: SaveStateRepository::new(pool.clone()),
                sessions: LaunchRepository::new(pool.clone()),
                runtime,
                launch,
                stability,
                pool,
            }
        }

        /// Open a session, capture its baseline, and hand back its id.
        async fn begin_session(&self, core: &[u8], content_unit_id: i64) -> PlaySessionId {
            let session = self
                .sessions
                .start_session(&NewPlaySession {
                    game_id: GameId(if content_unit_id == 3 { 2 } else { 1 }),
                    content_unit_id: ContentUnitId(content_unit_id),
                    core_id: CoreId::new("nestopia").unwrap(),
                    runtime_installation_id: "install-1".to_owned(),
                    runtime_release_id: "release-1".to_owned(),
                })
                .await
                .unwrap();
            self.service
                .capture_baseline(BaselineRequest {
                    session_id: session.id,
                    game_id: session.game_id,
                    content_unit_id: session.content_unit_id,
                    core_id: CoreId::new("nestopia").unwrap(),
                    core_component_id: SafeIdentifier::new("nestopia").unwrap(),
                    core_binary_sha256: sha256_bytes(core),
                    core_display_version: Some("1.53".to_owned()),
                    core_source_revision: Some("deadbeef".to_owned()),
                    runtime_installation_id: SafeIdentifier::new("install-1").unwrap(),
                    runtime_release_id: SafeIdentifier::new("release-1").unwrap(),
                })
                .await
                .expect("the baseline is captured");
            session.id
        }

        async fn end_session(&self, id: PlaySessionId, outcome: PlaySessionOutcome) {
            self.sessions
                .complete_session(id, outcome, Some(0))
                .await
                .unwrap();
        }

        /// One complete controlled session: baseline, the writes it performs, a certain end, and
        /// reconciliation.
        async fn run_session(
            &self,
            core: &[u8],
            content_unit_id: i64,
            writes: &[(&str, &[u8])],
        ) -> PlaySessionId {
            let id = self.begin_session(core, content_unit_id).await;
            for (path, bytes) in writes {
                self.write(path, bytes);
            }
            self.end_session(id, PlaySessionOutcome::Completed).await;
            self.service.reconcile_session(id).await;
            id
        }

        fn write(&self, relative: &str, bytes: &[u8]) {
            let target = self.states_root.join(relative);
            std::fs::create_dir_all(target.parent().unwrap()).unwrap();
            std::fs::write(target, bytes).unwrap();
        }

        fn exists(&self, relative: &str) -> bool {
            self.states_root.join(relative).exists()
        }

        fn read(&self, relative: &str) -> Vec<u8> {
            std::fs::read(self.states_root.join(relative)).unwrap()
        }

        async fn states(&self) -> Vec<crate::domain::save_state::SaveState> {
            self.save_states.available_states().await.unwrap()
        }

        async fn only_state(&self) -> crate::domain::save_state::SaveState {
            let mut states = self.states().await;
            assert_eq!(states.len(), 1, "expected exactly one available state");
            states.remove(0)
        }

        /// Override a seeded content unit's own basename, for tests that deliberately need a
        /// specific (or colliding) content basename rather than the seed default.
        async fn set_content_path(&self, content_unit_id: i64, relative_path: &str) {
            sqlx::query("UPDATE content_units SET primary_relative_path = ? WHERE id = ?")
                .bind(relative_path)
                .bind(content_unit_id)
                .execute(&self.pool)
                .await
                .unwrap();
        }
    }

    /// One game with two content units — the multi-disc case — plus a second game.
    async fn seed(pool: &sqlx::SqlitePool) {
        sqlx::query(
            "INSERT INTO content_roots \
             (id, path, kind, enabled, availability, created_at, updated_at) \
             VALUES (1, '/synthetic/library', 'managed', 1, 'available', ?, ?)",
        )
        .bind(TEST_TIME)
        .bind(TEST_TIME)
        .execute(pool)
        .await
        .unwrap();
        for (id, title) in [(1_i64, "Synthetic Disc Set"), (2, "Other Game")] {
            sqlx::query(
                "INSERT INTO games (id, system_id, local_title, availability, created_at, \
                 updated_at) VALUES (?, 'nes', ?, 'available', ?, ?)",
            )
            .bind(id)
            .bind(title)
            .bind(TEST_TIME)
            .bind(TEST_TIME)
            .execute(pool)
            .await
            .unwrap();
        }
        for (id, game_id, title, path) in [
            (1_i64, 1_i64, "Disc 1", "NES/Synthetic.nes"),
            (2, 1, "Disc 2", "NES/disc2.nes"),
            (3, 2, "Other", "NES/other.nes"),
        ] {
            sqlx::query(
                "INSERT INTO content_units \
                 (id, game_id, root_id, system_id, kind, local_title, primary_relative_path, \
                  fingerprint, availability, created_at, updated_at) \
                 VALUES (?, ?, 1, 'nes', 'single_file', ?, ?, NULL, 'available', ?, ?)",
            )
            .bind(id)
            .bind(game_id)
            .bind(title)
            .bind(path)
            .bind(TEST_TIME)
            .bind(TEST_TIME)
            .execute(pool)
            .await
            .unwrap();
        }
    }

    // ================================================================ reconciliation

    #[tokio::test]
    async fn a_new_stable_state_is_registered_with_complete_provenance() {
        let fixture = Fixture::build().await;
        let session = fixture
            .run_session(CORE_A, 1, &[("Nestopia/Synthetic.state1", b"state bytes")])
            .await;

        let state = fixture.only_state().await;
        assert_eq!(state.provenance.play_session_id, session);
        assert_eq!(state.provenance.game_id, GameId(1));
        assert_eq!(state.provenance.content_unit_id, ContentUnitId(1));
        assert_eq!(state.provenance.core_id, CoreId::new("nestopia").unwrap());
        assert_eq!(state.provenance.core_binary_sha256, sha256_bytes(CORE_A));
        assert_eq!(
            state.provenance.core_display_version.as_deref(),
            Some("1.53")
        );
        assert_eq!(
            state.provenance.originating_runtime_release_id.as_str(),
            "release-1"
        );
        assert_eq!(state.slot.get(), 1);
        assert_eq!(state.state.sha256, sha256_bytes(b"state bytes"));
        assert_eq!(state.state.size_bytes, 11);
        assert!(state.thumbnail.is_none());
        assert_eq!(state.status, SaveStateStatus::Available);
        // The baseline is consumed once the outcome is deterministic.
        assert!(fixture
            .save_states
            .baseline(session)
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn unsupported_slots_and_unrelated_files_are_never_attributed() {
        let fixture = Fixture::build().await;
        fixture
            .run_session(
                CORE_A,
                1,
                &[
                    // The one supported slot.
                    ("Nestopia/Synthetic.state1", b"managed"),
                    // Slot 0 and the automatic slot.
                    ("Nestopia/Synthetic.state", b"slot zero"),
                    ("Nestopia/Synthetic.state.auto", b"auto"),
                    // Out of range, ambiguous, and unrelated.
                    ("Nestopia/Synthetic.state1000", b"too high"),
                    ("Nestopia/Synthetic.state01", b"ambiguous"),
                    ("Nestopia/Synthetic.srm", b"save data"),
                ],
            )
            .await;

        let states = fixture.states().await;
        assert_eq!(states.len(), 1);
        assert_eq!(states[0].slot.get(), 1);
        assert_eq!(
            states[0].state.relative_path.as_str(),
            "Nestopia/Synthetic.state1"
        );
        // Nothing unsupported was touched on disk either.
        for path in [
            "Nestopia/Synthetic.state",
            "Nestopia/Synthetic.state.auto",
            "Nestopia/Synthetic.state1000",
            "Nestopia/Synthetic.srm",
        ] {
            assert!(fixture.exists(path), "{path} must be left alone");
        }
    }

    #[tokio::test]
    async fn a_file_that_predates_the_session_is_not_attributed_to_it() {
        let fixture = Fixture::build().await;
        // A legacy file shaped exactly like a valid RetroArch state, present before the launch.
        fixture.write("Nestopia/Legacy.state1", b"pre-existing");
        fixture
            .run_session(CORE_A, 1, &[("Nestopia/Synthetic.state2", b"written now")])
            .await;

        let states = fixture.states().await;
        assert_eq!(
            states.len(),
            1,
            "only the session's own write is attributed"
        );
        assert_eq!(
            states[0].state.relative_path.as_str(),
            "Nestopia/Synthetic.state2"
        );
        // The legacy file stays exactly where it is: untouched, unimported, invisible.
        assert!(fixture.exists("Nestopia/Legacy.state1"));
        assert_eq!(fixture.read("Nestopia/Legacy.state1"), b"pre-existing");
    }

    #[tokio::test]
    async fn an_unstable_candidate_is_skipped_while_its_siblings_are_still_registered() {
        let fixture = Fixture::build().await;
        fixture.stability.unstable("Nestopia/Synthetic.state2");

        let session = fixture
            .run_session(
                CORE_A,
                1,
                &[
                    ("Nestopia/Synthetic.state1", b"complete"),
                    ("Nestopia/Synthetic.state2", b"still being written"),
                ],
            )
            .await;

        // Partial independent success: one bad candidate does not discard a proved sibling.
        let states = fixture.states().await;
        assert_eq!(states.len(), 1);
        assert_eq!(
            states[0].state.relative_path.as_str(),
            "Nestopia/Synthetic.state1"
        );
        // The unstable file is left untouched and unregistered.
        assert!(fixture.exists("Nestopia/Synthetic.state2"));
        // And the baseline is kept, because the outcome was not deterministic.
        assert!(fixture
            .save_states
            .baseline(session)
            .await
            .unwrap()
            .is_some());
    }

    #[tokio::test]
    async fn reconciliation_is_idempotent_and_a_replay_changes_nothing() {
        let fixture = Fixture::build().await;
        let session = fixture
            .run_session(CORE_A, 1, &[("Nestopia/Synthetic.state1", b"state bytes")])
            .await;
        let first = fixture.only_state().await;

        // Replaying a completed reconciliation is a no-op, whatever drives it.
        for _ in 0..3 {
            fixture.service.reconcile_session(session).await;
        }
        assert_eq!(fixture.service.reconcile_on_startup().await.unwrap(), 0);

        let states = fixture.states().await;
        assert_eq!(states.len(), 1, "no duplicate row");
        assert_eq!(states[0], first, "nothing changed, updated_at included");
    }

    #[tokio::test]
    async fn a_crash_before_persistence_leaves_the_baseline_and_the_retry_completes() {
        let fixture = Fixture::build().await;
        let session = fixture.begin_session(CORE_A, 1).await;
        fixture.write("Nestopia/Synthetic.state1", b"state bytes");
        fixture
            .end_session(session, PlaySessionOutcome::Completed)
            .await;

        // A RetroFrontier crash between the snapshot and persistence: nothing was registered, and
        // the baseline is still there.
        assert!(fixture.states().await.is_empty());
        assert!(fixture
            .save_states
            .baseline(session)
            .await
            .unwrap()
            .is_some());

        // The retry — here, a restart — completes it.
        assert_eq!(fixture.service.reconcile_on_startup().await.unwrap(), 1);
        assert_eq!(fixture.only_state().await.slot.get(), 1);
        assert!(fixture
            .save_states
            .baseline(session)
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn an_open_session_attributes_nothing_and_keeps_its_baseline() {
        let fixture = Fixture::build().await;
        let session = fixture.begin_session(CORE_A, 1).await;
        fixture.write("Nestopia/Synthetic.state1", b"written while running");

        // The process may still be alive or of uncertain identity, so nothing is attributed and
        // nothing is destroyed — however many times reconciliation is driven.
        for _ in 0..3 {
            fixture.service.reconcile_session(session).await;
        }
        assert!(fixture.states().await.is_empty());
        assert!(fixture
            .save_states
            .baseline(session)
            .await
            .unwrap()
            .is_some());
        // Startup reconciliation deliberately does not pick it up either.
        assert_eq!(fixture.service.reconcile_on_startup().await.unwrap(), 0);

        // Once the end is certain, the very same baseline yields the attribution.
        fixture
            .end_session(session, PlaySessionOutcome::Completed)
            .await;
        fixture.service.reconcile_session(session).await;
        assert_eq!(fixture.only_state().await.slot.get(), 1);
    }

    #[tokio::test]
    async fn a_retroarch_crash_still_reconciles_the_state_it_managed_to_write() {
        let fixture = Fixture::build().await;
        let session = fixture.begin_session(CORE_A, 1).await;
        fixture.write("Nestopia/Synthetic.state1", b"saved before the crash");
        // A crash is a *certain* end, so the delta it left behind is still valid provenance.
        fixture
            .end_session(session, PlaySessionOutcome::Crashed)
            .await;
        fixture.service.reconcile_session(session).await;

        assert_eq!(
            fixture.only_state().await.state.sha256,
            sha256_bytes(b"saved before the crash")
        );
    }

    #[tokio::test]
    async fn the_same_core_binary_overwriting_its_own_slot_updates_the_state_in_place() {
        let fixture = Fixture::build().await;
        fixture
            .run_session(CORE_A, 1, &[("Nestopia/Synthetic.state1", b"first save")])
            .await;
        let original = fixture.only_state().await;

        let second = fixture
            .run_session(CORE_A, 1, &[("Nestopia/Synthetic.state1", b"second save!")])
            .await;

        let updated = fixture.only_state().await;
        assert_eq!(updated.id, original.id, "identity is preserved");
        assert_eq!(updated.created_at, original.created_at);
        assert_eq!(
            updated.provenance.core_binary_sha256,
            original.provenance.core_binary_sha256
        );
        // Only the physical facts and the producing session moved on.
        assert_eq!(updated.state.sha256, sha256_bytes(b"second save!"));
        assert_eq!(updated.provenance.play_session_id, second);
        assert_eq!(fixture.states().await.len(), 1);
    }

    #[tokio::test]
    async fn a_different_core_binary_at_the_same_path_supersedes_rather_than_rewriting() {
        let fixture = Fixture::build().await;
        fixture
            .run_session(CORE_A, 1, &[("Nestopia/Synthetic.state1", b"from core A")])
            .await;
        let original = fixture.only_state().await;

        fixture
            .run_session(CORE_B, 1, &[("Nestopia/Synthetic.state1", b"from core B")])
            .await;

        // The old object keeps its own immutable provenance and becomes history.
        let old = fixture
            .save_states
            .save_state(original.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(old.status, SaveStateStatus::Superseded);
        assert_eq!(
            old.provenance.core_binary_sha256,
            sha256_bytes(CORE_A),
            "provenance is never rewritten"
        );
        assert_eq!(old.state.sha256, original.state.sha256);

        // And a new object exists with the new provenance and digest.
        let current = fixture.only_state().await;
        assert_ne!(current.id, original.id);
        assert_eq!(current.provenance.core_binary_sha256, sha256_bytes(CORE_B));
        assert_eq!(current.state.sha256, sha256_bytes(b"from core B"));
        // Same slot, different core binary: both are representable, one is live.
        assert_eq!(current.slot, old.slot);
    }

    #[tokio::test]
    async fn an_externally_deleted_state_becomes_missing_and_no_replacement_is_sought() {
        let fixture = Fixture::build().await;
        fixture
            .run_session(CORE_A, 1, &[("Nestopia/Synthetic.state1", b"state bytes")])
            .await;
        let original = fixture.only_state().await;

        // Deleted outside RetroFrontier, then a later controlled session reconciles.
        std::fs::remove_file(fixture.states_root.join("Nestopia/Synthetic.state1")).unwrap();
        fixture
            .run_session(CORE_A, 1, &[("Nestopia/Synthetic.state2", b"unrelated")])
            .await;

        assert_eq!(
            fixture
                .save_states
                .save_state(original.id)
                .await
                .unwrap()
                .unwrap()
                .status,
            SaveStateStatus::Missing
        );
        // Nothing looked for a similarly named replacement: the other state is its own object.
        let live = fixture.only_state().await;
        assert_ne!(live.id, original.id);
        assert_eq!(
            live.state.relative_path.as_str(),
            "Nestopia/Synthetic.state2"
        );
    }

    #[tokio::test]
    async fn an_incomplete_enumeration_never_drives_a_missing_transition() {
        let fixture = Fixture::build().await;
        fixture
            .run_session(CORE_A, 1, &[("Nestopia/Synthetic.state1", b"state bytes")])
            .await;
        let original = fixture.only_state().await;

        // The next session starts from a tree that *can* be described, so it gets a baseline.
        let session = fixture.begin_session(CORE_A, 1).await;

        // Then the file goes away and the tree stops being completely describable, so absence is
        // no longer provable.
        std::fs::remove_file(fixture.states_root.join("Nestopia/Synthetic.state1")).unwrap();
        let protected = fixture.states_root.join("protected");
        std::fs::create_dir_all(&protected).unwrap();
        std::fs::write(protected.join("Hidden.state1"), b"hidden").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&protected, std::fs::Permissions::from_mode(0o000)).unwrap();

        fixture
            .end_session(session, PlaySessionOutcome::Completed)
            .await;
        fixture.service.reconcile_session(session).await;

        // Still available: an unreadable subtree is uncertainty, not evidence of deletion.
        assert_eq!(
            fixture
                .save_states
                .save_state(original.id)
                .await
                .unwrap()
                .unwrap()
                .status,
            SaveStateStatus::Available
        );
        std::fs::set_permissions(&protected, std::fs::Permissions::from_mode(0o700)).unwrap();
    }

    /// MEDIUM-1 regression: there is no retry-count cutoff that discards an indeterminate
    /// baseline. It is retried well past the old destructive limit (3) and survives every single
    /// time, attributes nothing while it stays pending, and still reconciles normally the moment
    /// the underlying condition resolves — proving genuine retention, not merely "not yet
    /// deleted".
    #[tokio::test]
    async fn an_indeterminate_baseline_is_retained_indefinitely_until_it_can_reconcile() {
        let fixture = Fixture::build().await;
        fixture.stability.unstable("Nestopia/Synthetic.state1");
        let session = fixture.begin_session(CORE_A, 1).await;
        fixture.write("Nestopia/Synthetic.state1", b"not yet settled");
        fixture
            .end_session(session, PlaySessionOutcome::Completed)
            .await;

        for attempt in 1..=12 {
            fixture.service.reconcile_session(session).await;
            assert!(
                fixture
                    .save_states
                    .baseline(session)
                    .await
                    .unwrap()
                    .is_some(),
                "the baseline must survive attempt {attempt}, well past the old cutoff of 3"
            );
        }
        // Nothing was falsely attributed and nothing on disk was touched while it stayed pending.
        assert!(fixture.states().await.is_empty());
        assert!(fixture.exists("Nestopia/Synthetic.state1"));

        // Once the underlying condition resolves, the very same baseline reconciles normally.
        fixture.stability.make_stable("Nestopia/Synthetic.state1");
        fixture.service.reconcile_session(session).await;

        let state = fixture.only_state().await;
        assert_eq!(state.provenance.play_session_id, session);
        assert!(fixture
            .save_states
            .baseline(session)
            .await
            .unwrap()
            .is_none());
    }

    /// **A stale baseline must never attribute a later session's files.**
    ///
    /// A baseline is retained when reconciliation is indeterminate, and the retry happens at the
    /// next startup. By then another session may have written its own states — which are absent
    /// from *this* baseline too, so the delta cannot say whose they are. Attributing them would
    /// register another game's save state under this session's game, content unit, and core
    /// binary, and supersede the row that legitimately owns it.
    #[tokio::test]
    async fn a_baseline_a_later_session_has_written_past_attributes_nothing() {
        let fixture = Fixture::build().await;
        fixture.set_content_path(1, "NES/GameA.nes").await;
        fixture.set_content_path(3, "NES/GameB.nes").await;

        // Session 1 ends indeterminate, so its baseline is retained.
        fixture.stability.unstable("Nestopia/GameA.state1");
        let first = fixture.begin_session(CORE_A, 1).await;
        fixture.write("Nestopia/GameA.state1", b"game A, never settled");
        fixture
            .end_session(first, PlaySessionOutcome::Completed)
            .await;
        fixture.service.reconcile_session(first).await;
        assert!(fixture.save_states.baseline(first).await.unwrap().is_some());
        assert!(fixture.states().await.is_empty());

        // Session 2 — a different game, a different core — runs and reconciles cleanly.
        fixture
            .run_session(CORE_B, 3, &[("bsnes-mercury/GameB.state1", b"game B")])
            .await;
        let owned_by_b = fixture.only_state().await;
        assert_eq!(owned_by_b.provenance.game_id, GameId(2));
        assert_eq!(
            owned_by_b.provenance.core_binary_sha256,
            sha256_bytes(CORE_B)
        );

        // Now the retained baseline is retried at startup. Game B's state is not in it.
        fixture.service.reconcile_on_startup().await.unwrap();

        // Nothing was attributed, and Game B's state is untouched: same id, same provenance,
        // same status. It was neither superseded nor re-registered under Game A.
        let after = fixture
            .save_states
            .save_state(owned_by_b.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(after, owned_by_b);
        assert_eq!(fixture.states().await, vec![owned_by_b]);
        assert!(fixture
            .service
            .list_save_states(GameId(1))
            .await
            .unwrap()
            .is_empty());
        // The unattributable baseline is dropped rather than retried forever, and the file it
        // could not prove stays on disk untouched.
        assert!(fixture.save_states.baseline(first).await.unwrap().is_none());
        assert_eq!(
            fixture.read("Nestopia/GameA.state1"),
            b"game A, never settled"
        );
    }

    /// Two games whose ROMs share a basename collide on one RetroArch state path under one core.
    ///
    /// Refreshing on the core binary alone would move the first game's row onto the second game's
    /// bytes while keeping the first game's ids — a state listed under the wrong game, which would
    /// boot the wrong ROM when loaded.
    #[tokio::test]
    async fn a_colliding_state_path_from_another_game_supersedes_rather_than_refreshing() {
        let fixture = Fixture::build().await;
        fixture.set_content_path(1, "NES/Tetris.nes").await;
        fixture.set_content_path(3, "GBA/Tetris.gba").await;
        // Game 1 saves at this path...
        fixture
            .run_session(CORE_A, 1, &[("Nestopia/Tetris.state1", b"game one")])
            .await;
        let first = fixture.only_state().await;
        assert_eq!(first.provenance.game_id, GameId(1));

        // ...and game 2, whose ROM happens to share the basename, saves at the same path with the
        // very same core binary.
        fixture
            .run_session(CORE_A, 3, &[("Nestopia/Tetris.state1", b"game two")])
            .await;

        // The first game's row was *not* refreshed onto the second game's bytes.
        let old = fixture
            .save_states
            .save_state(first.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(old.status, SaveStateStatus::Superseded);
        assert_eq!(old.provenance.game_id, GameId(1));
        assert_eq!(old.state.sha256, first.state.sha256);

        // A new object owns the file, under the game that really wrote it.
        let current = fixture.only_state().await;
        assert_ne!(current.id, first.id);
        assert_eq!(current.provenance.game_id, GameId(2));
        assert_eq!(current.provenance.content_unit_id, ContentUnitId(3));
        assert_eq!(current.state.sha256, sha256_bytes(b"game two"));

        // And each game lists only its own.
        assert!(fixture
            .service
            .list_save_states(GameId(1))
            .await
            .unwrap()
            .is_empty());
        let listed = fixture.service.list_save_states(GameId(2)).await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, current.id);
    }

    /// The listing must not retire a state a running emulator is mid-write on.
    ///
    /// `missing` is never reopened, so concluding it from a half-written file would cost the state
    /// its identity, its `created_at`, and its history — the session that ends would then register
    /// a brand-new object at the same path.
    #[tokio::test]
    async fn the_listing_never_marks_a_state_missing_while_a_managed_session_is_active() {
        let fixture = Fixture::build().await;
        fixture
            .run_session(
                CORE_A,
                1,
                &[("Nestopia/Synthetic.state1", b"registered bytes")],
            )
            .await;
        let state = fixture.only_state().await;

        // A game is running and the file is mid-write, so its size no longer matches.
        fixture.launch.active.store(true, Ordering::Relaxed);
        fixture.write("Nestopia/Synthetic.state1", b"half");

        let views = fixture.service.list_save_states(GameId(1)).await.unwrap();

        // Still listed, still `available`, and honestly reported as temporarily unavailable.
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].id, state.id);
        assert_eq!(
            views[0].capabilities.loadability,
            SaveStateLoadability::TemporarilyBlocked
        );
        assert!(!views[0].capabilities.deletable);
        assert_eq!(
            fixture
                .save_states
                .save_state(state.id)
                .await
                .unwrap()
                .unwrap()
                .status,
            SaveStateStatus::Available
        );

        // When the session ends, the same object takes on the finished content.
        fixture.launch.active.store(false, Ordering::Relaxed);
        let session = fixture.begin_session(CORE_A, 1).await;
        fixture.write("Nestopia/Synthetic.state1", b"the finished save");
        fixture
            .end_session(session, PlaySessionOutcome::Completed)
            .await;
        fixture.service.reconcile_session(session).await;

        let updated = fixture.only_state().await;
        assert_eq!(updated.id, state.id, "identity and history are preserved");
        assert_eq!(updated.created_at, state.created_at);
        assert_eq!(updated.state.sha256, sha256_bytes(b"the finished save"));
    }

    /// MEDIUM-3 regression: a refresh that proves no thumbnail exposes no thumbnail — it never
    /// keeps presenting the *previous* version's proved image as if it belonged to the new bytes.
    #[tokio::test]
    async fn a_refresh_without_a_proved_thumbnail_exposes_none_rather_than_the_previous_ones() {
        let fixture = Fixture::build().await;
        fixture
            .run_session(
                CORE_A,
                1,
                &[
                    ("Nestopia/Synthetic.state1", b"first save"),
                    ("Nestopia/Synthetic.state1.png", b"the thumbnail"),
                ],
            )
            .await;
        let original = fixture.only_state().await;
        assert!(original.thumbnail.is_some());

        // The next session overwrites the state but its thumbnail does not settle.
        fixture.stability.unstable("Nestopia/Synthetic.state1.png");
        fixture
            .run_session(
                CORE_A,
                1,
                &[
                    ("Nestopia/Synthetic.state1", b"second save"),
                    ("Nestopia/Synthetic.state1.png", b"half-written image"),
                ],
            )
            .await;

        let refreshed = fixture.only_state().await;
        assert_eq!(refreshed.id, original.id, "identity and history are preserved");
        assert_eq!(refreshed.state.sha256, sha256_bytes(b"second save"));
        // The new bytes are not associated with the previous version's proved thumbnail — this
        // controlled launch never proved that relationship for *these* bytes, so the exposed
        // thumbnail is `None` and the frontend renders the placeholder rather than a stale image.
        assert_eq!(refreshed.thumbnail, None);
        assert_ne!(refreshed.thumbnail, original.thumbnail);
    }

    // ================================================================ thumbnails

    #[tokio::test]
    async fn a_thumbnail_is_associated_only_when_this_session_proved_it() {
        let fixture = Fixture::build().await;
        fixture.set_content_path(1, "NES/New.nes").await;
        // A pre-existing image that is *not* part of the session's delta, and a thumbnail of a
        // state this session did not write.
        fixture.write("Nestopia/Old.state9", b"old state");
        fixture.write("Nestopia/Old.state9.png", b"old thumbnail");

        fixture
            .run_session(
                CORE_A,
                1,
                &[
                    ("Nestopia/New.state1", b"new state"),
                    ("Nestopia/New.state1.png", b"new thumbnail"),
                ],
            )
            .await;

        let state = fixture.only_state().await;
        let thumbnail = state.thumbnail.expect("the proved thumbnail is associated");
        assert_eq!(thumbnail.relative_path.as_str(), "Nestopia/New.state1.png");
        assert_eq!(thumbnail.sha256, sha256_bytes(b"new thumbnail"));
        assert_eq!(thumbnail.size_bytes, 13);
        // The old image was never associated with anything.
        assert!(fixture.exists("Nestopia/Old.state9.png"));
    }

    #[tokio::test]
    async fn a_state_without_a_provable_thumbnail_stays_valid_with_none() {
        let fixture = Fixture::build().await;
        // The image exists from before, so the session's delta does not include it.
        fixture.write("Nestopia/Synthetic.state1.png", b"stale image");
        fixture
            .run_session(CORE_A, 1, &[("Nestopia/Synthetic.state1", b"state bytes")])
            .await;

        let state = fixture.only_state().await;
        assert_eq!(state.status, SaveStateStatus::Available);
        assert!(
            state.thumbnail.is_none(),
            "an unproved image is never borrowed"
        );
        assert!(fixture.exists("Nestopia/Synthetic.state1.png"));
    }

    #[tokio::test]
    async fn an_unstable_thumbnail_leaves_a_valid_state_without_one() {
        let fixture = Fixture::build().await;
        fixture.stability.unstable("Nestopia/Synthetic.state1.png");
        fixture
            .run_session(
                CORE_A,
                1,
                &[
                    ("Nestopia/Synthetic.state1", b"state bytes"),
                    ("Nestopia/Synthetic.state1.png", b"half-written"),
                ],
            )
            .await;

        let state = fixture.only_state().await;
        assert_eq!(state.status, SaveStateStatus::Available);
        assert!(state.thumbnail.is_none());
    }

    // ================================================================ listing

    #[tokio::test]
    async fn the_listing_reports_only_available_states_with_honest_capabilities() {
        let fixture = Fixture::build().await;
        fixture.set_content_path(1, "NES/A.nes").await;
        fixture.set_content_path(2, "NES/B.nes").await;
        fixture
            .run_session(CORE_A, 1, &[("Nestopia/A.state1", b"from core A")])
            .await;
        fixture
            .run_session(CORE_B, 2, &[("Nestopia/B.state2", b"from core B")])
            .await;

        let views = fixture.service.list_save_states(GameId(1)).await.unwrap();
        assert_eq!(views.len(), 2);
        // Ordered by the backend, most recently updated first.
        assert!(views[0].updated_at >= views[1].updated_at);
        for view in &views {
            assert_eq!(view.capabilities.loadability, SaveStateLoadability::Ready);
            assert!(view.capabilities.deletable);
            assert!(view.thumbnail_ref.is_none());
        }
        // Two content units, so a disc label disambiguates.
        assert_eq!(views[0].content_unit_label.as_deref(), Some("Disc 2"));
        assert_eq!(views[1].content_unit_label.as_deref(), Some("Disc 1"));

        // The historical core of one of them disappears: that state is not loadable, but it stays
        // visible and stays deletable.
        fixture.runtime.remove(CORE_B);
        let views = fixture.service.list_save_states(GameId(1)).await.unwrap();
        let from_b = views
            .iter()
            .find(|view| view.content_unit_id == ContentUnitId(2))
            .unwrap();
        assert_eq!(
            from_b.capabilities.loadability,
            SaveStateLoadability::CoreUnavailable
        );
        assert!(from_b.capabilities.deletable, "deleting needs no emulator");

        // While a managed game runs, both mutations are refused and the listing says so.
        fixture.launch.active.store(true, Ordering::Relaxed);
        for view in fixture.service.list_save_states(GameId(1)).await.unwrap() {
            assert_eq!(
                view.capabilities.loadability,
                SaveStateLoadability::TemporarilyBlocked
            );
            assert!(!view.capabilities.deletable);
        }
    }

    #[tokio::test]
    async fn a_single_content_unit_game_shows_no_disc_label() {
        let fixture = Fixture::build().await;
        fixture.set_content_path(3, "NES/Other.nes").await;
        fixture
            .run_session(CORE_A, 3, &[("Nestopia/Other.state1", b"other game")])
            .await;

        let views = fixture.service.list_save_states(GameId(2)).await.unwrap();
        assert_eq!(views.len(), 1);
        assert!(views[0].content_unit_label.is_none());
    }

    #[tokio::test]
    async fn the_listing_transitions_a_vanished_state_to_missing_and_drops_it() {
        let fixture = Fixture::build().await;
        fixture
            .run_session(CORE_A, 1, &[("Nestopia/Synthetic.state1", b"state bytes")])
            .await;
        let state = fixture.only_state().await;

        std::fs::remove_file(fixture.states_root.join("Nestopia/Synthetic.state1")).unwrap();

        assert!(fixture
            .service
            .list_save_states(GameId(1))
            .await
            .unwrap()
            .is_empty());
        assert_eq!(
            fixture
                .save_states
                .save_state(state.id)
                .await
                .unwrap()
                .unwrap()
                .status,
            SaveStateStatus::Missing
        );
    }

    #[tokio::test]
    async fn a_state_with_a_proved_thumbnail_exposes_an_opaque_reference_and_no_path() {
        let fixture = Fixture::build().await;
        fixture
            .run_session(
                CORE_A,
                1,
                &[
                    ("Nestopia/Synthetic.state1", b"state bytes"),
                    ("Nestopia/Synthetic.state1.png", b"thumbnail"),
                ],
            )
            .await;

        let views = fixture.service.list_save_states(GameId(1)).await.unwrap();
        let reference = views[0].thumbnail_ref.as_deref().expect("a reference");
        assert!(reference.ends_with(&format!("save-state-thumbnail/{}", views[0].id.0)));
        // Never a filesystem path, and never a digest.
        let serialized = serde_json::to_string(&views).unwrap();
        assert!(!serialized.contains("Nestopia/"));
        assert!(!serialized.contains(&sha256_bytes(b"state bytes").to_hex()));

        // The bytes are served only after full re-verification.
        assert!(fixture
            .service
            .verified_thumbnail(views[0].id)
            .await
            .is_ok());
        fixture.write("Nestopia/Synthetic.state1.png", b"tampered thumbnail");
        assert_eq!(
            fixture.service.verified_thumbnail(views[0].id).await,
            Err(SaveStateError::IntegrityMismatch)
        );
    }

    // ================================================================ load

    #[tokio::test]
    async fn a_controlled_load_resolves_every_fact_from_the_identity_alone() {
        let fixture = Fixture::build().await;
        fixture.set_content_path(2, "SNES/Synthetic.sfc").await;
        fixture
            .run_session(
                CORE_A,
                2,
                &[("Nestopia/Synthetic.state7", b"disc two state")],
            )
            .await;
        let state = fixture.only_state().await;

        let response = fixture.service.load_save_state(state.id).await;
        assert!(matches!(response, LoadSaveStateResponse::Started { .. }));

        let plans = fixture.launch.plans.lock().unwrap();
        assert_eq!(plans.len(), 1);
        let plan = &plans[0];
        assert_eq!(plan.save_state_id, state.id);
        assert_eq!(plan.game_id, GameId(1));
        // The exact recorded content unit — a Disc 2 state is never offered as Disc 1.
        assert_eq!(plan.content_unit_id, ContentUnitId(2));
        assert_eq!(plan.core_binary_sha256, sha256_bytes(CORE_A));
        assert_eq!(plan.slot, state.slot);
    }

    #[tokio::test]
    async fn every_load_precondition_is_reproved_and_refuses_without_launching() {
        let fixture = Fixture::build().await;
        fixture
            .run_session(CORE_A, 1, &[("Nestopia/Synthetic.state1", b"state bytes")])
            .await;
        let state = fixture.only_state().await;

        // An identity that does not exist.
        assert_eq!(
            fixture.service.load_save_state(SaveStateId(9_999)).await,
            LoadSaveStateResponse::refused(SaveStateError::NotFound)
        );

        // A managed game is running.
        fixture.launch.active.store(true, Ordering::Relaxed);
        assert_eq!(
            fixture.service.load_save_state(state.id).await,
            LoadSaveStateResponse::refused(SaveStateError::TemporarilyBlocked)
        );
        fixture.launch.active.store(false, Ordering::Relaxed);

        // The exact historical core binary is gone.
        fixture.runtime.remove(CORE_A);
        assert_eq!(
            fixture.service.load_save_state(state.id).await,
            LoadSaveStateResponse::refused(SaveStateError::CoreUnavailable)
        );
        fixture
            .runtime
            .available
            .lock()
            .unwrap()
            .push(sha256_bytes(CORE_A));

        // The game's content is no longer available.
        sqlx::query("UPDATE content_units SET availability = 'missing' WHERE id = 1")
            .execute(&fixture.pool)
            .await
            .unwrap();
        assert_eq!(
            fixture.service.load_save_state(state.id).await,
            LoadSaveStateResponse::refused(SaveStateError::Unavailable)
        );
        sqlx::query("UPDATE content_units SET availability = 'available' WHERE id = 1")
            .execute(&fixture.pool)
            .await
            .unwrap();

        // Nothing was ever launched by any of them.
        assert!(fixture.launch.plans.lock().unwrap().is_empty());
        // And the state is still perfectly loadable once the world is right again.
        assert!(matches!(
            fixture.service.load_save_state(state.id).await,
            LoadSaveStateResponse::Started { .. }
        ));
    }

    /// A running emulator rewriting its own slot must not cost the player a Save State.
    ///
    /// `verified_state` marks a mismatched state `missing`, and a live RetroArch is entitled to be
    /// mid-write. Both mutations therefore refuse *before* they verify, and the session that ends
    /// reconciles the new content properly.
    #[tokio::test]
    async fn a_state_being_rewritten_by_a_running_game_is_never_marked_missing() {
        let fixture = Fixture::build().await;
        fixture
            .run_session(
                CORE_A,
                1,
                &[("Nestopia/Synthetic.state1", b"registered bytes")],
            )
            .await;
        let state = fixture.only_state().await;

        // A game is running and has just overwritten the slot this state occupies.
        fixture.launch.active.store(true, Ordering::Relaxed);
        fixture.write(
            "Nestopia/Synthetic.state1",
            b"mid-write bytes from the live emulator",
        );

        assert_eq!(
            fixture.service.load_save_state(state.id).await,
            LoadSaveStateResponse::refused(SaveStateError::TemporarilyBlocked)
        );
        assert_eq!(
            fixture.service.delete_save_state(state.id).await,
            DeleteSaveStateResponse::failed(SaveStateError::TemporarilyBlocked)
        );

        // Still available: the digest was never consulted, so nothing was concluded from it.
        assert_eq!(
            fixture
                .save_states
                .save_state(state.id)
                .await
                .unwrap()
                .unwrap()
                .status,
            SaveStateStatus::Available
        );

        // And once the session ends, reconciliation records the new content on the same object.
        fixture.launch.active.store(false, Ordering::Relaxed);
        let session = fixture.begin_session(CORE_A, 1).await;
        fixture.write("Nestopia/Synthetic.state1", b"the finished save");
        fixture
            .end_session(session, PlaySessionOutcome::Completed)
            .await;
        fixture.service.reconcile_session(session).await;

        let updated = fixture.only_state().await;
        assert_eq!(updated.id, state.id);
        assert_eq!(updated.state.sha256, sha256_bytes(b"the finished save"));
    }

    #[tokio::test]
    async fn a_tampered_state_refuses_to_load_and_the_new_bytes_are_left_untouched() {
        let fixture = Fixture::build().await;
        fixture
            .run_session(
                CORE_A,
                1,
                &[("Nestopia/Synthetic.state1", b"registered bytes")],
            )
            .await;
        let state = fixture.only_state().await;

        // Changed outside any attributable controlled launch.
        fixture.write("Nestopia/Synthetic.state1", b"unexplained new bytes");

        assert_eq!(
            fixture.service.load_save_state(state.id).await,
            LoadSaveStateResponse::refused(SaveStateError::IntegrityMismatch)
        );
        // The registered identity is no longer valid, so the object leaves the normal list...
        assert_eq!(
            fixture
                .save_states
                .save_state(state.id)
                .await
                .unwrap()
                .unwrap()
                .status,
            SaveStateStatus::Missing
        );
        // ...the old id can no longer delete anything...
        assert_eq!(
            fixture.service.delete_save_state(state.id).await,
            DeleteSaveStateResponse::failed(SaveStateError::Unavailable)
        );
        // ...and the untrusted file is exactly as the writer left it.
        assert_eq!(
            fixture.read("Nestopia/Synthetic.state1"),
            b"unexplained new bytes"
        );
        assert!(fixture.launch.plans.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_failed_launch_never_damages_the_save_state() {
        let fixture = Fixture::build().await;
        fixture
            .run_session(CORE_A, 1, &[("Nestopia/Synthetic.state1", b"state bytes")])
            .await;
        let before = fixture.only_state().await;
        fixture.launch.fail.store(true, Ordering::Relaxed);

        let response = fixture.service.load_save_state(before.id).await;

        // The launch pipeline's own verdict, kept distinct from a Save-State refusal.
        assert!(matches!(
            response,
            LoadSaveStateResponse::LaunchFailed { .. }
        ));
        // Nothing about the state changed: not its status, not its digest, not its provenance.
        let after = fixture
            .save_states
            .save_state(before.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(after, before);
        assert_eq!(after.status, SaveStateStatus::Available);

        // And a later retry succeeds once the launch works again.
        fixture.launch.fail.store(false, Ordering::Relaxed);
        assert!(matches!(
            fixture.service.load_save_state(before.id).await,
            LoadSaveStateResponse::Started { .. }
        ));
    }

    // ================================================================ delete

    #[tokio::test]
    async fn a_safe_delete_removes_the_state_and_its_proved_thumbnail() {
        let fixture = Fixture::build().await;
        fixture
            .run_session(
                CORE_A,
                1,
                &[
                    ("Nestopia/Synthetic.state1", b"state bytes"),
                    ("Nestopia/Synthetic.state1.png", b"thumbnail"),
                    ("Nestopia/Sibling.state2", b"keep me"),
                ],
            )
            .await;
        let state = fixture
            .states()
            .await
            .into_iter()
            .find(|state| state.slot.get() == 1)
            .unwrap();

        assert_eq!(
            fixture.service.delete_save_state(state.id).await,
            DeleteSaveStateResponse::Deleted {
                save_state_id: state.id
            }
        );

        assert!(!fixture.exists("Nestopia/Synthetic.state1"));
        assert!(!fixture.exists("Nestopia/Synthetic.state1.png"));
        // The sibling is untouched, and the lifecycle is persisted.
        assert_eq!(fixture.read("Nestopia/Sibling.state2"), b"keep me");
        assert_eq!(
            fixture
                .save_states
                .save_state(state.id)
                .await
                .unwrap()
                .unwrap()
                .status,
            SaveStateStatus::Deleted
        );
    }

    #[tokio::test]
    async fn a_state_whose_historical_core_is_gone_is_still_safely_deletable() {
        let fixture = Fixture::build().await;
        fixture
            .run_session(CORE_A, 1, &[("Nestopia/Synthetic.state1", b"state bytes")])
            .await;
        let state = fixture.only_state().await;
        fixture.runtime.remove(CORE_A);

        // It cannot be loaded...
        assert_eq!(
            fixture.service.load_save_state(state.id).await,
            LoadSaveStateResponse::refused(SaveStateError::CoreUnavailable)
        );
        // ...and deleting it needs no emulator at all.
        assert!(matches!(
            fixture.service.delete_save_state(state.id).await,
            DeleteSaveStateResponse::Deleted { .. }
        ));
        assert!(!fixture.exists("Nestopia/Synthetic.state1"));
    }

    #[tokio::test]
    async fn a_delete_is_refused_while_a_managed_game_is_active() {
        let fixture = Fixture::build().await;
        fixture
            .run_session(CORE_A, 1, &[("Nestopia/Synthetic.state1", b"state bytes")])
            .await;
        let state = fixture.only_state().await;
        fixture.launch.active.store(true, Ordering::Relaxed);

        assert_eq!(
            fixture.service.delete_save_state(state.id).await,
            DeleteSaveStateResponse::failed(SaveStateError::TemporarilyBlocked)
        );
        assert!(fixture.exists("Nestopia/Synthetic.state1"));
    }

    #[tokio::test]
    async fn a_delete_refuses_a_symlink_standing_where_the_registered_file_was() {
        let fixture = Fixture::build().await;
        fixture
            .run_session(CORE_A, 1, &[("Nestopia/Synthetic.state1", b"state bytes")])
            .await;
        let state = fixture.only_state().await;

        let target = fixture.states_root.join("Nestopia/Synthetic.state1");
        let elsewhere = fixture.states_root.join("Nestopia/moved.bin");
        std::fs::rename(&target, &elsewhere).unwrap();
        std::os::unix::fs::symlink(&elsewhere, &target).unwrap();

        assert_eq!(
            fixture.service.delete_save_state(state.id).await,
            DeleteSaveStateResponse::failed(SaveStateError::UnsafeFilesystemTarget)
        );
        // Neither the link nor its target was deleted.
        assert!(target.symlink_metadata().unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read(&elsewhere).unwrap(), b"state bytes");
    }

    #[tokio::test]
    async fn a_state_deletes_safely_even_when_its_thumbnail_can_no_longer_be_verified() {
        let fixture = Fixture::build().await;
        fixture
            .run_session(
                CORE_A,
                1,
                &[
                    ("Nestopia/Synthetic.state1", b"state bytes"),
                    ("Nestopia/Synthetic.state1.png", b"thumbnail"),
                ],
            )
            .await;
        let state = fixture.only_state().await;
        // The thumbnail changed outside RetroFrontier, so its registered identity is gone.
        fixture.write(
            "Nestopia/Synthetic.state1.png",
            b"a different image entirely",
        );

        assert!(matches!(
            fixture.service.delete_save_state(state.id).await,
            DeleteSaveStateResponse::Deleted { .. }
        ));

        // Safe deletion of the state was not sacrificed...
        assert!(!fixture.exists("Nestopia/Synthetic.state1"));
        // ...and the questionable image was left exactly where it is.
        assert_eq!(
            fixture.read("Nestopia/Synthetic.state1.png"),
            b"a different image entirely"
        );
        // The retained thumbnail identity records that RetroFrontier did *not* remove it.
        let deleted = fixture
            .save_states
            .save_state(state.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(deleted.status, SaveStateStatus::Deleted);
        assert!(deleted.thumbnail.is_some());
    }

    #[tokio::test]
    async fn a_persistence_failure_after_the_physical_delete_converges_on_the_filesystem_truth() {
        let fixture = Fixture::build().await;
        fixture
            .run_session(CORE_A, 1, &[("Nestopia/Synthetic.state1", b"state bytes")])
            .await;
        let state = fixture.only_state().await;

        // The physical delete succeeds and the lifecycle write then fails.
        sqlx::query("DROP TRIGGER IF EXISTS none")
            .execute(&fixture.pool)
            .await
            .ok();
        delete_then_fail_persistence(&fixture, state.id).await;

        // For one moment the row still claims `available` while its file is gone — the documented
        // window. The next listing re-verifies and converges on the physical truth.
        assert!(!fixture.exists("Nestopia/Synthetic.state1"));
        assert!(fixture
            .service
            .list_save_states(GameId(1))
            .await
            .unwrap()
            .is_empty());
        assert_eq!(
            fixture
                .save_states
                .save_state(state.id)
                .await
                .unwrap()
                .unwrap()
                .status,
            SaveStateStatus::Missing
        );
    }

    /// Simulate "the file was deleted, the database write did not happen".
    ///
    /// The physical delete is performed through the same adapter the service uses, and the
    /// lifecycle row is deliberately left untouched.
    async fn delete_then_fail_persistence(fixture: &Fixture, id: SaveStateId) {
        let state = fixture.save_states.save_state(id).await.unwrap().unwrap();
        crate::services::save_state_fs::delete_verified_managed_file(
            &fixture.states_root,
            &state.state.relative_path,
            state.state.size_bytes,
            state.state.sha256,
        )
        .expect("the physical delete succeeds");
    }

    // ================================================================ boundaries

    #[tokio::test]
    async fn normal_save_data_is_never_enumerated_interpreted_or_deleted() {
        let fixture = Fixture::build().await;
        // The adapter is only ever given the states root, so `saves/` is unreachable by
        // construction. This proves the *behaviour* that follows from it.
        fixture
            .run_session(
                CORE_A,
                1,
                &[
                    ("Nestopia/Synthetic.state1", b"a save state"),
                    ("Nestopia/Synthetic.srm", b"opaque SRAM"),
                    ("Nestopia/Synthetic.sav", b"opaque save data"),
                ],
            )
            .await;

        let views = fixture.service.list_save_states(GameId(1)).await.unwrap();
        assert_eq!(views.len(), 1, "only the save state is a domain object");

        // Deleting the state leaves every piece of save data exactly where it is.
        fixture.service.delete_save_state(views[0].id).await;
        assert_eq!(fixture.read("Nestopia/Synthetic.srm"), b"opaque SRAM");
        assert_eq!(fixture.read("Nestopia/Synthetic.sav"), b"opaque save data");
    }

    /// A capability snapshot never authorizes anything, however stale it is.
    ///
    /// The listing reports what was true when it ran. Everything can change afterwards, and the
    /// action re-proves every invariant from scratch — so a frontend holding a `ready`, deletable
    /// view is refused exactly as if it had held none.
    #[tokio::test]
    async fn a_stale_capability_snapshot_authorizes_nothing() {
        let fixture = Fixture::build().await;
        fixture
            .run_session(CORE_A, 1, &[("Nestopia/Synthetic.state1", b"state bytes")])
            .await;

        // The snapshot the frontend would be holding: everything is fine.
        let stale = fixture.service.list_save_states(GameId(1)).await.unwrap();
        assert_eq!(
            stale[0].capabilities.loadability,
            SaveStateLoadability::Ready
        );
        assert!(stale[0].capabilities.deletable);
        let id = stale[0].id;

        // The world moves on underneath it in three independent ways.
        fixture.launch.active.store(true, Ordering::Relaxed);
        assert_eq!(
            fixture.service.load_save_state(id).await,
            LoadSaveStateResponse::refused(SaveStateError::TemporarilyBlocked)
        );
        assert_eq!(
            fixture.service.delete_save_state(id).await,
            DeleteSaveStateResponse::failed(SaveStateError::TemporarilyBlocked)
        );

        fixture.launch.active.store(false, Ordering::Relaxed);
        fixture.runtime.remove(CORE_A);
        assert_eq!(
            fixture.service.load_save_state(id).await,
            LoadSaveStateResponse::refused(SaveStateError::CoreUnavailable)
        );

        fixture.write("Nestopia/Synthetic.state1", b"changed underneath");
        assert_eq!(
            fixture.service.delete_save_state(id).await,
            DeleteSaveStateResponse::failed(SaveStateError::IntegrityMismatch)
        );
        // Nothing was launched and nothing was deleted on the strength of the stale view.
        assert!(fixture.launch.plans.lock().unwrap().is_empty());
        assert!(fixture.exists("Nestopia/Synthetic.state1"));
    }

    /// The same content basename under two cores is two states, not one.
    ///
    /// `sort_savestates_enable` puts each core's states in its own directory, so identical
    /// basenames are ordinary — and because identity is the registered *path* plus proved
    /// provenance rather than the basename, neither collides with the other.
    #[tokio::test]
    async fn the_same_content_basename_under_two_cores_produces_two_independent_states() {
        let fixture = Fixture::build().await;
        fixture
            .run_session(CORE_A, 1, &[("Nestopia/Synthetic.state1", b"from core A")])
            .await;
        fixture
            .run_session(
                CORE_B,
                1,
                &[("bsnes-mercury/Synthetic.state1", b"from core B")],
            )
            .await;

        let states = fixture.states().await;
        assert_eq!(states.len(), 2, "two distinct physical paths, two objects");
        assert_eq!(
            states
                .iter()
                .map(|state| state.state.relative_path.as_str())
                .collect::<Vec<_>>(),
            vec![
                "Nestopia/Synthetic.state1",
                "bsnes-mercury/Synthetic.state1"
            ]
        );
        // Same slot and same basename, different provenance and different bytes.
        assert_eq!(states[0].slot, states[1].slot);
        assert_ne!(
            states[0].provenance.core_binary_sha256,
            states[1].provenance.core_binary_sha256
        );

        // Deleting one leaves the other completely untouched.
        assert!(matches!(
            fixture.service.delete_save_state(states[0].id).await,
            DeleteSaveStateResponse::Deleted { .. }
        ));
        assert!(!fixture.exists("Nestopia/Synthetic.state1"));
        assert_eq!(
            fixture.read("bsnes-mercury/Synthetic.state1"),
            b"from core B"
        );
        assert_eq!(
            fixture
                .save_states
                .save_state(states[1].id)
                .await
                .unwrap()
                .unwrap()
                .status,
            SaveStateStatus::Available
        );
    }

    // ================================================================ HIGH-1: delete/launch serialization

    /// HIGH-1 regression (load-vs-delete): a load and a delete of the exact same Save State must
    /// serialize safely rather than interleave.
    ///
    /// A delete is made to pause deterministically *after* it has entered its exclusion section and
    /// passed its first eligibility check — exactly the window the finding describes as
    /// unprotected before this fix — and a concurrent load is attempted while it is paused there.
    /// Before HIGH-1, nothing stopped the load's underlying launch attempt from proceeding while
    /// the delete was mid-flight. After it, the load's launch attempt shares the very same
    /// in-process exclusion the delete now holds, so it is refused outright rather than racing the
    /// file the delete may be about to remove.
    #[tokio::test]
    async fn a_concurrent_load_and_delete_of_the_same_save_state_serialize_safely() {
        let fixture = Fixture::build().await;
        fixture
            .run_session(CORE_A, 1, &[("Nestopia/Synthetic.state1", b"state bytes")])
            .await;
        let state = fixture.only_state().await;

        let reached = Arc::new(tokio::sync::Notify::new());
        let resume = Arc::new(tokio::sync::Notify::new());
        let checkpointed = SaveStateApplicationService::new(
            fixture.save_states.clone(),
            LibraryRepository::new(fixture.pool.clone()),
            fixture.sessions.clone(),
            fixture.runtime.clone(),
            &fixture.states_root,
            SaveStateConfig::default(),
        )
        .with_delete_checkpoint(reached.clone(), resume.clone());
        checkpointed.attach_launch(fixture.launch.clone());

        let delete_id = state.id;
        let delete_task =
            tokio::spawn(async move { checkpointed.delete_save_state(delete_id).await });

        // The delete has entered its exclusion section and passed its first eligibility check —
        // it now holds the same in-process critical section a launch would need.
        reached.notified().await;

        // A concurrent load attempt must not be able to reach a launch while the delete is
        // deciding whether to destroy the very file that load would need. It is refused outright,
        // and — because `StubLaunch::launch_save_state` itself contends for the same section a
        // real launch would — it never even records a launch attempt.
        let racing_load = fixture.service.load_save_state(state.id).await;
        assert!(
            matches!(
                racing_load,
                LoadSaveStateResponse::LaunchFailed { .. }
            ),
            "a load racing an in-flight delete must be refused, not started: {racing_load:?}"
        );
        assert!(
            fixture.launch.plans.lock().unwrap().is_empty(),
            "no launch attempt for the racing load may reach the launch pipeline"
        );
        // The file itself is of course still exactly what it was — the delete has not resumed yet.
        assert!(fixture.exists("Nestopia/Synthetic.state1"));

        // Let the delete finish.
        resume.notify_one();
        let outcome = delete_task.await.unwrap();
        assert!(matches!(
            outcome,
            DeleteSaveStateResponse::Deleted { .. }
        ));
        assert!(!fixture.exists("Nestopia/Synthetic.state1"));

        // Only now, once the delete has genuinely completed, does a load see the settled truth —
        // never a half-deleted file. The row survives as a closed `deleted` lifecycle, so the
        // verdict is `Unavailable`, not a dangling reference to bytes that might still exist.
        assert_eq!(
            fixture.service.load_save_state(state.id).await,
            LoadSaveStateResponse::refused(SaveStateError::Unavailable)
        );
    }

    /// With no launch port attached the service refuses every mutation rather than performing an
    /// unguarded one.
    ///
    /// The state is real and otherwise perfectly loadable and deletable, so the refusal can only
    /// come from the missing guard — which is the point.
    #[tokio::test]
    async fn an_unattached_service_fails_closed() {
        let fixture = Fixture::build().await;
        fixture
            .run_session(CORE_A, 1, &[("Nestopia/Synthetic.state1", b"state bytes")])
            .await;
        let state = fixture.only_state().await;
        // Everything is in order while the port is attached.
        assert_eq!(
            fixture
                .service
                .list_save_states(GameId(1))
                .await
                .unwrap()
                .first()
                .map(|view| view.capabilities.loadability),
            Some(SaveStateLoadability::Ready)
        );

        // A second service over exactly the same durable state, with no launch port.
        let unattached = SaveStateApplicationService::new(
            SaveStateRepository::new(fixture.pool.clone()),
            LibraryRepository::new(fixture.pool.clone()),
            LaunchRepository::new(fixture.pool.clone()),
            fixture.runtime.clone(),
            &fixture.states_root,
            SaveStateConfig::default(),
        );

        assert_eq!(
            unattached.load_save_state(state.id).await,
            LoadSaveStateResponse::refused(SaveStateError::TemporarilyBlocked)
        );
        assert_eq!(
            unattached.delete_save_state(state.id).await,
            DeleteSaveStateResponse::failed(SaveStateError::TemporarilyBlocked)
        );
        // And it really deleted nothing.
        assert!(fixture.exists("Nestopia/Synthetic.state1"));
        // The listing still works and reports the honest blocked capability.
        let views = unattached.list_save_states(GameId(1)).await.unwrap();
        assert_eq!(
            views[0].capabilities.loadability,
            SaveStateLoadability::TemporarilyBlocked
        );
        assert!(!views[0].capabilities.deletable);
    }

    // ================================================================ HIGH-2: content binding

    /// HIGH-2 regression (foreign namespace during session): a perfectly valid-looking managed
    /// slot that appears in the very same delta window must never be attributed to this session
    /// merely because it showed up at the same time — only the exact basename this session's own
    /// content derives is ever registered.
    #[tokio::test]
    async fn a_foreign_content_states_own_slot_is_never_attributed_during_another_sessions_delta() {
        let fixture = Fixture::build().await;
        let session = fixture
            .run_session(
                CORE_A,
                1,
                &[
                    // This session's own content, written under its own basename.
                    ("Nestopia/Synthetic.state1", b"this session's own save"),
                    // A perfectly valid, fully-settled managed slot that just happens to land in
                    // the same delta — but under a foreign game's basename, nothing this
                    // session's own content ever produces.
                    ("Nestopia/Foreign.state1", b"someone else's save"),
                ],
            )
            .await;

        let states = fixture.states().await;
        assert_eq!(
            states.len(),
            1,
            "only this session's own content basename is registered"
        );
        assert_eq!(
            states[0].state.relative_path.as_str(),
            "Nestopia/Synthetic.state1"
        );
        assert_eq!(states[0].provenance.play_session_id, session);
        // The foreign file is left exactly where it is: untouched, unimported, unregistered.
        assert!(fixture.exists("Nestopia/Foreign.state1"));
        assert_eq!(
            fixture.read("Nestopia/Foreign.state1"),
            b"someone else's save"
        );
    }

    /// HIGH-2 regression (verified-path-vs-loaded-path): a row whose file is byte-for-byte
    /// verified must still never be loaded if its own recorded path does not belong to the
    /// content it claims. Ordinary registration (`observe_delta`) can never produce such a row —
    /// it is constructed directly here to prove the load-time check is real defense in depth
    /// against a row established some other way (a direct database write, a future migration, a
    /// bug elsewhere), not merely a restatement of what registration already guarantees.
    #[tokio::test]
    async fn a_verified_file_at_a_path_foreign_to_its_own_content_never_loads() {
        let fixture = Fixture::build().await;
        let session_id = fixture.begin_session(CORE_A, 1).await;

        fixture.write("Nestopia/Foreign.state1", b"bytes that verify fine");
        let bytes = fixture.read("Nestopia/Foreign.state1");
        let row = fixture
            .save_states
            .register_state(&NewSaveState {
                provenance: SaveStateProvenance {
                    game_id: GameId(1),
                    content_unit_id: ContentUnitId(1),
                    play_session_id: session_id,
                    core_id: CoreId::new("nestopia").unwrap(),
                    core_component_id: SafeIdentifier::new("nestopia").unwrap(),
                    core_binary_sha256: sha256_bytes(CORE_A),
                    core_display_version: Some("1.53".to_owned()),
                    core_source_revision: Some("deadbeef".to_owned()),
                    originating_runtime_release_id: SafeIdentifier::new("release-1").unwrap(),
                },
                slot: SaveStateSlot::new(1).unwrap(),
                state: SaveStateFileIdentity {
                    relative_path: RelativePath::new("Nestopia/Foreign.state1").unwrap(),
                    sha256: sha256_bytes(&bytes),
                    size_bytes: bytes.len() as u64,
                },
                thumbnail: None,
            })
            .await
            .unwrap();

        assert_eq!(
            fixture.service.load_save_state(row.id).await,
            LoadSaveStateResponse::refused(SaveStateError::UnsafeFilesystemTarget)
        );
        assert!(fixture.launch.plans.lock().unwrap().is_empty());
    }
}
