//! Provider-neutral metadata domain model.
//!
//! Nothing in this module is ScreenScraper-specific: provider endpoint names, provider system
//! identifiers, provider field names, and HTTP status codes stay inside the adapter. The domain
//! only knows that *some* provider can be asked to identify content, that the answer must agree
//! with the local M4 evidence before it is trusted, and that provider state is always downstream
//! of local library identity.

use crate::domain::library::{
    ContentFileId, ContentUnit, ContentUnitId, ContentUnitKind, GameId, UnixTimestamp,
};
use crate::domain::system::SystemId;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Version of the match-evidence snapshot format.
///
/// Stored with every accepted match so a future evidence-rule change invalidates old matches
/// instead of silently reinterpreting them.
pub const EVIDENCE_SCHEMA_VERSION: i64 = 1;

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

id_type!(ProviderMatchId);
id_type!(MetadataJobId);

/// Stable provider identifier. This is the value persisted in SQLite.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MetadataProviderId {
    ScreenScraper,
}

impl MetadataProviderId {
    pub const fn as_db(self) -> &'static str {
        match self {
            Self::ScreenScraper => "screenscraper",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "screenscraper" => Some(Self::ScreenScraper),
            _ => None,
        }
    }
}

impl fmt::Display for MetadataProviderId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_db())
    }
}

/// Provider-specific lifecycle state for one game.
///
/// `Matched` is the only state that may be presented as an accepted provider relationship, and
/// only while its stored evidence still agrees with current M4 evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderMatchStatus {
    Pending,
    Matched,
    NoMatch,
    Ambiguous,
    Deferred,
    Failed,
    Stale,
}

impl ProviderMatchStatus {
    pub const fn as_db(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Matched => "matched",
            Self::NoMatch => "no_match",
            Self::Ambiguous => "ambiguous",
            Self::Deferred => "deferred",
            Self::Failed => "failed",
            Self::Stale => "stale",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        Some(match value {
            "pending" => Self::Pending,
            "matched" => Self::Matched,
            "no_match" => Self::NoMatch,
            "ambiguous" => Self::Ambiguous,
            "deferred" => Self::Deferred,
            "failed" => Self::Failed,
            "stale" => Self::Stale,
            _ => return None,
        })
    }
}

/// How a provider relationship was established.
///
/// The three deterministic variants record which hash carried the agreement, because CRC32 is
/// materially weaker than SHA-1/MD5 and must remain distinguishable after the fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MatchType {
    DeterministicSha1,
    DeterministicMd5,
    DeterministicCrc32,
    HeuristicUserConfirmed,
}

impl MatchType {
    pub const fn as_db(self) -> &'static str {
        match self {
            Self::DeterministicSha1 => "deterministic_sha1",
            Self::DeterministicMd5 => "deterministic_md5",
            Self::DeterministicCrc32 => "deterministic_crc32",
            Self::HeuristicUserConfirmed => "heuristic_user_confirmed",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        Some(match value {
            "deterministic_sha1" => Self::DeterministicSha1,
            "deterministic_md5" => Self::DeterministicMd5,
            "deterministic_crc32" => Self::DeterministicCrc32,
            "heuristic_user_confirmed" => Self::HeuristicUserConfirmed,
            _ => return None,
        })
    }

    pub const fn is_deterministic(self) -> bool {
        matches!(
            self,
            Self::DeterministicSha1 | Self::DeterministicMd5 | Self::DeterministicCrc32
        )
    }
}

/// Why automatic deterministic matching is not attempted for a content unit.
///
/// These are deliberately conservative product states, not provider errors: RetroFrontier refuses
/// to guess a canonical representation the provider has not documented.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UnsupportedContentReason {
    /// The RetroFrontier system has no unambiguous provider mapping.
    SystemNotMapped,
    /// CHD canonical byte representation is not documented by the provider.
    ChdRepresentationUndefined,
    /// CUE/BIN descriptor/track identity is not documented by the provider.
    CueBinRepresentationUndefined,
    /// GDI descriptor/track identity is not documented by the provider.
    GdiRepresentationUndefined,
    /// A playlist file is never provider identity, and disc-aware matching is deferred.
    PlaylistIsNotIdentity,
    /// A single-file container whose provider representation is not established (for example RVZ).
    ContainerRepresentationUndefined,
    /// The unit has no usable hash/size evidence yet.
    MissingContentEvidence,
    /// The unit has no primary content file.
    NoPrimaryContentFile,
}

