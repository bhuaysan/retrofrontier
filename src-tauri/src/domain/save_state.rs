//! The Save-State domain.
//!
//! A managed Save State exists because a *controlled launch* proved its provenance and
//! RetroFrontier subsequently verified the exact physical state content. Nothing in this module
//! knows how RetroArch names a state file: `.state`, `.stateN`, `.state.auto`, `.png`, and the
//! core-reported `library_name` directory are adapter facts and live only in
//! `crate::services::save_state_fs`. A test below asserts this module's source contains none of
//! them, so a naming convention cannot drift into the domain layer.
//!
//! Two existing domain types are reused deliberately rather than duplicated:
//!
//! - [`RelativePath`] is already the validated safe relative-path newtype (no absolute form, no
//!   `\`, no `.`/`..` component, no NUL, no control characters, bounded length). A second
//!   "managed relative path" type would be a parallel implementation of the same rule, so a
//!   `RelativePath::new` failure is mapped to [`SaveStateError::UnsafeFilesystemTarget`] at the
//!   boundary instead.
//! - [`Sha256Digest`] is already the parsed 32-byte digest with hex round-tripping.

use crate::domain::core::CoreId;
use crate::domain::launch::PlaySessionId;
use crate::domain::library::{ContentUnitId, GameId, UnixTimestamp};
use crate::domain::runtime::{RelativePath, SafeIdentifier, Sha256Digest};
use serde::{Deserialize, Serialize};
use std::fmt;

/// The semantic identity of a managed Save State. This is the only Save-State identity that
/// crosses IPC.
///
/// A slot, a filename, an absolute path, a content basename, a RetroArch core directory name, and
/// a timestamp are explicitly *not* Save-State identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SaveStateId(pub i64);

impl fmt::Display for SaveStateId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// The lowest and highest manual slot RetroFrontier manages.
///
/// RetroArch also has an automatic slot and a slot numbered zero. Neither is managed by M9, so
/// there is deliberately no constructor for either: the only way to obtain a `SaveStateSlot` is to
/// pass a number this range accepts.
pub const MIN_MANAGED_SLOT: u16 = 1;
pub const MAX_MANAGED_SLOT: u16 = 999;

/// A manual RetroArch state slot RetroFrontier manages.
///
/// A slot is *scoped metadata*, not identity: the same slot under different immutable core-binary
/// provenance is a different Save State, and two such states are both representable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct SaveStateSlot(u16);

impl<'de> Deserialize<'de> for SaveStateSlot {
    /// Deserialization validates, so an out-of-range slot cannot enter the domain through a
    /// persisted row or a payload.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = u16::deserialize(deserializer)?;
        Self::new(value).map_err(|_| {
            serde::de::Error::custom("a managed save-state slot must be between 1 and 999")
        })
    }
}

impl SaveStateSlot {
    pub fn new(value: u16) -> Result<Self, SaveStateError> {
        if !(MIN_MANAGED_SLOT..=MAX_MANAGED_SLOT).contains(&value) {
            return Err(SaveStateError::Unavailable);
        }
        Ok(Self(value))
    }

    pub const fn get(self) -> u16 {
        self.0
    }

    /// The first slot a normal managed game launch makes active.
    ///
    /// The previously active slot is deliberately not persisted as a RetroFrontier preference, so
    /// every ordinary launch starts here.
    pub const fn default_launch_slot() -> Self {
        Self(MIN_MANAGED_SLOT)
    }
}

impl fmt::Display for SaveStateSlot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// The lifecycle of a managed Save State. Only `Available` appears in the normal Game Detail list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SaveStateStatus {
    /// The registered physical content is present and still matches its registered identity.
    Available,
    /// The previously registered physical content is no longer present — deleted outside
    /// RetroFrontier, or replaced by content that no longer matches the registered identity.
    Missing,
    /// A controlled session proved that the same physical RetroArch slot was replaced by state
    /// content with *different* immutable core-binary provenance.
    Superseded,
    /// RetroFrontier itself safely deleted the registered state after explicit user confirmation.
    Deleted,
}

