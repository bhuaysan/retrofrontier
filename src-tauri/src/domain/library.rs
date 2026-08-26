use crate::domain::system::{SystemCatalog, SystemId};
use serde::{Deserialize, Serialize};
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
    pub members: Vec<ScannedMember>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedRoot {
    pub root: ContentRoot,
    pub authoritative: bool,
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
