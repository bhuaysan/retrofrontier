use crate::error::AppError;
use std::fs;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};

const PRIVATE_DIRECTORY_MODE: u32 = 0o700;

/// Every RetroArch path RetroFrontier controls at launch time.
///
/// All of it lives below the OS application-data directory, so nothing is written beside user
/// ROMs, into the user's own RetroArch configuration, or inside a replaceable runtime version
/// tree. No home directory is hard-coded: the root comes from the platform path API.
#[derive(Debug, Clone)]
pub struct LaunchPaths {
    app_data_root: PathBuf,
    runtime_user_root: PathBuf,
    saves_root: PathBuf,
    states_root: PathBuf,
    screenshots_root: PathBuf,
    log_root: PathBuf,
}

impl LaunchPaths {
    pub fn new(app_data_root: impl Into<PathBuf>) -> Self {
        let app_data_root = app_data_root.into();
        Self {
            runtime_user_root: app_data_root.join("runtime-user"),
            saves_root: app_data_root.join("saves"),
            states_root: app_data_root.join("states"),
            screenshots_root: app_data_root.join("screenshots"),
            log_root: app_data_root.join("logs").join("retroarch"),
            app_data_root,
        }
    }

    pub fn app_data_root(&self) -> &Path {
        &self.app_data_root
    }

    pub fn runtime_user_root(&self) -> &Path {
        &self.runtime_user_root
    }

    /// The one RetroFrontier-owned RetroArch configuration file.
    pub fn config_file(&self) -> PathBuf {
        self.runtime_user_root.join("config").join("retroarch.cfg")
    }

    pub fn config_root(&self) -> PathBuf {
        self.runtime_user_root.join("config")
    }

    /// The composed RetroArch `system_directory`.
    ///
    /// RetroFrontier owns this directory and links validated user BIOS files and verified managed
    /// support assets into it. User BIOS files themselves are never modified, moved, or copied.
    pub fn system_root(&self) -> PathBuf {
        self.runtime_user_root.join("system")
    }

    pub fn core_info_root(&self) -> PathBuf {
        self.runtime_user_root.join("core-info")
    }

    pub fn core_options_file(&self) -> PathBuf {
        self.runtime_user_root
            .join("core-options")
            .join("core-options.cfg")
    }

    pub fn core_options_root(&self) -> PathBuf {
        self.runtime_user_root.join("core-options")
    }

    pub fn assets_root(&self) -> PathBuf {
        self.runtime_user_root.join("assets")
    }

    pub fn core_assets_root(&self) -> PathBuf {
        self.runtime_user_root.join("core-assets")
    }

    pub fn shaders_root(&self) -> PathBuf {
        self.runtime_user_root.join("shaders")
    }

    pub fn video_filters_root(&self) -> PathBuf {
        self.runtime_user_root.join("filters").join("video")
    }

    pub fn audio_filters_root(&self) -> PathBuf {
        self.runtime_user_root.join("filters").join("audio")
    }

    pub fn playlists_root(&self) -> PathBuf {
        self.runtime_user_root.join("playlists")
    }

    pub fn history_root(&self) -> PathBuf {
        self.runtime_user_root.join("history")
    }

    pub fn content_history_file(&self) -> PathBuf {
        self.history_root().join("content_history.lpl")
    }

    pub fn content_music_history_file(&self) -> PathBuf {
        self.history_root().join("content_music_history.lpl")
    }

    pub fn content_image_history_file(&self) -> PathBuf {
        self.history_root().join("content_image_history.lpl")
    }

    pub fn content_video_history_file(&self) -> PathBuf {
        self.history_root().join("content_video_history.lpl")
    }

    pub fn content_favorites_file(&self) -> PathBuf {
        self.history_root().join("content_favorites.lpl")
    }

    pub fn remaps_root(&self) -> PathBuf {
        self.runtime_user_root.join("remaps")
    }

    pub fn autoconfig_root(&self) -> PathBuf {
        self.runtime_user_root.join("autoconfig")
    }

    pub fn cache_root(&self) -> PathBuf {
        self.runtime_user_root.join("cache")
    }

    pub fn thumbnails_root(&self) -> PathBuf {
        self.runtime_user_root.join("thumbnails")
    }

    pub fn wallpapers_root(&self) -> PathBuf {
        self.runtime_user_root.join("wallpapers")
    }

    pub fn overlays_root(&self) -> PathBuf {
        self.runtime_user_root.join("overlays")
    }

    pub fn database_root(&self) -> PathBuf {
        self.runtime_user_root.join("database")
    }

    pub fn recordings_output_root(&self) -> PathBuf {
        self.runtime_user_root.join("recordings").join("output")
    }

    pub fn recordings_config_root(&self) -> PathBuf {
        self.runtime_user_root.join("recordings").join("config")
    }

    pub fn menu_browser_root(&self) -> PathBuf {
        self.runtime_user_root.join("menu").join("browser")
    }

    pub fn menu_config_root(&self) -> PathBuf {
        self.runtime_user_root.join("menu").join("config")
    }

    /// The child's XDG base directories. They replace whatever the host environment declared, so a
    /// hostile `XDG_CONFIG_HOME` cannot redirect RetroArch at another configuration.
    pub fn xdg_config_root(&self) -> PathBuf {
        self.runtime_user_root.join("xdg").join("config")
    }

