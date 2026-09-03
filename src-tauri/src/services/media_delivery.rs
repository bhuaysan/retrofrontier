//! Narrow native delivery boundary for application-owned cached covers.
//!
//! A WebView receives an opaque target-specific reference (`rfmedia://localhost/cover/<game-id>` on
//! Linux/macOS desktop, `http://rfmedia.localhost/cover/<game-id>` on Windows). It never receives
//! the persisted relative cache path, an absolute path, or a provider URL. The route is resolved
//! back to the current durable media row and the cover cache performs the final containment and
//! image validation checks before bytes leave Rust.

use crate::adapters::metadata_paths::MetadataPaths;
use crate::domain::library::GameId;
use crate::domain::metadata::{MediaAssetKind, MediaAssetState, MetadataProviderId};
use crate::repositories::metadata::MetadataRepository;
use crate::services::metadata_media::{CoverCache, DeliveredCover};
use std::sync::Arc;
use thiserror::Error;

pub const CACHED_COVER_PROTOCOL: &str = "rfmedia";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum CoverDeliveryError {
    #[error("the requested cached cover was not found")]
    NotFound,
    #[error("cached cover storage is unavailable")]
    Internal,
}

#[derive(Clone)]
pub struct CachedCoverDelivery {
    repository: MetadataRepository,
    covers: Arc<CoverCache>,
}

impl CachedCoverDelivery {
    pub fn new(pool: sqlx::SqlitePool, paths: MetadataPaths) -> Self {
        Self {
            repository: MetadataRepository::new(pool),
            covers: Arc::new(CoverCache::new(paths)),
        }
    }

    /// Loads one cover by stable local game identity, never by a caller-provided path.
    pub async fn load_cover(&self, game_id: GameId) -> Result<DeliveredCover, CoverDeliveryError> {
        let asset = self
            .repository
            .load_media_asset(
                game_id,
                MetadataProviderId::ScreenScraper,
                MediaAssetKind::Cover,
            )
            .await
            .map_err(|error| {
                tracing::warn!(error = %error, game_id = %game_id, "cached cover lookup failed");
                CoverDeliveryError::Internal
            })?
            .ok_or(CoverDeliveryError::NotFound)?;
        if asset.state != MediaAssetState::Cached {
            return Err(CoverDeliveryError::NotFound);
        }
        let relative_path = asset
            .cache_relative_path
            .as_deref()
            .ok_or(CoverDeliveryError::NotFound)?;

        self.covers
            .read_for_delivery(relative_path, asset.content_type.as_deref())
            .map_err(|error| {
                tracing::debug!(error = %error, game_id = %game_id, "cached cover is not deliverable");
                CoverDeliveryError::NotFound
            })
    }
}

/// Delivery of one verified Save-State thumbnail.
///
/// It mirrors the cached-cover boundary exactly: the WebView holds an opaque reference keyed by
/// `SaveStateId`, never a path, and the bytes are read only after the *registered* size and digest
/// have been re-proved against the current file through the no-follow adapter. A thumbnail whose
/// state is no longer `available`, or whose file no longer matches, is simply not found.
#[derive(Clone)]
pub struct SaveStateThumbnailDelivery {
    save_states: Arc<crate::application::SaveStateApplicationService>,
}

impl SaveStateThumbnailDelivery {
    pub fn new(save_states: Arc<crate::application::SaveStateApplicationService>) -> Self {
        Self { save_states }
    }

    pub async fn load_thumbnail(
        &self,
        id: crate::domain::save_state::SaveStateId,
    ) -> Result<DeliveredCover, CoverDeliveryError> {
        let (path, expected_size) = self
            .save_states
            .verified_thumbnail(id)
            .await
            .map_err(|error| {
                tracing::debug!(save_state_id = %id, code = error.as_str(), "no deliverable save-state thumbnail");
                CoverDeliveryError::NotFound
            })?;
        let bytes = std::fs::read(&path).map_err(|_| CoverDeliveryError::NotFound)?;
        // The digest was verified a moment ago; the length is re-checked here so a concurrent
        // rewrite cannot deliver a different number of bytes than the one that was proved.
        if bytes.len() as u64 != expected_size {
            return Err(CoverDeliveryError::NotFound);
        }
        Ok(DeliveredCover {
            bytes,
            // RetroArch writes its state thumbnails as PNG. The value is fixed rather than sniffed,
            // and `X-Content-Type-Options: nosniff` is already set on the response.
            content_type: "image/png".to_owned(),
        })
    }
}

/// Parses the Save-State thumbnail route.
///
/// Percent-encoded, absolute, query-like, and traversal-shaped values are rejected instead of
/// being decoded into anything, exactly as the cover route rejects them.
pub fn parse_save_state_thumbnail_route(
    path: &str,
) -> Option<crate::domain::save_state::SaveStateId> {
    let id = path.strip_prefix("/save-state-thumbnail/")?;
    if id.is_empty() || !id.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let value = id.parse::<i64>().ok()?;
    (value > 0).then_some(crate::domain::save_state::SaveStateId(value))
}

/// Parses the only route understood by the custom protocol. Percent-encoded, absolute, query-like,
/// and traversal-shaped values are rejected instead of being decoded into filesystem paths.
pub fn parse_cover_route(path: &str) -> Option<GameId> {
    let id = path.strip_prefix("/cover/")?;
    if id.is_empty() || !id.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let value = id.parse::<i64>().ok()?;
    (value > 0).then_some(GameId(value))
}

