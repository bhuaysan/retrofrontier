use crate::domain::save_state::SaveStateSlot;
use crate::services::retroarch_input::{
    SaveStateHotkeys, ENABLE_HOTKEY_KEY, SAVE_STATE_KEY, SLOT_DECREASE_KEY, SLOT_INCREASE_KEY,
};
use crate::services::retroarch_paths::LaunchPaths;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

/// Everything one launch decides about the generated configuration.
///
/// It is a struct rather than a positional argument list because M9 added two values that are
/// genuinely per-launch — which slot starts active, and which managed save-state hotkeys resolved —
/// and a five-argument `build` would make their order the reader's problem.
#[derive(Debug, Clone, Copy)]
pub struct RetroArchConfigRequest<'a> {
    pub paths: &'a LaunchPaths,
    /// The managed cores directory inside the verified immutable version tree. RetroArch only
    /// reads it; every writable path points into RetroFrontier's own data.
    pub core_directory: &'a Path,
    /// The verified managed joypad-autoconfig tree, likewise inside the immutable version tree and
    /// likewise read-only. Both are passed in rather than derived here, because only the runtime
    /// layer can say which installation is currently verified.
    pub controller_profiles_root: &'a Path,
    /// Which RetroArch state slot is active when the game starts.
    ///
    /// A normal launch starts on the first managed slot. A save-state launch starts on the loaded
    /// state's own slot, so saving again lands where the player expects. The previously active slot
    /// is deliberately never persisted as a RetroFrontier preference.
    pub state_slot: SaveStateSlot,
    /// The managed save-state hotkeys, when the authenticated controller profiles resolved them.
    ///
    /// `None` writes no hotkey at all rather than a guessed button index, and never fails a launch.
    pub save_state_hotkeys: Option<&'a SaveStateHotkeys>,
}

/// The RetroFrontier-owned RetroArch configuration.
///
/// There is exactly one generated file. It contains only RetroFrontier-controlled values, is
/// deterministic for a given application-data root, installation, and launch, and is rewritten
/// before every launch. Because the core comes from `-L` and the content from `argv`, nothing
/// per-game has to be written, so RetroFrontier creates no per-game configuration files at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetroArchConfig {
    entries: Vec<(String, String)>,
}

