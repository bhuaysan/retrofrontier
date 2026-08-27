//! App-owned filesystem layout for cached provider metadata media.
//!
//! Deliberately narrow: this type can only produce paths below the application data directory's
//! `metadata/` subtree. It exposes no ROM, BIOS, runtime, save, or database path, so a media
//! download cannot land beside user content even through a bug.

use std::path::{Path, PathBuf};

const METADATA_DIRECTORY: &str = "metadata";
const MEDIA_DIRECTORY: &str = "media";

#[derive(Debug, Clone)]
pub struct MetadataPaths {
    metadata_root: PathBuf,
    media_root: PathBuf,
}

impl MetadataPaths {
    pub fn new(app_data_root: impl AsRef<Path>) -> Self {
        let metadata_root = app_data_root.as_ref().join(METADATA_DIRECTORY);
        Self {
            media_root: metadata_root.join(MEDIA_DIRECTORY),
            metadata_root,
        }
    }

    pub fn metadata_root(&self) -> &Path {
        &self.metadata_root
    }

    /// Root of the cached media tree. Persisted media paths are relative to this directory so the
    /// database never depends on an absolute developer or user path.
    pub fn media_root(&self) -> &Path {
        &self.media_root
    }

    /// Resolves a persisted relative media path.
    ///
    /// Returns `None` for any path that is absolute or tries to escape the media root, so a
    /// corrupted database row cannot be turned into an arbitrary filesystem read.
    pub fn resolve_media(&self, relative_path: &str) -> Option<PathBuf> {
        if relative_path.is_empty() {
            return None;
        }
        let candidate = Path::new(relative_path);
        if candidate.is_absolute() {
            return None;
        }
        if candidate.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        }) {
            return None;
        }
        Some(self.media_root.join(candidate))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_paths_stay_inside_the_app_owned_metadata_subtree() {
        let paths = MetadataPaths::new("/app-data");

        assert_eq!(paths.metadata_root(), Path::new("/app-data/metadata"));
        assert_eq!(paths.media_root(), Path::new("/app-data/metadata/media"));
        assert_eq!(
            paths.resolve_media("covers/screenscraper/12.png"),
            Some(PathBuf::from(
                "/app-data/metadata/media/covers/screenscraper/12.png"
            ))
        );
    }

    #[test]
    fn traversal_and_absolute_relative_paths_are_refused() {
        let paths = MetadataPaths::new("/app-data");

        assert_eq!(paths.resolve_media(""), None);
        assert_eq!(paths.resolve_media("../../etc/passwd"), None);
        assert_eq!(paths.resolve_media("covers/../../escape.png"), None);
        assert_eq!(paths.resolve_media("/etc/passwd"), None);
    }

    #[test]
    fn the_media_root_is_never_a_rom_bios_or_runtime_location() {
        let paths = MetadataPaths::new("/app-data");
        let rendered = paths.media_root().to_string_lossy().to_string();

        for forbidden in ["ROMs", "BIOS", "runtime", "saves", "states", "database"] {
            assert!(
                !rendered.contains(forbidden),
                "the media cache must not live under {forbidden}"
            );
        }
    }
}
