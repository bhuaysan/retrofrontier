//! Primary cover cache.
//!
//! M5 caches exactly one front cover per game and provider. Publication is atomic: bytes are
//! validated and written to a temporary file in the same directory and only then renamed over the
//! target, so an interrupted or rejected download can never replace a valid existing cover.

use crate::adapters::metadata_paths::MetadataPaths;
use crate::adapters::screenscraper::parse::ACCEPTED_COVER_CONTENT_TYPES;
use crate::domain::library::GameId;
use crate::domain::metadata::MetadataProviderId;
use crate::services::metadata_provider::DownloadedMedia;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use thiserror::Error;

/// Hard cap for one cached cover. Matches the transport cap so a provider cannot fill the disk.
pub const MAX_COVER_BYTES: u64 = crate::adapters::http::MAX_MEDIA_RESPONSE_BYTES;

/// Smallest plausible image. Anything shorter cannot carry a valid header.
const MIN_COVER_BYTES: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum MediaCacheError {
    #[error("the provider returned no usable cover content type")]
    UnsupportedContentType,
    #[error("the provider cover exceeded the permitted size")]
    TooLarge,
    #[error("the provider cover content did not match its declared type")]
    ContentMismatch,
    #[error("the cover cache could not be written")]
    Unwritable,
}

/// A cover that is now safely on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishedCover {
    /// Path relative to the media cache root.
    pub relative_path: String,
    pub content_type: String,
    pub size_bytes: u64,
    pub content_sha256: String,
}

pub struct CoverCache {
    paths: MetadataPaths,
    temporary_counter: AtomicU64,
}

impl CoverCache {
    pub fn new(paths: MetadataPaths) -> Self {
        Self {
            paths,
            temporary_counter: AtomicU64::new(0),
        }
    }

    /// Absolute path of a cached cover, or `None` for an unusable stored path.
    pub fn absolute_path(&self, relative_path: &str) -> Option<PathBuf> {
        self.paths.resolve_media(relative_path)
    }

    /// True when the recorded cover is still readable on disk.
    pub fn is_cached(&self, relative_path: &str) -> bool {
        self.absolute_path(relative_path)
            .is_some_and(|path| path.is_file())
    }

    /// Validates and atomically publishes downloaded cover bytes.
    ///
    /// Every failure path leaves the previous cover untouched: validation happens before any
    /// existing file is involved, and the target is only ever replaced by a completed rename.
    pub fn publish(
        &self,
        game_id: GameId,
        provider_id: MetadataProviderId,
        media: &DownloadedMedia,
    ) -> Result<PublishedCover, MediaCacheError> {
        let content_type = media
            .content_type
            .as_deref()
            .map(|value| value.split(';').next().unwrap_or(value).trim())
            .filter(|value| {
                ACCEPTED_COVER_CONTENT_TYPES
                    .iter()
                    .any(|accepted| accepted.eq_ignore_ascii_case(value))
            })
            .ok_or(MediaCacheError::UnsupportedContentType)?
            .to_ascii_lowercase();

        if media.bytes.len() as u64 > MAX_COVER_BYTES {
            return Err(MediaCacheError::TooLarge);
        }
        if media.bytes.len() < MIN_COVER_BYTES {
            return Err(MediaCacheError::ContentMismatch);
        }
        // The declared content type is provider-controlled, so the bytes have to agree with it.
        if !content_matches(&content_type, &media.bytes) {
            return Err(MediaCacheError::ContentMismatch);
        }

        let relative_path = format!(
            "covers/{}/{}.{}",
            provider_id.as_db(),
            game_id.0,
            extension_for(&content_type)
        );
        let target = self
            .paths
            .resolve_media(&relative_path)
            .ok_or(MediaCacheError::Unwritable)?;
        let parent = target.parent().ok_or(MediaCacheError::Unwritable)?;
        fs::create_dir_all(parent).map_err(|_| MediaCacheError::Unwritable)?;

        // Temporary file in the target directory keeps the rename atomic on one filesystem.
        let temporary = parent.join(format!(
            ".{}.{}.part",
            game_id.0,
            self.temporary_counter.fetch_add(1, Ordering::Relaxed)
        ));
        write_and_sync(&temporary, &media.bytes).inspect_err(|_| {
            let _ = fs::remove_file(&temporary);
        })?;
        fs::rename(&temporary, &target).map_err(|_| {
            let _ = fs::remove_file(&temporary);
            MediaCacheError::Unwritable
        })?;

        let content_sha256 = hex_digest(&media.bytes);
        Ok(PublishedCover {
            relative_path,
            content_type,
            size_bytes: media.bytes.len() as u64,
            content_sha256,
        })
    }

