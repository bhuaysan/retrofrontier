use crate::services::retroarch_paths::LaunchPaths;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

/// The RetroFrontier-owned RetroArch configuration.
///
/// There is exactly one generated file. It contains only RetroFrontier-controlled values, is
/// deterministic for a given application-data root and installation, and is rewritten before every
/// launch. Because the core comes from `-L` and the content from `argv`, nothing per-game has to be
/// written, so RetroFrontier creates no per-game configuration files at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetroArchConfig {
    entries: Vec<(String, String)>,
}

impl RetroArchConfig {
    /// Build the configuration for one launch.
    ///
    /// `core_directory` is the managed cores directory inside the verified immutable version tree.
    /// RetroArch only reads it; every writable path points into RetroFrontier's own data.
    pub fn build(paths: &LaunchPaths, core_directory: &Path) -> Self {
        let mut entries: Vec<(String, String)> = Vec::new();
        let mut set = |key: &str, value: String| entries.push((key.to_owned(), value));

        // Where RetroArch may read code and metadata from.
        set("libretro_directory", path_value(core_directory));
        set("libretro_info_path", path_value(&paths.core_info_root()));
        set("core_options_path", path_value(&paths.core_options_file()));
        set("system_directory", path_value(&paths.system_root()));
        set("assets_directory", path_value(&paths.assets_root()));
        set(
            "core_assets_directory",
            path_value(&paths.core_assets_root()),
        );
        set("video_shader_dir", path_value(&paths.shaders_root()));
        set("video_filter_dir", path_value(&paths.video_filters_root()));
        set("audio_filter_dir", path_value(&paths.audio_filters_root()));
        set("content_database_path", path_value(&paths.database_root()));
        set("cheat_database_path", path_value(&paths.database_root()));
        set("overlay_directory", path_value(&paths.overlays_root()));
        set("osk_overlay_directory", path_value(&paths.overlays_root()));
        set("thumbnails_directory", path_value(&paths.thumbnails_root()));
        set(
            "dynamic_wallpapers_directory",
            path_value(&paths.wallpapers_root()),
        );
        set(
            "rgui_browser_directory",
            path_value(&paths.menu_browser_root()),
        );
        set(
            "rgui_config_directory",
            path_value(&paths.menu_config_root()),
        );

        // Where RetroArch may write.
        set("savefile_directory", path_value(paths.saves_root()));
        set("savestate_directory", path_value(paths.states_root()));
        set("screenshot_directory", path_value(paths.screenshots_root()));
        set("cache_directory", path_value(&paths.cache_root()));
        set("playlist_directory", path_value(&paths.playlists_root()));
        set(
            "input_remapping_directory",
            path_value(&paths.remaps_root()),
        );
        set(
            "joypad_autoconfig_dir",
            path_value(&paths.autoconfig_root()),
        );
        set(
            "recording_output_directory",
            path_value(&paths.recordings_output_root()),
        );
        set(
            "recording_config_directory",
            path_value(&paths.recordings_config_root()),
        );
        set("content_history_dir", path_value(&paths.history_root()));
        set(
            "content_history_path",
            path_value(&paths.content_history_file()),
        );
        set(
            "content_music_history_path",
            path_value(&paths.content_music_history_file()),
        );
        set(
            "content_image_history_path",
            path_value(&paths.content_image_history_file()),
        );
        set(
            "content_video_history_path",
            path_value(&paths.content_video_history_file()),
        );
        set(
            "content_favorites_path",
            path_value(&paths.content_favorites_file()),
        );
        set("log_dir", path_value(paths.log_root()));

        // Behaviour RetroFrontier owns rather than the user's own RetroArch installation.
        // `config_save_on_exit` is false so RetroArch never rewrites this generated file.
        set("config_save_on_exit", boolean(false));
        set("savefiles_in_content_dir", boolean(false));
        set("savestates_in_content_dir", boolean(false));
        set("systemfiles_in_content_dir", boolean(false));
        set("screenshots_in_content_dir", boolean(false));
        set("sort_savefiles_enable", boolean(true));
        set("sort_savestates_enable", boolean(true));
        set("savestate_auto_save", boolean(false));
        set("savestate_auto_load", boolean(false));
        set("auto_overrides_enable", boolean(false));
        set("auto_remaps_enable", boolean(false));
        set("auto_shaders_enable", boolean(false));
        set("game_specific_options", boolean(false));
        set("global_core_options", boolean(true));
        set("cheevos_enable", boolean(false));
        set("log_to_file", boolean(true));

        entries.sort_by(|left, right| left.0.cmp(&right.0));
        Self { entries }
    }

