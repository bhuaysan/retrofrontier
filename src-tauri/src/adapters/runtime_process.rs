use crate::adapters::runtime_paths::RuntimePaths;
use crate::adapters::runtime_trust::atomic_replace;
use crate::domain::runtime::{
    parse_strict_json, ManagedProcessRecord, RuntimeError, SafeIdentifier,
};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub trait ManagedProcessInspector: Send + Sync {
    fn ensure_no_active_game(&self, paths: &RuntimePaths) -> Result<(), RuntimeError>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct LinuxManagedProcessInspector;

impl ManagedProcessInspector for LinuxManagedProcessInspector {
    fn ensure_no_active_game(&self, paths: &RuntimePaths) -> Result<(), RuntimeError> {
        let Some(record) = read_process_record(paths)? else {
            return Ok(());
        };
        if !record_targets_runtime(paths, &record) {
            return Err(RuntimeError::GameActive);
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
    let record: ManagedProcessRecord =
        parse_strict_json(&bytes).map_err(|_| RuntimeError::GameActive)?;
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
        Ok(_) => {
            fs::remove_file(paths.game_process_record())?;
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(RuntimeError::Io(error)),
    }
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
        schema_version: 1,
        phase,
        pid,
        process_start_time_ticks: process_start_time_ticks(pid)?,
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

#[cfg(test)]
mod tests {
    use super::{parse_start_time_ticks, LinuxManagedProcessInspector, ManagedProcessInspector};
    use crate::adapters::runtime_paths::RuntimePaths;
    use crate::domain::runtime::{ManagedProcessPhase, ManagedProcessRecord, SafeIdentifier};
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn proc_stat_parser_handles_parentheses_in_process_names() {
        let stat = "42 (retro)arch) S 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 12345";
        assert_eq!(parse_start_time_ticks(stat), Some(12345));
    }

    #[test]
    fn malformed_or_mismatched_process_records_block_concurrent_mutation() {
        let directory = tempdir().unwrap();
        let paths = RuntimePaths::new(directory.path());
        paths.prepare().unwrap();
        fs::write(paths.game_process_record(), b"not json").unwrap();
        assert!(matches!(
            LinuxManagedProcessInspector.ensure_no_active_game(&paths),
            Err(crate::domain::runtime::RuntimeError::GameActive)
        ));

        fs::remove_file(paths.game_process_record()).unwrap();
        let installation_id: SafeIdentifier = "install-1".try_into().unwrap();
        let installation = paths.version_path(&installation_id);
        fs::create_dir_all(&installation).unwrap();
        let apprun = installation.join("AppRun");
        fs::copy(std::env::current_exe().unwrap(), &apprun).unwrap();
        let record = ManagedProcessRecord {
            schema_version: 1,
            phase: ManagedProcessPhase::Running,
            pid: std::process::id(),
            process_start_time_ticks: super::process_start_time_ticks(std::process::id()).unwrap(),
            installation_id,
            expected_apprun_path: apprun.to_str().unwrap().to_owned(),
            expected_executable_path: Some("/bin/sh".to_owned()),
        };
        super::write_process_record(&paths, &record).unwrap();
        assert!(matches!(
            LinuxManagedProcessInspector.ensure_no_active_game(&paths),
            Err(crate::domain::runtime::RuntimeError::GameActive)
        ));
    }
}