    /// Removes a superseded cover file. Failure is not an error: the database row is authoritative.
    pub fn remove(&self, relative_path: &str) {
        if let Some(path) = self.absolute_path(relative_path) {
            let _ = fs::remove_file(path);
        }
    }

    /// Deletes leftover partial files from an interrupted download.
    pub fn clean_partial_downloads(&self) {
        let Some(covers) = self.paths.resolve_media("covers") else {
            return;
        };
        let Ok(providers) = fs::read_dir(covers) else {
            return;
        };
        for provider in providers.flatten() {
            let Ok(entries) = fs::read_dir(provider.path()) else {
                continue;
            };
            for entry in entries.flatten() {
                if entry.file_name().to_string_lossy().ends_with(".part") {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }
    }
}

fn write_and_sync(path: &Path, bytes: &[u8]) -> Result<(), MediaCacheError> {
    let mut file = fs::File::create(path).map_err(|_| MediaCacheError::Unwritable)?;
    file.write_all(bytes)
        .map_err(|_| MediaCacheError::Unwritable)?;
    file.sync_all().map_err(|_| MediaCacheError::Unwritable)?;
    Ok(())
}

/// Checks the container signature so a mislabelled or truncated payload is rejected.
fn content_matches(content_type: &str, bytes: &[u8]) -> bool {
    match content_type {
        "image/png" => bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]),
        "image/jpeg" => bytes.starts_with(&[0xFF, 0xD8, 0xFF]),
        "image/webp" => bytes.starts_with(b"RIFF") && bytes.len() > 12 && &bytes[8..12] == b"WEBP",
        _ => false,
    }
}

fn extension_for(content_type: &str) -> &'static str {
    match content_type {
        "image/jpeg" => "jpg",
        "image/webp" => "webp",
        _ => "png",
    }
}