    pub fn xdg_data_root(&self) -> PathBuf {
        self.runtime_user_root.join("xdg").join("data")
    }

    pub fn xdg_cache_root(&self) -> PathBuf {
        self.runtime_user_root.join("xdg").join("cache")
    }

    pub fn xdg_state_root(&self) -> PathBuf {
        self.runtime_user_root.join("xdg").join("state")
    }

    /// Normal emulator save data. Deliberately outside every runtime version tree so a runtime
    /// update, repair, or rollback can never remove it.
    pub fn saves_root(&self) -> &Path {
        &self.saves_root
    }

    pub fn states_root(&self) -> &Path {
        &self.states_root
    }

    pub fn screenshots_root(&self) -> &Path {
        &self.screenshots_root
    }

    pub fn log_root(&self) -> &Path {
        &self.log_root
    }

    /// Every directory RetroArch may write into during a launch.
    pub fn owned_directories(&self) -> Vec<PathBuf> {
        vec![
            self.runtime_user_root.clone(),
            self.config_root(),
            self.system_root(),
            self.core_info_root(),
            self.core_options_root(),
            self.assets_root(),
            self.core_assets_root(),
            self.shaders_root(),
            self.video_filters_root(),
            self.audio_filters_root(),
            self.playlists_root(),
            self.history_root(),
            self.remaps_root(),
            self.autoconfig_root(),
            self.cache_root(),
            self.thumbnails_root(),
            self.wallpapers_root(),
            self.overlays_root(),
            self.database_root(),
            self.recordings_output_root(),
            self.recordings_config_root(),
            self.menu_browser_root(),
            self.menu_config_root(),
            self.xdg_config_root(),
            self.xdg_data_root(),
            self.xdg_cache_root(),
            self.xdg_state_root(),
            self.saves_root.clone(),
            self.states_root.clone(),
            self.screenshots_root.clone(),
            self.log_root.clone(),
        ]
    }

    /// Create the owned tree with user-only permissions. A symlinked or non-directory owned path
    /// is refused rather than followed.
    pub fn prepare(&self) -> Result<(), AppError> {
        ensure_directory(&self.app_data_root)?;
        for path in self.owned_directories() {
            ensure_directory(&path)?;
        }
        Ok(())
    }
}

fn ensure_directory(path: &Path) -> Result<(), AppError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            Err(AppError::Storage(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "a RetroFrontier-owned launch directory is not a real directory",
            )))
        }
        Ok(_) => set_private_permissions(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if let Some(parent) = path.parent() {
                if !parent.exists() {
                    create_private_directory_all(parent)?;
                }
            }
            create_private_directory(path)
        }
        Err(error) => Err(AppError::Storage(error)),
    }
}

fn create_private_directory_all(path: &Path) -> Result<(), AppError> {
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        create_private_directory_all(parent)?;
    }
    create_private_directory(path)
}

fn create_private_directory(path: &Path) -> Result<(), AppError> {
    #[cfg(unix)]
    {
        fs::DirBuilder::new()
            .mode(PRIVATE_DIRECTORY_MODE)
            .create(path)
            .map_err(AppError::Storage)?;
    }
    #[cfg(not(unix))]
    {
        fs::create_dir(path).map_err(AppError::Storage)?;
    }
    set_private_permissions(path)
}

fn set_private_permissions(path: &Path) -> Result<(), AppError> {
    #[cfg(unix)]
    {
        fs::set_permissions(path, fs::Permissions::from_mode(PRIVATE_DIRECTORY_MODE))
            .map_err(AppError::Storage)?;
    }
    let _ = path;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::LaunchPaths;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn every_owned_directory_is_created_below_the_application_data_root() {
        let directory = tempdir().unwrap();
        let paths = LaunchPaths::new(directory.path().join("RetroFrontier"));

        paths.prepare().unwrap();

        for owned in paths.owned_directories() {
            assert!(owned.is_dir(), "{} should exist", owned.display());
            assert!(owned.starts_with(paths.app_data_root()));
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = fs::symlink_metadata(&owned).unwrap().permissions().mode();
                assert_eq!(mode & 0o777, 0o700, "{} should be private", owned.display());
            }
        }
        assert!(paths.config_file().starts_with(paths.config_root()));
        assert!(paths
            .core_options_file()
            .starts_with(paths.core_options_root()));
        assert!(paths
            .content_history_file()
            .starts_with(paths.history_root()));
    }

    #[test]
    fn user_data_never_lives_inside_a_replaceable_runtime_version_tree() {
        let paths = LaunchPaths::new("/synthetic/app-data");
        let versions_root = std::path::Path::new("/synthetic/app-data/runtime/versions");

        for owned in paths.owned_directories() {
            assert!(
                !owned.starts_with(versions_root),
                "{} must stay outside runtime versions",
                owned.display()
            );
        }
    }

    #[test]
    fn a_symlinked_owned_directory_is_refused_instead_of_followed() {
        let directory = tempdir().unwrap();
        let app_data = directory.path().join("RetroFrontier");
        let paths = LaunchPaths::new(&app_data);
        let elsewhere = directory.path().join("elsewhere");
        fs::create_dir_all(&elsewhere).unwrap();
        fs::create_dir_all(&app_data).unwrap();
        std::os::unix::fs::symlink(&elsewhere, app_data.join("saves")).unwrap();

        assert!(paths.prepare().is_err());
    }
}