impl RetroArchConfig {
    /// Build the configuration for one launch.
    pub fn build(request: &RetroArchConfigRequest<'_>) -> Self {
        let RetroArchConfigRequest {
            paths,
            core_directory,
            controller_profiles_root,
            state_slot,
            save_state_hotkeys,
        } = *request;
        let mut entries: Vec<(String, String)> = Vec::new();
        let mut set = |key: &str, value: String| entries.push((key.to_owned(), value));

        // Where RetroArch may read code and metadata from.
        set("libretro_directory", path_value(core_directory));
        // Controller profiles are read-only managed support data, so RetroArch is pointed straight
        // at the verified immutable tree rather than at a directory RetroFrontier composes.
        //
        // Real hardware qualification found this to be the whole DualSense defect: the directory
        // named here used to be a private, empty `runtime-user/autoconfig`, so the managed
        // RetroArch detected the pad on `udev` and then logged
        // `[Autoconf] Sony Interactive Entertainment DualSense Wireless Controller (1356/3302)
        // not configured` — an unconfigured pad has no RetroPad binds at all, which is exactly the
        // "controller does nothing in the game" the operator saw. RetroArch resolves profiles at
        // `joypad_autoconfig_dir/<joypad driver>/`, so this value is the *parent* of the managed
        // driver directory. No host RetroArch location is ever consulted.
        set(
            "joypad_autoconfig_dir",
            path_value(controller_profiles_root),
        );
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

        // M9 save states.
        //
        // `savestate_thumbnail_enable` is what makes RetroArch write `<state path>.png` beside a
        // state it saves. RetroFrontier associates one only when the controlled launch delta
        // *proves* the relationship, so enabling it produces a candidate, never an association.
        set("savestate_thumbnail_enable", boolean(true));
        // Which slot is active when the game starts. A normal launch starts on slot 1; a
        // save-state launch starts on the loaded state's own slot.
        //
        // `--entryslot` is the documented mechanism and remains the launch contract, but this
        // generated file is RetroFrontier's single control path over RetroArch's behaviour, so the
        // active slot is stated here as well rather than left to be inferred. The file is rewritten
        // before every launch, so saying it costs nothing and makes the invariant deterministic.
        set("state_slot", state_slot.get().to_string());
        set("auto_overrides_enable", boolean(false));
        set("auto_remaps_enable", boolean(false));
        set("auto_shaders_enable", boolean(false));
        set("game_specific_options", boolean(false));
        set("global_core_options", boolean(true));
        set("cheevos_enable", boolean(false));
        set("log_to_file", boolean(true));

        // Launch presentation: a game started from RetroFrontier fills the display at once.
        //
        // RetroArch 1.22.2's compiled-in `DEFAULT_FULLSCREEN` is `false` for a generic Linux build
        // (`config.def.h`: only Steam, Dingux, WinRT, and Winapi-Family builds default to true), so
        // a generated configuration that said nothing about fullscreen inherited that default and
        // opened the small default window real hardware qualification saw. The generated file is the
        // canonical control path — `video_fullscreen` is the setting RetroArch itself reads for this
        // (`configuration.c`: `SETTING_BOOL("video_fullscreen", ...)`) — so no launch flag is added
        // and there is exactly one place that decides this.
        set("video_fullscreen", boolean(true));
        // *How* fullscreen is entered, owned rather than inherited. Borderless fullscreen at the
        // current desktop resolution needs no video-mode change, which a Wayland client cannot
        // request at all, and it never shows an intermediate window. `video_fullscreen_x/y` apply
        // only to the exclusive path and are therefore deliberately not written.
        set("video_windowed_fullscreen", boolean(true));

        // The managed save-state hotkeys, derived from the authenticated controller profiles.
        //
        // Select + R1 saves; Select + D-Pad Right/Left changes slot. There is deliberately **no**
        // `input_load_state_btn`: controlled loading happens through Game Detail, where the exact
        // historical core binary, content unit, and file identity can be re-proved first, and a
        // hotkey could prove none of that.
        //
        // When the profiles did not resolve, nothing is written. A guessed button index would bind
        // "Save State" to whatever that number happens to be on the player's pad, and a launch must
        // not fail merely because a save hotkey could not be derived.
        if let Some(hotkeys) = save_state_hotkeys {
            set(ENABLE_HOTKEY_KEY, escape(&hotkeys.enable_hotkey));
            set(SAVE_STATE_KEY, escape(&hotkeys.save_state));
            set(SLOT_INCREASE_KEY, escape(&hotkeys.slot_increase));
            set(SLOT_DECREASE_KEY, escape(&hotkeys.slot_decrease));
        }

        entries.sort_by(|left, right| left.0.cmp(&right.0));
        Self { entries }
    }

    #[cfg(test)]
    pub fn entries(&self) -> &[(String, String)] {
        &self.entries
    }

    #[cfg(test)]
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
    use super::{RetroArchConfig, RetroArchConfigRequest};
    use crate::domain::save_state::SaveStateSlot;
    use crate::services::retroarch_input::SaveStateHotkeys;
    use crate::services::retroarch_paths::LaunchPaths;
    use std::path::Path;
    use tempfile::tempdir;

    /// The managed cores directory and the managed controller-profile tree: verified, immutable,
    /// and read-only. Everything else RetroFrontier controls is writable and lives outside every
    /// runtime version tree.
    const READ_ONLY_MANAGED_KEYS: &[&str] = &["libretro_directory", "joypad_autoconfig_dir"];

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

    const SYNTHETIC_PROFILES: &str =
        "/synthetic/app-data/runtime/versions/install-1/runtime/support/joypad-autoconfig";

    /// The four values the real authenticated DualSense profile declares for the M9 roles.
    fn synthetic_hotkeys() -> SaveStateHotkeys {
        SaveStateHotkeys {
            enable_hotkey: "8".to_owned(),
            save_state: "5".to_owned(),
            slot_increase: "h0right".to_owned(),
            slot_decrease: "h0left".to_owned(),
        }
    }