fn hex_digest(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().fold(String::new(), |mut output, byte| {
        use std::fmt::Write;
        let _ = write!(output, "{byte:02x}");
        output
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// Smallest synthetic PNG-signature payload. Not real provider artwork.
    fn synthetic_png() -> DownloadedMedia {
        let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        bytes.extend_from_slice(b"synthetic-cover-a");
        DownloadedMedia {
            content_type: Some("image/png".to_owned()),
            bytes,
        }
    }

    fn synthetic_jpeg() -> DownloadedMedia {
        let mut bytes = vec![0xFF, 0xD8, 0xFF];
        bytes.extend_from_slice(b"synthetic-cover-jpeg-payload");
        DownloadedMedia {
            content_type: Some("image/jpeg".to_owned()),
            bytes,
        }
    }

    fn cache(root: &Path) -> CoverCache {
        CoverCache::new(MetadataPaths::new(root))
    }

    #[test]
    fn a_valid_cover_is_published_under_the_app_owned_media_root() {
        let directory = tempdir().expect("temporary directory");
        let cache = cache(directory.path());

        let published = cache
            .publish(
                GameId(12),
                MetadataProviderId::ScreenScraper,
                &synthetic_png(),
            )
            .expect("a valid synthetic cover should publish");

        assert_eq!(published.relative_path, "covers/screenscraper/12.png");
        assert_eq!(published.content_type, "image/png");
        assert_eq!(published.size_bytes, synthetic_png().bytes.len() as u64);
        assert_eq!(published.content_sha256.len(), 64);
        let absolute = cache.absolute_path(&published.relative_path).unwrap();
        assert!(absolute.is_file());
        assert!(absolute.starts_with(directory.path().join("metadata").join("media")));
        assert!(cache.is_cached(&published.relative_path));
    }

    #[test]
    fn an_invalid_content_type_is_refused_before_anything_is_written() {
        let directory = tempdir().expect("temporary directory");
        let cache = cache(directory.path());

        for content_type in [
            None,
            Some("text/html".to_owned()),
            Some("video/mp4".to_owned()),
        ] {
            let media = DownloadedMedia {
                content_type,
                bytes: synthetic_png().bytes,
            };
            assert_eq!(
                cache.publish(GameId(1), MetadataProviderId::ScreenScraper, &media),
                Err(MediaCacheError::UnsupportedContentType)
            );
        }
        assert!(!directory.path().join("metadata").join("media").exists());
    }

    #[test]
    fn oversized_and_mislabelled_content_is_refused() {
        let directory = tempdir().expect("temporary directory");
        let cache = cache(directory.path());

        let oversized = DownloadedMedia {
            content_type: Some("image/png".to_owned()),
            bytes: vec![0; MAX_COVER_BYTES as usize + 1],
        };
        assert_eq!(
            cache.publish(GameId(1), MetadataProviderId::ScreenScraper, &oversized),
            Err(MediaCacheError::TooLarge)
        );

        let mislabelled = DownloadedMedia {
            content_type: Some("image/png".to_owned()),
            bytes: b"this is definitely not a png file".to_vec(),
        };
        assert_eq!(
            cache.publish(GameId(1), MetadataProviderId::ScreenScraper, &mislabelled),
            Err(MediaCacheError::ContentMismatch)
        );

        let truncated = DownloadedMedia {
            content_type: Some("image/png".to_owned()),
            bytes: vec![0x89, b'P', b'N', b'G'],
        };
        assert_eq!(
            cache.publish(GameId(1), MetadataProviderId::ScreenScraper, &truncated),
            Err(MediaCacheError::ContentMismatch)
        );
    }

    #[test]
    fn a_rejected_refresh_leaves_the_previous_cover_in_place() {
        let directory = tempdir().expect("temporary directory");
        let cache = cache(directory.path());
        let published = cache
            .publish(
                GameId(9),
                MetadataProviderId::ScreenScraper,
                &synthetic_png(),
            )
            .expect("first publication should succeed");
        let absolute = cache.absolute_path(&published.relative_path).unwrap();
        let original = fs::read(&absolute).expect("cover should be readable");

        let broken = DownloadedMedia {
            content_type: Some("text/plain".to_owned()),
            bytes: b"NOMEDIA".to_vec(),
        };
        assert!(cache
            .publish(GameId(9), MetadataProviderId::ScreenScraper, &broken)
            .is_err());

        assert_eq!(
            fs::read(&absolute).expect("cover should still be readable"),
            original,
            "a failed refresh must retain the last-known-good cover"
        );
    }

    #[test]
    fn a_successful_refresh_replaces_the_cover_atomically() {
        let directory = tempdir().expect("temporary directory");
        let cache = cache(directory.path());
        let first = cache
            .publish(
                GameId(4),
                MetadataProviderId::ScreenScraper,
                &synthetic_png(),
            )
            .expect("first publication");

        let mut replacement = synthetic_png();
        replacement.bytes.extend_from_slice(b"-refreshed");
        let second = cache
            .publish(GameId(4), MetadataProviderId::ScreenScraper, &replacement)
            .expect("refresh should publish");

        assert_eq!(first.relative_path, second.relative_path);
        assert_ne!(first.content_sha256, second.content_sha256);
        let absolute = cache.absolute_path(&second.relative_path).unwrap();
        assert_eq!(fs::read(absolute).unwrap(), replacement.bytes);
        // No partial file may survive a successful publication.
        let provider_directory = directory.path().join("metadata/media/covers/screenscraper");
        let leftovers: Vec<_> = fs::read_dir(&provider_directory)
            .unwrap()
            .flatten()
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".part"))
            .collect();
        assert!(leftovers.is_empty());
    }

    #[test]
    fn a_content_type_change_produces_a_new_cache_identity() {
        let directory = tempdir().expect("temporary directory");
        let cache = cache(directory.path());

        let png = cache
            .publish(
                GameId(7),
                MetadataProviderId::ScreenScraper,
                &synthetic_png(),
            )
            .expect("png publication");
        let jpeg = cache
            .publish(
                GameId(7),
                MetadataProviderId::ScreenScraper,
                &synthetic_jpeg(),
            )
            .expect("jpeg publication");

        assert_eq!(png.relative_path, "covers/screenscraper/7.png");
        assert_eq!(jpeg.relative_path, "covers/screenscraper/7.jpg");
        cache.remove(&png.relative_path);
        assert!(!cache.is_cached(&png.relative_path));
        assert!(cache.is_cached(&jpeg.relative_path));
    }

    #[test]
    fn leftover_partial_files_are_cleaned_up_on_demand() {
        let directory = tempdir().expect("temporary directory");
        let cache = cache(directory.path());
        cache
            .publish(
                GameId(1),
                MetadataProviderId::ScreenScraper,
                &synthetic_png(),
            )
            .expect("publication");
        let provider_directory = directory.path().join("metadata/media/covers/screenscraper");
        let partial = provider_directory.join(".interrupted.part");
        fs::write(&partial, b"partial").expect("partial fixture should be written");

        cache.clean_partial_downloads();

        assert!(!partial.exists());
        assert!(cache.is_cached("covers/screenscraper/1.png"));
    }

    #[test]
    fn an_unusable_stored_path_cannot_escape_the_cache_root() {
        let directory = tempdir().expect("temporary directory");
        let cache = cache(directory.path());

        assert_eq!(cache.absolute_path("../../escape.png"), None);
        assert!(!cache.is_cached("../../escape.png"));
    }
}
