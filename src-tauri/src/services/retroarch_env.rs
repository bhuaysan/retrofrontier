use crate::services::retroarch_paths::LaunchPaths;
use std::collections::BTreeMap;

/// Host and session variables the managed RetroArch child genuinely needs.
///
/// The Linux qualification showed that an extracted AppDir is relocatable for its bundled
/// libraries but not self-contained for host desktop services: the compositor, X server, D-Bus,
/// audio service, GPU selection, and locale all come from the user's session. Anything not listed
/// here is absent from the child by construction, so `LD_PRELOAD`, `LD_LIBRARY_PATH`, a stray
/// `RETROARCH_*` variable, and a hostile `XDG_CONFIG_HOME` cannot influence the launch.
const PRESERVED_HOST_VARIABLES: &[&str] = &[
    // Display and session.
    "DISPLAY",
    "WAYLAND_DISPLAY",
    "XAUTHORITY",
    "XDG_CURRENT_DESKTOP",
    "XDG_RUNTIME_DIR",
    "XDG_SEAT",
    "XDG_SESSION_DESKTOP",
    "XDG_SESSION_TYPE",
    "XDG_VTNR",
    // Session IPC.
    "DBUS_SESSION_BUS_ADDRESS",
    // Audio services.
    "PIPEWIRE_RUNTIME_DIR",
    "PULSE_COOKIE",
    "PULSE_RUNTIME_PATH",
    "PULSE_SERVER",
    // Graphics adapter selection made by the user's session.
    "DRI_PRIME",
    "MESA_LOADER_DRIVER_OVERRIDE",
    "__GLX_VENDOR_LIBRARY_NAME",
    "__NV_PRIME_RENDER_OFFLOAD",
    "__VK_LAYER_NV_optimus",
    // Identity and locale.
    "HOME",
    "LANG",
    "LANGUAGE",
    "LC_ADDRESS",
    "LC_ALL",
    "LC_COLLATE",
    "LC_CTYPE",
    "LC_IDENTIFICATION",
    "LC_MEASUREMENT",
    "LC_MESSAGES",
    "LC_MONETARY",
    "LC_NAME",
    "LC_NUMERIC",
    "LC_PAPER",
    "LC_TELEPHONE",
    "LC_TIME",
    "LOGNAME",
    "TZ",
    "USER",
];

/// A fixed minimal `PATH`.
///
/// The managed executable is always launched by absolute path, so `PATH` exists only for helper
/// processes the runtime may run. Inheriting the user's `PATH` would be the one way a `retroarch`
/// from the host could enter the picture.
const CHILD_PATH: &str = "/usr/bin:/bin";

/// Read the current process environment as UTF-8 pairs.
///
/// A non-UTF-8 variable is skipped: none of the preserved variables is expected to be non-UTF-8,
/// and silently passing through bytes RetroFrontier cannot inspect would defeat the allowlist.
pub fn host_environment() -> BTreeMap<String, String> {
    std::env::vars_os()
        .filter_map(|(key, value)| Some((key.into_string().ok()?, value.into_string().ok()?)))
        .collect()
}

/// Construct the child environment from an allowlist plus RetroFrontier-owned values.
///
/// This is neither blind inheritance nor a blind `env_clear`: the desktop session facts RetroArch
/// needs are preserved, while everything that could redirect RetroArch's configuration, data, or
/// library loading is replaced with a RetroFrontier-owned value or removed.
pub fn build_child_environment(
    paths: &LaunchPaths,
    host: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut environment = BTreeMap::new();
    for name in PRESERVED_HOST_VARIABLES {
        if let Some(value) = host.get(*name) {
            if !value.is_empty() {
                environment.insert((*name).to_owned(), value.clone());
            }
        }
    }

    environment.insert("PATH".to_owned(), CHILD_PATH.to_owned());
    environment.insert(
        "XDG_CONFIG_HOME".to_owned(),
        paths.xdg_config_root().to_string_lossy().into_owned(),
    );
    environment.insert(
        "XDG_DATA_HOME".to_owned(),
        paths.xdg_data_root().to_string_lossy().into_owned(),
    );
    environment.insert(
        "XDG_CACHE_HOME".to_owned(),
        paths.xdg_cache_root().to_string_lossy().into_owned(),
    );
    environment.insert(
        "XDG_STATE_HOME".to_owned(),
        paths.xdg_state_root().to_string_lossy().into_owned(),
    );
    environment
}

#[cfg(test)]
mod tests {
    use super::{build_child_environment, host_environment};
    use crate::services::retroarch_paths::LaunchPaths;
    use std::collections::BTreeMap;

