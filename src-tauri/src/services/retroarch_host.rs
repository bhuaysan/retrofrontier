use crate::domain::launch::HostPrerequisite;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Validates the Linux host capabilities RetroArch depends on.
///
/// This is deliberately separate from managed-runtime integrity: an extracted AppDir is
/// relocatable for its bundled libraries but not self-contained for host graphics, audio, and
/// input services. A missing host capability must therefore never mark the managed runtime
/// corrupt or trigger a repair.
pub trait HostPrerequisiteInspector: Send + Sync {
    /// Report every prerequisite that is not satisfied, blocking or not.
    fn inspect(&self, environment: &BTreeMap<String, String>) -> Vec<HostPrerequisite>;
}

#[derive(Debug, Clone)]
pub struct LinuxHostPrerequisiteInspector {
    graphics_device_root: PathBuf,
    input_device_root: PathBuf,
}

impl Default for LinuxHostPrerequisiteInspector {
    fn default() -> Self {
        Self {
            graphics_device_root: PathBuf::from("/dev/dri"),
            input_device_root: PathBuf::from("/dev/input"),
        }
    }
}

impl LinuxHostPrerequisiteInspector {
    #[cfg(test)]
    pub fn new(graphics_device_root: PathBuf, input_device_root: PathBuf) -> Self {
        Self {
            graphics_device_root,
            input_device_root,
        }
    }
}

impl HostPrerequisiteInspector for LinuxHostPrerequisiteInspector {
    fn inspect(&self, environment: &BTreeMap<String, String>) -> Vec<HostPrerequisite> {
        let mut missing = Vec::new();

        if !has_display_session(environment) {
            missing.push(HostPrerequisite::DisplaySession);
        }
        if !readable_directory(&self.graphics_device_root) {
            missing.push(HostPrerequisite::GraphicsDevice);
        }
        if !has_audio_service(environment) {
            missing.push(HostPrerequisite::AudioService);
        }
        if !readable_directory(&self.input_device_root) {
            missing.push(HostPrerequisite::InputDevices);
        }
        missing
    }
}

/// A Wayland display needs its session runtime directory; an X11 display needs only `DISPLAY`,
/// because XWayland and native X servers both advertise themselves that way.
fn has_display_session(environment: &BTreeMap<String, String>) -> bool {
    let wayland = non_empty(environment, "WAYLAND_DISPLAY").is_some()
        && non_empty(environment, "XDG_RUNTIME_DIR").is_some();
    wayland || non_empty(environment, "DISPLAY").is_some()
}

fn has_audio_service(environment: &BTreeMap<String, String>) -> bool {
    if non_empty(environment, "PULSE_SERVER").is_some() {
        return true;
    }
    non_empty(environment, "XDG_RUNTIME_DIR")
        .map(|runtime| Path::new(runtime).join("pulse").join("native"))
        .is_some_and(|socket| fs::metadata(socket).is_ok())
}

fn non_empty<'a>(environment: &'a BTreeMap<String, String>, name: &str) -> Option<&'a str> {
    environment
        .get(name)
        .map(String::as_str)
        .filter(|value| !value.is_empty())
}

fn readable_directory(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|metadata| metadata.is_dir()) && fs::read_dir(path).is_ok()
}

#[cfg(test)]
mod tests {
    use super::{HostPrerequisiteInspector, LinuxHostPrerequisiteInspector};
    use crate::domain::launch::HostPrerequisite;
    use std::collections::BTreeMap;
    use std::fs;
    use tempfile::{tempdir, TempDir};

    fn environment(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect()
    }

    fn devices(present: bool) -> (TempDir, LinuxHostPrerequisiteInspector) {
        let directory = tempdir().unwrap();
        let graphics = directory.path().join("dri");
        let input = directory.path().join("input");
        if present {
            fs::create_dir_all(&graphics).unwrap();
            fs::create_dir_all(&input).unwrap();
        }
        (
            directory,
            LinuxHostPrerequisiteInspector::new(graphics, input),
        )
    }

    #[test]
    fn a_complete_desktop_session_reports_no_missing_prerequisite() {
        let (directory, inspector) = devices(true);
        let runtime_dir = directory.path().join("run");
        fs::create_dir_all(runtime_dir.join("pulse")).unwrap();
        fs::write(runtime_dir.join("pulse").join("native"), b"socket").unwrap();

        let missing = inspector.inspect(&environment(&[
            ("WAYLAND_DISPLAY", "wayland-0"),
            ("XDG_RUNTIME_DIR", runtime_dir.to_str().unwrap()),
        ]));

        assert!(missing.is_empty(), "unexpected: {missing:?}");
    }

    #[test]
    fn only_a_missing_display_session_blocks_the_launch() {
        let (_directory, inspector) = devices(false);

        let missing = inspector.inspect(&environment(&[]));

        assert!(missing.contains(&HostPrerequisite::DisplaySession));
        assert!(missing.contains(&HostPrerequisite::GraphicsDevice));
        assert!(missing.contains(&HostPrerequisite::AudioService));
        assert!(missing.contains(&HostPrerequisite::InputDevices));
        assert_eq!(
            missing
                .iter()
                .filter(|prerequisite| prerequisite.blocks_launch())
                .count(),
            1
        );
    }

    #[test]
    fn a_degraded_host_still_launches_with_visible_diagnostics() {
        let (_directory, inspector) = devices(false);

        // An X11 session with no audio service, no GPU node, and no input nodes.
        let missing = inspector.inspect(&environment(&[("DISPLAY", ":0")]));

        assert!(!missing.contains(&HostPrerequisite::DisplaySession));
        assert!(missing
            .iter()
            .all(|prerequisite| !prerequisite.blocks_launch()));
        assert_eq!(missing.len(), 3);
    }

    #[test]
    fn a_wayland_display_without_a_session_runtime_directory_is_not_a_display_session() {
        let (_directory, inspector) = devices(true);

        let missing = inspector.inspect(&environment(&[("WAYLAND_DISPLAY", "wayland-0")]));

        assert!(missing.contains(&HostPrerequisite::DisplaySession));
    }

    #[test]
    fn an_explicit_pulse_server_satisfies_the_audio_prerequisite() {
        let (_directory, inspector) = devices(true);

        let missing = inspector.inspect(&environment(&[
            ("DISPLAY", ":0"),
            ("PULSE_SERVER", "unix:/run/user/1000/pulse/native"),
        ]));

        assert!(!missing.contains(&HostPrerequisite::AudioService));
    }
}