impl SaveStateStatus {
    pub const ALL: &'static [Self] = &[
        Self::Available,
        Self::Missing,
        Self::Superseded,
        Self::Deleted,
    ];

    pub const fn as_db(self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::Missing => "missing",
            Self::Superseded => "superseded",
            Self::Deleted => "deleted",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "available" => Some(Self::Available),
            "missing" => Some(Self::Missing),
            "superseded" => Some(Self::Superseded),
            "deleted" => Some(Self::Deleted),
            _ => None,
        }
    }

    pub const fn is_available(self) -> bool {
        matches!(self, Self::Available)
    }
}

/// Everything that proves *where a Save State came from*.
///
/// Every field is captured once, from a controlled launch whose facts were already authenticated,
/// and none of it is derived from a filename. `core_binary_sha256` is the decisive core identity
/// and is immutable for the life of the object: there is no method here or on
/// [`SaveStateRepository`](crate::repositories::save_state::SaveStateRepository) that rewrites it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveStateProvenance {
    pub game_id: GameId,
    pub content_unit_id: ContentUnitId,
    pub play_session_id: PlaySessionId,
    pub core_id: CoreId,
    pub core_component_id: SafeIdentifier,
    pub core_binary_sha256: Sha256Digest,
    /// A human-readable core label from the authenticated release manifest. It is recorded so a
    /// state stays describable after its originating Runtime Release is gone, and it is never the
    /// load-compatibility identity.
    pub core_display_version: Option<String>,
    /// The authenticated upstream source revision of the core component, for the same reason.
    pub core_source_revision: Option<String>,
    pub originating_runtime_release_id: SafeIdentifier,
}

/// The exact physical content a Save State is bound to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveStateFileIdentity {
    /// Validated and relative to the RetroFrontier-owned states root. Never absolute.
    pub relative_path: RelativePath,
    pub sha256: Sha256Digest,
    pub size_bytes: u64,
}

/// A state thumbnail's own physical identity, stored independently of the state's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveStateThumbnailIdentity {
    pub relative_path: RelativePath,
    pub sha256: Sha256Digest,
    pub size_bytes: u64,
}

/// One managed Save State.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveState {
    pub id: SaveStateId,
    pub provenance: SaveStateProvenance,
    pub slot: SaveStateSlot,
    pub state: SaveStateFileIdentity,
    /// Present only when the relationship to this state was *proved* from the verified RetroArch
    /// mechanism and a controlled launch delta. A valid state with no provable thumbnail keeps no
    /// thumbnail at all rather than borrowing a nearby image.
    pub thumbnail: Option<SaveStateThumbnailIdentity>,
    pub created_at: UnixTimestamp,
    pub updated_at: UnixTimestamp,
    pub status: SaveStateStatus,
}

/// Whether a *controlled load attempt* is currently permitted.
///
/// This is deliberately not a compatibility claim. Even with the exact same core binary, M9 does
/// not guarantee that a state will deserialize successfully.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SaveStateLoadability {
    /// Every invariant held at the moment this snapshot was taken.
    Ready,
    /// The exact recorded core binary is not available from any currently installed, authenticated,
    /// allowed Runtime installation. The Save State itself is not damaged by this.
    CoreUnavailable,
    /// A managed RetroArch session is launching, running, or of uncertain identity. Nothing about
    /// the Save State is wrong.
    TemporarilyBlocked,
}

impl SaveStateLoadability {
    pub const ALL: &'static [Self] =
        &[Self::Ready, Self::CoreUnavailable, Self::TemporarilyBlocked];

    pub const fn is_ready(self) -> bool {
        matches!(self, Self::Ready)
    }
}

/// What the backend currently believes a user may do with one Save State.
///
/// It is a **UI snapshot only**. Every invariant is re-proved when the action is actually invoked,
/// so stale frontend capability state can never authorize anything. `loadable` and `deletable` are
/// independent: a state whose historical core is gone is still safely deletable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveStateCapabilities {
    pub loadability: SaveStateLoadability,
    pub deletable: bool,
}

