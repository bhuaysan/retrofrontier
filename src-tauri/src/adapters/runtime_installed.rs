use crate::adapters::runtime_integrity::{sha256_bytes, sha256_file};
use crate::adapters::runtime_paths::{fsync_directory, RuntimePaths};
use crate::domain::runtime::{
    parse_strict_json, serialize_json, CompleteMarker, InstalledEntry, InstalledEntryType,
    RelativePath, RuntimeError, RuntimeManifest, SymlinkTarget, MAX_MANIFEST_BYTES,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;

pub const RELEASE_MANIFEST_FILE: &str = "release-manifest.json";
pub const COMPLETE_MARKER_FILE: &str = "complete.json";

#[derive(Debug, Clone)]
pub struct VerifiedInstallation {
    pub installation_id: crate::domain::runtime::SafeIdentifier,
    pub manifest: RuntimeManifest,
    pub manifest_sha256: crate::domain::runtime::Sha256Digest,
    pub storage_bytes: u64,
}

pub fn write_release_manifest(path: &Path, manifest_bytes: &[u8]) -> Result<(), RuntimeError> {
    if manifest_bytes.len() as u64 > MAX_MANIFEST_BYTES {
        return Err(RuntimeError::InstalledTree(
            "release manifest is too large".to_owned(),
        ));
    }
    let manifest = RuntimeManifest::parse(manifest_bytes)?;
    let target = path.join(RELEASE_MANIFEST_FILE);
    let mut options = OpenOptions::new();
    options.write(true).create_new(true).mode(0o600);
    let mut file = options.open(&target)?;
    file.write_all(manifest_bytes)?;
    file.flush()?;
    file.sync_all()?;
    drop(file);
    let parsed = read_manifest(path)?;
    if parsed.0.release.release_id != manifest.release.release_id
        || parsed.1 != sha256_bytes(manifest_bytes)
    {
        return Err(RuntimeError::InstalledTree(
            "release manifest changed while being written".to_owned(),
        ));
    }
    fsync_directory(path)?;
    Ok(())
}

pub fn read_manifest(
    path: &Path,
) -> Result<(RuntimeManifest, crate::domain::runtime::Sha256Digest), RuntimeError> {
    let metadata = fs::symlink_metadata(path.join(RELEASE_MANIFEST_FILE))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(RuntimeError::InstalledTree(
            "installed release manifest is not a regular file".to_owned(),
        ));
    }
    if metadata.len() > MAX_MANIFEST_BYTES {
        return Err(RuntimeError::InstalledTree(
            "installed release manifest is too large".to_owned(),
        ));
    }
    let bytes = fs::read(path.join(RELEASE_MANIFEST_FILE))?;
    let manifest = RuntimeManifest::parse(&bytes)?;
    Ok((manifest, sha256_bytes(&bytes)))
}

pub fn write_complete_marker(
    path: &Path,
    installation_id: &crate::domain::runtime::SafeIdentifier,
    manifest_sha256: crate::domain::runtime::Sha256Digest,
) -> Result<(), RuntimeError> {
    let marker = CompleteMarker {
        schema_version: crate::domain::runtime::COMPLETE_MARKER_SCHEMA_VERSION,
        installation_id: installation_id.clone(),
        manifest_sha256,
    };
    let bytes = serialize_json(&marker)?;
    let target = path.join(COMPLETE_MARKER_FILE);
    let mut options = OpenOptions::new();
    options.write(true).create_new(true).mode(0o600);
    let mut file = options.open(&target)?;
    file.write_all(&bytes)?;
    file.flush()?;
    file.sync_all()?;
    drop(file);
    let parsed = read_complete_marker(path)?;
    if parsed.installation_id != *installation_id || parsed.manifest_sha256 != manifest_sha256 {
        return Err(RuntimeError::InstalledTree(
            "completion marker changed while being written".to_owned(),
        ));
    }
    fsync_directory(path)?;
    Ok(())
}