impl UnsupportedContentReason {
    pub const fn as_db(self) -> &'static str {
        match self {
            Self::SystemNotMapped => "system_not_mapped",
            Self::ChdRepresentationUndefined => "chd_representation_undefined",
            Self::CueBinRepresentationUndefined => "cue_bin_representation_undefined",
            Self::GdiRepresentationUndefined => "gdi_representation_undefined",
            Self::PlaylistIsNotIdentity => "playlist_is_not_identity",
            Self::ContainerRepresentationUndefined => "container_representation_undefined",
            Self::MissingContentEvidence => "missing_content_evidence",
            Self::NoPrimaryContentFile => "no_primary_content_file",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        Some(match value {
            "system_not_mapped" => Self::SystemNotMapped,
            "chd_representation_undefined" => Self::ChdRepresentationUndefined,
            "cue_bin_representation_undefined" => Self::CueBinRepresentationUndefined,
            "gdi_representation_undefined" => Self::GdiRepresentationUndefined,
            "playlist_is_not_identity" => Self::PlaylistIsNotIdentity,
            "container_representation_undefined" => Self::ContainerRepresentationUndefined,
            "missing_content_evidence" => Self::MissingContentEvidence,
            "no_primary_content_file" => Self::NoPrimaryContentFile,
            _ => return None,
        })
    }
}

/// The local content evidence used to justify (and later revalidate) a provider match.
///
/// M4 deliberately keeps `GameId`, `ContentUnitId`, and `ContentFileId` stable across same-path
/// byte replacement, so identity alone proves nothing. The hashes, size, and unit fingerprint are
/// what make a stored match trustworthy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchEvidence {
    pub game_id: GameId,
    pub content_unit_id: ContentUnitId,
    pub system_id: SystemId,
    pub content_unit_kind: ContentUnitKind,
    pub content_file_id: Option<ContentFileId>,
    pub size_bytes: u64,
    pub crc32: Option<String>,
    pub md5: Option<String>,
    pub sha1: Option<String>,
    pub fingerprint: Option<String>,
    pub evidence_version: i64,
}

impl MatchEvidence {
    /// True when `current` still describes exactly the content that established this match.
    ///
    /// A missing current hash never silently satisfies a stored hash: an unreadable file degrades
    /// to "not current" and triggers revalidation rather than pretending the match still holds.
    pub fn agrees_with(&self, current: &Self) -> bool {
        self.evidence_version == current.evidence_version
            && self.content_unit_id == current.content_unit_id
            && self.system_id == current.system_id
            && self.content_unit_kind == current.content_unit_kind
            && self.size_bytes == current.size_bytes
            && self.fingerprint == current.fingerprint
            && hash_agrees(self.sha1.as_deref(), current.sha1.as_deref())
            && hash_agrees(self.md5.as_deref(), current.md5.as_deref())
            && hash_agrees(self.crc32.as_deref(), current.crc32.as_deref())
    }
}

fn hash_agrees(stored: Option<&str>, current: Option<&str>) -> bool {
    match (stored, current) {
        (None, _) => true,
        (Some(stored), Some(current)) => stored.eq_ignore_ascii_case(current),
        (Some(_), None) => false,
    }
}

/// The provider's own identifiers for a game.
///
/// These are a replaceable relationship, never a RetroFrontier `GameId`. Game ID and ROM ID are
/// stored separately because the provider exposes them in different schema locations and they are
/// not aliases.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderIdentity {
    pub provider_id: MetadataProviderId,
    pub provider_game_id: String,
    pub provider_rom_id: Option<String>,
}

/// One heuristic name-search suggestion. Never an attachment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCandidate {
    pub provider_game_id: String,
    pub title: String,
    pub release_date: Option<String>,
}

/// Persisted provider relationship for one game and provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderMatch {
    pub id: ProviderMatchId,
    pub game_id: GameId,
    pub provider_id: MetadataProviderId,
    pub status: ProviderMatchStatus,
    pub match_type: Option<MatchType>,
    pub provider_game_id: Option<String>,
    pub provider_rom_id: Option<String>,
    pub unsupported_reason: Option<UnsupportedContentReason>,
    pub last_failure: Option<ProviderFailureClass>,
    pub last_checked_at: Option<UnixTimestamp>,
    pub last_matched_at: Option<UnixTimestamp>,
    pub evidence: Option<MatchEvidence>,
    pub candidates: Vec<ProviderCandidate>,
    pub created_at: UnixTimestamp,
    pub updated_at: UnixTimestamp,
}

/// Provider-independent metadata cached for local and offline use.
///
/// Deliberately small: it carries what M6 needs to render a library entry and nothing else. It is
/// replaceable provider-derived data and is never merged with user-owned state.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NormalizedMetadata {
    pub title: String,
    pub sort_title: Option<String>,
    pub synopsis: Option<String>,
    pub release_date: Option<String>,
    pub developer: Option<String>,
    pub publisher: Option<String>,
    pub genre: Option<String>,
    pub players: Option<String>,
    pub region: Option<String>,
}

