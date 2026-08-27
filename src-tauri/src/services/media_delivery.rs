//! Narrow native delivery boundary for application-owned cached covers.
//!
//! A WebView receives an opaque `rfmedia://localhost/cover/<game-id>` reference. It never receives
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
    use crate::domain::library::GameId;
    use crate::services::metadata_media::DeliveredCover;

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
}