/// The bounded Save-State projection Game Detail renders.
///
/// It carries no digest, no filesystem path, and no runtime path. The thumbnail is an opaque
/// reference understood by the native media protocol, resolved back to durable state by Rust.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveStateView {
    pub id: SaveStateId,
    pub game_id: GameId,
    pub content_unit_id: ContentUnitId,
    pub slot: SaveStateSlot,
    pub core_id: CoreId,
    pub core_display_version: Option<String>,
    pub core_source_revision: Option<String>,
    /// Set only when the game has more than one content unit, so a disc label appears exactly when
    /// it disambiguates.
    pub content_unit_label: Option<String>,
    pub created_at: UnixTimestamp,
    pub updated_at: UnixTimestamp,
    pub thumbnail_ref: Option<String>,
    pub capabilities: SaveStateCapabilities,
}

/// One entry of a durable pre-launch save-state filesystem baseline.
///
/// The baseline records cheap physical identity rather than a digest. The approved reconciliation
/// order computes SHA-256 *after* the process ended, and pre-hashing a whole state tree before
/// every launch would add unbounded launch latency without improving provenance. A size-, mtime-
/// and inode-preserving external rewrite is therefore invisible to the delta — which fails
/// *closed*: such a file is simply never attributed to the session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchStateBaselineEntry {
    pub relative_path: RelativePath,
    pub size_bytes: u64,
    pub mtime_nanos: i128,
    pub inode: u64,
}

/// The durable pre-launch baseline of one play session.
///
/// It must exist durably *before* RetroArch is spawned, and it must survive a RetroFrontier
/// restart, so a process adopted after a crash can still be reconciled once it certainly ends.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchStateBaseline {
    pub provenance: SaveStateProvenance,
    pub runtime_installation_id: SafeIdentifier,
    pub captured_at: UnixTimestamp,
    /// How many times reconciliation has run for this baseline without reaching a deterministic
    /// outcome. Bounded so a permanently indeterminate baseline cannot leak forever.
    pub attempts: u32,
    pub entries: Vec<LaunchStateBaselineEntry>,
}

/// Stable semantic Save-State failures.
///
/// There is deliberately no `Corrupt` variant. A digest mismatch means the *registered identity*
/// is no longer valid; it is not proof that the new bytes are corrupt, and one failed state load
/// is not proof of anything at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SaveStateError {
    NotFound,
    Unavailable,
    CoreUnavailable,
    TemporarilyBlocked,
    IntegrityMismatch,
    UnsafeFilesystemTarget,
    /// The file could not be examined at all — the observation failed, rather than proving
    /// anything about the file.
    ///
    /// This is deliberately distinct from [`Self::UnsafeFilesystemTarget`], which is a *proof*
    /// that the target is not the managed regular file it must be. Running out of file
    /// descriptors, a read error, or a momentarily unreadable tree prove nothing, and a
    /// lifecycle that can never be reopened must not be closed on the strength of them.
    Indeterminate,
    ReconciliationFailed,
    LaunchFailed,
    DeleteFailed,
}

