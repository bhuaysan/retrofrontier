use crate::adapters::runtime_paths::{fsync_directory, RuntimePaths};
use crate::adapters::runtime_trust::atomic_replace;
use crate::domain::runtime::{
    parse_strict_json, ManagedProcessPhase, ManagedProcessRecord, RuntimeError, SafeIdentifier,
    MANAGED_PROCESS_RECORD_SCHEMA_VERSION,
};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

static PROCESS_RECORD_COUNTER: AtomicU64 = AtomicU64::new(0);

pub trait ManagedProcessInspector: Send + Sync {
    fn ensure_no_active_game(&self, paths: &RuntimePaths) -> Result<(), RuntimeError>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct LinuxManagedProcessInspector;

impl ManagedProcessInspector for LinuxManagedProcessInspector {
    fn ensure_no_active_game(&self, paths: &RuntimePaths) -> Result<(), RuntimeError> {
        let record = match read_process_record(paths) {
            Ok(Some(record)) => record,
            Ok(None) => return Ok(()),
            Err(RuntimeError::GameActive) => {
                // A regular record that cannot be parsed or validated is not evidence of a live
                // process. Quarantine it before removing it; symlinks and non-regular paths are
                // left blocking because they cannot be safely owned by this adapter.
                if quarantine_unusable_process_record(paths)? {
                    return Ok(());
                }
                return Err(RuntimeError::GameActive);
            }
            Err(RuntimeError::ProcessRecordSchema) => {
                // An old, newer, or otherwise incompatible schema may contain a live process
                // identity that this binary cannot safely interpret. Preserve it and keep all
                // runtime mutation blocked until an explicit recovery path handles it.
                return Err(RuntimeError::GameActive);
            }
            Err(error) => return Err(error),
        };

        // Fail closed: only proof that no managed process survives may remove the record.
        match managed_process_is_live(paths, &record) {
            Ok(true) => Err(RuntimeError::GameActive),
            Ok(false) => {
                clear_process_record(paths)?;
                Ok(())
            }
            Err(_) => Err(RuntimeError::GameActive),
        }
    }
}

/// Decide whether the recorded managed process can still be alive.
///
/// `Ok(false)` is a proof of absence, `Ok(true)` a proof of presence, and `Err` uncertainty. Only
/// `Ok(false)` may lead to deleting the durable record.
fn managed_process_is_live(
    paths: &RuntimePaths,
    record: &ManagedProcessRecord,
) -> Result<bool, std::io::Error> {
    // A record from a previous boot cannot describe a live process, whatever its phase.
    if current_boot_id().map_err(std::io::Error::other)? != record.boot_id {
        return Ok(false);
    }
    match record.phase {
        ManagedProcessPhase::Running => process_identity(record),
        // A launching record has no PID by construction, so liveness is decided by looking for any
        // process of this user that could be the managed child. The scan deliberately
        // over-detects: a false positive keeps runtime mutation blocked, a false negative would
        // let an update run underneath a live emulator.
        ManagedProcessPhase::Launching => managed_process_exists(paths, record),
    }
}

/// Look for a live process that could be the managed RetroArch child.
///
/// A process qualifies when its resolved executable is inside the managed versions root, or when
/// *any* element of its command line names the authenticated AppRun.
///
/// Scanning the whole command line, rather than `argv[0]`, is what makes a script AppRun
/// detectable. When Linux executes a `#!` file it runs the interpreter instead, with a command
/// line of the shape `interpreter [optional-arg] script-path original-argv[1..]`. The original
/// `argv[0]` is not preserved: `/proc/<pid>/exe` resolves to a host interpreter outside the
/// managed tree and the AppRun appears as an interpreter *argument*. Matching only `argv[0]`
/// therefore failed to see a live managed child, cleared `game-process.json`, and allowed runtime
/// mutation underneath a running emulator.
///
/// The scan deliberately over-detects: an unrelated process that merely mentions the AppRun path
/// matches too. A false positive only keeps runtime mutation blocked, whereas a false negative
/// would let an update run underneath a live emulator.
///
/// The AppRun is a valid match key only while it belongs to the managed versions tree, so a record
/// naming a host path can never make an arbitrary process look managed.
fn managed_process_exists(
    paths: &RuntimePaths,
    record: &ManagedProcessRecord,
) -> Result<bool, std::io::Error> {
    let versions_root = fs::canonicalize(paths.versions_root())?;
    let expected_apprun = Path::new(&record.expected_apprun_path);
    let canonical_apprun = fs::canonicalize(expected_apprun)
        .ok()
        .filter(|apprun| apprun.starts_with(&versions_root));
    // An installation can be moved or removed underneath a process that is still running, and the
    // recorded AppRun then no longer resolves. The absolute path `write_process_record` already
    // validated against this tree stays a legitimate match key in that case: it is a command line
    // the managed launch composed itself, so matching it can only over-detect.
    let contained = canonical_apprun.is_some()
        || (expected_apprun.is_absolute()
            && (expected_apprun.starts_with(&versions_root)
                || expected_apprun.starts_with(paths.versions_root())));
    let apprun_file_name = expected_apprun.file_name().map(ToOwned::to_owned);
    let self_pid = std::process::id();

    for entry in fs::read_dir("/proc")? {
        // A directory entry that cannot be read at all names a process that has already gone or
        // that this user cannot inspect, and neither can be the managed child this scan is looking
        // for. Skipping it matches how a per-process read failure below is already treated;
        // propagating it would turn ordinary `/proc` churn into a false "a game is running".
        let Ok(entry) = entry else {
            continue;
        };
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let Ok(pid) = name.parse::<u32>() else {
            continue;
        };
        if pid == self_pid {
            continue;
        }
        // A process this user cannot inspect at all is not a process this user could have
        // spawned, so a per-process read failure is skipped rather than treated as uncertainty.
        if let Ok(executable) = fs::read_link(format!("/proc/{pid}/exe")) {
            if let Ok(executable) = fs::canonicalize(&executable) {
                if executable.starts_with(&versions_root) {
                    return Ok(true);
                }
            }
        }
        if !contained {
            continue;
        }
        let Ok(cmdline) = read_capped(&format!("/proc/{pid}/cmdline"), CMDLINE_SCAN_LIMIT) else {
            continue;
        };
        for argument in cmdline.split(|byte| *byte == 0) {
            let Ok(argument) = std::str::from_utf8(argument) else {
                continue;
            };
            if argument.is_empty() {
                continue;
            }
            let argument = Path::new(argument);
            if argument == expected_apprun {
                return Ok(true);
            }
            let Some(canonical_apprun) = canonical_apprun.as_ref() else {
                continue;
            };
            // Canonicalizing every argument of every process would cost a syscall per argument,
            // so only arguments that could name the same file are resolved. A differently spelled
            // path still keeps the AppRun's file name.
            if argument.file_name() != apprun_file_name.as_deref() {
                continue;
            }
            if fs::canonicalize(argument).is_ok_and(|path| &path == canonical_apprun) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// How much of one `/proc/<pid>/cmdline` the scan reads. A command line longer than this cannot
/// belong to a managed launch, whose arguments RetroFrontier composes itself.
const CMDLINE_SCAN_LIMIT: u64 = 64 * 1024;

fn read_capped(path: &str, limit: u64) -> Result<Vec<u8>, std::io::Error> {
    let mut bytes = Vec::new();
    fs::File::open(path)?.take(limit).read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// Test and embedding hook for callers that have a managed-process supervisor of their own.
#[derive(Debug, Clone, Default)]
pub struct StaticManagedProcessInspector {
    active: Arc<Mutex<bool>>,
}

impl StaticManagedProcessInspector {
    pub fn set_active(&self, active: bool) {
        *self
            .active
            .lock()
            .expect("process test lock should not be poisoned") = active;
    }
}

impl ManagedProcessInspector for StaticManagedProcessInspector {
    fn ensure_no_active_game(&self, _paths: &RuntimePaths) -> Result<(), RuntimeError> {
        if *self
            .active
            .lock()
            .expect("process test lock should not be poisoned")
        {
            Err(RuntimeError::GameActive)
        } else {
            Ok(())
        }
    }
}

pub fn read_process_record(
    paths: &RuntimePaths,
) -> Result<Option<ManagedProcessRecord>, RuntimeError> {
    let path = paths.game_process_record();
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(RuntimeError::Io(error)),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > 4096 {
        return Err(RuntimeError::GameActive);
    }
    let mut bytes = Vec::new();
    fs::File::open(path)?.take(4097).read_to_end(&mut bytes)?;
    if bytes.len() > 4096 {
        return Err(RuntimeError::GameActive);
    }
    let value: serde_json::Value =
        parse_strict_json(&bytes).map_err(|_| RuntimeError::GameActive)?;
    let declared_schema = value.get("schema_version");
    if declared_schema.and_then(serde_json::Value::as_u64)
        != Some(MANAGED_PROCESS_RECORD_SCHEMA_VERSION as u64)
    {
        if declared_schema.is_some() {
            return Err(RuntimeError::ProcessRecordSchema);
        }
        return Err(RuntimeError::GameActive);
    }
    let record: ManagedProcessRecord =
        serde_json::from_value(value).map_err(|_| RuntimeError::GameActive)?;
    record.validate()?;
    Ok(Some(record))
}

pub fn write_process_record(
    paths: &RuntimePaths,
    record: &ManagedProcessRecord,
) -> Result<(), RuntimeError> {
    record.validate()?;
    if !record_targets_runtime(paths, record) {
        return Err(RuntimeError::GameActive);
    }
    let bytes = serde_json::to_vec(record).map_err(|error| {
        RuntimeError::Storage(format!("process record serialization failed: {error}"))
    })?;
    atomic_replace(paths.game_process_record(), &bytes, 4096, |bytes| {
        let parsed: ManagedProcessRecord =
            parse_strict_json(bytes).map_err(|_| RuntimeError::GameActive)?;
        parsed.validate()
    })
}

pub fn clear_process_record(paths: &RuntimePaths) -> Result<(), RuntimeError> {
    match fs::symlink_metadata(paths.game_process_record()) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(RuntimeError::GameActive),
        Ok(metadata) if metadata.is_file() => {
            fs::remove_file(paths.game_process_record())?;
            Ok(())
        }
        Ok(_) => Err(RuntimeError::GameActive),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(RuntimeError::Io(error)),
    }
}

fn quarantine_unusable_process_record(paths: &RuntimePaths) -> Result<bool, RuntimeError> {
    let record_path = paths.game_process_record();
    let metadata = match fs::symlink_metadata(record_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(RuntimeError::Io(error)),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Ok(false);
    }

    let counter = PROCESS_RECORD_COUNTER.fetch_add(1, Ordering::Relaxed);
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let quarantine = paths.runtime_root().join(format!(
        ".game-process.json.invalid-{}-{counter}-{stamp}",
        std::process::id()
    ));
    if fs::symlink_metadata(&quarantine).is_ok() {
        return Err(RuntimeError::GameActive);
    }
    fs::rename(record_path, &quarantine)?;
    fsync_directory(paths.runtime_root())?;
    if let Err(error) = fs::remove_file(&quarantine) {
        tracing::warn!(
            path = %quarantine.display(),
            error = %error,
            "invalid managed process record remains in quarantine"
        );
    } else {
        fsync_directory(paths.runtime_root())?;
    }
    Ok(true)
}

fn record_targets_runtime(paths: &RuntimePaths, record: &ManagedProcessRecord) -> bool {
    let expected = Path::new(&record.expected_apprun_path);
    let canonical_expected = match fs::canonicalize(expected) {
        Ok(path) => path,
        Err(_) => return false,
    };
    let versions_root = match fs::canonicalize(paths.versions_root()) {
        Ok(path) => path,
        Err(_) => return false,
    };
    if !canonical_expected.starts_with(&versions_root) {
        return false;
    }
    let expected_id = paths.version_path(&record.installation_id);
    fs::canonicalize(expected_id)
        .map(|installation| canonical_expected.starts_with(installation))
        .unwrap_or(false)
}

fn process_identity(record: &ManagedProcessRecord) -> Result<bool, std::io::Error> {
    let (Some(pid), Some(expected_ticks)) = (record.pid, record.process_start_time_ticks) else {
        // A running record without identity never validates, so this is defensive only.
        return Err(std::io::Error::other(
            "managed process record has no identity",
        ));
    };
    let stat_path = PathBuf::from(format!("/proc/{pid}/stat"));
    let stat = match fs::read_to_string(&stat_path) {
        Ok(stat) => stat,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    let start_time_ticks = parse_start_time_ticks(&stat).ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid /proc stat")
    })?;
    if start_time_ticks != expected_ticks {
        return Ok(false);
    }
    let exe_path = PathBuf::from(format!("/proc/{pid}/exe"));
    let actual_exe = match fs::read_link(exe_path) {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    let expected_exe_path = record
        .expected_executable_path
        .as_deref()
        .unwrap_or(&record.expected_apprun_path);
    let expected_exe = fs::canonicalize(expected_exe_path)?;
    let actual_exe = match fs::canonicalize(actual_exe) {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    if actual_exe != expected_exe {
        // A PID/start-time match with a different executable is uncertainty, not proof that the
        // process is gone. Keep activation blocked rather than deleting the record.
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "managed process executable identity differs",
        ));
    }
    Ok(true)
}

pub fn parse_start_time_ticks(stat: &str) -> Option<u64> {
    let closing_name = stat.rfind(')')?;
    let fields = stat.get(closing_name + 1..)?.split_whitespace();
    // The suffix starts at field 3 (state), so field 22 (starttime) is offset 19.
    fields.clone().nth(19)?.parse().ok()
}

pub fn process_start_time_ticks(pid: u32) -> Result<u64, RuntimeError> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat"))?;
    parse_start_time_ticks(&stat).ok_or_else(|| {
        RuntimeError::Storage("could not parse managed process start time".to_owned())
    })
}

/// The conservative pre-spawn record. It names the launch and session but claims no process
/// identity, because the child does not exist yet.
pub fn make_launching_record(
    launch_id: SafeIdentifier,
    play_session_id: i64,
    installation_id: SafeIdentifier,
    expected_apprun_path: &Path,
) -> Result<ManagedProcessRecord, RuntimeError> {
    let record = ManagedProcessRecord {
        schema_version: MANAGED_PROCESS_RECORD_SCHEMA_VERSION,
        phase: crate::domain::runtime::ManagedProcessPhase::Launching,
        launch_id,
        play_session_id,
        boot_id: current_boot_id()?,
        installation_id,
        expected_apprun_path: absolute_path(expected_apprun_path)?,
        pid: None,
        process_start_time_ticks: None,
        expected_executable_path: None,
    };
    record.validate()?;
    Ok(record)
}

/// Complete the record with strong process identity once the child exists.
pub fn make_running_record(
    launching: &ManagedProcessRecord,
    pid: u32,
) -> Result<ManagedProcessRecord, RuntimeError> {
    let record = ManagedProcessRecord {
        schema_version: MANAGED_PROCESS_RECORD_SCHEMA_VERSION,
        phase: crate::domain::runtime::ManagedProcessPhase::Running,
        launch_id: launching.launch_id.clone(),
        play_session_id: launching.play_session_id,
        boot_id: current_boot_id()?,
        installation_id: launching.installation_id.clone(),
        expected_apprun_path: launching.expected_apprun_path.clone(),
        pid: Some(pid),
        process_start_time_ticks: Some(process_start_time_ticks(pid)?),
        expected_executable_path: Some(
            fs::read_link(format!("/proc/{pid}/exe"))
                .map_err(|_| RuntimeError::GameActive)?
                .to_str()
                .ok_or(RuntimeError::GameActive)?
                .to_owned(),
        ),
    };
    record.validate()?;
    Ok(record)
}

fn absolute_path(path: &Path) -> Result<String, RuntimeError> {
    if !path.is_absolute() {
        return Err(RuntimeError::GameActive);
    }
    path.to_str()
        .map(str::to_owned)
        .ok_or(RuntimeError::GameActive)
}

pub fn current_boot_id() -> Result<String, RuntimeError> {
    let boot_id = fs::read_to_string("/proc/sys/kernel/random/boot_id")?;
    let boot_id = boot_id.trim();
    if boot_id.is_empty() {
        return Err(RuntimeError::Storage(
            "managed process boot identity is empty".to_owned(),
        ));
    }
    Ok(boot_id.to_owned())
}

#[cfg(test)]
mod tests {
    use super::{
        make_launching_record, make_running_record, parse_start_time_ticks, read_process_record,
        LinuxManagedProcessInspector, ManagedProcessInspector,
    };
    use crate::adapters::runtime_paths::RuntimePaths;
    use crate::domain::runtime::{
        ManagedProcessPhase, ManagedProcessRecord, RuntimeError, SafeIdentifier,
        MANAGED_PROCESS_RECORD_SCHEMA_VERSION,
    };
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command, Stdio};
    use tempfile::{tempdir, TempDir};

    fn current_boot_id() -> String {
        fs::read_to_string("/proc/sys/kernel/random/boot_id")
            .unwrap()
            .trim()
            .to_owned()
    }

    fn fixture() -> (TempDir, RuntimePaths, SafeIdentifier, PathBuf) {
        let directory = tempdir().unwrap();
        let paths = RuntimePaths::new(directory.path());
        paths.prepare().unwrap();
        let installation_id: SafeIdentifier = "install-1".try_into().unwrap();
        let installation = paths.version_path(&installation_id);
        fs::create_dir_all(&installation).unwrap();
        let apprun = installation.join("AppRun");
        (directory, paths, installation_id, apprun)
    }

    /// A real executable inside the managed versions tree, so `/proc/<pid>/exe` resolves there.
    fn managed_shell(apprun: &Path) {
        fs::copy("/bin/sh", apprun).unwrap();
    }

    fn make_executable(path: &Path) {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// An AppRun that is a `#!` script. Linux runs the *interpreter*, which lives outside the
    /// managed versions tree, so neither `/proc/<pid>/exe` nor `argv[0]` names the AppRun.
    ///
    /// The script announces itself before blocking, so a test can wait until the interpreter has
    /// finished reading it.
    fn shebang_apprun(apprun: &Path) {
        fs::write(
            apprun,
            format!(
                "#!/bin/sh\n: > '{}'\nread line\n",
                started_marker(apprun).display()
            ),
        )
        .unwrap();
        make_executable(apprun);
    }

    fn started_marker(apprun: &Path) -> PathBuf {
        apprun.with_file_name("apprun-started")
    }

    /// A child that blocks on stdin, so it has no grandchildren to outlive it.
    ///
    /// Every test spawns through this. A shell given a command to run forks a grandchild that the
    /// test never owns: killing and reaping the shell leaves that grandchild behind, and while it
    /// is still between `fork` and `exec` it carries the shell's own executable and command line,
    /// so the liveness scan correctly sees a live managed process the test believed it had
    /// stopped. A shell reading commands from a pipe never forks, so the test controls exactly
    /// one process.
    ///
    /// Spawning an executable that was just written can transiently fail with `ETXTBSY` when a
    /// parallel test thread forked while the writing descriptor was still open, so the spawn is
    /// retried briefly. This is a test-harness concern only.
    fn spawn_blocking_child(apprun: &Path) -> Child {
        for _ in 0..50 {
            match Command::new(apprun).stdin(Stdio::piped()).spawn() {
                Ok(child) => return child,
                Err(error) if error.raw_os_error() == Some(26) => {
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
                Err(error) => panic!("managed child should spawn: {error}"),
            }
        }
        panic!("managed child should spawn before the retry budget is exhausted");
    }

    /// A shebang child that is known to be running its script rather than still starting up.
    fn spawn_started_child(apprun: &Path) -> Child {
        let marker = started_marker(apprun);
        let mut child = spawn_blocking_child(apprun);
        for _ in 0..500 {
            if marker.exists() {
                return child;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let _ = child.kill();
        let _ = child.wait();
        panic!("the shebang AppRun should start before the wait budget is exhausted");
    }

    #[test]
    fn proc_stat_parser_handles_parentheses_in_process_names() {
        let stat = "42 (retro)arch) S 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 12345";
        assert_eq!(parse_start_time_ticks(stat), Some(12345));
    }

    #[test]
    fn a_launching_record_carries_no_process_identity_and_a_running_record_requires_one() {
        let (_directory, _paths, installation_id, apprun) = fixture();
        managed_shell(&apprun);
        let launch_id: SafeIdentifier = "launch-1".try_into().unwrap();

        let launching =
            make_launching_record(launch_id, 7, installation_id.clone(), &apprun).unwrap();
        assert_eq!(
            launching.schema_version,
            MANAGED_PROCESS_RECORD_SCHEMA_VERSION
        );
        assert_eq!(launching.phase, ManagedProcessPhase::Launching);
        assert_eq!(launching.play_session_id, 7);
        assert!(launching.pid.is_none());
        assert!(launching.process_start_time_ticks.is_none());
        assert!(launching.expected_executable_path.is_none());
        launching.validate().unwrap();

        // A launching record that claims an identity is refused, so a later liveness check can
        // never confuse a fabricated identity with a real one.
        let dishonest = ManagedProcessRecord {
            pid: Some(1),
            ..launching.clone()
        };
        assert!(matches!(
            dishonest.validate(),
            Err(RuntimeError::GameActive)
        ));

        // A running record without full identity is refused; PID alone is never identity.
        let incomplete = ManagedProcessRecord {
            phase: ManagedProcessPhase::Running,
            pid: Some(std::process::id()),
            process_start_time_ticks: None,
            expected_executable_path: None,
            ..launching.clone()
        };
        assert!(matches!(
            incomplete.validate(),
            Err(RuntimeError::GameActive)
        ));

        let running = make_running_record(&launching, std::process::id()).unwrap();
        assert_eq!(running.phase, ManagedProcessPhase::Running);
        assert_eq!(running.launch_id, launching.launch_id);
        assert_eq!(running.play_session_id, launching.play_session_id);
        assert!(running.pid.is_some());
        assert!(running.process_start_time_ticks.is_some());
        assert!(running.expected_executable_path.is_some());
        running.validate().unwrap();
    }

    #[test]
    fn a_launching_record_blocks_while_a_managed_process_is_alive() {
        let (_directory, paths, installation_id, apprun) = fixture();
        managed_shell(&apprun);
        let record =
            make_launching_record("launch-1".try_into().unwrap(), 3, installation_id, &apprun)
                .unwrap();
        super::write_process_record(&paths, &record).unwrap();
        let mut child = spawn_blocking_child(&apprun);

        assert!(matches!(
            LinuxManagedProcessInspector.ensure_no_active_game(&paths),
            Err(RuntimeError::GameActive)
        ));
        assert!(paths.game_process_record().exists());

        child.kill().unwrap();
        child.wait().unwrap();

        LinuxManagedProcessInspector
            .ensure_no_active_game(&paths)
            .unwrap();
        assert!(!paths.game_process_record().exists());
    }

    #[test]
    fn a_launching_record_from_a_previous_boot_is_cleared() {
        let (_directory, paths, installation_id, apprun) = fixture();
        managed_shell(&apprun);
        let record = ManagedProcessRecord {
            boot_id: "previous-boot".to_owned(),
            ..make_launching_record("launch-1".try_into().unwrap(), 4, installation_id, &apprun)
                .unwrap()
        };
        super::write_process_record(&paths, &record).unwrap();
        let mut child = spawn_blocking_child(&apprun);

        // Even with a live managed process, a pre-reboot record cannot describe it.
        LinuxManagedProcessInspector
            .ensure_no_active_game(&paths)
            .unwrap();
        assert!(!paths.game_process_record().exists());

        child.kill().unwrap();
        child.wait().unwrap();
    }

    #[test]
    fn malformed_process_records_are_recovered_but_mismatched_live_identity_blocks() {
        let (_directory, paths, installation_id, apprun) = fixture();
        fs::write(paths.game_process_record(), b"not json").unwrap();
        LinuxManagedProcessInspector
            .ensure_no_active_game(&paths)
            .unwrap();
        assert!(!paths.game_process_record().exists());

        fs::copy(std::env::current_exe().unwrap(), &apprun).unwrap();
        let record = ManagedProcessRecord {
            schema_version: MANAGED_PROCESS_RECORD_SCHEMA_VERSION,
            phase: ManagedProcessPhase::Running,
            launch_id: "launch-1".try_into().unwrap(),
            play_session_id: 1,
            boot_id: current_boot_id(),
            installation_id,
            expected_apprun_path: apprun.to_str().unwrap().to_owned(),
            pid: Some(std::process::id()),
            process_start_time_ticks: Some(
                super::process_start_time_ticks(std::process::id()).unwrap(),
            ),
            expected_executable_path: Some("/bin/sh".to_owned()),
        };
        super::write_process_record(&paths, &record).unwrap();
        assert!(matches!(
            LinuxManagedProcessInspector.ensure_no_active_game(&paths),
            Err(RuntimeError::GameActive)
        ));
        assert!(paths.game_process_record().exists());

        fs::remove_file(paths.game_process_record()).unwrap();
        let old_boot_record = ManagedProcessRecord {
            boot_id: "previous-boot".to_owned(),
            ..record.clone()
        };
        super::write_process_record(&paths, &old_boot_record).unwrap();
        LinuxManagedProcessInspector
            .ensure_no_active_game(&paths)
            .unwrap();
        assert!(!paths.game_process_record().exists());
    }

    #[test]
    fn a_stale_pid_and_a_reused_pid_are_both_treated_as_gone() {
        let (_directory, paths, installation_id, apprun) = fixture();
        managed_shell(&apprun);
        let launching =
            make_launching_record("launch-1".try_into().unwrap(), 5, installation_id, &apprun)
                .unwrap();

        // A PID that does not exist any more.
        let stale = ManagedProcessRecord {
            phase: ManagedProcessPhase::Running,
            pid: Some(u32::MAX),
            process_start_time_ticks: Some(1),
            expected_executable_path: Some(apprun.to_str().unwrap().to_owned()),
            ..launching.clone()
        };
        super::write_process_record(&paths, &stale).unwrap();
        LinuxManagedProcessInspector
            .ensure_no_active_game(&paths)
            .unwrap();
        assert!(!paths.game_process_record().exists());

        // A live PID whose start time differs is a reused PID, not the recorded process.
        let mut child = spawn_blocking_child(&apprun);
        let reused = ManagedProcessRecord {
            phase: ManagedProcessPhase::Running,
            pid: Some(child.id()),
            process_start_time_ticks: Some(
                super::process_start_time_ticks(child.id()).unwrap() + 1,
            ),
            expected_executable_path: Some(apprun.to_str().unwrap().to_owned()),
            ..launching
        };
        super::write_process_record(&paths, &reused).unwrap();
        LinuxManagedProcessInspector
            .ensure_no_active_game(&paths)
            .unwrap();
        assert!(!paths.game_process_record().exists());

        child.kill().unwrap();
        child.wait().unwrap();
    }

    #[test]
    fn a_running_managed_child_keeps_runtime_mutation_blocked_until_it_exits() {
        let (_directory, paths, installation_id, apprun) = fixture();
        managed_shell(&apprun);
        let launching =
            make_launching_record("launch-1".try_into().unwrap(), 6, installation_id, &apprun)
                .unwrap();
        let mut child = spawn_blocking_child(&apprun);
        let running = make_running_record(&launching, child.id()).unwrap();
        super::write_process_record(&paths, &running).unwrap();

        assert!(matches!(
            LinuxManagedProcessInspector.ensure_no_active_game(&paths),
            Err(RuntimeError::GameActive)
        ));

        child.kill().unwrap();
        child.wait().unwrap();

        LinuxManagedProcessInspector
            .ensure_no_active_game(&paths)
            .unwrap();
        assert!(!paths.game_process_record().exists());
    }

    #[test]
    fn current_process_record_schema_round_trips_and_incompatible_versions_remain_blocking() {
        let (_directory, paths, installation_id, apprun) = fixture();
        fs::write(&apprun, b"placeholder").unwrap();
        let current = ManagedProcessRecord {
            schema_version: MANAGED_PROCESS_RECORD_SCHEMA_VERSION,
            phase: ManagedProcessPhase::Launching,
            launch_id: "launch-1".try_into().unwrap(),
            play_session_id: 2,
            boot_id: "current-boot".to_owned(),
            installation_id,
            expected_apprun_path: apprun.to_str().unwrap().to_owned(),
            pid: None,
            process_start_time_ticks: None,
            expected_executable_path: None,
        };
        super::write_process_record(&paths, &current).unwrap();
        assert_eq!(
            read_process_record(&paths).unwrap().unwrap().schema_version,
            MANAGED_PROCESS_RECORD_SCHEMA_VERSION
        );
        fs::remove_file(paths.game_process_record()).unwrap();

        // The pre-M7 record is not deleted; it is uncertain and keeps mutation blocked.
        let previous = serde_json::json!({
            "schema_version": 2,
            "phase": "running",
            "pid": 1,
            "process_start_time_ticks": 1,
            "boot_id": "previous-schema-boot",
            "installation_id": "install-1",
            "expected_apprun_path": "/tmp/runtime/install-1/AppRun"
        });
        fs::write(
            paths.game_process_record(),
            serde_json::to_vec(&previous).unwrap(),
        )
        .unwrap();
        assert!(matches!(
            read_process_record(&paths),
            Err(RuntimeError::ProcessRecordSchema)
        ));
        assert!(matches!(
            LinuxManagedProcessInspector.ensure_no_active_game(&paths),
            Err(RuntimeError::GameActive)
        ));
        assert!(paths.game_process_record().exists());

        let newer = serde_json::json!({
            "schema_version": MANAGED_PROCESS_RECORD_SCHEMA_VERSION + 1,
            "phase": "launching",
            "launch_id": "launch-1",
            "play_session_id": 1,
            "boot_id": "future-boot",
            "installation_id": "install-1",
            "expected_apprun_path": "/tmp/runtime/install-1/AppRun"
        });
        fs::write(
            paths.game_process_record(),
            serde_json::to_vec(&newer).unwrap(),
        )
        .unwrap();
        assert!(matches!(
            LinuxManagedProcessInspector.ensure_no_active_game(&paths),
            Err(RuntimeError::GameActive)
        ));
        assert!(paths.game_process_record().exists());
    }

    #[test]
    fn stale_record_for_deleted_installation_is_cleared_after_liveness_check() {
        let directory = tempdir().unwrap();
        let paths = RuntimePaths::new(directory.path());
        paths.prepare().unwrap();
        let installation_id: SafeIdentifier = "deleted-install".try_into().unwrap();
        let expected_apprun = paths.version_path(&installation_id).join("AppRun");
        let record = ManagedProcessRecord {
            schema_version: MANAGED_PROCESS_RECORD_SCHEMA_VERSION,
            phase: ManagedProcessPhase::Launching,
            launch_id: "launch-1".try_into().unwrap(),
            play_session_id: 1,
            boot_id: current_boot_id(),
            installation_id,
            expected_apprun_path: expected_apprun.to_str().unwrap().to_owned(),
            pid: None,
            process_start_time_ticks: None,
            expected_executable_path: None,
        };
        fs::write(
            paths.game_process_record(),
            serde_json::to_vec(&record).unwrap(),
        )
        .unwrap();

        LinuxManagedProcessInspector
            .ensure_no_active_game(&paths)
            .unwrap();
        assert!(!paths.game_process_record().exists());
    }

    #[test]
    fn a_launching_record_blocks_while_a_shebang_apprun_runs_under_a_host_interpreter() {
        let (_directory, paths, installation_id, apprun) = fixture();
        shebang_apprun(&apprun);
        let record =
            make_launching_record("launch-1".try_into().unwrap(), 8, installation_id, &apprun)
                .unwrap();
        super::write_process_record(&paths, &record).unwrap();
        let mut child = spawn_started_child(&apprun);

        // The interpreter is the host shell, so the executable is outside the managed tree and
        // the AppRun appears only as an *argument* of the interpreter.
        let executable =
            fs::canonicalize(fs::read_link(format!("/proc/{}/exe", child.id())).unwrap()).unwrap();
        assert!(!executable.starts_with(fs::canonicalize(paths.versions_root()).unwrap()));
        let cmdline = fs::read(format!("/proc/{}/cmdline", child.id())).unwrap();
        let arguments: Vec<&[u8]> = cmdline.split(|byte| *byte == 0).collect();
        assert_ne!(arguments[0], apprun.to_str().unwrap().as_bytes());

        assert!(matches!(
            LinuxManagedProcessInspector.ensure_no_active_game(&paths),
            Err(RuntimeError::GameActive)
        ));
        assert!(paths.game_process_record().exists());

        child.kill().unwrap();
        child.wait().unwrap();

        LinuxManagedProcessInspector
            .ensure_no_active_game(&paths)
            .unwrap();
        assert!(!paths.game_process_record().exists());
    }

    #[test]
    fn a_launching_record_blocks_while_a_symlinked_apprun_runs_from_the_managed_tree() {
        let (_directory, paths, installation_id, apprun) = fixture();
        // An ELF payload inside the installation, reached through an `AppRun` symlink.
        let payload = apprun.with_file_name("retroarch.bin");
        fs::copy("/bin/sh", &payload).unwrap();
        make_executable(&payload);
        std::os::unix::fs::symlink(&payload, &apprun).unwrap();
        let record =
            make_launching_record("launch-1".try_into().unwrap(), 9, installation_id, &apprun)
                .unwrap();
        super::write_process_record(&paths, &record).unwrap();
        let mut child = spawn_blocking_child(&apprun);

        assert!(matches!(
            LinuxManagedProcessInspector.ensure_no_active_game(&paths),
            Err(RuntimeError::GameActive)
        ));
        assert!(paths.game_process_record().exists());

        child.kill().unwrap();
        child.wait().unwrap();

        LinuxManagedProcessInspector
            .ensure_no_active_game(&paths)
            .unwrap();
        assert!(!paths.game_process_record().exists());
    }

    #[test]
    fn a_launching_record_still_blocks_when_the_installation_moves_under_a_live_process() {
        let (_directory, paths, installation_id, apprun) = fixture();
        shebang_apprun(&apprun);
        let record =
            make_launching_record("launch-1".try_into().unwrap(), 11, installation_id, &apprun)
                .unwrap();
        super::write_process_record(&paths, &record).unwrap();
        let mut child = spawn_started_child(&apprun);
        // The installation is renamed away underneath the live child, so the recorded AppRun no
        // longer resolves. It still names the managed tree, so it stays a valid match key.
        let installation = apprun.parent().unwrap();
        fs::rename(installation, installation.with_file_name("moved")).unwrap();

        assert!(matches!(
            LinuxManagedProcessInspector.ensure_no_active_game(&paths),
            Err(RuntimeError::GameActive)
        ));
        assert!(paths.game_process_record().exists());

        child.kill().unwrap();
        child.wait().unwrap();

        LinuxManagedProcessInspector
            .ensure_no_active_game(&paths)
            .unwrap();
        assert!(!paths.game_process_record().exists());
    }

    #[test]
    fn an_apprun_path_outside_the_managed_tree_is_never_a_match_key() {
        let (_directory, paths, installation_id, apprun) = fixture();
        managed_shell(&apprun);
        let record = ManagedProcessRecord {
            expected_apprun_path: "/bin/sh".to_owned(),
            ..make_launching_record("launch-1".try_into().unwrap(), 10, installation_id, &apprun)
                .unwrap()
        };
        // The record is written directly: `write_process_record` would refuse it, and that
        // containment must also hold when a tampered record is read back.
        fs::write(
            paths.game_process_record(),
            serde_json::to_vec(&record).unwrap(),
        )
        .unwrap();
        let mut child = spawn_blocking_child(Path::new("/bin/sh"));

        // A host process merely mentioning `/bin/sh` must not keep the record alive forever.
        LinuxManagedProcessInspector
            .ensure_no_active_game(&paths)
            .unwrap();
        assert!(!paths.game_process_record().exists());

        child.kill().unwrap();
        child.wait().unwrap();
    }
}