pub fn read_complete_marker(path: &Path) -> Result<CompleteMarker, RuntimeError> {
    let marker_path = path.join(COMPLETE_MARKER_FILE);
    let metadata = fs::symlink_metadata(&marker_path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(RuntimeError::InstalledTree(
            "completion marker is not a regular file".to_owned(),
        ));
    }
    if metadata.len() > 4096 {
        return Err(RuntimeError::InstalledTree(
            "completion marker is too large".to_owned(),
        ));
    }
    let bytes = fs::read(marker_path)?;
    let marker: CompleteMarker =
        parse_strict_json(&bytes).map_err(|error| RuntimeError::InstalledTree(error.to_owned()))?;
    marker.validate()?;
    Ok(marker)
}

pub fn verify_installation(
    paths: &RuntimePaths,
    installation_id: &crate::domain::runtime::SafeIdentifier,
    expected_manifest: &RuntimeManifest,
    expected_manifest_sha256: crate::domain::runtime::Sha256Digest,
) -> Result<VerifiedInstallation, RuntimeError> {
    let path = paths.version_path(installation_id);
    if !paths.is_owned_version_path(&path) {
        return Err(RuntimeError::InstalledTree(
            "installation path is outside the versions root".to_owned(),
        ));
    }
    let metadata = fs::symlink_metadata(&path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(RuntimeError::InstalledTree(
            "installation is not a real directory".to_owned(),
        ));
    }
    let (manifest, manifest_sha256) = read_manifest(&path)?;
    if manifest_sha256 != expected_manifest_sha256
        || manifest.release.release_id != expected_manifest.release.release_id
        || manifest.release.release_sequence != expected_manifest.release.release_sequence
    {
        return Err(RuntimeError::InstalledTree(
            "installed manifest does not match the trusted release".to_owned(),
        ));
    }
    let marker = read_complete_marker(&path)?;
    if marker.installation_id != *installation_id || marker.manifest_sha256 != manifest_sha256 {
        return Err(RuntimeError::InstalledTree(
            "completion marker does not match the installation".to_owned(),
        ));
    }
    verify_tree(&path, &manifest)?;
    let storage_bytes = directory_size(&path)?;
    Ok(VerifiedInstallation {
        installation_id: installation_id.clone(),
        manifest,
        manifest_sha256,
        storage_bytes,
    })
}

pub fn verify_tree(path: &Path, manifest: &RuntimeManifest) -> Result<(), RuntimeError> {
    let expected: BTreeMap<_, _> = manifest
        .release
        .inventory
        .iter()
        .map(|entry| (entry.path.clone(), entry))
        .collect();
    let mut actual = BTreeSet::new();
    walk_tree(path, Path::new(""), &mut actual, &expected)?;
    for entry in &manifest.release.inventory {
        if !actual.contains(&entry.path) {
            return Err(RuntimeError::InstalledTree(format!(
                "installed runtime is missing '{}'",
                entry.path
            )));
        }
    }
    if actual.len() != expected.len() {
        return Err(RuntimeError::InstalledTree(
            "installed runtime contains unexpected files".to_owned(),
        ));
    }
    validate_symlink_graph(manifest)?;
    Ok(())
}