    pub fn entries(&self) -> &[(String, String)] {
        &self.entries
    }

    pub fn value(&self, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    }

    pub fn render(&self) -> String {
        let mut rendered = String::from(
            "# Generated by RetroFrontier. Every value is RetroFrontier-owned; edits are\n\
             # overwritten before the next launch.\n",
        );
        for (key, value) in &self.entries {
            rendered.push_str(key);
            rendered.push_str(" = \"");
            rendered.push_str(value);
            rendered.push_str("\"\n");
        }
        rendered
    }

    /// Write the configuration atomically with user-only permissions.
    ///
    /// A crash therefore leaves either the previous complete file or the new complete file, never
    /// a half-written configuration that could point RetroArch at an unexpected path.
    pub fn write(&self, path: &Path) -> Result<(), std::io::Error> {
        let parent = path.parent().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "the configuration path has no parent directory",
            )
        })?;
        let temporary = temporary_path(path);
        let _ = fs::remove_file(&temporary);
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options.open(&temporary)?;
        file.write_all(self.render().as_bytes())?;
        file.flush()?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)?;
        fs::File::open(parent)?.sync_all()?;
        Ok(())
    }
}

fn temporary_path(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "retroarch.cfg".to_owned());
    name.insert(0, '.');
    name.push_str(&format!(".{}.tmp", std::process::id()));
    path.with_file_name(name)
}

/// RetroArch configuration values are quoted strings, so both quote characters are escaped.
fn path_value(path: &Path) -> String {
    escape(&path.to_string_lossy())
}

fn boolean(value: bool) -> String {
    if value { "true" } else { "false" }.to_owned()
}