impl SaveStateError {
    pub const ALL: &'static [Self] = &[
        Self::NotFound,
        Self::Unavailable,
        Self::CoreUnavailable,
        Self::TemporarilyBlocked,
        Self::IntegrityMismatch,
        Self::UnsafeFilesystemTarget,
        Self::Indeterminate,
        Self::ReconciliationFailed,
        Self::LaunchFailed,
        Self::DeleteFailed,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotFound => "notFound",
            Self::Unavailable => "unavailable",
            Self::CoreUnavailable => "coreUnavailable",
            Self::TemporarilyBlocked => "temporarilyBlocked",
            Self::IntegrityMismatch => "integrityMismatch",
            Self::UnsafeFilesystemTarget => "unsafeFilesystemTarget",
            Self::Indeterminate => "indeterminate",
            Self::ReconciliationFailed => "reconciliationFailed",
            Self::LaunchFailed => "launchFailed",
            Self::DeleteFailed => "deleteFailed",
        }
    }

    /// The user-facing sentence RetroFrontier generates for this code.
    ///
    /// Every message is a fixed string: no operating-system error text, path, digest, or internal
    /// identifier is ever interpolated, and none of them claims that a file is corrupt or that a
    /// state is incompatible.
    pub const fn message(self) -> &'static str {
        match self {
            Self::NotFound => "That save state is no longer known to RetroFrontier.",
            Self::Unavailable => "That save state is not available.",
            Self::CoreUnavailable => {
                "The exact emulation core this save state was made with is not installed, so it \
                 cannot be loaded."
            }
            Self::TemporarilyBlocked => {
                "A game is running or starting. Close it before loading or deleting a save state."
            }
            Self::IntegrityMismatch => {
                "This save state no longer matches what RetroFrontier recorded, so it was left \
                 untouched."
            }
            Self::UnsafeFilesystemTarget => {
                "RetroFrontier could not confirm the save-state file it manages, so it did nothing."
            }
            Self::Indeterminate => {
                "RetroFrontier could not read this save state just now. Nothing was changed; try \
                 again."
            }
            Self::ReconciliationFailed => {
                "RetroFrontier could not record the save states from the last session."
            }
            Self::LaunchFailed => "RetroFrontier could not start the game from this save state.",
            Self::DeleteFailed => "RetroFrontier could not delete this save state.",
        }
    }
}

impl SaveStateError {
    /// Whether this outcome is *evidence* that the registered file is gone or is no longer the
    /// content that was registered.
    ///
    /// Only such an outcome may close a Save State's lifecycle, because `missing` is never
    /// reopened. A failed observation proves nothing and must leave the row exactly as it is.
    pub const fn proves_absence_or_mismatch(self) -> bool {
        matches!(self, Self::IntegrityMismatch | Self::UnsafeFilesystemTarget)
    }
}

impl fmt::Display for SaveStateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A failed Save-State operation, in the shape React consumes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveStateFailure {
    pub code: SaveStateError,
    pub message: String,
}

impl SaveStateFailure {
    pub fn new(code: SaveStateError) -> Self {
        Self {
            code,
            message: code.message().to_owned(),
        }
    }
}

impl From<SaveStateError> for SaveStateFailure {
    fn from(code: SaveStateError) -> Self {
        Self::new(code)
    }
}

/// Opaque reference understood by the native cached-media protocol. It is not a filesystem path.
///
/// It resolves back to durable Save-State provenance by identity, exactly as a cached cover
/// resolves by `GameId`, and the protocol handler re-verifies the thumbnail's registered size and
/// digest before any byte leaves Rust. The origin is target-specific for the same reason the cover
/// reference's is: desktop WebViews on Windows address the handler through its localhost HTTP
/// origin, while Linux and macOS use the registered scheme.
pub fn save_state_thumbnail_reference(id: SaveStateId) -> String {
    format!(
        "{}save-state-thumbnail/{}",
        crate::domain::library::CACHED_COVER_REFERENCE_PREFIX.trim_end_matches("cover/"),
        id.0
    )
}

/// The result of a load request.
///
/// A save-state load has two genuinely different ways to be refused, and collapsing them would
/// make the UI guess. `Refused` is a Save-State verdict — the state is gone, its identity no
/// longer matches, its historical core is unavailable, or a game is running. `LaunchFailed` is the
/// managed launch pipeline's own normalized verdict about a launch that was otherwise permitted.
///
/// There is deliberately no content-selection arm: the content unit is recorded provenance, so a
/// save-state load never has a choice to offer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum LoadSaveStateResponse {
    Started {
        session: crate::domain::launch::RunningGameSession,
        diagnostics: Vec<crate::domain::launch::LaunchDiagnostic>,
    },
    Refused {
        error: SaveStateFailure,
    },
    LaunchFailed {
        error: crate::domain::launch::LaunchFailure,
    },
}

