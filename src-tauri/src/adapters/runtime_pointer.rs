use crate::adapters::runtime_paths::{fsync_directory, RuntimePaths};
use crate::domain::runtime::{
    parse_strict_json, serialize_json, ActivePointer, RuntimeError, MAX_ACTIVE_POINTER_BYTES,
};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

static POINTER_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn read_active_pointer(paths: &RuntimePaths) -> Result<Option<ActivePointer>, RuntimeError> {
    let path = paths.active_pointer();
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(RuntimeError::Io(error)),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(RuntimeError::Pointer(
            "active.json is not a regular file".to_owned(),
        ));
    }
    let bytes = read_bounded(path, MAX_ACTIVE_POINTER_BYTES)?;
    let pointer: ActivePointer =
        parse_strict_json(&bytes).map_err(|error| RuntimeError::Pointer(error.to_owned()))?;
    pointer.validate()?;
    Ok(Some(pointer))
}

/// Replace active.json using the Linux durability protocol from ADR-011.
pub fn write_active_pointer(
    paths: &RuntimePaths,
    pointer: &ActivePointer,
) -> Result<(), RuntimeError> {
    // TODO(Sol Max review): re-audit durability assumptions, filesystem semantics, and recovery
    // behavior if this pointer becomes the basis for production rollback guarantees.
    pointer.validate()?;
    let bytes = serialize_json(pointer)?;
    if bytes.len() as u64 > MAX_ACTIVE_POINTER_BYTES {
        return Err(RuntimeError::Pointer(
            "active pointer exceeds its maximum size".to_owned(),
        ));
    }
    let parent = paths.runtime_root();
    ensure_real_directory(parent)?;
    let temporary = unique_temporary_path(parent, paths.active_pointer())?;
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options.open(&temporary)?;
        file.write_all(&bytes)?;
        file.flush()?;
        file.sync_all()?;
        drop(file);

        let reopened = read_bounded(&temporary, MAX_ACTIVE_POINTER_BYTES)?;
        let parsed: ActivePointer = parse_strict_json(&reopened)
            .map_err(|error| RuntimeError::Pointer(error.to_owned()))?;
        parsed.validate()?;
        ensure_same_pointer(&parsed, pointer)?;

        fs::rename(&temporary, paths.active_pointer())?;
        fsync_directory(parent)?;

        let final_bytes = read_bounded(paths.active_pointer(), MAX_ACTIVE_POINTER_BYTES)?;
        let final_pointer: ActivePointer = parse_strict_json(&final_bytes)
            .map_err(|error| RuntimeError::Pointer(error.to_owned()))?;
        final_pointer.validate()?;
        ensure_same_pointer(&final_pointer, pointer)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub fn remove_pointer_temporary_files(paths: &RuntimePaths) -> Result<(), RuntimeError> {
    remove_temporary_files(
        paths.runtime_root(),
        &[
            ".active.json.tmp-",
            ".game-process.json.tmp-",
            ".game-process.json.invalid-",
        ],
    )?;
    remove_temporary_files(paths.trust_root(), &[".trust-state.json.tmp-"])?;
    Ok(())
}

fn remove_temporary_files(root: &Path, prefixes: &[&str]) -> Result<(), RuntimeError> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !prefixes.iter().any(|prefix| name.starts_with(prefix)) {
            continue;
        }
        let metadata = fs::symlink_metadata(entry.path())?;
        if metadata.file_type().is_symlink() {
            return Err(RuntimeError::Pointer(
                "managed runtime state temporary is a symlink".to_owned(),
            ));
        }
        if metadata.is_file() {
            fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}

fn ensure_real_directory(path: &Path) -> Result<(), RuntimeError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(RuntimeError::Pointer(format!(
            "active pointer parent is not a real directory: {}",
            path.display()
        )));
    }
    Ok(())
}

fn read_bounded(path: &Path, max_size: u64) -> Result<Vec<u8>, RuntimeError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(RuntimeError::Pointer(format!(
            "pointer file is not a regular file: {}",
            path.display()
        )));
    }
    if metadata.len() > max_size {
        return Err(RuntimeError::Pointer(format!(
            "pointer file exceeds {max_size} bytes"
        )));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    File::open(path)?
        .take(max_size.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max_size {
        return Err(RuntimeError::Pointer(format!(
            "pointer file exceeds {max_size} bytes"
        )));
    }
    Ok(bytes)
}

fn unique_temporary_path(parent: &Path, target: &Path) -> Result<PathBuf, RuntimeError> {
    let name = target
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| RuntimeError::Pointer("active pointer filename is not UTF-8".to_owned()))?;
    let counter = POINTER_COUNTER.fetch_add(1, Ordering::Relaxed);
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    Ok(parent.join(format!(
        ".{name}.tmp-{}-{counter}-{stamp}",
        std::process::id()
    )))
}

