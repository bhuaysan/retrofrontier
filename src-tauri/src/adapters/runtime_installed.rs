use crate::adapters::runtime_integrity::{sha256_bytes, sha256_file};
use crate::adapters::runtime_paths::{fsync_directory, RuntimePaths};
use crate::domain::runtime::{
    parse_strict_json, serialize_json, CompleteMarker, InstalledEntry, InstalledEntryType,
    RelativePath, RuntimeError, RuntimeManifest, SymlinkTarget, VerifiedRuntimeManifest,
    MAX_DETACHED_INVENTORY_BYTES, MAX_MANIFEST_BYTES,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;

pub const RELEASE_MANIFEST_FILE: &str = "release-manifest.json";
pub const COMPLETE_MARKER_FILE: &str = "complete.json";

/// Where an installation keeps the authenticated bytes of a *detached* installed-file inventory.
///
/// ADR-012 requires an installed runtime to stay verifiable and launchable with no network, so a
/// detached inventory cannot be re-fetched at verification time. The exact authenticated bytes are
/// therefore stored beside the manifest, and every later read re-checks them against the length and
/// SHA-256 the authenticated manifest binds — which makes this file a cache of authenticated data,
/// never an authority of its own. It is absent for an inline release.
pub const RELEASE_INVENTORY_FILE: &str = "release-inventory.json";

#[derive(Debug, Clone)]
pub struct VerifiedInstallation {
    pub installation_id: crate::domain::runtime::SafeIdentifier,
    pub manifest: VerifiedRuntimeManifest,
    pub manifest_sha256: crate::domain::runtime::Sha256Digest,
    pub storage_bytes: u64,
}

/// Write the installation's release metadata: the manifest, plus the detached inventory bytes when
/// the manifest declares one.
///
/// `inventory_bytes` must be `Some` exactly when the manifest declares a detached inventory;
/// `read_manifest` re-resolves both files immediately afterwards, so a mismatch fails here rather
/// than at first launch.
pub fn write_release_metadata(
    path: &Path,
    manifest_bytes: &[u8],
    inventory_bytes: Option<&[u8]>,
) -> Result<(), RuntimeError> {
    if manifest_bytes.len() as u64 > MAX_MANIFEST_BYTES {
        return Err(RuntimeError::InstalledTree(
            "release manifest is too large".to_owned(),
        ));
    }
    let manifest = RuntimeManifest::parse(manifest_bytes)?;
    write_private_file(&path.join(RELEASE_MANIFEST_FILE), manifest_bytes)?;
    match (manifest.release.inventory.detached(), inventory_bytes) {
        (Some(_), Some(bytes)) => {
            if bytes.len() as u64 > MAX_DETACHED_INVENTORY_BYTES {
                return Err(RuntimeError::InstalledTree(
                    "detached release inventory is too large".to_owned(),
                ));
            }
            write_private_file(&path.join(RELEASE_INVENTORY_FILE), bytes)?;
        }
        (None, None) => {}
        (Some(_), None) => {
            return Err(RuntimeError::InstalledTree(
                "the release declares a detached inventory but none was staged".to_owned(),
            ))
        }
        (None, Some(_)) => {
            return Err(RuntimeError::InstalledTree(
                "a detached inventory was staged for an inline release".to_owned(),
            ))
        }
    }
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

fn write_private_file(target: &Path, bytes: &[u8]) -> Result<(), RuntimeError> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true).mode(0o600);
    let mut file = options.open(target)?;
    file.write_all(bytes)?;
    file.flush()?;
    file.sync_all()?;
    Ok(())
}

/// Read and resolve the installed release metadata.
///
/// The returned digest is always the *manifest's* digest, because the manifest is what `active.json`
/// and the completion marker bind. A detached inventory adds no second identity: it is accepted only
/// when its length and digest equal what the manifest already declared.
pub fn read_manifest(
    path: &Path,
) -> Result<
    (
        VerifiedRuntimeManifest,
        crate::domain::runtime::Sha256Digest,
    ),
    RuntimeError,
> {
    let bytes = read_metadata_file(
        &path.join(RELEASE_MANIFEST_FILE),
        MAX_MANIFEST_BYTES,
        "installed release manifest",
    )?;
    let manifest = RuntimeManifest::parse(&bytes)?;
    let manifest_sha256 = sha256_bytes(&bytes);
    let verified = match manifest.release.inventory.detached() {
        None => VerifiedRuntimeManifest::from_inline(manifest)?,
        Some(reference) => {
            // Bounded by the manifest's own reference, which structural validation already held
            // to `MAX_DETACHED_INVENTORY_BYTES`. A file of any other length is refused before it
            // is read at all.
            let expected = reference.size_bytes;
            let inventory_bytes = read_metadata_file(
                &path.join(RELEASE_INVENTORY_FILE),
                expected,
                "installed release inventory",
            )?;
            VerifiedRuntimeManifest::from_detached_bytes(manifest, &inventory_bytes)?
        }
    };
    Ok((verified, manifest_sha256))
}

/// Read one installation metadata file under an explicit byte bound.
///
/// The bound is enforced twice on purpose. The `stat` refuses an oversized file before it is
/// opened, and the read itself is capped, so a file that grows between the two — the same-user race
/// ADR-012 acknowledges it cannot eliminate — still cannot make this read unbounded.
fn read_metadata_file(target: &Path, max_bytes: u64, label: &str) -> Result<Vec<u8>, RuntimeError> {
    let metadata = fs::symlink_metadata(target)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(RuntimeError::InstalledTree(format!(
            "{label} is not a regular file"
        )));
    }
    if metadata.len() > max_bytes {
        return Err(RuntimeError::InstalledTree(format!("{label} is too large")));
    }
    let mut bytes = Vec::with_capacity(usize::try_from(metadata.len()).unwrap_or(0));
    std::io::Read::take(fs::File::open(target)?, max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max_bytes {
        return Err(RuntimeError::InstalledTree(format!("{label} is too large")));
    }
    Ok(bytes)
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

pub fn verify_tree(path: &Path, manifest: &VerifiedRuntimeManifest) -> Result<(), RuntimeError> {
    let expected: BTreeMap<_, _> = manifest
        .inventory()
        .iter()
        .map(|entry| (entry.path.clone(), entry))
        .collect();
    let mut actual = BTreeSet::new();
    // Installation metadata is not payload. `release-inventory.json` is skipped only for a release
    // that actually declares a detached inventory, so an inline installation still refuses a file
    // by that name as an unexpected tree entry.
    let metadata_files: &[&str] = if manifest.release.inventory.is_detached() {
        &[
            RELEASE_MANIFEST_FILE,
            COMPLETE_MARKER_FILE,
            RELEASE_INVENTORY_FILE,
        ]
    } else {
        &[RELEASE_MANIFEST_FILE, COMPLETE_MARKER_FILE]
    };
    walk_tree(path, Path::new(""), &mut actual, &expected, metadata_files)?;
    for entry in manifest.inventory() {
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
    metadata_files: &[&str],
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
        if relative.as_os_str().is_empty() && metadata_files.contains(&name.as_str()) {
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
                walk_tree(root, &relative.join(name), actual, expected, metadata_files)?;
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

fn validate_symlink_graph(manifest: &VerifiedRuntimeManifest) -> Result<(), RuntimeError> {
    let expected: BTreeMap<_, _> = manifest
        .inventory()
        .iter()
        .map(|entry| (entry.path.clone(), entry))
        .collect();
    for entry in manifest.inventory() {
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
    manifest: &VerifiedRuntimeManifest,
) -> Result<(), RuntimeError> {
    for entry in manifest.inventory() {
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