fn walk_tree(
    root: &Path,
    relative: &Path,
    actual: &mut BTreeSet<RelativePath>,
    expected: &BTreeMap<RelativePath, &InstalledEntry>,
) -> Result<(), RuntimeError> {
    let directory = root.join(relative);
    let metadata = fs::symlink_metadata(&directory)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(RuntimeError::InstalledTree(format!(
            "runtime tree parent is not a real directory: {}",
            directory.display()
        )));
    }
    for item in fs::read_dir(&directory)? {
        let item = item?;
        let name = item
            .file_name()
            .to_str()
            .ok_or_else(|| RuntimeError::InstalledTree("runtime filename is not UTF-8".to_owned()))?
            .to_owned();
        if relative.as_os_str().is_empty()
            && matches!(name.as_str(), RELEASE_MANIFEST_FILE | COMPLETE_MARKER_FILE)
        {
            continue;
        }
        let child_relative = if relative.as_os_str().is_empty() {
            RelativePath::new(name.clone())
        } else {
            RelativePath::new(format!("{}/{}", relative.display(), name))
        }
        .map_err(|_| {
            RuntimeError::InstalledTree("runtime tree contains an unsafe path".to_owned())
        })?;
        let metadata = fs::symlink_metadata(item.path())?;
        let Some(expected_entry) = expected.get(&child_relative) else {
            return Err(RuntimeError::InstalledTree(format!(
                "runtime tree contains unexpected '{}'",
                child_relative
            )));
        };
        match expected_entry.entry_type {
            InstalledEntryType::File => {
                if metadata.file_type().is_symlink() || !metadata.is_file() {
                    return Err(RuntimeError::InstalledTree(format!(
                        "'{}' is not the expected regular file",
                        child_relative
                    )));
                }
                if metadata.len() != expected_entry.size_bytes {
                    return Err(RuntimeError::InstalledTree(format!(
                        "file '{}' has the wrong size",
                        child_relative
                    )));
                }
                let (_, hash) = sha256_file(&item.path())?;
                if expected_entry.sha256 != Some(hash) {
                    return Err(RuntimeError::InstalledTree(format!(
                        "file '{}' has the wrong SHA-256",
                        child_relative
                    )));
                }
                let executable = metadata.permissions().mode() & 0o111 != 0;
                if executable != expected_entry.executable {
                    return Err(RuntimeError::InstalledTree(format!(
                        "file '{}' has the wrong executable mode",
                        child_relative
                    )));
                }
            }
            InstalledEntryType::Directory => {
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err(RuntimeError::InstalledTree(format!(
                        "'{}' is not the expected directory",
                        child_relative
                    )));
                }
                walk_tree(root, &relative.join(name), actual, expected)?;
            }
            InstalledEntryType::Symlink => {
                if !metadata.file_type().is_symlink() {
                    return Err(RuntimeError::InstalledTree(format!(
                        "'{}' is not the expected symlink",
                        child_relative
                    )));
                }
                let target = fs::read_link(item.path())?;
                let target = target.to_str().ok_or_else(|| {
                    RuntimeError::InstalledTree("runtime symlink target is not UTF-8".to_owned())
                })?;
                let target = SymlinkTarget::new(target.to_owned()).map_err(|_| {
                    RuntimeError::InstalledTree(format!(
                        "symlink '{}' has an unsafe target",
                        child_relative
                    ))
                })?;
                if expected_entry.link_target.as_ref() != Some(&target) {
                    return Err(RuntimeError::InstalledTree(format!(
                        "symlink '{}' has the wrong target",
                        child_relative
                    )));
                }
                let resolved = resolve_link(&child_relative, &target)?;
                if !expected.contains_key(&resolved) {
                    return Err(RuntimeError::InstalledTree(format!(
                        "symlink '{}' escapes the trusted tree",
                        child_relative
                    )));
                }
            }
        }
        actual.insert(child_relative);
    }
    Ok(())
}

fn validate_symlink_graph(manifest: &RuntimeManifest) -> Result<(), RuntimeError> {
    let expected: BTreeMap<_, _> = manifest
        .release
        .inventory
        .iter()
        .map(|entry| (entry.path.clone(), entry))
        .collect();
    for entry in &manifest.release.inventory {
        if entry.entry_type != InstalledEntryType::Symlink {
            continue;
        }
        let mut current = entry.path.clone();
        let mut visited = BTreeSet::new();
        for _ in 0..32 {
            if !visited.insert(current.clone()) {
                return Err(RuntimeError::Manifest(format!(
                    "symlink graph contains a cycle at '{}'",
                    entry.path
                )));
            }
            let Some(next) = expected.get(&current) else {
                break;
            };
            if next.entry_type != InstalledEntryType::Symlink {
                break;
            }
            let target = next
                .link_target
                .as_ref()
                .ok_or_else(|| RuntimeError::Manifest("symlink has no target".to_owned()))?;
            current = resolve_link(&current, target)?;
        }
    }
    Ok(())
}