impl LoadSaveStateResponse {
    pub fn refused(code: SaveStateError) -> Self {
        Self::Refused {
            error: SaveStateFailure::new(code),
        }
    }
}

/// The result of a delete request. Every anticipated problem is a tagged response, not an IPC
/// error, so React can act on a stable code instead of parsing text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "status",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum DeleteSaveStateResponse {
    Deleted { save_state_id: SaveStateId },
    Failed { error: SaveStateFailure },
}

impl DeleteSaveStateResponse {
    pub fn failed(code: SaveStateError) -> Self {
        Self::Failed {
            error: SaveStateFailure::new(code),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DeleteSaveStateResponse, LaunchStateBaseline, LaunchStateBaselineEntry, SaveState,
        SaveStateCapabilities, SaveStateError, SaveStateFileIdentity, SaveStateId,
        SaveStateLoadability, SaveStateProvenance, SaveStateSlot, SaveStateStatus,
        SaveStateThumbnailIdentity, SaveStateView, MAX_MANAGED_SLOT, MIN_MANAGED_SLOT,
    };
    use crate::domain::core::CoreId;
    use crate::domain::launch::PlaySessionId;
    use crate::domain::library::{ContentUnitId, GameId};
    use crate::domain::runtime::{RelativePath, SafeIdentifier, Sha256Digest};

    const TEST_TIME: i64 = 1_756_900_000_000;

    fn digest(seed: char) -> Sha256Digest {
        Sha256Digest::from_hex(&seed.to_string().repeat(64)).unwrap()
    }

    fn provenance(core_binary: char) -> SaveStateProvenance {
        SaveStateProvenance {
            game_id: GameId(7),
            content_unit_id: ContentUnitId(11),
            play_session_id: PlaySessionId(23),
            core_id: CoreId::new("nestopia").unwrap(),
            core_component_id: SafeIdentifier::new("nestopia").unwrap(),
            core_binary_sha256: digest(core_binary),
            core_display_version: Some("1.53".to_owned()),
            core_source_revision: Some("deadbeef".to_owned()),
            originating_runtime_release_id: SafeIdentifier::new(
                "rf-runtime-1.22.2-linux-x86_64-002",
            )
            .unwrap(),
        }
    }

    fn save_state(id: i64, slot: u16, core_binary: char) -> SaveState {
        SaveState {
            id: SaveStateId(id),
            provenance: provenance(core_binary),
            slot: SaveStateSlot::new(slot).unwrap(),
            state: SaveStateFileIdentity {
                relative_path: RelativePath::new("Nestopia/Synthetic.state1").unwrap(),
                sha256: digest('a'),
                size_bytes: 4096,
            },
            thumbnail: None,
            created_at: TEST_TIME,
            updated_at: TEST_TIME,
            status: SaveStateStatus::Available,
        }
    }

    #[test]
    fn the_managed_slot_range_is_one_to_nine_hundred_and_ninety_nine() {
        assert_eq!((MIN_MANAGED_SLOT, MAX_MANAGED_SLOT), (1, 999));
        for slot in [1_u16, 2, 42, 998, 999] {
            assert_eq!(SaveStateSlot::new(slot).unwrap().get(), slot);
        }
        // Slot zero is RetroArch's unnumbered base state and is deliberately not managed.
        assert_eq!(SaveStateSlot::new(0), Err(SaveStateError::Unavailable));
        assert_eq!(SaveStateSlot::new(1000), Err(SaveStateError::Unavailable));
        assert_eq!(
            SaveStateSlot::new(u16::MAX),
            Err(SaveStateError::Unavailable)
        );
        // A normal launch starts on the first managed slot.
        assert_eq!(SaveStateSlot::default_launch_slot().get(), 1);
    }

    /// The module's own declarations, with doc comments, line comments, and the test module
    /// stripped.
    ///
    /// Several invariants below are statements about what this layer may *contain*, not about a
    /// value it produces, so they are asserted against the source. Scanning only the production
    /// half keeps those assertions from matching the very strings the tests use to express them.
    fn production_declarations() -> String {
        let source = include_str!("save_state.rs");
        let production = source
            .split_once("#[cfg(test)]")
            .map(|(production, _)| production)
            .expect("the module has a test section");
        production
            .lines()
            .filter(|line| {
                let trimmed = line.trim_start();
                !trimmed.starts_with("//")
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// There is no way to *construct* an automatic slot, which is how AUTO stays out of the domain.
    ///
    /// AUTO is not a number at all in RetroArch's layout, so the only constructor taking a `u16`
    /// cannot express it, and no variant, constant, or parser here admits it.
    #[test]
    fn the_automatic_slot_cannot_be_expressed_at_all() {
        let declarations = production_declarations();
        for forbidden in ["Auto", "AUTO", "automatic_slot"] {
            assert!(
                !declarations.contains(forbidden),
                "the domain must not name an automatic slot ({forbidden})"
            );
        }
        // The only constructor is numeric, and it refuses everything outside 1..=999.
        assert!(SaveStateSlot::new(0).is_err());
    }

    /// RetroArch filename conventions are adapter facts, so none of them may appear here.
    #[test]
    fn no_retroarch_filesystem_naming_convention_leaks_into_the_domain() {
        let declarations = production_declarations();
        for forbidden in [
            ".state",
            ".png",
            "library_name",
            "sort_savestates",
            "entryslot",
        ] {
            assert!(
                !declarations.contains(forbidden),
                "the domain must not carry the RetroArch naming convention {forbidden:?}"
            );
        }
    }

    #[test]
    fn the_slot_is_scoped_metadata_and_never_save_state_identity() {
        let from_core_a = save_state(1, 1, 'a');
        let from_core_b = save_state(2, 1, 'b');

        // Same slot, different immutable core-binary provenance: two distinct Save States, and
        // both are representable at the same time.
        assert_eq!(from_core_a.slot, from_core_b.slot);
        assert_ne!(from_core_a.id, from_core_b.id);
        assert_ne!(
            from_core_a.provenance.core_binary_sha256,
            from_core_b.provenance.core_binary_sha256
        );
        assert_ne!(from_core_a, from_core_b);

        // And the identity that crosses IPC is the id, not the slot, the path, or the digest.
        let serialized = serde_json::to_value(SaveStateId(42)).unwrap();
        assert_eq!(serialized, serde_json::json!(42));
    }

    /// Core-binary provenance is immutable: the domain offers no way to change it in place.
    #[test]
    fn core_binary_provenance_has_no_mutating_api() {
        let declarations = production_declarations();
        for forbidden in [
            "fn set_core_binary",
            "fn with_core_binary",
            "fn set_provenance",
        ] {
            assert!(
                !declarations.contains(forbidden),
                "core-binary provenance must not be rewritable ({forbidden})"
            );
        }
        // Changing it can only mean building a *different* value, which is a different Save State.
        let original = save_state(1, 1, 'a');
        let mut replacement = original.clone();
        replacement.provenance = provenance('b');
        assert_ne!(original.provenance, replacement.provenance);
    }

    #[test]
    fn every_lifecycle_value_round_trips_through_its_database_representation() {
        let expected = ["available", "missing", "superseded", "deleted"];
        let actual: Vec<_> = SaveStateStatus::ALL
            .iter()
            .map(|status| status.as_db())
            .collect();
        assert_eq!(actual, expected);

        for status in SaveStateStatus::ALL {
            assert_eq!(SaveStateStatus::from_db(status.as_db()), Some(*status));
            assert_eq!(
                serde_json::to_value(status).unwrap(),
                serde_json::json!(status.as_db())
            );
        }
        assert_eq!(SaveStateStatus::from_db("corrupt"), None);
        assert_eq!(SaveStateStatus::from_db(""), None);
        assert!(SaveStateStatus::Available.is_available());
        for status in [
            SaveStateStatus::Missing,
            SaveStateStatus::Superseded,
            SaveStateStatus::Deleted,
        ] {
            assert!(!status.is_available());
        }
    }

    #[test]
    fn loadability_is_never_a_compatibility_claim() {
        let expected = ["ready", "coreUnavailable", "temporarilyBlocked"];
        let actual: Vec<_> = SaveStateLoadability::ALL
            .iter()
            .map(|value| {
                serde_json::to_value(value)
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_owned()
            })
            .collect();
        assert_eq!(actual, expected);
        assert!(SaveStateLoadability::Ready.is_ready());
        assert!(!SaveStateLoadability::CoreUnavailable.is_ready());

        // No variant, and no user-facing message, promises compatibility.
        let declarations = production_declarations();
        for forbidden in ["compatible", "Compatible", "incompatible"] {
            assert!(
                !declarations.contains(forbidden),
                "M9 must never claim compatibility ({forbidden})"
            );
        }
    }

    #[test]
    fn loadability_and_deletability_are_independent() {
        // The historical core is gone, so the state cannot be loaded — but the exact registered
        // file can still be reverified, so deleting it is safe.
        let capabilities = SaveStateCapabilities {
            loadability: SaveStateLoadability::CoreUnavailable,
            deletable: true,
        };
        let serialized = serde_json::to_value(capabilities).unwrap();
        assert_eq!(
            serialized["loadability"],
            serde_json::json!("coreUnavailable")
        );
        assert_eq!(serialized["deletable"], serde_json::json!(true));
    }

    #[test]
    fn every_save_state_error_has_a_stable_camel_case_wire_value_and_a_safe_message() {
        let expected = [
            "notFound",
            "unavailable",
            "coreUnavailable",
            "temporarilyBlocked",
            "integrityMismatch",
            "unsafeFilesystemTarget",
            "indeterminate",
            "reconciliationFailed",
            "launchFailed",
            "deleteFailed",
        ];
        let actual: Vec<_> = SaveStateError::ALL
            .iter()
            .map(|code| code.as_str())
            .collect();
        assert_eq!(actual, expected);

        for code in SaveStateError::ALL {
            assert_eq!(
                serde_json::to_value(code).unwrap(),
                serde_json::json!(code.as_str())
            );
            let message = code.message();
            assert!(!message.trim().is_empty());
            assert!(!message.contains('/'), "{code} must not carry a path");
            assert!(!message.contains("os error"));
            // "corrupt" is not a conclusion M9 may draw from a digest mismatch.
            assert!(!message.to_lowercase().contains("corrupt"), "{code}");
        }
        // And there is no variant that would let that conclusion be invented.
        assert!(!production_declarations().contains("Corrupt"));
    }

    #[test]
    fn the_bounded_view_carries_no_digest_and_no_filesystem_path() {
        let view = SaveStateView {
            id: SaveStateId(9),
            game_id: GameId(7),
            content_unit_id: ContentUnitId(11),
            slot: SaveStateSlot::new(3).unwrap(),
            core_id: CoreId::new("beetle-psx").unwrap(),
            core_display_version: Some("0.9.44.1".to_owned()),
            core_source_revision: Some("abc1234".to_owned()),
            content_unit_label: Some("DISC 2".to_owned()),
            created_at: TEST_TIME,
            updated_at: TEST_TIME + 5,
            thumbnail_ref: Some("rfmedia://localhost/save-state-thumbnail/9".to_owned()),
            capabilities: SaveStateCapabilities {
                loadability: SaveStateLoadability::Ready,
                deletable: true,
            },
        };

        let serialized = serde_json::to_string(&view).unwrap();
        assert!(serialized.contains("\"slot\":3"));
        assert!(serialized.contains("\"coreId\":\"beetle-psx\""));
        assert!(serialized.contains("\"contentUnitLabel\":\"DISC 2\""));
        for forbidden in [
            "sha256",
            "Sha256",
            "relativePath",
            "stateRelativePath",
            "corePath",
            "sizeBytes",
            ".state",
        ] {
            assert!(
                !serialized.contains(forbidden),
                "the view must not expose {forbidden}"
            );
        }
        // The thumbnail is an opaque protocol reference, never a filesystem path.
        assert!(serialized.contains("rfmedia://localhost/save-state-thumbnail/9"));

        let without_thumbnail = SaveStateView {
            thumbnail_ref: None,
            content_unit_label: None,
            core_display_version: None,
            core_source_revision: None,
            ..view
        };
        let serialized = serde_json::to_value(&without_thumbnail).unwrap();
        assert_eq!(serialized["thumbnailRef"], serde_json::Value::Null);
        assert_eq!(serialized["contentUnitLabel"], serde_json::Value::Null);
    }

    #[test]
    fn a_state_file_identity_is_a_validated_relative_path_and_never_an_absolute_one() {
        // The reused `RelativePath` refuses every unsafe form, which is why M9 adds no parallel
        // path type of its own.
        for unsafe_path in [
            "/absolute/Nestopia/Synthetic.state1",
            "../escape.state1",
            "Nestopia/../../escape.state1",
            "Nestopia/./Synthetic.state1",
            "Nestopia\\Synthetic.state1",
            "",
        ] {
            assert!(
                RelativePath::new(unsafe_path).is_err(),
                "{unsafe_path} must be refused"
            );
        }

        let identity = SaveStateFileIdentity {
            relative_path: RelativePath::new("Nestopia/Synthetic.state1").unwrap(),
            sha256: digest('c'),
            size_bytes: 12,
        };
        let thumbnail = SaveStateThumbnailIdentity {
            relative_path: RelativePath::new("Nestopia/Synthetic.state1.png").unwrap(),
            sha256: digest('d'),
            size_bytes: 34,
        };
        // The thumbnail's identity is stored independently of the state's.
        assert_ne!(identity.sha256, thumbnail.sha256);
        assert_ne!(identity.size_bytes, thumbnail.size_bytes);
        assert_ne!(identity.relative_path, thumbnail.relative_path);
    }

    #[test]
    fn a_baseline_carries_its_provenance_and_its_cheap_physical_entries() {
        let baseline = LaunchStateBaseline {
            provenance: provenance('a'),
            runtime_installation_id: SafeIdentifier::new("i-18d14638042bd789-1-51189").unwrap(),
            captured_at: TEST_TIME,
            attempts: 0,
            entries: vec![LaunchStateBaselineEntry {
                relative_path: RelativePath::new("Nestopia/Synthetic.state1").unwrap(),
                size_bytes: 4096,
                mtime_nanos: 1_756_900_000_123_456_789,
                inode: 424_242,
            }],
        };

        assert_eq!(baseline.provenance.play_session_id, PlaySessionId(23));
        assert_eq!(baseline.entries.len(), 1);
        // No digest: SHA-256 is computed after the process ended, per the approved ordering.
        let serialized = serde_json::to_value(&baseline).unwrap();
        assert!(serialized["entries"][0].get("sha256").is_none());
        assert_eq!(
            serialized["entries"][0]["inode"],
            serde_json::json!(424_242)
        );
    }

    #[test]
    fn a_delete_response_is_a_status_tagged_union() {
        let deleted = DeleteSaveStateResponse::Deleted {
            save_state_id: SaveStateId(9),
        };
        let failed = DeleteSaveStateResponse::failed(SaveStateError::IntegrityMismatch);

        assert_eq!(
            serde_json::to_value(&deleted).unwrap(),
            serde_json::json!({ "status": "deleted", "saveStateId": 9 })
        );
        let failed = serde_json::to_value(&failed).unwrap();
        assert_eq!(failed["status"], serde_json::json!("failed"));
        assert_eq!(
            failed["error"]["code"],
            serde_json::json!("integrityMismatch")
        );
        assert_eq!(
            failed["error"]["message"],
            serde_json::json!(SaveStateError::IntegrityMismatch.message())
        );
    }
}
