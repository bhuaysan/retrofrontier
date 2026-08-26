use crate::adapters::runtime_paths::{fsync_directory, RuntimePaths};
use crate::adapters::runtime_trust::atomic_replace;
use crate::domain::runtime::{
    parse_strict_json, ManagedProcessRecord, RuntimeError, SafeIdentifier,
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
        if !record_targets_runtime(paths, &record) {
            return match process_identity(&record) {
                Ok(false) => {
                    clear_process_record(paths)?;
                    Ok(())
                }
                Ok(true) | Err(_) => Err(RuntimeError::GameActive),
            };
        }
        match process_identity(&record) {
            Ok(true) => Err(RuntimeError::GameActive),
            Ok(false) => {
                fs::remove_file(paths.game_process_record())?;
                Ok(())
            }
            Err(_) => Err(RuntimeError::GameActive),
        }
    }
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
    // TODO(Sol Max review): re-audit Linux /proc identity semantics and record lifecycle before
    // game launching relies on this boundary.
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
    let boot_id = fs::read_to_string("/proc/sys/kernel/random/boot_id")?;
    if boot_id.trim() != record.boot_id {
        return Ok(false);
    }
    let stat_path = PathBuf::from(format!("/proc/{}/stat", record.pid));
    let stat = match fs::read_to_string(&stat_path) {
        Ok(stat) => stat,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    let start_time_ticks = parse_start_time_ticks(&stat).ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid /proc stat")
    })?;
    if start_time_ticks != record.process_start_time_ticks {
        return Ok(false);
    }
    let exe_path = PathBuf::from(format!("/proc/{}/exe", record.pid));
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

pub fn make_process_record(
    phase: crate::domain::runtime::ManagedProcessPhase,
    pid: u32,
    installation_id: SafeIdentifier,
    expected_apprun_path: &Path,
) -> Result<ManagedProcessRecord, RuntimeError> {
    let record = ManagedProcessRecord {
        schema_version: MANAGED_PROCESS_RECORD_SCHEMA_VERSION,
        phase,
        pid,
        process_start_time_ticks: process_start_time_ticks(pid)?,
        boot_id: current_boot_id()?,
        installation_id,
        expected_apprun_path: expected_apprun_path
            .to_str()
            .ok_or(RuntimeError::GameActive)?
            .to_owned(),
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

fn current_boot_id() -> Result<String, RuntimeError> {
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
        parse_start_time_ticks, read_process_record, LinuxManagedProcessInspector,
        ManagedProcessInspector,
    };
    use crate::adapters::runtime_paths::RuntimePaths;
    use crate::domain::runtime::{
        ManagedProcessPhase, ManagedProcessRecord, RuntimeError, SafeIdentifier,
        MANAGED_PROCESS_RECORD_SCHEMA_VERSION,
    };
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn proc_stat_parser_handles_parentheses_in_process_names() {
        let stat = "42 (retro)arch) S 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 12345";
        assert_eq!(parse_start_time_ticks(stat), Some(12345));
    }

    #[test]
    fn malformed_process_records_are_recovered_but_mismatched_live_identity_blocks() {
        let directory = tempdir().unwrap();
        let paths = RuntimePaths::new(directory.path());
        paths.prepare().unwrap();
        fs::write(paths.game_process_record(), b"not json").unwrap();
        LinuxManagedProcessInspector
            .ensure_no_active_game(&paths)
            .unwrap();
        assert!(!paths.game_process_record().exists());

        let installation_id: SafeIdentifier = "install-1".try_into().unwrap();
        let installation = paths.version_path(&installation_id);
        fs::create_dir_all(&installation).unwrap();
        let apprun = installation.join("AppRun");
        fs::copy(std::env::current_exe().unwrap(), &apprun).unwrap();
        let record = ManagedProcessRecord {
            schema_version: MANAGED_PROCESS_RECORD_SCHEMA_VERSION,
            phase: ManagedProcessPhase::Running,
            pid: std::process::id(),
            process_start_time_ticks: super::process_start_time_ticks(std::process::id()).unwrap(),
            boot_id: fs::read_to_string("/proc/sys/kernel/random/boot_id")
                .unwrap()
                .trim()
                .to_owned(),
            installation_id,
            expected_apprun_path: apprun.to_str().unwrap().to_owned(),
            expected_executable_path: Some("/bin/sh".to_owned()),
        };
        super::write_process_record(&paths, &record).unwrap();
        assert!(matches!(
            LinuxManagedProcessInspector.ensure_no_active_game(&paths),
            Err(crate::domain::runtime::RuntimeError::GameActive)
        ));

        fs::remove_file(paths.game_process_record()).unwrap();
        let old_boot_record = ManagedProcessRecord {
            boot_id: "previous-boot".to_owned(),
            ..record
        };
        super::write_process_record(&paths, &old_boot_record).unwrap();
        LinuxManagedProcessInspector
            .ensure_no_active_game(&paths)
            .unwrap();
        assert!(!paths.game_process_record().exists());
    }

    #[test]
    fn current_process_record_schema_round_trips_and_incompatible_versions_remain_blocking() {
        let directory = tempdir().unwrap();
        let paths = RuntimePaths::new(directory.path());
        paths.prepare().unwrap();

        let installation_id = SafeIdentifier::new("install-1").unwrap();
        let installation = paths.version_path(&installation_id);
        fs::create_dir(&installation).unwrap();
        let apprun = installation.join("AppRun");
        fs::write(&apprun, b"placeholder").unwrap();
        let current = ManagedProcessRecord {
            schema_version: MANAGED_PROCESS_RECORD_SCHEMA_VERSION,
            phase: ManagedProcessPhase::Launching,
            pid: u32::MAX,
            process_start_time_ticks: 1,
            boot_id: "current-boot".to_owned(),
            installation_id,
            expected_apprun_path: apprun.to_str().unwrap().to_owned(),
            expected_executable_path: None,
        };
        super::write_process_record(&paths, &current).unwrap();
        assert_eq!(
            read_process_record(&paths).unwrap().unwrap().schema_version,
            MANAGED_PROCESS_RECORD_SCHEMA_VERSION
        );
        fs::remove_file(paths.game_process_record()).unwrap();

        let old = serde_json::json!({
            "schema_version": 1,
            "phase": "launching",
            "pid": 1,
            "process_start_time_ticks": 1,
            "installation_id": "install-1",
            "expected_apprun_path": "/tmp/runtime/install-1/AppRun"
        });
        fs::write(
            paths.game_process_record(),
            serde_json::to_vec(&old).unwrap(),
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
            "pid": 1,
            "process_start_time_ticks": 1,
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
            pid: u32::MAX,
            process_start_time_ticks: 1,
            boot_id: fs::read_to_string("/proc/sys/kernel/random/boot_id")
                .unwrap()
                .trim()
                .to_owned(),
            installation_id,
            expected_apprun_path: expected_apprun.to_str().unwrap().to_owned(),
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
}