fn resolve_link(path: &RelativePath, target: &SymlinkTarget) -> Result<RelativePath, RuntimeError> {
    let mut components = Vec::new();
    if let Some(parent) = path.parent() {
        components.extend(parent.as_str().split('/').map(str::to_owned));
    }
    components.extend(target.as_str().split('/').map(str::to_owned));
    let mut normalized = Vec::new();
    for component in components {
        match component.as_str() {
            "." => {}
            ".." => {
                if normalized.pop().is_none() {
                    return Err(RuntimeError::InstalledTree(
                        "symlink target escapes the installation root".to_owned(),
                    ));
                }
            }
            value => normalized.push(value.to_owned()),
        }
    }
    RelativePath::new(normalized.join("/"))
}

pub fn validate_app_run(path: &Path, manifest: &RuntimeManifest) -> Result<(), RuntimeError> {
    let app_run = path.join(manifest.app_run_path().to_path_buf());
    let metadata = fs::symlink_metadata(&app_run)?;
    if !metadata.file_type().is_symlink() && !metadata.is_file() {
        return Err(RuntimeError::InstalledTree(
            "authenticated AppRun is not a file or symlink".to_owned(),
        ));
    }
    if metadata.file_type().is_symlink() {
        let target = fs::read_link(&app_run)?;
        let target = target.to_str().ok_or_else(|| {
            RuntimeError::InstalledTree("AppRun symlink target is not UTF-8".to_owned())
        })?;
        let target = SymlinkTarget::new(target.to_owned()).map_err(|_| {
            RuntimeError::InstalledTree("AppRun symlink target is unsafe".to_owned())
        })?;
        let resolved = resolve_link(manifest.app_run_path(), &target)?;
        let final_path = path.join(resolved.to_path_buf());
        let final_metadata = fs::symlink_metadata(final_path)?;
        if final_metadata.file_type().is_symlink()
            || !final_metadata.is_file()
            || final_metadata.permissions().mode() & 0o111 == 0
        {
            return Err(RuntimeError::InstalledTree(
                "AppRun symlink does not resolve to an executable file".to_owned(),
            ));
        }
    } else if metadata.permissions().mode() & 0o111 == 0 {
        return Err(RuntimeError::InstalledTree(
            "authenticated AppRun is not executable".to_owned(),
        ));
    }
    Ok(())
}

pub fn apply_inventory_permissions(
    path: &Path,
    manifest: &RuntimeManifest,
) -> Result<(), RuntimeError> {
    for entry in &manifest.release.inventory {
        let target = path.join(entry.path.to_path_buf());
        match entry.entry_type {
            InstalledEntryType::File => {
                let mode = if entry.executable { 0o755 } else { 0o644 };
                fs::set_permissions(target, fs::Permissions::from_mode(mode))?;
            }
            InstalledEntryType::Directory => {
                fs::set_permissions(target, fs::Permissions::from_mode(0o755))?;
            }
            InstalledEntryType::Symlink => {}
        }
    }
    Ok(())
}

pub fn directory_size(path: &Path) -> Result<u64, RuntimeError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Ok(0);
    }
    if metadata.is_file() {
        return Ok(metadata.len());
    }
    let mut total = 0_u64;
    for entry in fs::read_dir(path)? {
        total = total.saturating_add(directory_size(&entry?.path())?);
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::{read_complete_marker, write_complete_marker};
    use crate::domain::runtime::{SafeIdentifier, Sha256Digest};
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn completion_marker_is_written_and_strictly_read() {
        let directory = tempdir().unwrap();
        let id: SafeIdentifier = "install-1".try_into().unwrap();
        let digest = Sha256Digest::from_hex(&"a".repeat(64)).unwrap();
        write_complete_marker(directory.path(), &id, digest).unwrap();
        let marker = read_complete_marker(directory.path()).unwrap();
        assert_eq!(marker.installation_id, id);
        assert_eq!(marker.manifest_sha256, digest);
        fs::write(directory.path().join("complete.json"), b"{}\n").unwrap();
        assert!(read_complete_marker(directory.path()).is_err());
    }
}