/// Where a normalized record came from, so M6 can present attribution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataProvenance {
    pub provider_id: MetadataProviderId,
    pub provider_game_id: String,
    /// Provider-reported source/category credit where available. Never a legal conclusion.
    pub source_credit: Option<String>,
    pub fetched_at: UnixTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderMetadataRecord {
    pub metadata: NormalizedMetadata,
    pub provenance: MetadataProvenance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MediaAssetKind {
    Cover,
}

impl MediaAssetKind {
    pub const fn as_db(self) -> &'static str {
        match self {
            Self::Cover => "cover",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "cover" => Some(Self::Cover),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MediaAssetState {
    Cached,
    Missing,
    Failed,
}

impl MediaAssetState {
    pub const fn as_db(self) -> &'static str {
        match self {
            Self::Cached => "cached",
            Self::Missing => "missing",
            Self::Failed => "failed",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        Some(match value {
            "cached" => Self::Cached,
            "missing" => Self::Missing,
            "failed" => Self::Failed,
            _ => return None,
        })
    }
}

/// The single V1 primary cover asset for one game and provider.
///
/// `cache_relative_path` is relative to the app-owned media cache root so the database never
/// depends on an absolute developer or user path, and never stores a provider URL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaAsset {
    pub game_id: GameId,
    pub provider_id: MetadataProviderId,
    pub kind: MediaAssetKind,
    pub state: MediaAssetState,
    pub provider_media_type: Option<String>,
    pub region: Option<String>,
    pub cache_relative_path: Option<String>,
    pub content_type: Option<String>,
    pub size_bytes: Option<u64>,
    pub content_sha256: Option<String>,
    pub provider_crc32: Option<String>,
    pub provider_md5: Option<String>,
    pub provider_sha1: Option<String>,
    pub source_credit: Option<String>,
    pub last_failure: Option<ProviderFailureClass>,
    pub fetched_at: Option<UnixTimestamp>,
    pub updated_at: UnixTimestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MetadataJobKind {
    /// Identify or re-identify a game against the provider and enrich it.
    Identify,
    /// Refetch normalized metadata for an already accepted provider identity.
    RefreshMetadata,
    /// Refetch the primary cover for an already accepted provider identity.
    RefreshCover,
}

impl MetadataJobKind {
    pub const fn as_db(self) -> &'static str {
        match self {
            Self::Identify => "identify",
            Self::RefreshMetadata => "refresh_metadata",
            Self::RefreshCover => "refresh_cover",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        Some(match value {
            "identify" => Self::Identify,
            "refresh_metadata" => Self::RefreshMetadata,
            "refresh_cover" => Self::RefreshCover,
            _ => return None,
        })
    }

    /// Lower runs first. Identification precedes enrichment refreshes.
    pub const fn default_priority(self) -> i64 {
        match self {
            Self::Identify => 100,
            Self::RefreshMetadata => 200,
            Self::RefreshCover => 300,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MetadataJobState {
    Pending,
    Running,
    Deferred,
    Failed,
    Completed,
}

impl MetadataJobState {
    pub const fn as_db(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Deferred => "deferred",
            Self::Failed => "failed",
            Self::Completed => "completed",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        Some(match value {
            "pending" => Self::Pending,
            "running" => Self::Running,
            "deferred" => Self::Deferred,
            "failed" => Self::Failed,
            "completed" => Self::Completed,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataJob {
    pub id: MetadataJobId,
    pub game_id: GameId,
    pub provider_id: MetadataProviderId,
    pub kind: MetadataJobKind,
    pub state: MetadataJobState,
    pub priority: i64,
    pub attempts: i64,
    pub last_failure: Option<ProviderFailureClass>,
    pub earliest_next_attempt_at: Option<UnixTimestamp>,
    pub claimed_at: Option<UnixTimestamp>,
    pub created_at: UnixTimestamp,
    pub updated_at: UnixTimestamp,
}

/// Provider-neutral failure classification.
///
/// Every provider adapter must map its transport and protocol errors onto exactly one of these so
/// scheduling, retry, and deferral policy never has to reason about HTTP status codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderFailureClass {
    /// Malformed or rejected request/configuration. Retrying it unchanged cannot help.
    InvalidRequest,
    /// Provider temporarily restricted access for this account class (for example under load).
    ProviderRestricted,
    /// Application/developer authentication failed.
    DeveloperAuthenticationFailed,
    /// Optional personal account authentication failed.
    UserAuthenticationFailed,
    /// The provider deterministically knows nothing about the submitted evidence.
    NoMatch,
    /// The provider is unavailable/in maintenance.
    ProviderUnavailable,
    /// This client build was rejected as non-conforming or obsolete.
    ClientRejected,
    /// Concurrency, per-minute, or global capacity backpressure.
    CapacityDeferred,
    /// Daily request budget exhausted.
    DailyQuotaExceeded,
    /// Daily negative-lookup budget exhausted.
    NegativeQuotaExceeded,
    /// DNS/TLS/timeout/connection failure. Also the observable form of "offline".
    Transport,
    /// Undocumented transient server-side failure.
    TransientServer,
    /// A 2xx response whose body could not be understood.
    MalformedResponse,
    /// No usable application credentials are configured in this build.
    CredentialsUnavailable,
    /// The provider offered no acceptable primary media, or the download failed validation.
    MediaUnavailable,
}

/// What the scheduler should do about a failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureDisposition {
    /// Bounded exponential backoff with jitter.
    RetryWithBackoff,
    /// Wait for provider capacity/quota rather than counting an attempt against the retry budget.
    DeferForProvider,
    /// Do not retry until configuration or content changes.
    Permanent,
    /// A definitive negative answer, cached against the submitted evidence.
    NegativeResult,
}

impl ProviderFailureClass {
    pub const fn as_db(self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid_request",
            Self::ProviderRestricted => "provider_restricted",
            Self::DeveloperAuthenticationFailed => "developer_authentication_failed",
            Self::UserAuthenticationFailed => "user_authentication_failed",
            Self::NoMatch => "no_match",
            Self::ProviderUnavailable => "provider_unavailable",
            Self::ClientRejected => "client_rejected",
            Self::CapacityDeferred => "capacity_deferred",
            Self::DailyQuotaExceeded => "daily_quota_exceeded",
            Self::NegativeQuotaExceeded => "negative_quota_exceeded",
            Self::Transport => "transport",
            Self::TransientServer => "transient_server",
            Self::MalformedResponse => "malformed_response",
            Self::CredentialsUnavailable => "credentials_unavailable",
            Self::MediaUnavailable => "media_unavailable",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        Some(match value {
            "invalid_request" => Self::InvalidRequest,
            "provider_restricted" => Self::ProviderRestricted,
            "developer_authentication_failed" => Self::DeveloperAuthenticationFailed,
            "user_authentication_failed" => Self::UserAuthenticationFailed,
            "no_match" => Self::NoMatch,
            "provider_unavailable" => Self::ProviderUnavailable,
            "client_rejected" => Self::ClientRejected,
            "capacity_deferred" => Self::CapacityDeferred,
            "daily_quota_exceeded" => Self::DailyQuotaExceeded,
            "negative_quota_exceeded" => Self::NegativeQuotaExceeded,
            "transport" => Self::Transport,
            "transient_server" => Self::TransientServer,
            "malformed_response" => Self::MalformedResponse,
            "credentials_unavailable" => Self::CredentialsUnavailable,
            "media_unavailable" => Self::MediaUnavailable,
            _ => return None,
        })
    }

    /// Retry/defer policy for this failure class.
    ///
    /// Deliberately explicit rather than "anything that is not 200 gets retried": permanent
    /// configuration and client-lifecycle failures must not burn provider budget in a loop, and
    /// quota deferral must not consume the bounded retry budget meant for transient faults.
    pub const fn disposition(self) -> FailureDisposition {
        match self {
            Self::InvalidRequest
            | Self::DeveloperAuthenticationFailed
            | Self::UserAuthenticationFailed
            | Self::ClientRejected
            | Self::CredentialsUnavailable => FailureDisposition::Permanent,
            Self::NoMatch => FailureDisposition::NegativeResult,
            Self::CapacityDeferred
            | Self::DailyQuotaExceeded
            | Self::NegativeQuotaExceeded
            | Self::ProviderRestricted
            | Self::ProviderUnavailable => FailureDisposition::DeferForProvider,
            Self::Transport
            | Self::TransientServer
            | Self::MalformedResponse
            | Self::MediaUnavailable => FailureDisposition::RetryWithBackoff,
        }
    }

    /// True when the failure describes the provider rather than this specific job, so the whole
    /// provider should stop issuing requests for a while.
    pub const fn defers_provider(self) -> bool {
        matches!(self.disposition(), FailureDisposition::DeferForProvider)
    }
}

/// Dynamic provider quota/concurrency snapshot.
///
/// Every field is optional: the scheduler must stay conservative when the provider reports
/// nothing, and must never substitute a value observed once during research.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderQuotaSnapshot {
    pub max_threads: Option<i64>,
    pub max_requests_per_minute: Option<i64>,
    pub max_requests_per_day: Option<i64>,
    pub max_negative_requests_per_day: Option<i64>,
    pub requests_today: Option<i64>,
    pub negative_requests_today: Option<i64>,
}

/// Persisted provider scheduling state, including the last quota snapshot and any deferral.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSchedulerState {
    pub provider_id: MetadataProviderId,
    pub quota: ProviderQuotaSnapshot,
    pub observed_at: Option<UnixTimestamp>,
    pub deferred_until: Option<UnixTimestamp>,
    pub defer_reason: Option<ProviderFailureClass>,
    pub consecutive_transport_failures: i64,
}

impl ProviderSchedulerState {
    pub fn empty(provider_id: MetadataProviderId) -> Self {
        Self {
            provider_id,
            quota: ProviderQuotaSnapshot::default(),
            observed_at: None,
            deferred_until: None,
            defer_reason: None,
            consecutive_transport_failures: 0,
        }
    }

    /// Concurrency permitted right now.
    ///
    /// Falls back to one in-flight request when the provider has reported nothing, and never
    /// exceeds the provider-advertised thread count.
    pub fn permitted_concurrency(&self, configured_maximum: usize) -> usize {
        let advertised = self
            .quota
            .max_threads
            .filter(|threads| *threads > 0)
            .map_or(1, |threads| threads.min(i64::from(u32::MAX)) as usize);
        advertised.min(configured_maximum.max(1)).max(1)
    }

    /// True when the provider reports a budget class as exhausted.
    pub fn daily_budget_exhausted(&self) -> bool {
        budget_exhausted(self.quota.requests_today, self.quota.max_requests_per_day)
    }

    pub fn negative_budget_exhausted(&self) -> bool {
        budget_exhausted(
            self.quota.negative_requests_today,
            self.quota.max_negative_requests_per_day,
        )
    }
}

fn budget_exhausted(used: Option<i64>, maximum: Option<i64>) -> bool {
    match (used, maximum) {
        (Some(used), Some(maximum)) if maximum > 0 => used >= maximum,
        _ => false,
    }
}

/// Optional personal provider account state exposed to the UI.
///
/// There is deliberately no variant, field, or serialization path that can carry a password.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UserAccountState {
    NotConfigured,
    Configured,
    Invalid,
    /// A record exists but the OS credential vault could not be read.
    VaultUnavailable,
}

impl UserAccountState {
    pub const fn as_db(self) -> Option<&'static str> {
        match self {
            Self::Configured => Some("configured"),
            Self::Invalid => Some("invalid"),
            Self::NotConfigured | Self::VaultUnavailable => None,
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        Some(match value {
            "configured" => Self::Configured,
            "invalid" => Self::Invalid,
            _ => return None,
        })
    }
}

/// A user-owned decision to pin one provider game for a local game.
///
/// Stored apart from provider-derived data: a provider refresh replaces normalized metadata and
/// media but must never create, alter, or remove a user selection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserProviderSelection {
    pub game_id: GameId,
    pub provider_id: MetadataProviderId,
    pub provider_game_id: String,
    pub updated_at: UnixTimestamp,
}

/// Everything the UI needs about one game's metadata, with no provider payload or secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameMetadataState {
    pub game_id: GameId,
    pub provider_id: MetadataProviderId,
    pub status: ProviderMatchStatus,
    pub match_type: Option<MatchType>,
    /// True only while a deterministic match's stored evidence still agrees with current content.
    pub deterministic: bool,
    pub provider_game_id: Option<String>,
    pub provider_rom_id: Option<String>,
    pub unsupported_reason: Option<UnsupportedContentReason>,
    pub last_failure: Option<ProviderFailureClass>,
    pub last_checked_at: Option<UnixTimestamp>,
    pub metadata: Option<ProviderMetadataRecord>,
    pub cover: Option<MediaAsset>,
    pub candidates: Vec<ProviderCandidate>,
    pub user_selection: Option<UserProviderSelection>,
    pub jobs: Vec<MetadataJob>,
}

/// Provider-wide status summary for diagnostics and settings surfaces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataProviderStatus {
    pub provider_id: MetadataProviderId,
    pub credentials_configured: bool,
    pub user_account: UserAccountState,
    pub user_account_name: Option<String>,
    pub quota: ProviderQuotaSnapshot,
    pub quota_observed_at: Option<UnixTimestamp>,
    pub deferred_until: Option<UnixTimestamp>,
    pub defer_reason: Option<ProviderFailureClass>,
    pub offline: bool,
    pub pending_jobs: i64,
    pub deferred_jobs: i64,
    pub failed_jobs: i64,
}

/// Stable, non-secret application identity sent to metadata providers.
///
/// Centralized so no call site can invent its own value and the frontend can never supply one.
/// It carries product, version, and platform information, which is what makes HTTP 426-style
/// client-lifecycle signals actionable, and deliberately carries nothing about the user.
pub fn application_softname() -> String {
    format!(
        "RetroFrontier/{} ({}-{})",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH
    )
}

/// HTTP user agent for provider requests. Same identity, HTTP-conventional shape.
pub fn application_user_agent() -> String {
    format!(
        "RetroFrontier/{} ({}; {})",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH
    )
}

/// Local content evidence for one content unit, or the reason it cannot be used.
///
/// This is the only place that decides which M4 content shapes may take part in *automatic*
/// deterministic matching. Refusals stay conservative: an undocumented canonical representation
/// is deferred, never guessed.
pub fn evidence_for_unit(unit: &ContentUnit) -> Result<MatchEvidence, UnsupportedContentReason> {
    match unit.kind {
        ContentUnitKind::Chd => return Err(UnsupportedContentReason::ChdRepresentationUndefined),
        ContentUnitKind::CueBin => {
            return Err(UnsupportedContentReason::CueBinRepresentationUndefined)
        }
        ContentUnitKind::Gdi => return Err(UnsupportedContentReason::GdiRepresentationUndefined),
        ContentUnitKind::M3u => return Err(UnsupportedContentReason::PlaylistIsNotIdentity),
        ContentUnitKind::SingleFile => {}
    }

    let primary = unit
        .files
        .iter()
        .min_by_key(|membership| membership.ordinal)
        .ok_or(UnsupportedContentReason::NoPrimaryContentFile)?;

    if !supports_automatic_deterministic_matching(unit.system_id, &primary.file.relative_path) {
        return Err(UnsupportedContentReason::ContainerRepresentationUndefined);
    }

    let file = &primary.file;
    if file.sha1.is_none() && file.md5.is_none() && file.crc32.is_none() {
        return Err(UnsupportedContentReason::MissingContentEvidence);
    }

    Ok(MatchEvidence {
        game_id: unit.game_id,
        content_unit_id: unit.id,
        system_id: unit.system_id,
        content_unit_kind: unit.kind,
        content_file_id: Some(file.id),
        size_bytes: file.size_bytes,
        crc32: file.crc32.clone(),
        md5: file.md5.clone(),
        sha1: file.sha1.clone(),
        fingerprint: unit.fingerprint.clone(),
        evidence_version: EVIDENCE_SCHEMA_VERSION,
    })
}

/// Single-file extensions whose whole-file bytes are an established provider lookup identity.
///
/// The list is intentionally an allowlist. The finalized capability matrix confirms automatic
/// matching only for ordinary single-file ROM content, so a container whose canonical
/// representation the provider has not published (RVZ, GCM, PBP, CDI, and bare disc tracks on
/// disc systems) is deferred to heuristic candidates instead of being hashed and hoped for.
fn supports_automatic_deterministic_matching(system: SystemId, relative_path: &str) -> bool {
    let extension = file_extension(relative_path);
    match system {
        SystemId::Nes
        | SystemId::Snes
        | SystemId::Nintendo64
        | SystemId::GameBoy
        | SystemId::GameBoyColor
        | SystemId::GameBoyAdvance
        | SystemId::MegaDrive => matches!(
            extension.as_str(),
            ".nes"
                | ".sfc"
                | ".smc"
                | ".n64"
                | ".z64"
                | ".v64"
                | ".gb"
                | ".gbc"
                | ".gba"
                | ".md"
                | ".gen"
                | ".smd"
                | ".bin"
        ),
        // The provider's GameCube entry lists ISO images; RVZ and GCM are not established.
        SystemId::NintendoGameCube => extension == ".iso",
        // Disc systems expose only container formats whose canonical identity is undocumented.
        SystemId::PlayStation | SystemId::SegaSaturn | SystemId::SegaDreamcast => false,
    }
}

fn file_extension(relative_path: &str) -> String {
    relative_path
        .rsplit_once('.')
        .map(|(_, extension)| format!(".{}", extension.to_ascii_lowercase()))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::library::{
        ContentFile, ContentFileAvailability, ContentFileMembership, ContentFileRole,
        ContentRootId, ContentUnitAvailability,
    };

    fn file(relative_path: &str) -> ContentFile {
        ContentFile {
            id: ContentFileId(7),
            root_id: ContentRootId(1),
            relative_path: relative_path.to_owned(),
            size_bytes: 1024,
            modified_at: 0,
            crc32: Some("AABBCCDD".to_owned()),
            md5: Some("d41d8cd98f00b204e9800998ecf8427e".to_owned()),
            sha1: Some("da39a3ee5e6b4b0d3255bfef95601890afd80709".to_owned()),
            availability: ContentFileAvailability::Available,
        }
    }

    fn unit(kind: ContentUnitKind, system_id: SystemId, relative_path: &str) -> ContentUnit {
        ContentUnit {
            id: ContentUnitId(3),
            game_id: GameId(2),
            root_id: ContentRootId(1),
            system_id,
            kind,
            local_title: "Example".to_owned(),
            primary_relative_path: relative_path.to_owned(),
            fingerprint: Some("fingerprint-1".to_owned()),
            availability: ContentUnitAvailability::Available,
            created_at: 0,
            updated_at: 0,
            files: vec![ContentFileMembership {
                ordinal: 0,
                role: ContentFileRole::Standalone,
                file: file(relative_path),
            }],
        }
    }

    #[test]
    fn single_file_cartridge_content_produces_evidence() {
        let evidence = evidence_for_unit(&unit(
            ContentUnitKind::SingleFile,
            SystemId::Snes,
            "SNES/game.sfc",
        ))
        .expect("ordinary single-file ROM content is supported");

        assert_eq!(evidence.evidence_version, EVIDENCE_SCHEMA_VERSION);
        assert_eq!(evidence.size_bytes, 1024);
        assert!(evidence.sha1.is_some());
    }

    #[test]
    fn deferred_container_formats_are_refused_conservatively() {
        let cases = [
            (
                ContentUnitKind::Chd,
                SystemId::PlayStation,
                "PS/game.chd",
                UnsupportedContentReason::ChdRepresentationUndefined,
            ),
            (
                ContentUnitKind::CueBin,
                SystemId::PlayStation,
                "PS/game.cue",
                UnsupportedContentReason::CueBinRepresentationUndefined,
            ),
            (
                ContentUnitKind::Gdi,
                SystemId::SegaDreamcast,
                "DC/game.gdi",
                UnsupportedContentReason::GdiRepresentationUndefined,
            ),
            (
                ContentUnitKind::M3u,
                SystemId::PlayStation,
                "PS/game.m3u",
                UnsupportedContentReason::PlaylistIsNotIdentity,
            ),
            (
                ContentUnitKind::SingleFile,
                SystemId::NintendoGameCube,
                "GC/game.rvz",
                UnsupportedContentReason::ContainerRepresentationUndefined,
            ),
            (
                ContentUnitKind::SingleFile,
                SystemId::PlayStation,
                "PS/game.iso",
                UnsupportedContentReason::ContainerRepresentationUndefined,
            ),
        ];

        for (kind, system, path, expected) in cases {
            assert_eq!(
                evidence_for_unit(&unit(kind, system, path)),
                Err(expected),
                "{path} must not take part in automatic deterministic matching"
            );
        }
    }

    #[test]
    fn evidence_without_any_hash_cannot_establish_a_match() {
        let mut candidate = unit(ContentUnitKind::SingleFile, SystemId::Nes, "NES/game.nes");
        candidate.files[0].file.crc32 = None;
        candidate.files[0].file.md5 = None;
        candidate.files[0].file.sha1 = None;

        assert_eq!(
            evidence_for_unit(&candidate),
            Err(UnsupportedContentReason::MissingContentEvidence)
        );
    }

    #[test]
    fn changed_hashes_or_fingerprint_stop_agreeing() {
        let stored = evidence_for_unit(&unit(
            ContentUnitKind::SingleFile,
            SystemId::Snes,
            "SNES/game.sfc",
        ))
        .expect("evidence should be produced");

        assert!(stored.agrees_with(&stored));

        let mut replaced_bytes = stored.clone();
        replaced_bytes.sha1 = Some("0000000000000000000000000000000000000000".to_owned());
        assert!(!stored.agrees_with(&replaced_bytes));

        let mut replaced_fingerprint = stored.clone();
        replaced_fingerprint.fingerprint = Some("fingerprint-2".to_owned());
        assert!(!stored.agrees_with(&replaced_fingerprint));

        let mut unreadable = stored.clone();
        unreadable.sha1 = None;
        assert!(
            !stored.agrees_with(&unreadable),
            "a missing current hash must trigger revalidation rather than satisfy a stored hash"
        );
    }

    #[test]
    fn permitted_concurrency_never_exceeds_the_advertised_thread_count() {
        let mut state = ProviderSchedulerState::empty(MetadataProviderId::ScreenScraper);
        assert_eq!(
            state.permitted_concurrency(8),
            1,
            "an unknown quota must stay conservative"
        );

        state.quota.max_threads = Some(4);
        assert_eq!(state.permitted_concurrency(8), 4);
        assert_eq!(state.permitted_concurrency(2), 2);

        state.quota.max_threads = Some(0);
        assert_eq!(state.permitted_concurrency(8), 1);
    }

    #[test]
    fn quota_classes_are_evaluated_independently() {
        let mut state = ProviderSchedulerState::empty(MetadataProviderId::ScreenScraper);
        state.quota.requests_today = Some(10_000);
        state.quota.max_requests_per_day = Some(10_000);
        state.quota.negative_requests_today = Some(5);
        state.quota.max_negative_requests_per_day = Some(1_000);

        assert!(state.daily_budget_exhausted());
        assert!(!state.negative_budget_exhausted());
    }

    #[test]
    fn failure_dispositions_separate_permanent_quota_and_transient_classes() {
        assert_eq!(
            ProviderFailureClass::InvalidRequest.disposition(),
            FailureDisposition::Permanent
        );
        assert_eq!(
            ProviderFailureClass::ClientRejected.disposition(),
            FailureDisposition::Permanent
        );
        assert_eq!(
            ProviderFailureClass::NoMatch.disposition(),
            FailureDisposition::NegativeResult
        );
        assert_eq!(
            ProviderFailureClass::DailyQuotaExceeded.disposition(),
            FailureDisposition::DeferForProvider
        );
        assert_eq!(
            ProviderFailureClass::Transport.disposition(),
            FailureDisposition::RetryWithBackoff
        );
        assert!(ProviderFailureClass::CapacityDeferred.defers_provider());
        assert!(!ProviderFailureClass::Transport.defers_provider());
    }

    #[test]
    fn application_identity_is_stable_and_carries_product_version_and_platform() {
        let softname = application_softname();

        assert!(softname.starts_with("RetroFrontier/"));
        assert!(softname.contains(env!("CARGO_PKG_VERSION")));
        assert!(softname.contains(std::env::consts::OS));
        assert!(softname.contains(std::env::consts::ARCH));
        assert_eq!(softname, application_softname());
        assert!(application_user_agent().starts_with("RetroFrontier/"));
    }

    /// The frontend types in `src/platform/ipc.ts` mirror these serialized names, so a rename here
    /// must be a deliberate, matching change on both sides.
    #[test]
    fn ipc_serialization_uses_the_documented_camel_case_names() {
        assert_eq!(
            serde_json::to_value(MetadataProviderId::ScreenScraper).unwrap(),
            "screenScraper"
        );
        assert_eq!(
            serde_json::to_value(ProviderMatchStatus::NoMatch).unwrap(),
            "noMatch"
        );
        assert_eq!(
            serde_json::to_value(MatchType::DeterministicSha1).unwrap(),
            "deterministicSha1"
        );
        assert_eq!(
            serde_json::to_value(MatchType::HeuristicUserConfirmed).unwrap(),
            "heuristicUserConfirmed"
        );
        assert_eq!(
            serde_json::to_value(UnsupportedContentReason::PlaylistIsNotIdentity).unwrap(),
            "playlistIsNotIdentity"
        );
        assert_eq!(
            serde_json::to_value(ProviderFailureClass::DailyQuotaExceeded).unwrap(),
            "dailyQuotaExceeded"
        );
        assert_eq!(
            serde_json::to_value(MetadataJobKind::RefreshCover).unwrap(),
            "refreshCover"
        );
        assert_eq!(
            serde_json::to_value(MediaAssetState::Cached).unwrap(),
            "cached"
        );
        assert_eq!(
            serde_json::to_value(UserAccountState::VaultUnavailable).unwrap(),
            "vaultUnavailable"
        );

        let quota = serde_json::to_value(ProviderQuotaSnapshot::default()).unwrap();
        let keys: Vec<&String> = quota.as_object().unwrap().keys().collect();
        assert_eq!(
            keys,
            vec![
                "maxNegativeRequestsPerDay",
                "maxRequestsPerDay",
                "maxRequestsPerMinute",
                "maxThreads",
                "negativeRequestsToday",
                "requestsToday",
            ]
        );

        let metadata = serde_json::to_value(NormalizedMetadata::default()).unwrap();
        let mut metadata_keys: Vec<&String> = metadata.as_object().unwrap().keys().collect();
        metadata_keys.sort();
        assert_eq!(
            metadata_keys,
            vec![
                "developer",
                "genre",
                "players",
                "publisher",
                "region",
                "releaseDate",
                "sortTitle",
                "synopsis",
                "title",
            ]
        );
    }

    #[test]
    fn database_encodings_round_trip() {
        for status in [
            ProviderMatchStatus::Pending,
            ProviderMatchStatus::Matched,
            ProviderMatchStatus::NoMatch,
            ProviderMatchStatus::Ambiguous,
            ProviderMatchStatus::Deferred,
            ProviderMatchStatus::Failed,
            ProviderMatchStatus::Stale,
        ] {
            assert_eq!(ProviderMatchStatus::from_db(status.as_db()), Some(status));
        }
        for match_type in [
            MatchType::DeterministicSha1,
            MatchType::DeterministicMd5,
            MatchType::DeterministicCrc32,
            MatchType::HeuristicUserConfirmed,
        ] {
            assert_eq!(MatchType::from_db(match_type.as_db()), Some(match_type));
        }
        for failure in [
            ProviderFailureClass::InvalidRequest,
            ProviderFailureClass::ProviderRestricted,
            ProviderFailureClass::DeveloperAuthenticationFailed,
            ProviderFailureClass::UserAuthenticationFailed,
            ProviderFailureClass::NoMatch,
            ProviderFailureClass::ProviderUnavailable,
            ProviderFailureClass::ClientRejected,
            ProviderFailureClass::CapacityDeferred,
            ProviderFailureClass::DailyQuotaExceeded,
            ProviderFailureClass::NegativeQuotaExceeded,
            ProviderFailureClass::Transport,
            ProviderFailureClass::TransientServer,
            ProviderFailureClass::MalformedResponse,
            ProviderFailureClass::CredentialsUnavailable,
            ProviderFailureClass::MediaUnavailable,
        ] {
            assert_eq!(
                ProviderFailureClass::from_db(failure.as_db()),
                Some(failure)
            );
        }
        for reason in [
            UnsupportedContentReason::SystemNotMapped,
            UnsupportedContentReason::ChdRepresentationUndefined,
            UnsupportedContentReason::CueBinRepresentationUndefined,
            UnsupportedContentReason::GdiRepresentationUndefined,
            UnsupportedContentReason::PlaylistIsNotIdentity,
            UnsupportedContentReason::ContainerRepresentationUndefined,
            UnsupportedContentReason::MissingContentEvidence,
            UnsupportedContentReason::NoPrimaryContentFile,
        ] {
            assert_eq!(
                UnsupportedContentReason::from_db(reason.as_db()),
                Some(reason)
            );
        }
        for kind in [
            MetadataJobKind::Identify,
            MetadataJobKind::RefreshMetadata,
            MetadataJobKind::RefreshCover,
        ] {
            assert_eq!(MetadataJobKind::from_db(kind.as_db()), Some(kind));
        }
        for state in [
            MetadataJobState::Pending,
            MetadataJobState::Running,
            MetadataJobState::Deferred,
            MetadataJobState::Failed,
            MetadataJobState::Completed,
        ] {
            assert_eq!(MetadataJobState::from_db(state.as_db()), Some(state));
        }
        assert_eq!(
            MetadataProviderId::from_db(MetadataProviderId::ScreenScraper.as_db()),
            Some(MetadataProviderId::ScreenScraper)
        );
        assert_eq!(
            MediaAssetKind::from_db(MediaAssetKind::Cover.as_db()),
            Some(MediaAssetKind::Cover)
        );
        for state in [
            MediaAssetState::Cached,
            MediaAssetState::Missing,
            MediaAssetState::Failed,
        ] {
            assert_eq!(MediaAssetState::from_db(state.as_db()), Some(state));
        }
    }
}
