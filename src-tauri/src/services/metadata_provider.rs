//! Provider-neutral metadata boundary.
//!
//! The operations here are semantic ("identify this content", "search candidates", "fetch this
//! game", "download the selected cover"), not a mirror of any provider's HTTP endpoints. Provider
//! endpoint names, provider system identifiers, provider field names, and HTTP status codes must
//! not appear above this line.

use crate::domain::metadata::{
    MatchEvidence, MetadataProviderId, NormalizedMetadata, ProviderCandidate, ProviderFailureClass,
    ProviderQuotaSnapshot,
};
use crate::domain::system::SystemId;
use async_trait::async_trait;
use std::fmt;

/// Content submitted for deterministic identification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentIdentificationRequest {
    pub system_id: SystemId,
    /// Local evidence snapshot. All available hashes are offered; the provider decides what it can
    /// use, and the caller validates whatever comes back.
    pub evidence: MatchEvidence,
    /// Basename only. Providers reject paths, and a path would leak the user's directory layout.
    pub file_basename: String,
}

/// Heuristic name search. Never a source of deterministic evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateSearchRequest {
    pub system_id: SystemId,
    pub title: String,
}

/// A concrete content record the provider claims to hold, with its own evidence.
///
/// This is what makes a match deterministic: RetroFrontier compares these returned values against
/// the local evidence instead of trusting a successful response.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProviderRomRecord {
    pub provider_rom_id: Option<String>,
    pub filename: Option<String>,
    pub size_bytes: Option<u64>,
    pub crc32: Option<String>,
    pub md5: Option<String>,
    pub sha1: Option<String>,
    pub support_number: Option<i64>,
    pub support_count: Option<i64>,
}

impl ProviderRomRecord {
    pub fn has_any_hash(&self) -> bool {
        self.crc32.is_some() || self.md5.is_some() || self.sha1.is_some()
    }
}

/// Opaque provider-side locator for one selected media asset.
///
/// It may embed credential material, so it has a redacted `Debug`, cannot be serialized, is never
/// persisted, and is never returned through IPC. Its only legitimate use is to hand it straight
/// back to the adapter that produced it.
#[derive(Clone, PartialEq, Eq)]
pub struct ProviderMediaLocator(String);

impl ProviderMediaLocator {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ProviderMediaLocator {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(crate::adapters::credentials::REDACTED)
    }
}

/// The provider's selected primary cover, described without its URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderCoverDescriptor {
    /// Provider media category identifier, kept for provenance and refresh comparison.
    pub provider_media_type: String,
    pub region: Option<String>,
    pub crc32: Option<String>,
    pub md5: Option<String>,
    pub sha1: Option<String>,
    pub source_credit: Option<String>,
    pub locator: ProviderMediaLocator,
}

/// One provider game, already normalized.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderGameRecord {
    pub provider_game_id: String,
    /// Top-level content identifier. Stored separately from `matched_rom` because providers may
    /// expose these in different schema locations with different values.
    pub provider_rom_id: Option<String>,
    /// The specific record the provider matched to the submitted evidence, when it reported one.
    pub matched_rom: Option<ProviderRomRecord>,
    /// Every content record the provider lists for this game, used to detect conflicts.
    pub roms: Vec<ProviderRomRecord>,
    pub metadata: NormalizedMetadata,
    pub source_credit: Option<String>,
    pub primary_cover: Option<ProviderCoverDescriptor>,
}

/// Downloaded media bytes. Publication to the cache is the media service's job, not the adapter's.
#[derive(Clone, PartialEq, Eq)]
pub struct DownloadedMedia {
    pub content_type: Option<String>,
    pub bytes: Vec<u8>,
}

impl fmt::Debug for DownloadedMedia {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DownloadedMedia")
            .field("content_type", &self.content_type)
            .field("size_bytes", &self.bytes.len())
            .finish()
    }
}

/// A successful provider call plus any quota information it reported.
///
/// Quota travels with the response so the scheduler always consumes the provider's own current
/// numbers instead of a value observed once during research.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderResponse<T> {
    pub value: T,
    pub quota: Option<ProviderQuotaSnapshot>,
}

impl<T> ProviderResponse<T> {
    pub fn new(value: T, quota: Option<ProviderQuotaSnapshot>) -> Self {
        Self { value, quota }
    }
}

pub type ProviderResult<T> = Result<ProviderResponse<T>, ProviderFailureClass>;

#[async_trait]
pub trait MetadataProvider: Send + Sync {
    fn provider_id(&self) -> MetadataProviderId;

    /// True when this provider has an unambiguous mapping for the RetroFrontier system.
    fn supports_system(&self, system: SystemId) -> bool;

    /// Ask the provider to identify concrete content from hash/size evidence.
    async fn identify_content(
        &self,
        request: &ContentIdentificationRequest,
    ) -> ProviderResult<ProviderGameRecord>;

    /// Heuristic title search. Results are suggestions only.
    async fn search_candidates(
        &self,
        request: &CandidateSearchRequest,
    ) -> ProviderResult<Vec<ProviderCandidate>>;

    /// Retrieve a known provider game by its own identifier.
    ///
    /// This is identity retrieval after a match, never new matching evidence.
    async fn fetch_game(
        &self,
        system: SystemId,
        provider_game_id: &str,
    ) -> ProviderResult<ProviderGameRecord>;

    /// Download the previously selected primary cover.
    async fn download_media(
        &self,
        locator: &ProviderMediaLocator,
    ) -> ProviderResult<DownloadedMedia>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_locators_and_downloads_never_render_their_contents() {
        let locator = ProviderMediaLocator::new(
            "https://provider.invalid/media?devid=real-id&devpassword=real-password",
        );

        let rendered = format!("{locator:?}");
        assert_eq!(rendered, crate::adapters::credentials::REDACTED);
        assert!(!rendered.contains("real-password"));

        let media = DownloadedMedia {
            content_type: Some("image/png".to_owned()),
            bytes: vec![1, 2, 3, 4],
        };
        let rendered_media = format!("{media:?}");
        assert!(rendered_media.contains("size_bytes: 4"));
        assert!(!rendered_media.contains("[1, 2, 3, 4]"));
    }
}