pub fn protocol_error_response(status: u16) -> tauri::http::Response<Vec<u8>> {
    tauri::http::Response::builder()
        .status(status)
        .header("Content-Type", "text/plain; charset=utf-8")
        .header("X-Content-Type-Options", "nosniff")
        .body(Vec::new())
        .unwrap_or_else(|_| tauri::http::Response::new(Vec::new()))
}

pub fn cover_response(cover: DeliveredCover) -> tauri::http::Response<Vec<u8>> {
    tauri::http::Response::builder()
        .status(200)
        .header("Content-Type", cover.content_type)
        .header("Cache-Control", "no-store")
        .header("X-Content-Type-Options", "nosniff")
        .body(cover.bytes)
        .unwrap_or_else(|_| tauri::http::Response::new(Vec::new()))
}

pub fn app_error_status(error: CoverDeliveryError) -> u16 {
    match error {
        CoverDeliveryError::NotFound => 404,
        CoverDeliveryError::Internal => 500,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        app_error_status, cover_response, parse_cover_route, protocol_error_response,
        CACHED_COVER_PROTOCOL,
    };
    use crate::adapters::database::Database;
    use crate::adapters::metadata_paths::MetadataPaths;
    use crate::domain::library::GameId;
    use crate::domain::metadata::{MediaAssetKind, MediaAssetState, MetadataProviderId};
    use crate::repositories::metadata::{MediaAssetWrite, MetadataRepository};
    use crate::services::metadata_media::DeliveredCover;
    use crate::services::metadata_provider::DownloadedMedia;
    use tempfile::tempdir;

    #[test]
    fn only_positive_game_id_cover_routes_are_accepted() {
        assert_eq!(CACHED_COVER_PROTOCOL, "rfmedia");
        assert_eq!(parse_cover_route("/cover/42"), Some(GameId(42)));
        for route in [
            "/cover/",
            "/cover/0",
            "/cover/-1",
            "/cover/1/../2",
            "/cover/%2e%2e/1",
            "/etc/passwd",
            "/cover/1?path=/etc/passwd",
        ] {
            assert_eq!(
                parse_cover_route(route),
                None,
                "route should be rejected: {route}"
            );
        }
    }

    #[test]
    fn protocol_responses_preserve_valid_mime_and_safe_error_statuses() {
        let response = cover_response(DeliveredCover {
            bytes: vec![1, 2, 3],
            content_type: "image/webp".to_owned(),
        });
        assert_eq!(response.status(), 200);
        assert_eq!(response.headers()["Content-Type"], "image/webp");
        assert_eq!(response.headers()["X-Content-Type-Options"], "nosniff");
        assert_eq!(response.body(), &vec![1, 2, 3]);

        let missing =
            protocol_error_response(app_error_status(super::CoverDeliveryError::NotFound));
        assert_eq!(missing.status(), 404);
        assert_eq!(missing.headers()["X-Content-Type-Options"], "nosniff");
    }

    #[tokio::test]
    async fn durable_cached_media_is_delivered_as_valid_bytes_and_content_type() {
        let directory = tempdir().expect("temporary directory");
        let database = Database::open(directory.path().join("database.sqlite3"))
            .await
            .expect("database should open");
        let game_id = GameId(1);
        sqlx::query(
            "INSERT INTO games (id, system_id, local_title, availability, created_at, updated_at) \
             VALUES (?, 'nes', 'Synthetic Game', 'available', 1, 1)",
        )
        .bind(game_id.0)
        .execute(database.pool())
        .await
        .expect("synthetic game should persist");

        let paths = MetadataPaths::new(directory.path());
        let cache = crate::services::metadata_media::CoverCache::new(paths.clone());
        let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        bytes.extend_from_slice(b"durable-synthetic-cover");
        let media = DownloadedMedia {
            content_type: Some("image/png".to_owned()),
            bytes: bytes.clone(),
        };
        let published = cache
            .publish(game_id, MetadataProviderId::ScreenScraper, &media)
            .expect("synthetic cover should publish");
        MetadataRepository::new(database.pool().clone())
            .persist_media_asset(
                &MediaAssetWrite {
                    game_id,
                    provider_id: MetadataProviderId::ScreenScraper,
                    kind: MediaAssetKind::Cover,
                    state: MediaAssetState::Cached,
                    provider_media_type: None,
                    region: None,
                    cache_relative_path: Some(published.relative_path.clone()),
                    content_type: Some(published.content_type.clone()),
                    size_bytes: Some(published.size_bytes),
                    content_sha256: Some(published.content_sha256.clone()),
                    provider_crc32: None,
                    provider_md5: None,
                    provider_sha1: None,
                    source_credit: None,
                    last_failure: None,
                    fetched_at: Some(1),
                },
                1,
            )
            .await
            .expect("durable media row should persist");

        let delivery = super::CachedCoverDelivery::new(database.pool().clone(), paths);
        let delivered = delivery
            .load_cover(game_id)
            .await
            .expect("durable cached cover should load");
        assert_eq!(delivered.bytes, bytes);
        assert_eq!(delivered.content_type, "image/png");
    }
}