    fn host() -> BTreeMap<String, String> {
        [
            ("DISPLAY", ":0"),
            ("WAYLAND_DISPLAY", "wayland-0"),
            ("XDG_SESSION_TYPE", "wayland"),
            ("XDG_RUNTIME_DIR", "/run/user/1000"),
            ("XAUTHORITY", "/run/user/1000/xauth"),
            ("DBUS_SESSION_BUS_ADDRESS", "unix:path=/run/user/1000/bus"),
            ("PULSE_SERVER", "unix:/run/user/1000/pulse/native"),
            ("DRI_PRIME", "1"),
            ("HOME", "/home/player"),
            ("LANG", "en_US.UTF-8"),
            ("LC_TIME", "de_DE.UTF-8"),
            ("USER", "player"),
            // Hostile or irrelevant state that must not reach the child.
            ("XDG_CONFIG_HOME", "/tmp/attacker/config"),
            ("XDG_DATA_HOME", "/tmp/attacker/data"),
            ("LD_PRELOAD", "/tmp/attacker/evil.so"),
            ("LD_LIBRARY_PATH", "/tmp/attacker/lib"),
            ("LD_AUDIT", "/tmp/attacker/audit.so"),
            (
                "RETROARCH_CFG",
                "/home/player/.config/retroarch/retroarch.cfg",
            ),
            ("LIBRETRO_DIRECTORY", "/home/player/.config/retroarch/cores"),
            ("SDL_VIDEODRIVER", "dummy"),
            ("PATH", "/tmp/attacker/bin:/usr/bin"),
        ]
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value.to_owned()))
        .collect()
    }

    #[test]
    fn hostile_or_unrelated_state_never_reaches_the_child() {
        let paths = LaunchPaths::new("/synthetic/app-data");

        let environment = build_child_environment(&paths, &host());

        for removed in [
            "LD_PRELOAD",
            "LD_LIBRARY_PATH",
            "LD_AUDIT",
            "RETROARCH_CFG",
            "LIBRETRO_DIRECTORY",
            "SDL_VIDEODRIVER",
        ] {
            assert!(
                !environment.contains_key(removed),
                "{removed} must not reach the child"
            );
        }
        assert_eq!(
            environment.get("PATH").map(String::as_str),
            Some("/usr/bin:/bin")
        );
        assert!(!environment
            .get("PATH")
            .is_some_and(|path| path.contains("attacker")));
    }

    #[test]
    fn the_child_xdg_base_directories_are_retrofrontier_owned() {
        let paths = LaunchPaths::new("/synthetic/app-data");

        let environment = build_child_environment(&paths, &host());

        assert_eq!(
            environment.get("XDG_CONFIG_HOME").map(String::as_str),
            Some("/synthetic/app-data/runtime-user/xdg/config")
        );
        assert_eq!(
            environment.get("XDG_DATA_HOME").map(String::as_str),
            Some("/synthetic/app-data/runtime-user/xdg/data")
        );
        assert_eq!(
            environment.get("XDG_CACHE_HOME").map(String::as_str),
            Some("/synthetic/app-data/runtime-user/xdg/cache")
        );
        assert_eq!(
            environment.get("XDG_STATE_HOME").map(String::as_str),
            Some("/synthetic/app-data/runtime-user/xdg/state")
        );
    }

    #[test]
    fn the_required_desktop_session_variables_are_preserved() {
        let paths = LaunchPaths::new("/synthetic/app-data");

        let environment = build_child_environment(&paths, &host());

        for (name, value) in [
            ("DISPLAY", ":0"),
            ("WAYLAND_DISPLAY", "wayland-0"),
            ("XDG_SESSION_TYPE", "wayland"),
            ("XDG_RUNTIME_DIR", "/run/user/1000"),
            ("XAUTHORITY", "/run/user/1000/xauth"),
            ("DBUS_SESSION_BUS_ADDRESS", "unix:path=/run/user/1000/bus"),
            ("PULSE_SERVER", "unix:/run/user/1000/pulse/native"),
            ("DRI_PRIME", "1"),
            ("HOME", "/home/player"),
            ("LANG", "en_US.UTF-8"),
            ("LC_TIME", "de_DE.UTF-8"),
        ] {
            assert_eq!(
                environment.get(name).map(String::as_str),
                Some(value),
                "{name} must be preserved"
            );
        }
    }

    #[test]
    fn an_empty_or_absent_host_variable_is_not_forwarded() {
        let paths = LaunchPaths::new("/synthetic/app-data");
        let mut host = host();
        host.insert("WAYLAND_DISPLAY".to_owned(), String::new());
        host.remove("DISPLAY");

        let environment = build_child_environment(&paths, &host);

        assert!(!environment.contains_key("WAYLAND_DISPLAY"));
        assert!(!environment.contains_key("DISPLAY"));
    }

    #[test]
    fn the_host_environment_reader_returns_utf8_pairs() {
        let environment = host_environment();

        assert!(environment
            .keys()
            .all(|key| !key.is_empty() && !key.contains('=')));
    }
}