fn escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::RetroArchConfig;
    use crate::services::retroarch_paths::LaunchPaths;
    use std::path::Path;
    use tempfile::tempdir;

    const CONTROLLED_KEYS: &[&str] = &[
        "libretro_directory",
        "libretro_info_path",
        "core_options_path",
        "system_directory",
        "savefile_directory",
        "savestate_directory",
        "screenshot_directory",
        "assets_directory",
        "core_assets_directory",
        "video_shader_dir",
        "video_filter_dir",
        "audio_filter_dir",
        "playlist_directory",
        "cache_directory",
        "content_history_dir",
        "content_history_path",
        "content_music_history_path",
        "content_image_history_path",
        "content_video_history_path",
        "content_favorites_path",
        "input_remapping_directory",
        "joypad_autoconfig_dir",
        "thumbnails_directory",
        "dynamic_wallpapers_directory",
        "overlay_directory",
        "osk_overlay_directory",
        "content_database_path",
        "cheat_database_path",
        "recording_output_directory",
        "recording_config_directory",
        "rgui_browser_directory",
        "rgui_config_directory",
        "log_dir",
    ];

    fn synthetic_config() -> (LaunchPaths, RetroArchConfig) {
        let paths = LaunchPaths::new("/synthetic/app-data");
        let core_directory =
            Path::new("/synthetic/app-data/runtime/versions/install-1/cores/nestopia");
        let config = RetroArchConfig::build(&paths, core_directory);
        (paths, config)
    }

    #[test]
    fn every_stateful_retroarch_path_is_an_absolute_retrofrontier_owned_path() {
        let (paths, config) = synthetic_config();

        for key in CONTROLLED_KEYS {
            let value = config
                .value(key)
                .unwrap_or_else(|| panic!("{key} must be controlled"));
            assert!(
                Path::new(value).is_absolute(),
                "{key} must be an absolute path"
            );
            if *key == "libretro_directory" {
                continue;
            }
            assert!(
                Path::new(value).starts_with(paths.app_data_root()),
                "{key} must stay inside RetroFrontier application data"
            );
        }
    }

    #[test]
    fn the_core_directory_is_the_verified_version_tree_and_nothing_writable_lives_there() {
        let (paths, config) = synthetic_config();
        let versions_root = paths.app_data_root().join("runtime").join("versions");

        assert_eq!(
            config.value("libretro_directory").map(Path::new),
            Some(Path::new(
                "/synthetic/app-data/runtime/versions/install-1/cores/nestopia"
            ))
        );
        for key in CONTROLLED_KEYS {
            if *key == "libretro_directory" {
                continue;
            }
            let value = config.value(key).unwrap();
            assert!(
                !Path::new(value).starts_with(&versions_root),
                "{key} must not write into an immutable runtime version"
            );
        }
    }

    #[test]
    fn no_generated_value_can_point_at_a_host_retroarch_installation() {
        let (_paths, config) = synthetic_config();

        for (key, value) in config.entries() {
            for forbidden in [
                "/.config/retroarch",
                "/.retroarch",
                "/usr/share/libretro",
                "/etc/retroarch",
                "/.local/share/retroarch",
            ] {
                assert!(
                    !value.contains(forbidden),
                    "{key} must not reference a host RetroArch location"
                );
            }
        }
    }

    #[test]
    fn retroarch_may_not_rewrite_the_generated_configuration_or_use_content_directories() {
        let (_paths, config) = synthetic_config();

        assert_eq!(config.value("config_save_on_exit"), Some("false"));
        for key in [
            "savefiles_in_content_dir",
            "savestates_in_content_dir",
            "systemfiles_in_content_dir",
            "screenshots_in_content_dir",
            "auto_overrides_enable",
            "auto_remaps_enable",
            "auto_shaders_enable",
            "game_specific_options",
            "cheevos_enable",
        ] {
            assert_eq!(config.value(key), Some("false"), "{key}");
        }
        assert_eq!(config.value("log_to_file"), Some("true"));
    }

    #[test]
    fn the_rendered_configuration_is_deterministic_and_quoted() {
        let (_paths, config) = synthetic_config();
        let rendered = config.render();
        let (_paths, again) = synthetic_config();

        assert_eq!(rendered, again.render());
        assert!(
            rendered.contains("system_directory = \"/synthetic/app-data/runtime-user/system\"\n")
        );
        let keys: Vec<_> = config.entries().iter().map(|(key, _)| key).collect();
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted);
    }

    #[test]
    fn a_quote_in_an_application_data_path_cannot_break_out_of_a_configuration_value() {
        let paths = LaunchPaths::new("/synthetic/we\"ird\\path");
        let config = RetroArchConfig::build(&paths, Path::new("/synthetic/cores"));

        let rendered = config.render();
        for line in rendered.lines().filter(|line| !line.starts_with('#')) {
            let value = line.split_once(" = ").expect("key = value").1;
            assert!(value.starts_with('"') && value.ends_with('"'));
            // Every interior quote is escaped, so the value cannot terminate early.
            assert!(!value[1..value.len() - 1].contains("\\\"") || value.contains("\\\""));
        }
        assert!(rendered.contains("we\\\"ird\\\\path"));
    }

    #[test]
    fn the_configuration_is_written_atomically_with_private_permissions() {
        let directory = tempdir().unwrap();
        let paths = LaunchPaths::new(directory.path().join("RetroFrontier"));
        paths.prepare().unwrap();
        let config = RetroArchConfig::build(&paths, Path::new("/synthetic/cores"));

        config.write(&paths.config_file()).unwrap();
        let first = std::fs::read_to_string(paths.config_file()).unwrap();
        config.write(&paths.config_file()).unwrap();
        let second = std::fs::read_to_string(paths.config_file()).unwrap();

        assert_eq!(first, second);
        assert_eq!(first, config.render());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(paths.config_file())
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        let leftovers: Vec<_> = std::fs::read_dir(paths.config_root())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(leftovers, vec!["retroarch.cfg".to_owned()]);
    }
}