    fn config_for(
        paths: &LaunchPaths,
        core_directory: &Path,
        controller_profiles_root: &Path,
        state_slot: u16,
        save_state_hotkeys: Option<&SaveStateHotkeys>,
    ) -> RetroArchConfig {
        RetroArchConfig::build(&RetroArchConfigRequest {
            paths,
            core_directory,
            controller_profiles_root,
            state_slot: SaveStateSlot::new(state_slot).unwrap(),
            save_state_hotkeys,
        })
    }

    fn synthetic_config() -> (LaunchPaths, RetroArchConfig) {
        let paths = LaunchPaths::new("/synthetic/app-data");
        let core_directory =
            Path::new("/synthetic/app-data/runtime/versions/install-1/cores/nestopia");
        let hotkeys = synthetic_hotkeys();
        let config = config_for(
            &paths,
            core_directory,
            Path::new(SYNTHETIC_PROFILES),
            1,
            Some(&hotkeys),
        );
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
            if READ_ONLY_MANAGED_KEYS.contains(key) {
                continue;
            }
            let value = config.value(key).unwrap();
            assert!(
                !Path::new(value).starts_with(&versions_root),
                "{key} must not write into an immutable runtime version"
            );
        }
        // B3: the profile tree is the verified immutable one, and it is the only thing RetroArch is
        // told about controller profiles.
        assert_eq!(
            config.value("joypad_autoconfig_dir").map(Path::new),
            Some(Path::new(SYNTHETIC_PROFILES))
        );
        assert!(
            Path::new(config.value("joypad_autoconfig_dir").unwrap()).starts_with(&versions_root)
        );
        assert!(config.value("input_autodetect_enable").is_none());
        assert!(config.value("input_joypad_driver").is_none());
    }

    /// B3: `joypad_autoconfig_dir` follows the installation it was built for and nothing else.
    #[test]
    fn the_controller_profile_directory_is_the_verified_tree_it_was_given() {
        let config = config_for(
            &LaunchPaths::new("/other/root"),
            Path::new("/other/root/runtime/versions/install-9/cores/dolphin"),
            Path::new("/other/root/runtime/versions/install-9/runtime/support/joypad-autoconfig"),
            1,
            None,
        );

        assert_eq!(
            config.value("joypad_autoconfig_dir"),
            Some("/other/root/runtime/versions/install-9/runtime/support/joypad-autoconfig")
        );
        // Never the old private writable directory, which is exactly what shipped nothing.
        assert!(!config
            .value("joypad_autoconfig_dir")
            .unwrap()
            .contains("runtime-user/autoconfig"));
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
                // B4: the host autoconfig locations RetroArch would otherwise be pointed at.
                "/usr/share/libretro/autoconfig",
                "/usr/local/share/libretro/autoconfig",
                "/.config/retroarch/autoconfig",
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

    /// B6: the controller fix changes the profile directory and nothing else about the contract.
    #[test]
    fn the_existing_configuration_contract_is_unchanged_by_the_controller_profile_directory() {
        let (paths, config) = synthetic_config();

        assert_eq!(config.value("video_fullscreen"), Some("true"));
        assert_eq!(config.value("video_windowed_fullscreen"), Some("true"));
        assert_eq!(config.value("config_save_on_exit"), Some("false"));
        assert_eq!(
            config.value("system_directory").map(Path::new),
            Some(paths.system_root().as_path())
        );
        assert_eq!(
            config.value("savefile_directory").map(Path::new),
            Some(paths.saves_root())
        );
        assert_eq!(
            config.value("log_dir").map(Path::new),
            Some(paths.log_root())
        );
    }

    /// B1: fullscreen presentation is a RetroFrontier-owned decision, not an inherited default.
    ///
    /// RetroArch 1.22.2's compiled-in `DEFAULT_FULLSCREEN` is `false` on a generic Linux build, so a
    /// generated configuration that stays silent about it gets RetroArch's small default window.
    #[test]
    fn the_generated_configuration_requests_fullscreen_explicitly() {
        let (_paths, config) = synthetic_config();

        assert_eq!(config.value("video_fullscreen"), Some("true"));
        // Borderless fullscreen at the desktop resolution rather than an exclusive mode switch:
        // a Wayland client cannot set a video mode, and there is no tiny intermediate window.
        assert_eq!(config.value("video_windowed_fullscreen"), Some("true"));

        let rendered = config.render();
        assert!(rendered.contains("video_fullscreen = \"true\"\n"));
        assert!(rendered.contains("video_windowed_fullscreen = \"true\"\n"));
    }

    /// B2: repeated generation produces byte-identical fullscreen entries.
    #[test]
    fn repeated_generation_renders_identical_fullscreen_entries() {
        let (_paths, first) = synthetic_config();
        let (_paths, second) = synthetic_config();

        for key in ["video_fullscreen", "video_windowed_fullscreen"] {
            assert_eq!(first.value(key), second.value(key), "{key}");
        }
        let fullscreen_lines = |config: &RetroArchConfig| -> Vec<String> {
            config
                .render()
                .lines()
                .filter(|line| {
                    line.starts_with("video_fullscreen")
                        || line.starts_with("video_windowed_fullscreen")
                })
                .map(str::to_owned)
                .collect()
        };
        assert_eq!(fullscreen_lines(&first), fullscreen_lines(&second));
        assert_eq!(
            fullscreen_lines(&first),
            vec![
                "video_fullscreen = \"true\"".to_owned(),
                "video_windowed_fullscreen = \"true\"".to_owned(),
            ]
        );
    }

    /// B3: the fullscreen request depends on nothing outside RetroFrontier's own generated file.
    #[test]
    fn the_fullscreen_request_is_independent_of_any_host_or_user_retroarch_state() {
        let first = config_for(
            &LaunchPaths::new("/synthetic/app-data"),
            Path::new("/synthetic/app-data/runtime/versions/install-1/cores/nestopia"),
            Path::new(SYNTHETIC_PROFILES),
            1,
            None,
        );
        // A completely different application-data root and installation: only paths may differ.
        let second = config_for(
            &LaunchPaths::new("/other/root"),
            Path::new("/other/root/runtime/versions/install-9/cores/beetle-psx"),
            Path::new("/other/root/runtime/versions/install-9/runtime/support/joypad-autoconfig"),
            1,
            None,
        );

        for key in ["video_fullscreen", "video_windowed_fullscreen"] {
            assert_eq!(first.value(key), Some("true"), "{key}");
            assert_eq!(second.value(key), Some("true"), "{key}");
        }
        // Nothing about fullscreen is read from, or written into, a host RetroArch location, and
        // RetroArch may not persist a different answer over it.
        assert_eq!(first.value("config_save_on_exit"), Some("false"));
        for (key, value) in first.entries() {
            if !key.starts_with("video_fullscreen") && !key.starts_with("video_windowed_fullscreen")
            {
                continue;
            }
            assert!(!value.contains('/'), "{key} must not reference any path");
        }
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
        let config = config_for(
            &paths,
            Path::new("/synthetic/cores"),
            Path::new("/synthetic/profiles"),
            1,
            None,
        );

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
        let config = config_for(
            &paths,
            Path::new("/synthetic/cores"),
            Path::new("/synthetic/profiles"),
            1,
            None,
        );

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

    /// M9: the controlled save-state configuration.
    #[test]
    fn the_generated_configuration_owns_every_save_state_behaviour_retrofrontier_decides() {
        let (paths, config) = synthetic_config();

        // States are written where RetroFrontier owns them, never beside user ROMs.
        assert_eq!(
            config.value("savestate_directory").map(Path::new),
            Some(paths.states_root())
        );
        assert_eq!(config.value("savestates_in_content_dir"), Some("false"));
        // RetroArch never saves or loads a state behind the player's back: M9's whole provenance
        // model depends on every managed state coming from a deliberate save.
        assert_eq!(config.value("savestate_auto_save"), Some("false"));
        assert_eq!(config.value("savestate_auto_load"), Some("false"));
        // And it writes `<state path>.png` beside a state it saves, which is what makes a
        // *provable* thumbnail candidate exist at all.
        assert_eq!(config.value("savestate_thumbnail_enable"), Some("true"));
    }

    #[test]
    fn a_normal_launch_starts_on_the_first_managed_slot_and_a_save_state_launch_on_its_own() {
        let (_paths, normal) = synthetic_config();
        assert_eq!(normal.value("state_slot"), Some("1"));

        let hotkeys = synthetic_hotkeys();
        for slot in [1_u16, 2, 42, 999] {
            let config = config_for(
                &LaunchPaths::new("/synthetic/app-data"),
                Path::new("/synthetic/cores"),
                Path::new(SYNTHETIC_PROFILES),
                slot,
                Some(&hotkeys),
            );
            assert_eq!(config.value("state_slot"), Some(slot.to_string().as_str()));
        }
    }

    #[test]
    fn the_managed_save_state_hotkeys_are_exactly_the_derived_profile_values() {
        let (_paths, config) = synthetic_config();

        // Select + R1 saves; Select + D-Pad Right/Left changes slot. The values are the
        // authenticated profile's own, hat notation included.
        assert_eq!(config.value("input_enable_hotkey_btn"), Some("8"));
        assert_eq!(config.value("input_save_state_btn"), Some("5"));
        assert_eq!(
            config.value("input_state_slot_increase_btn"),
            Some("h0right")
        );
        assert_eq!(
            config.value("input_state_slot_decrease_btn"),
            Some("h0left")
        );

        // A different derived set produces different values: nothing here is a constant.
        let other = SaveStateHotkeys {
            enable_hotkey: "6".to_owned(),
            save_state: "7".to_owned(),
            slot_increase: "h1right".to_owned(),
            slot_decrease: "h1left".to_owned(),
        };
        let config = config_for(
            &LaunchPaths::new("/synthetic/app-data"),
            Path::new("/synthetic/cores"),
            Path::new(SYNTHETIC_PROFILES),
            1,
            Some(&other),
        );
        assert_eq!(config.value("input_enable_hotkey_btn"), Some("6"));
        assert_eq!(config.value("input_save_state_btn"), Some("7"));
        assert_eq!(
            config.value("input_state_slot_increase_btn"),
            Some("h1right")
        );
        assert_eq!(
            config.value("input_state_slot_decrease_btn"),
            Some("h1left")
        );
    }

    /// There is deliberately no RetroFrontier-provided ingame Load State hotkey.
    ///
    /// Controlled loading happens through Game Detail, where the exact historical core binary, the
    /// exact content unit, and the exact file identity are all re-proved first. A hotkey could
    /// prove none of that, so no input under any configuration may bind one.
    #[test]
    fn no_ingame_load_state_hotkey_is_ever_written() {
        let hotkeys = synthetic_hotkeys();
        for save_state_hotkeys in [Some(&hotkeys), None] {
            let config = config_for(
                &LaunchPaths::new("/synthetic/app-data"),
                Path::new("/synthetic/cores"),
                Path::new(SYNTHETIC_PROFILES),
                1,
                save_state_hotkeys,
            );
            for (key, _) in config.entries() {
                assert!(
                    !key.contains("load_state"),
                    "no configuration key may bind an ingame load ({key})"
                );
            }
            assert!(config.value("input_load_state_btn").is_none());
            assert!(!config.render().contains("load_state"));
        }
    }

    /// When the authenticated profiles do not resolve, nothing is guessed.
    #[test]
    fn an_unresolved_hotkey_set_writes_no_hotkey_rather_than_a_guessed_button_index() {
        let config = config_for(
            &LaunchPaths::new("/synthetic/app-data"),
            Path::new("/synthetic/cores"),
            Path::new(SYNTHETIC_PROFILES),
            1,
            None,
        );

        for key in [
            "input_enable_hotkey_btn",
            "input_save_state_btn",
            "input_state_slot_increase_btn",
            "input_state_slot_decrease_btn",
        ] {
            assert!(config.value(key).is_none(), "{key} must be absent entirely");
        }
        // The rest of the configuration is unaffected, so a launch still proceeds normally.
        assert_eq!(config.value("savestate_thumbnail_enable"), Some("true"));
        assert_eq!(config.value("state_slot"), Some("1"));
        assert_eq!(config.value("video_fullscreen"), Some("true"));
    }

    #[test]
    fn the_save_state_configuration_is_deterministic_and_still_sorted() {
        let (_paths, first) = synthetic_config();
        let (_paths, second) = synthetic_config();

        assert_eq!(first.render(), second.render());
        let keys: Vec<_> = first.entries().iter().map(|(key, _)| key).collect();
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted);
        assert!(first.render().contains("state_slot = \"1\"\n"));
        assert!(first
            .render()
            .contains("input_state_slot_increase_btn = \"h0right\"\n"));
    }
}