fn ensure_same_pointer(
    actual: &ActivePointer,
    expected: &ActivePointer,
) -> Result<(), RuntimeError> {
    if actual.schema_version != expected.schema_version
        || actual.installation_id != expected.installation_id
        || actual.manifest_sha256 != expected.manifest_sha256
    {
        return Err(RuntimeError::Pointer(
            "active pointer contents changed during replacement".to_owned(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{read_active_pointer, remove_pointer_temporary_files, write_active_pointer};
    use crate::adapters::runtime_paths::RuntimePaths;
    use crate::domain::runtime::{ActivePointer, Sha256Digest};
    use std::fs;
    use tempfile::tempdir;

    fn pointer() -> ActivePointer {
        ActivePointer {
            schema_version: 1,
            installation_id: "install-1".try_into().unwrap(),
            manifest_sha256: Sha256Digest::from_hex(&"a".repeat(64)).unwrap(),
        }
    }

    #[test]
    fn pointer_round_trip_and_corruption_are_explicit() {
        let directory = tempdir().unwrap();
        let paths = RuntimePaths::new(directory.path());
        paths.prepare().unwrap();
        write_active_pointer(&paths, &pointer()).unwrap();
        assert!(read_active_pointer(&paths).unwrap().is_some());
        fs::write(paths.active_pointer(), b"{\"schema_version\":1}").unwrap();
        assert!(read_active_pointer(&paths).is_err());
    }

    #[test]
    fn no_pointer_is_not_inferred() {
        let directory = tempdir().unwrap();
        let paths = RuntimePaths::new(directory.path());
        paths.prepare().unwrap();
        assert!(read_active_pointer(&paths).unwrap().is_none());
    }

    #[test]
    fn pointer_rejects_oversize_and_unsafe_installation_ids() {
        let directory = tempdir().unwrap();
        let paths = RuntimePaths::new(directory.path());
        paths.prepare().unwrap();

        fs::write(paths.active_pointer(), vec![b'x'; 4097]).unwrap();
        assert!(read_active_pointer(&paths).is_err());

        fs::write(
            paths.active_pointer(),
            br#"{"schema_version":1,"installation_id":"../escape","manifest_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}"#,
        )
        .unwrap();
        assert!(read_active_pointer(&paths).is_err());
    }

    #[test]
    fn crash_leftover_is_removed_and_final_pointer_is_exact() {
        let directory = tempdir().unwrap();
        let paths = RuntimePaths::new(directory.path());
        paths.prepare().unwrap();
        fs::write(
            paths.runtime_root().join(".active.json.tmp-crash"),
            b"partial",
        )
        .unwrap();
        fs::write(
            paths.trust_root().join(".trust-state.json.tmp-crash"),
            b"partial",
        )
        .unwrap();
        fs::write(
            paths.runtime_root().join(".game-process.json.tmp-crash"),
            b"partial",
        )
        .unwrap();
        fs::write(
            paths
                .runtime_root()
                .join(".game-process.json.invalid-crash"),
            b"invalid",
        )
        .unwrap();
        remove_pointer_temporary_files(&paths).unwrap();
        assert!(!paths.runtime_root().join(".active.json.tmp-crash").exists());
        assert!(!paths
            .trust_root()
            .join(".trust-state.json.tmp-crash")
            .exists());
        assert!(!paths
            .runtime_root()
            .join(".game-process.json.tmp-crash")
            .exists());
        assert!(!paths
            .runtime_root()
            .join(".game-process.json.invalid-crash")
            .exists());

        let first = pointer();
        write_active_pointer(&paths, &first).unwrap();
        let second = ActivePointer {
            installation_id: "install-2".try_into().unwrap(),
            ..first
        };
        write_active_pointer(&paths, &second).unwrap();
        assert_eq!(
            read_active_pointer(&paths)
                .unwrap()
                .unwrap()
                .installation_id,
            second.installation_id
        );
    }
}
