use crate::domain::system::{SystemCatalog, SystemId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt;

pub type UnixTimestamp = i64;

macro_rules! id_type {
    ($name:ident) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(pub i64);

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }
    };
}

id_type!(ContentRootId);
id_type!(GameId);
id_type!(ContentUnitId);
id_type!(ContentFileId);
id_type!(ScanRunId);
id_type!(ScanIssueId);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ContentRootKind {
    Managed,
    External,
}

impl ContentRootKind {
    pub const fn as_db(self) -> &'static str {
        match self {
            Self::Managed => "managed",
            Self::External => "external",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "managed" => Some(Self::Managed),
            "external" => Some(Self::External),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ContentRootAvailability {
    Available,
    PartiallyAvailable,
    Unavailable,
    Disabled,
    Unsafe,
}

impl ContentRootAvailability {
    pub const fn as_db(self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::PartiallyAvailable => "partially_available",
            Self::Unavailable => "unavailable",
            Self::Disabled => "disabled",
            Self::Unsafe => "unsafe",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "available" => Some(Self::Available),
            "partially_available" => Some(Self::PartiallyAvailable),
            "unavailable" => Some(Self::Unavailable),
            "disabled" => Some(Self::Disabled),
            "unsafe" => Some(Self::Unsafe),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GameAvailability {
    Available,
    Unavailable,
}

impl GameAvailability {
    pub const fn as_db(self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::Unavailable => "unavailable",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "available" => Some(Self::Available),
            "unavailable" => Some(Self::Unavailable),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ContentUnitAvailability {
    Available,
    Incomplete,
    Missing,
}

impl ContentUnitAvailability {
    pub const fn as_db(self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::Incomplete => "incomplete",
            Self::Missing => "missing",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "available" => Some(Self::Available),
            "incomplete" => Some(Self::Incomplete),
            "missing" => Some(Self::Missing),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ContentFileAvailability {
    Available,
    Unavailable,
    Missing,
}

impl ContentFileAvailability {
    pub const fn as_db(self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::Unavailable => "unavailable",
            Self::Missing => "missing",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "available" => Some(Self::Available),
            "unavailable" => Some(Self::Unavailable),
            "missing" => Some(Self::Missing),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ContentUnitKind {
    SingleFile,
    Chd,
    CueBin,
    Gdi,
    M3u,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentFormat {
    SingleFile,
    Chd,
    Cue,
    Gdi,
    M3u,
}

impl ContentFormat {
    pub fn from_extension(extension: &str) -> Self {
        match extension.to_ascii_lowercase().as_str() {
            ".chd" => Self::Chd,
            ".cue" => Self::Cue,
            ".gdi" => Self::Gdi,
            ".m3u" => Self::M3u,
            _ => Self::SingleFile,
        }
    }
}

impl ContentUnitKind {
    pub const fn as_db(self) -> &'static str {
        match self {
            Self::SingleFile => "single_file",
            Self::Chd => "chd",
            Self::CueBin => "cue_bin",
            Self::Gdi => "gdi",
            Self::M3u => "m3u",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "single_file" => Some(Self::SingleFile),
            "chd" => Some(Self::Chd),
            "cue_bin" => Some(Self::CueBin),
            "gdi" => Some(Self::Gdi),
            "m3u" => Some(Self::M3u),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ContentFileRole {
    Standalone,
    Descriptor,
    Track,
    Playlist,
    Disc,
    DiscDescriptor,
    DiscTrack,
}

impl ContentFileRole {
    pub const fn as_db(self) -> &'static str {
        match self {
            Self::Standalone => "standalone",
            Self::Descriptor => "descriptor",
            Self::Track => "track",
            Self::Playlist => "playlist",
            Self::Disc => "disc",
            Self::DiscDescriptor => "disc_descriptor",
            Self::DiscTrack => "disc_track",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "standalone" => Some(Self::Standalone),
            "descriptor" => Some(Self::Descriptor),
            "track" => Some(Self::Track),
            "playlist" => Some(Self::Playlist),
            "disc" => Some(Self::Disc),
            "disc_descriptor" => Some(Self::DiscDescriptor),
            "disc_track" => Some(Self::DiscTrack),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentRoot {
    pub id: ContentRootId,
    pub path: String,
    pub kind: ContentRootKind,
    pub enabled: bool,
    pub system_hint: Option<SystemId>,
    pub availability: ContentRootAvailability,
    pub last_scan_at: Option<UnixTimestamp>,
    pub last_successful_scan_at: Option<UnixTimestamp>,
    pub created_at: UnixTimestamp,
    pub updated_at: UnixTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Game {
    pub id: GameId,
    pub system_id: SystemId,
    pub local_title: String,
    pub availability: GameAvailability,
    pub created_at: UnixTimestamp,
    pub updated_at: UnixTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentFile {
    pub id: ContentFileId,
    pub root_id: ContentRootId,
    pub relative_path: String,
    pub size_bytes: u64,
    pub modified_at: UnixTimestamp,
    pub crc32: Option<String>,
    pub md5: Option<String>,
    pub sha1: Option<String>,
    pub availability: ContentFileAvailability,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentFileMembership {
    pub ordinal: i64,
    pub role: ContentFileRole,
    pub file: ContentFile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentUnit {
    pub id: ContentUnitId,
    pub game_id: GameId,
    pub root_id: ContentRootId,
    pub system_id: SystemId,
    pub kind: ContentUnitKind,
    pub local_title: String,
    pub primary_relative_path: String,
    pub fingerprint: Option<String>,
    pub availability: ContentUnitAvailability,
    pub created_at: UnixTimestamp,
    pub updated_at: UnixTimestamp,
    pub files: Vec<ContentFileMembership>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameSnapshot {
    pub game: Game,
    pub content_units: Vec<ContentUnit>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibrarySnapshot {
    pub games: Vec<GameSnapshot>,
}

/// The bounded, provider-neutral metadata state needed by a library list.
///
/// This deliberately collapses the provider relationship to a small UI state. The full M5
/// metadata state remains available through its existing game-specific command. `Pending` covers
/// both a game with no provider row yet and one with queued provider work; the list does not expose
/// a separate `notRequested` state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LibraryMetadataMatchState {
    Pending,
    Matched,
    NoMatch,
    Ambiguous,
    Deferred,
    Failed,
    Stale,
}

/// M6.1 currently needs one predictable title ordering. Keeping it an enum makes the IPC
/// contract explicit without inventing an unbounded sorting framework.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LibrarySort {
    #[default]
    TitleAsc,
}

pub const DEFAULT_LIBRARY_PAGE_SIZE: u32 = 60;
pub const MAX_LIBRARY_PAGE_SIZE: u32 = 60;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LibraryQuery {
    pub search: Option<String>,
    pub system_id: Option<SystemId>,
    pub favorites_only: bool,
    pub genre: Option<String>,
    pub region: Option<String>,
    pub availability: Option<GameAvailability>,
    /// Restricts the page to games whose provider match needs a human decision.
    ///
    /// One narrow flag rather than a general match-state filter: the only state a user can act on
    /// from Game Detail is an ambiguous candidate set. A no-match, an unsupported shape or a parked
    /// failure has no candidate list to choose from, so listing them under "review" would be an
    /// invitation to do something the UI cannot offer.
    pub needs_metadata_review: bool,
    pub sort: LibrarySort,
    /// Zero means the bounded default. Values above the backend maximum are capped.
    pub limit: u32,
    pub offset: u64,
}

impl LibraryQuery {
    pub fn normalized_search(&self) -> Option<String> {
        self.search
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    }

    pub fn bounded_limit(&self) -> u32 {
        let limit = if self.limit == 0 {
            DEFAULT_LIBRARY_PAGE_SIZE
        } else {
            self.limit
        };
        limit.min(MAX_LIBRARY_PAGE_SIZE)
    }
}

/// Opaque reference understood by the native cached-media protocol. It is not a filesystem path.
///
/// Tauri's custom protocol origin is target-specific: desktop WebViews on Windows address the
/// handler through its localhost HTTP origin, while Linux and macOS use the registered scheme.
#[cfg(any(windows, target_os = "android"))]
pub const CACHED_COVER_REFERENCE_PREFIX: &str = "http://rfmedia.localhost/cover/";
#[cfg(not(any(windows, target_os = "android")))]
pub const CACHED_COVER_REFERENCE_PREFIX: &str = "rfmedia://localhost/cover/";

pub fn cached_cover_reference(game_id: GameId) -> String {
    format!("{CACHED_COVER_REFERENCE_PREFIX}{}", game_id.0)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryListItem {
    pub game_id: GameId,
    pub system_id: SystemId,
    pub local_title: String,
    pub metadata_title: Option<String>,
    pub display_title: String,
    pub sort_title: String,
    pub availability: GameAvailability,
    pub favorite: bool,
    pub metadata_match_state: LibraryMetadataMatchState,
    pub release_date: Option<String>,
    pub genre: Option<String>,
    pub region: Option<String>,
    /// Present only when the durable media row says a cover is cached. The protocol still
    /// revalidates the file before serving it, so a missing file is never trusted blindly.
    pub cover_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryPage {
    pub items: Vec<LibraryListItem>,
    pub total: u64,
    pub offset: u64,
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibrarySystemCount {
    pub system_id: SystemId,
    pub game_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibrarySummary {
    pub total_games: u64,
    pub favorite_games: u64,
    pub systems: Vec<LibrarySystemCount>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryContentUnitSummary {
    pub unit_id: ContentUnitId,
    pub root_id: ContentRootId,
    pub kind: ContentUnitKind,
    pub local_title: String,
    pub primary_relative_path: String,
    pub file_count: u64,
    pub availability: ContentUnitAvailability,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryGameDetail {
    pub game_id: GameId,
    pub system_id: SystemId,
    pub local_title: String,
    pub availability: GameAvailability,
    pub favorite: bool,
    pub content_units: Vec<LibraryContentUnitSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameFavorite {
    pub game_id: GameId,
    pub favorite: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentHashes {
    pub crc32: String,
    pub md5: String,
    pub sha1: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedFile {
    pub relative_path: String,
    pub size_bytes: u64,
    pub modified_at: UnixTimestamp,
    pub hashes: Option<ContentHashes>,
    pub available: bool,
    pub hash_failed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedMember {
    pub relative_path: String,
    pub ordinal: i64,
    pub role: ContentFileRole,
    pub present: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedUnit {
    pub system_id: SystemId,
    pub kind: ContentUnitKind,
    pub local_title: String,
    pub primary_relative_path: String,
    pub fingerprint: Option<String>,
    pub complete: bool,
    pub hash_failed: bool,
    pub members: Vec<ScannedMember>,
}

/// Describes which parts of a root were observed reliably enough to reconcile absence.
///
/// A successful directory enumeration is authoritative for its direct children and, through the
/// ancestor walk in `can_reconcile_file`, for descendants whose intermediate directories have
/// disappeared. An unsafe or incomplete entry protects only that entry/prefix, while a failed
/// enumeration of a directory protects the whole directory subtree. Unrepresentable entries
/// prevent a root from being fully authoritative but do not invalidate representable siblings. This
/// keeps absence reconciliation conservative without making one bad sibling invalidate the complete
/// remainder of a root.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScanAuthority {
    pub root_enumerated: bool,
    pub enumerated_directories: BTreeSet<String>,
    pub incomplete_prefixes: BTreeSet<String>,
    pub has_unrepresentable_entries: bool,
}

impl ScanAuthority {
    pub fn mark_directory_enumerated(&mut self, relative_path: &str) {
        self.enumerated_directories.insert(relative_path.to_owned());
        if relative_path.is_empty() {
            self.root_enumerated = true;
        }
    }

    pub fn mark_incomplete(&mut self, relative_path: &str) {
        self.incomplete_prefixes.insert(relative_path.to_owned());
        if relative_path.is_empty() {
            self.root_enumerated = false;
        }
    }

    pub fn mark_unrepresentable_entry(&mut self) {
        self.has_unrepresentable_entries = true;
    }

    pub fn is_fully_authoritative(&self) -> bool {
        self.root_enumerated
            && self.incomplete_prefixes.is_empty()
            && !self.has_unrepresentable_entries
    }

    pub fn can_reconcile_file(&self, relative_path: &str) -> bool {
        if !self.root_enumerated {
            return false;
        }
        if self
            .incomplete_prefixes
            .iter()
            .any(|prefix| path_prefix_contains(prefix, relative_path))
        {
            return false;
        }

        let mut ancestor = relative_path
            .rsplit_once('/')
            .map_or("", |(parent, _)| parent);
        loop {
            if self.enumerated_directories.contains(ancestor) {
                return true;
            }
            if ancestor.is_empty() {
                return false;
            }
            ancestor = ancestor.rsplit_once('/').map_or("", |(parent, _)| parent);
        }
    }
}

fn path_prefix_contains(prefix: &str, relative_path: &str) -> bool {
    prefix.is_empty()
        || prefix == relative_path
        || relative_path
            .strip_prefix(prefix)
            .is_some_and(|remainder| remainder.starts_with('/'))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedRoot {
    pub root: ContentRoot,
    pub authority: ScanAuthority,
    pub files: Vec<ScannedFile>,
    pub units: Vec<ScannedUnit>,
    pub issues: Vec<ScanIssue>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ScanIssueKind {
    RootUnavailable,
    UnreadablePath,
    UnsafePath,
    UnrepresentablePath,
    UnsupportedSystem,
    AmbiguousSystem,
    IncompatibleSystemHint,
    MalformedCue,
    MalformedGdi,
    MalformedM3u,
    UnsafeDescriptorReference,
    MissingReferencedFile,
    ReferenceCycle,
    HashReadFailure,
    DuplicateContent,
    AmbiguousReconciliation,
    OverlappingContentRoot,
    WatcherFailure,
}

impl ScanIssueKind {
    pub const fn as_db(self) -> &'static str {
        match self {
            Self::RootUnavailable => "root_unavailable",
            Self::UnreadablePath => "unreadable_path",
            Self::UnsafePath => "unsafe_path",
            Self::UnrepresentablePath => "unrepresentable_path",
            Self::UnsupportedSystem => "unsupported_system",
            Self::AmbiguousSystem => "ambiguous_system",
            Self::IncompatibleSystemHint => "incompatible_system_hint",
            Self::MalformedCue => "malformed_cue",
            Self::MalformedGdi => "malformed_gdi",
            Self::MalformedM3u => "malformed_m3u",
            Self::UnsafeDescriptorReference => "unsafe_descriptor_reference",
            Self::MissingReferencedFile => "missing_referenced_file",
            Self::ReferenceCycle => "reference_cycle",
            Self::HashReadFailure => "hash_read_failure",
            Self::DuplicateContent => "duplicate_content",
            Self::AmbiguousReconciliation => "ambiguous_reconciliation",
            Self::OverlappingContentRoot => "overlapping_content_root",
            Self::WatcherFailure => "watcher_failure",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        Some(match value {
            "root_unavailable" => Self::RootUnavailable,
            "unreadable_path" => Self::UnreadablePath,
            "unsafe_path" => Self::UnsafePath,
            "unrepresentable_path" => Self::UnrepresentablePath,
            "unsupported_system" => Self::UnsupportedSystem,
            "ambiguous_system" => Self::AmbiguousSystem,
            "incompatible_system_hint" => Self::IncompatibleSystemHint,
            "malformed_cue" => Self::MalformedCue,
            "malformed_gdi" => Self::MalformedGdi,
            "malformed_m3u" => Self::MalformedM3u,
            "unsafe_descriptor_reference" => Self::UnsafeDescriptorReference,
            "missing_referenced_file" => Self::MissingReferencedFile,
            "reference_cycle" => Self::ReferenceCycle,
            "hash_read_failure" => Self::HashReadFailure,
            "duplicate_content" => Self::DuplicateContent,
            "ambiguous_reconciliation" => Self::AmbiguousReconciliation,
            "overlapping_content_root" => Self::OverlappingContentRoot,
            "watcher_failure" => Self::WatcherFailure,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanIssue {
    pub id: Option<ScanIssueId>,
    pub scan_run_id: Option<ScanRunId>,
    pub root_id: Option<ContentRootId>,
    pub kind: ScanIssueKind,
    pub relative_path: Option<String>,
    pub related_path: Option<String>,
    pub detail: Option<String>,
    pub created_at: UnixTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanIssuePage {
    pub issues: Vec<ScanIssue>,
    pub scan_run_id: Option<ScanRunId>,
    pub total: u64,
    pub offset: u64,
    pub limit: u32,
}

pub const DEFAULT_SCAN_ISSUE_PAGE_SIZE: u32 = 50;
pub const MAX_SCAN_ISSUE_PAGE_SIZE: u32 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ScanPhase {
    Discovery,
    RelationshipResolution,
    Hashing,
    Reconciliation,
    Completed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ScanRunState {
    Running,
    Completed,
    Failed,
}

impl ScanRunState {
    pub const fn as_db(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "running" => Some(Self::Running),
            "completed" => Some(Self::Completed),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanCounters {
    pub roots_discovered: u64,
    pub roots_completed: u64,
    pub files_discovered: u64,
    pub files_processed: u64,
    pub files_hashed: u64,
    pub bytes_hashed: u64,
    pub issues_found: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    pub run_id: ScanRunId,
    pub phase: ScanPhase,
    pub counters: ScanCounters,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanSummary {
    pub run_id: ScanRunId,
    pub state: ScanRunState,
    pub counters: ScanCounters,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanStatus {
    pub running: bool,
    pub progress: Option<ScanProgress>,
    pub last_result: Option<ScanSummary>,
}

/// Domain-facing check used by bootstrap and root management. It deliberately compares path
/// components instead of string prefixes so `/roms-a` does not overlap `/roms`.
pub fn roots_overlap(left: &str, right: &str) -> bool {
    let left = std::path::Path::new(left);
    let right = std::path::Path::new(right);
    left == right || left.starts_with(right) || right.starts_with(left)
}

pub fn catalog_managed_folder_names(catalog: &SystemCatalog) -> Vec<String> {
    catalog
        .systems()
        .iter()
        .map(|system| system.managed_rom_folder_name.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{cached_cover_reference, ScanAuthority};
    use crate::domain::library::GameId;

    #[test]
    fn scan_authority_walks_to_an_enumerated_ancestor() {
        let mut authority = ScanAuthority::default();
        authority.mark_directory_enumerated("");
        authority.mark_directory_enumerated("a");

        assert!(authority.can_reconcile_file("a/b/c/game.nes"));
        assert!(authority.can_reconcile_file("deleted/game.nes"));
    }

    #[test]
    fn scan_authority_prefixes_are_component_aware() {
        let mut authority = ScanAuthority::default();
        authority.mark_directory_enumerated("");
        authority.mark_directory_enumerated("foobar");
        authority.mark_incomplete("foo");

        assert!(!authority.can_reconcile_file("foo/game.nes"));
        assert!(authority.can_reconcile_file("foobar/game.nes"));
    }

    #[test]
    fn cached_cover_reference_uses_the_current_target_origin() {
        let reference = cached_cover_reference(GameId(42));

        #[cfg(any(windows, target_os = "android"))]
        assert_eq!(reference, "http://rfmedia.localhost/cover/42");
        #[cfg(not(any(windows, target_os = "android")))]
        assert_eq!(reference, "rfmedia://localhost/cover/42");
    }
}
