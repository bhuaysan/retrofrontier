use crate::adapters::runtime_integrity::copy_with_limit;
use crate::adapters::runtime_paths::{ensure_empty_directory, fsync_directory};
use crate::domain::runtime::{
    ArchiveFormat, ExtractionLimits, InstalledEntry, InstalledEntryType, RelativePath,
    RuntimeComponent, RuntimeError, SymlinkTarget,
};
use backhand::{FilesystemReader, InnerNode};
use sevenz_rust2::{ArchiveReader, Password};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use zip::ZipArchive;

/// Extraction is intentionally a small, synchronous adapter. RuntimeManager only calls it with
/// a private staging destination and never with a finalized installation.
pub trait RuntimeArchiveExtractor: Send + Sync {
    fn extract(
        &self,
        component: &RuntimeComponent,
        artifact: &Path,
        destination: &Path,
        inventory: &[InstalledEntry],
        limits: &ExtractionLimits,
    ) -> Result<(), RuntimeError>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct LinuxRuntimeArchiveExtractor;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MemberType {
    File,
    Directory,
    Symlink,
}

#[derive(Debug, Clone)]
struct ArchiveMember {
    path: RelativePath,
    member_type: MemberType,
    size: u64,
    compressed_size: u64,
    link_target: Option<String>,
}

impl RuntimeArchiveExtractor for LinuxRuntimeArchiveExtractor {
    fn extract(
        &self,
        component: &RuntimeComponent,
        artifact: &Path,
        destination: &Path,
        inventory: &[InstalledEntry],
        limits: &ExtractionLimits,
    ) -> Result<(), RuntimeError> {
        // TODO(Sol Max review): re-audit archive parser behavior, path normalization, and symlink
        // handling before enabling production runtime artifacts.
        limits.validate()?;
        ensure_empty_directory(destination)?;
        match component.archive_format {
            ArchiveFormat::Tar => extract_tar(component, artifact, destination, inventory, limits),
            ArchiveFormat::Zip => extract_zip(component, artifact, destination, inventory, limits),
            ArchiveFormat::SevenZip => {
                extract_seven_zip(component, artifact, destination, inventory, limits)
            }
            ArchiveFormat::AppImage => {
                extract_appimage(component, artifact, destination, inventory, limits)
            }
        }
    }
}

fn extract_tar(
    component: &RuntimeComponent,
    artifact: &Path,
    destination: &Path,
    inventory: &[InstalledEntry],
    limits: &ExtractionLimits,
) -> Result<(), RuntimeError> {
    let artifact_size = fs::metadata(artifact)?.len();
    let mut archive = tar::Archive::new(BufReader::new(File::open(artifact)?));
    let mut members = Vec::new();
    for entry in archive
        .entries()
        .map_err(|error| RuntimeError::Extraction(format!("tar could not be read: {error}")))?
    {
        let entry = entry.map_err(|error| {
            RuntimeError::Extraction(format!("tar entry could not be read: {error}"))
        })?;
        let entry_type = entry.header().entry_type();
        let member_type = if entry_type.is_dir() {
            MemberType::Directory
        } else if entry_type.is_file() {
            MemberType::File
        } else if entry_type.is_symlink() {
            MemberType::Symlink
        } else if entry_type.is_hard_link() {
            return Err(RuntimeError::Extraction(
                "tar hard links are not accepted in runtime artifacts".to_owned(),
            ));
        } else {
            return Err(RuntimeError::Extraction(
                "tar special entries are not accepted in runtime artifacts".to_owned(),
            ));
        };
        let path = archive_path(
            &entry.path().map_err(|error| {
                RuntimeError::Extraction(format!("tar path is invalid: {error}"))
            })?,
            member_type == MemberType::Directory,
            limits,
        )?;
        let link_target = if member_type == MemberType::Symlink {
            Some(
                entry
                    .link_name()
                    .map_err(|error| {
                        RuntimeError::Extraction(format!("tar link target is invalid: {error}"))
                    })?
                    .ok_or_else(|| {
                        RuntimeError::Extraction("tar symlink has no target".to_owned())
                    })?
                    .to_str()
                    .ok_or_else(|| {
                        RuntimeError::Extraction("tar symlink target is not UTF-8".to_owned())
                    })?
                    .to_owned(),
            )
        } else {
            None
        };
        members.push(ArchiveMember {
            path,
            member_type,
            size: if member_type == MemberType::File {
                entry.header().size()?
            } else {
                0
            },
            compressed_size: 0,
            link_target,
        });
    }
    validate_members(component, &members, artifact_size, inventory, limits)?;

    let mut archive = tar::Archive::new(BufReader::new(File::open(artifact)?));
    let entries = archive
        .entries()
        .map_err(|error| RuntimeError::Extraction(format!("tar could not be reopened: {error}")))?;
    let mut pending_links = Vec::new();
    for entry in entries {
        let mut entry = entry.map_err(|error| {
            RuntimeError::Extraction(format!("tar entry could not be read: {error}"))
        })?;
        let entry_type = entry.header().entry_type();
        let member_type = if entry_type.is_dir() {
            MemberType::Directory
        } else if entry_type.is_file() {
            MemberType::File
        } else {
            MemberType::Symlink
        };
        let path = archive_path(
            &entry.path().map_err(|error| {
                RuntimeError::Extraction(format!("tar path is invalid: {error}"))
            })?,
            member_type == MemberType::Directory,
            limits,
        )?;
        match member_type {
            MemberType::Directory => create_directory(destination, &path)?,
            MemberType::File => {
                let output = output_path(destination, &path)?;
                ensure_secure_parent(destination, &path)?;
                let expected_size = entry.header().size()?;
                let mut file = create_new_file(&output)?;
                let copied =
                    copy_with_limit(&mut entry, &mut file, expected_size.saturating_add(1))?;
                if copied != expected_size {
                    return Err(RuntimeError::Extraction(format!(
                        "tar file '{}' was truncated",
                        path
                    )));
                }
                file.sync_all()?;
            }
            MemberType::Symlink => {
                let link_target = entry
                    .link_name()
                    .map_err(|error| {
                        RuntimeError::Extraction(format!("tar link target is invalid: {error}"))
                    })?
                    .ok_or_else(|| {
                        RuntimeError::Extraction("tar symlink has no target".to_owned())
                    })?
                    .to_str()
                    .ok_or_else(|| {
                        RuntimeError::Extraction("tar symlink target is not UTF-8".to_owned())
                    })?
                    .to_owned();
                pending_links.push((path, link_target));
            }
        }
    }
    for (path, link_target) in pending_links {
        create_symlink(destination, &path, &link_target)?;
    }
    fsync_directory(destination)?;
    Ok(())
}

fn extract_zip(
    component: &RuntimeComponent,
    artifact: &Path,
    destination: &Path,
    inventory: &[InstalledEntry],
    limits: &ExtractionLimits,
) -> Result<(), RuntimeError> {
    let artifact_size = fs::metadata(artifact)?.len();
    let mut archive = ZipArchive::new(File::open(artifact)?)
        .map_err(|error| RuntimeError::Extraction(format!("zip could not be read: {error}")))?;
    let mut members = Vec::new();
    for index in 0..archive.len() {
        let entry = archive.by_index(index).map_err(|error| {
            RuntimeError::Extraction(format!("zip entry could not be read: {error}"))
        })?;
        if let Some(mode) = entry.unix_mode() {
            let file_type = mode & 0o170000;
            let expected_type = if entry.is_dir() { 0o040000 } else { 0o100000 };
            if file_type != 0 && file_type != expected_type {
                return Err(RuntimeError::Extraction(
                    "zip symlinks and special entries are not accepted in runtime artifacts"
                        .to_owned(),
                ));
            }
        }
        let member_type = if entry.is_dir() {
            MemberType::Directory
        } else {
            MemberType::File
        };
        let path = archive_path(entry.name(), member_type == MemberType::Directory, limits)?;
        members.push(ArchiveMember {
            path,
            member_type,
            size: if member_type == MemberType::File {
                entry.size()
            } else {
                0
            },
            compressed_size: entry.compressed_size(),
            link_target: None,
        });
    }
    validate_members(component, &members, artifact_size, inventory, limits)?;

    let mut archive = ZipArchive::new(File::open(artifact)?)
        .map_err(|error| RuntimeError::Extraction(format!("zip could not be reopened: {error}")))?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| {
            RuntimeError::Extraction(format!("zip entry could not be read: {error}"))
        })?;
        let member_type = if entry.is_dir() {
            MemberType::Directory
        } else {
            MemberType::File
        };
        let path = archive_path(entry.name(), member_type == MemberType::Directory, limits)?;
        match member_type {
            MemberType::Directory => create_directory(destination, &path)?,
            MemberType::File => {
                let output = output_path(destination, &path)?;
                ensure_secure_parent(destination, &path)?;
                let expected_size = entry.size();
                let mut file = create_new_file(&output)?;
                let copied =
                    copy_with_limit(&mut entry, &mut file, expected_size.saturating_add(1))?;
                if copied != expected_size {
                    return Err(RuntimeError::Extraction(format!(
                        "zip file '{}' was truncated",
                        path
                    )));
                }
                file.sync_all()?;
            }
            MemberType::Symlink => unreachable!("zip symlinks are rejected during preflight"),
        }
    }
    fsync_directory(destination)?;
    Ok(())
}

fn extract_seven_zip(
    component: &RuntimeComponent,
    artifact: &Path,
    destination: &Path,
    inventory: &[InstalledEntry],
    limits: &ExtractionLimits,
) -> Result<(), RuntimeError> {
    let Some(payload_filename) = component.payload_filename.as_ref() else {
        return extract_seven_zip_contents(component, artifact, destination, inventory, limits)
            .map(|_| ());
    };

    // Qualified Linux releases may be a 7z container around the AppImage. Keep the outer
    // payload in a sibling staging directory, then run the reviewed non-executing AppImage
    // extractor against that payload. It must never become an unverified final-tree file.
    let payload_root = artifact
        .parent()
        .ok_or_else(|| RuntimeError::Extraction("7z artifact has no staging parent".to_owned()))?
        .join(format!(
            ".{}-payload-{}",
            artifact
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| RuntimeError::Extraction(
                    "7z artifact filename is not UTF-8".to_owned(),
                ))?,
            std::process::id()
        ));
    ensure_empty_directory(&payload_root)?;
    let result = (|| {
        let members =
            extract_seven_zip_contents(component, artifact, &payload_root, inventory, limits)?;
        let payload = members
            .iter()
            .find(|member| member.path == *payload_filename)
            .ok_or_else(|| {
                RuntimeError::Extraction(format!("7z payload '{}' is missing", payload_filename))
            })?;
        if payload.member_type != MemberType::File
            || members.iter().any(|member| {
                member.member_type == MemberType::File && member.path != *payload_filename
            })
            || members.iter().any(|member| {
                member.member_type == MemberType::Directory
                    && !payload_filename.starts_with(member.path.as_str())
            })
        {
            return Err(RuntimeError::Extraction(
                "7z AppImage container contains an unexpected entry".to_owned(),
            ));
        }
        let payload_path = output_path(&payload_root, payload_filename)?;
        let metadata = fs::symlink_metadata(&payload_path)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(RuntimeError::Extraction(
                "7z AppImage payload is not a regular file".to_owned(),
            ));
        }
        extract_appimage(component, &payload_path, destination, inventory, limits)
    })();
    let cleanup = fs::remove_dir_all(&payload_root);
    if let Err(error) = cleanup {
        if result.is_ok() {
            return Err(RuntimeError::Storage(error.to_string()));
        }
    }
    result
}

fn extract_seven_zip_contents(
    component: &RuntimeComponent,
    artifact: &Path,
    destination: &Path,
    inventory: &[InstalledEntry],
    limits: &ExtractionLimits,
) -> Result<Vec<ArchiveMember>, RuntimeError> {
    let artifact_size = fs::metadata(artifact)?.len();
    let mut reader = ArchiveReader::open(artifact, Password::empty())
        .map_err(|error| RuntimeError::Extraction(format!("7z could not be read: {error}")))?;
    reader.set_thread_count(1);
    let members = reader
        .archive()
        .files
        .iter()
        .map(|entry| {
            if entry.is_anti_item {
                return Err(RuntimeError::Extraction(
                    "7z anti-items are not accepted in runtime artifacts".to_owned(),
                ));
            }
            let member_type = if entry.is_directory {
                MemberType::Directory
            } else {
                MemberType::File
            };
            Ok(ArchiveMember {
                path: archive_path(&entry.name, member_type == MemberType::Directory, limits)?,
                member_type,
                size: if member_type == MemberType::File {
                    entry.size
                } else {
                    0
                },
                compressed_size: entry.compressed_size,
                link_target: None,
            })
        })
        .collect::<Result<Vec<_>, RuntimeError>>()?;
    validate_members(component, &members, artifact_size, inventory, limits)?;

    let mut callback_error = None;
    let result = reader.for_each_entries(|entry, input| {
        let operation = (|| {
            let member_type = if entry.is_directory {
                MemberType::Directory
            } else {
                MemberType::File
            };
            let path = archive_path(&entry.name, member_type == MemberType::Directory, limits)?;
            match member_type {
                MemberType::Directory => create_directory(destination, &path)?,
                MemberType::File => {
                    let output = output_path(destination, &path)?;
                    ensure_secure_parent(destination, &path)?;
                    let mut file = create_new_file(&output)?;
                    let copied = copy_with_limit(input, &mut file, entry.size.saturating_add(1))?;
                    if copied != entry.size {
                        return Err(RuntimeError::Extraction(format!(
                            "7z file '{}' was truncated",
                            path
                        )));
                    }
                    file.sync_all()?;
                }
                MemberType::Symlink => unreachable!(),
            }
            Ok(())
        })();
        if let Err(error) = operation {
            callback_error = Some(error);
            Err(std::io::Error::other("safe 7z extraction failed").into())
        } else {
            Ok(true)
        }
    });
    if let Some(error) = callback_error {
        return Err(error);
    }
    result.map_err(|error| RuntimeError::Extraction(format!("7z extraction failed: {error}")))?;
    fsync_directory(destination)?;
    Ok(members)
}

fn extract_appimage(
    component: &RuntimeComponent,
    artifact: &Path,
    destination: &Path,
    inventory: &[InstalledEntry],
    limits: &ExtractionLimits,
) -> Result<(), RuntimeError> {
    let offset = find_squashfs_offset(artifact)?;
    let filesystem =
        FilesystemReader::from_reader_with_offset(BufReader::new(File::open(artifact)?), offset)
            .map_err(|error| {
                RuntimeError::Extraction(format!("AppImage SquashFS is invalid: {error}"))
            })?;
    let artifact_size = fs::metadata(artifact)?.len();
    let members = squashfs_members(component, &filesystem, artifact_size, inventory, limits)?;
    validate_members(component, &members, artifact_size, inventory, limits)?;

    for node in filesystem.files() {
        let Some(path) = squashfs_path(&node.fullpath, limits)? else {
            continue;
        };
        match &node.inner {
            InnerNode::Dir(_) => create_directory(destination, &path)?,
            InnerNode::File(file) => {
                let output = output_path(destination, &path)?;
                ensure_secure_parent(destination, &path)?;
                let mut output_file = create_new_file(&output)?;
                let mut input = filesystem.file(file).reader();
                let copied =
                    copy_with_limit(&mut input, &mut output_file, file.file_len() as u64 + 1)?;
                if copied != file.file_len() as u64 {
                    return Err(RuntimeError::Extraction(format!(
                        "AppImage file '{}' was truncated",
                        path
                    )));
                }
                output_file.sync_all()?;
            }
            InnerNode::Symlink(link) => {
                let target = link.link.to_str().ok_or_else(|| {
                    RuntimeError::Extraction("AppImage link is not UTF-8".to_owned())
                })?;
                create_symlink(destination, &path, target)?;
            }
            InnerNode::CharacterDevice(_)
            | InnerNode::BlockDevice(_)
            | InnerNode::NamedPipe
            | InnerNode::Socket => {
                return Err(RuntimeError::Extraction(
                    "AppImage contains a special filesystem entry".to_owned(),
                ));
            }
        }
    }
    fsync_directory(destination)?;
    Ok(())
}

fn squashfs_members(
    component: &RuntimeComponent,
    filesystem: &FilesystemReader<'_>,
    artifact_size: u64,
    inventory: &[InstalledEntry],
    limits: &ExtractionLimits,
) -> Result<Vec<ArchiveMember>, RuntimeError> {
    let mut members = Vec::new();
    for node in filesystem.files() {
        let Some(path) = squashfs_path(&node.fullpath, limits)? else {
            continue;
        };
        let (member_type, size, link_target) = match &node.inner {
            InnerNode::Dir(_) => (MemberType::Directory, 0, None),
            InnerNode::File(file) => (MemberType::File, file.file_len() as u64, None),
            InnerNode::Symlink(link) => {
                let target = link
                    .link
                    .to_str()
                    .ok_or_else(|| {
                        RuntimeError::Extraction("AppImage link is not UTF-8".to_owned())
                    })?
                    .to_owned();
                (MemberType::Symlink, 0, Some(target))
            }
            InnerNode::CharacterDevice(_)
            | InnerNode::BlockDevice(_)
            | InnerNode::NamedPipe
            | InnerNode::Socket => {
                return Err(RuntimeError::Extraction(
                    "AppImage contains a special filesystem entry".to_owned(),
                ));
            }
        };
        members.push(ArchiveMember {
            path,
            member_type,
            size,
            compressed_size: 0,
            link_target,
        });
    }
    // Keep these arguments visible at the call site: the inventory is part of the AppImage
    // preflight contract even though common validation owns the comparison.
    let _ = (component, artifact_size, inventory);
    Ok(members)
}

fn validate_members(
    component: &RuntimeComponent,
    members: &[ArchiveMember],
    artifact_size: u64,
    inventory: &[InstalledEntry],
    limits: &ExtractionLimits,
) -> Result<(), RuntimeError> {
    if members.is_empty() {
        return Err(RuntimeError::Extraction(
            "archive contains no entries".to_owned(),
        ));
    }
    if members.len() as u64 > limits.max_entries {
        return Err(RuntimeError::Extraction(
            "archive has too many entries".to_owned(),
        ));
    }
    let mut paths = BTreeMap::new();
    let mut expanded = 0_u64;
    for member in members {
        if paths
            .insert(member.path.clone(), member.member_type)
            .is_some()
        {
            return Err(RuntimeError::Extraction(format!(
                "archive contains duplicate path '{}'",
                member.path
            )));
        }
        if let Some(parent) = member.path.parent() {
            if paths
                .get(&parent)
                .is_some_and(|kind| *kind != MemberType::Directory)
            {
                return Err(RuntimeError::Extraction(format!(
                    "archive path '{}' is below a non-directory",
                    member.path
                )));
            }
        }
        if member.member_type == MemberType::File {
            if member.size > limits.max_file_bytes {
                return Err(RuntimeError::Extraction(format!(
                    "archive file '{}' exceeds the file-size limit",
                    member.path
                )));
            }
            expanded = expanded
                .checked_add(member.size)
                .ok_or_else(|| RuntimeError::Extraction("archive size overflow".to_owned()))?;
            if member.compressed_size > 0
                && member.size
                    > member
                        .compressed_size
                        .saturating_mul(limits.max_compression_ratio)
            {
                return Err(RuntimeError::Extraction(format!(
                    "archive file '{}' exceeds the compression-ratio limit",
                    member.path
                )));
            }
        } else if member.size != 0 {
            return Err(RuntimeError::Extraction(format!(
                "non-file archive entry '{}' has a size",
                member.path
            )));
        }
        if let Some(target) = member.link_target.as_deref() {
            validate_link_target(component, &member.path, target, inventory)?;
        }
        validate_expected_type(component, &member.path, member.member_type, inventory)?;
    }
    // A second pass catches a child listed before its directory parent.
    for member in members {
        let mut parent = member.path.parent();
        while let Some(candidate) = parent {
            if paths
                .get(&candidate)
                .is_some_and(|kind| *kind != MemberType::Directory)
            {
                return Err(RuntimeError::Extraction(format!(
                    "archive path '{}' escapes through a non-directory",
                    member.path
                )));
            }
            parent = candidate.parent();
        }
    }
    if expanded > limits.max_expanded_bytes {
        return Err(RuntimeError::Extraction(
            "archive expanded size exceeds the trusted limit".to_owned(),
        ));
    }
    if artifact_size > 0 && expanded > artifact_size.saturating_mul(limits.max_compression_ratio) {
        return Err(RuntimeError::Extraction(
            "archive compression ratio exceeds the trusted limit".to_owned(),
        ));
    }
    Ok(())
}

fn validate_expected_type(
    component: &RuntimeComponent,
    path: &RelativePath,
    member_type: MemberType,
    inventory: &[InstalledEntry],
) -> Result<(), RuntimeError> {
    if let Some(expected) = expected_inventory_entry(component, path, inventory) {
        let expected_type = match expected.entry_type {
            InstalledEntryType::File => MemberType::File,
            InstalledEntryType::Directory => MemberType::Directory,
            InstalledEntryType::Symlink => MemberType::Symlink,
        };
        if expected_type != member_type {
            return Err(RuntimeError::Extraction(format!(
                "archive entry '{}' does not match trusted inventory type",
                path
            )));
        }
    }
    Ok(())
}

fn validate_link_target(
    component: &RuntimeComponent,
    path: &RelativePath,
    target: &str,
    inventory: &[InstalledEntry],
) -> Result<(), RuntimeError> {
    let target_path = SymlinkTarget::new(target.to_owned()).map_err(|_| {
        RuntimeError::Extraction(format!("symlink '{}' has an unsafe target", path))
    })?;
    let expected = expected_inventory_entry(component, path, inventory).ok_or_else(|| {
        RuntimeError::Extraction(format!("symlink '{}' is not in trusted inventory", path))
    })?;
    if expected.entry_type != InstalledEntryType::Symlink
        || expected.link_target.as_ref() != Some(&target_path)
    {
        return Err(RuntimeError::Extraction(format!(
            "symlink '{}' does not match trusted inventory",
            path
        )));
    }
    let global_path = component_path(component, path)?;
    let global_target = resolve_link(&global_path, &target_path)?;
    let target_entry = inventory.iter().find(|entry| entry.path == global_target);
    if target_entry.is_none() {
        return Err(RuntimeError::Extraction(format!(
            "symlink '{}' points outside the trusted inventory",
            path
        )));
    }
    Ok(())
}

fn expected_inventory_entry<'a>(
    component: &RuntimeComponent,
    path: &RelativePath,
    inventory: &'a [InstalledEntry],
) -> Option<&'a InstalledEntry> {
    let global = component_path(component, path).ok()?;
    inventory.iter().find(|entry| entry.path == global)
}

fn component_path(
    component: &RuntimeComponent,
    path: &RelativePath,
) -> Result<RelativePath, RuntimeError> {
    RelativePath::new(format!("{}/{}", component.install_path, path))
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
                    return Err(RuntimeError::Extraction(
                        "symlink target escapes the extraction root".to_owned(),
                    ));
                }
            }
            component => normalized.push(component.to_owned()),
        }
    }
    RelativePath::new(normalized.join("/"))
}

fn archive_path(
    raw: impl AsRef<Path>,
    is_directory: bool,
    limits: &ExtractionLimits,
) -> Result<RelativePath, RuntimeError> {
    let raw = raw.as_ref();
    let raw = raw
        .to_str()
        .ok_or_else(|| RuntimeError::Extraction("archive path is not UTF-8".to_owned()))?;
    let raw = if is_directory {
        raw.trim_end_matches('/')
    } else {
        raw
    };
    if raw.is_empty() || raw.len() as u64 > limits.max_path_bytes {
        return Err(RuntimeError::Extraction(
            "archive path is empty or too long".to_owned(),
        ));
    }
    if raw.starts_with('/') || raw.starts_with('\\') || raw.contains('\\') || raw.contains('\0') {
        return Err(RuntimeError::Extraction(format!(
            "archive path '{raw}' is absolute or uses an unsafe separator"
        )));
    }
    RelativePath::new(raw.to_owned())
        .map_err(|_| RuntimeError::Extraction(format!("archive path '{raw}' is unsafe")))
}

fn output_path(destination: &Path, path: &RelativePath) -> Result<PathBuf, RuntimeError> {
    let output = destination.join(path.to_path_buf());
    if !output.starts_with(destination) {
        return Err(RuntimeError::Extraction(
            "archive output escaped the extraction root".to_owned(),
        ));
    }
    Ok(output)
}

fn ensure_secure_parent(destination: &Path, path: &RelativePath) -> Result<(), RuntimeError> {
    let mut current = destination.to_path_buf();
    if let Some(parent) = path.parent() {
        for component in parent.as_str().split('/') {
            current.push(component);
            match fs::symlink_metadata(&current) {
                Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                    return Err(RuntimeError::Extraction(format!(
                        "archive parent is not a real directory: {}",
                        current.display()
                    )))
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    fs::create_dir(&current)?;
                }
                Err(error) => return Err(RuntimeError::Io(error)),
            }
        }
    }
    Ok(())
}

fn create_directory(destination: &Path, path: &RelativePath) -> Result<(), RuntimeError> {
    let output = output_path(destination, path)?;
    let mut current = destination.to_path_buf();
    for component in path.as_str().split('/') {
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(RuntimeError::Extraction(format!(
                    "archive directory conflicts with an existing path: {}",
                    current.display()
                )))
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => fs::create_dir(&current)?,
            Err(error) => return Err(RuntimeError::Io(error)),
        }
    }
    let _ = output;
    Ok(())
}

fn create_new_file(path: &Path) -> Result<File, RuntimeError> {
    Ok(OpenOptions::new().write(true).create_new(true).open(path)?)
}

fn create_symlink(
    destination: &Path,
    path: &RelativePath,
    target: &str,
) -> Result<(), RuntimeError> {
    let target = SymlinkTarget::new(target.to_owned()).map_err(|_| {
        RuntimeError::Extraction(format!("symlink '{}' has an unsafe target", path))
    })?;
    let output = output_path(destination, path)?;
    ensure_secure_parent(destination, path)?;
    std::os::unix::fs::symlink(target.as_str(), &output)?;
    Ok(())
}

fn squashfs_path(
    path: &Path,
    limits: &ExtractionLimits,
) -> Result<Option<RelativePath>, RuntimeError> {
    let path = path
        .to_str()
        .ok_or_else(|| RuntimeError::Extraction("AppImage path is not UTF-8".to_owned()))?;
    if path == "/" {
        return Ok(None);
    }
    archive_path(path.strip_prefix('/').unwrap_or(path), false, limits).map(Some)
}

fn find_squashfs_offset(path: &Path) -> Result<u64, RuntimeError> {
    const MAX_HEADER_SCAN: usize = 64 * 1024 * 1024;
    let mut file = File::open(path)?;
    let mut bytes = Vec::new();
    std::io::Read::by_ref(&mut file)
        .take(MAX_HEADER_SCAN as u64)
        .read_to_end(&mut bytes)?;
    let magic = *b"hsqs";
    bytes
        .windows(magic.len())
        .position(|window| window == magic)
        .map(|offset| offset as u64)
        .ok_or_else(|| RuntimeError::Extraction("AppImage has no SquashFS payload".to_owned()))
}

#[cfg(test)]
mod tests {
    use super::{
        archive_path, validate_members, ArchiveMember, LinuxRuntimeArchiveExtractor, MemberType,
        RuntimeArchiveExtractor,
    };
    use crate::domain::runtime::{
        ArchiveFormat, ComponentKind, ExtractionLimits, InstalledEntry, InstalledEntryType,
        RelativePath, RuntimeComponent, Sha256Digest,
    };
    use std::fs::{self, File};
    use std::path::Path;
    use tempfile::tempdir;

    fn component() -> RuntimeComponent {
        RuntimeComponent {
            id: "runtime".try_into().unwrap(),
            kind: ComponentKind::Runtime,
            target_name: "runtime.tar".to_owned(),
            source_id: None,
            source_url: None,
            archive_format: ArchiveFormat::Tar,
            archive_size_bytes: 1,
            sha256: Sha256Digest::from_hex(&"a".repeat(64)).unwrap(),
            install_path: RelativePath::new("runtime/app").unwrap(),
            expected_root: None,
            payload_filename: None,
            executable_relative_path: None,
            display_version: None,
            source_revision: None,
            source_pinning: None,
            license: "GPL-3.0-or-later".to_owned(),
            systems: Vec::new(),
        }
    }

    fn inventory_for_app_run(content: &[u8]) -> Vec<InstalledEntry> {
        vec![
            InstalledEntry {
                path: RelativePath::new("runtime/app").unwrap(),
                entry_type: InstalledEntryType::Directory,
                size_bytes: 0,
                sha256: None,
                executable: false,
                link_target: None,
            },
            InstalledEntry {
                path: RelativePath::new("runtime/app/AppRun").unwrap(),
                entry_type: InstalledEntryType::File,
                size_bytes: content.len() as u64,
                sha256: Some(crate::adapters::runtime_integrity::sha256_bytes(content)),
                executable: true,
                link_target: None,
            },
        ]
    }

    fn tar_file(path: &Path, member_name: &str, content: &[u8]) {
        let file = File::create(path).unwrap();
        let mut builder = tar::Builder::new(file);
        let mut header = tar::Header::new_gnu();
        header.set_path(member_name).unwrap();
        header.set_size(content.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        builder.append(&header, content).unwrap();
        builder.finish().unwrap();
    }

    #[test]
    fn archive_paths_reject_escape_and_absolute_names() {
        let limits = ExtractionLimits::default();
        assert!(archive_path("../outside", false, &limits).is_err());
        assert!(archive_path("/etc/passwd", false, &limits).is_err());
        assert!(archive_path("dir\\file", false, &limits).is_err());
        assert!(archive_path("safe/file", false, &limits).is_ok());
    }

    #[test]
    fn member_preflight_rejects_duplicate_and_escaping_links() {
        let component = RuntimeComponent {
            id: "runtime".try_into().unwrap(),
            kind: crate::domain::runtime::ComponentKind::Runtime,
            target_name: "runtime.tar".to_owned(),
            source_id: None,
            source_url: None,
            archive_format: crate::domain::runtime::ArchiveFormat::Tar,
            archive_size_bytes: 1,
            sha256: crate::domain::runtime::Sha256Digest::from_hex(&"a".repeat(64)).unwrap(),
            install_path: RelativePath::new("runtime/app").unwrap(),
            expected_root: None,
            payload_filename: None,
            executable_relative_path: None,
            display_version: None,
            source_revision: None,
            source_pinning: None,
            license: "GPL-3.0-or-later".to_owned(),
            systems: Vec::new(),
        };
        let limits = ExtractionLimits::default();
        let duplicate = vec![
            ArchiveMember {
                path: RelativePath::new("AppRun").unwrap(),
                member_type: MemberType::File,
                size: 1,
                compressed_size: 1,
                link_target: None,
            },
            ArchiveMember {
                path: RelativePath::new("AppRun").unwrap(),
                member_type: MemberType::File,
                size: 1,
                compressed_size: 1,
                link_target: None,
            },
        ];
        assert!(validate_members(&component, &duplicate, 2, &[], &limits).is_err());
        let link = vec![ArchiveMember {
            path: RelativePath::new("link").unwrap(),
            member_type: MemberType::Symlink,
            size: 0,
            compressed_size: 0,
            link_target: Some("../../outside".to_owned()),
        }];
        assert!(validate_members(&component, &link, 2, &[], &limits).is_err());
        let _ = LinuxRuntimeArchiveExtractor;
    }

    #[test]
    fn extracts_a_valid_synthetic_tar_and_rejects_corrupt_or_oversized_payloads() {
        let directory = tempdir().unwrap();
        let artifact = directory.path().join("runtime.tar");
        let content = b"#!/bin/sh\nexit 0\n";
        tar_file(&artifact, "AppRun", content);
        let inventory = inventory_for_app_run(content);
        let destination = directory.path().join("destination");
        fs::create_dir(&destination).unwrap();
        LinuxRuntimeArchiveExtractor
            .extract(
                &component(),
                &artifact,
                &destination,
                &inventory,
                &ExtractionLimits::default(),
            )
            .unwrap();
        assert_eq!(fs::read(destination.join("AppRun")).unwrap(), content);

        let corrupt = directory.path().join("corrupt.tar");
        fs::write(&corrupt, b"not a tar archive").unwrap();
        let corrupt_destination = directory.path().join("corrupt-destination");
        fs::create_dir(&corrupt_destination).unwrap();
        assert!(LinuxRuntimeArchiveExtractor
            .extract(
                &component(),
                &corrupt,
                &corrupt_destination,
                &inventory,
                &ExtractionLimits::default(),
            )
            .is_err());

        let limits = ExtractionLimits {
            max_file_bytes: 1,
            ..ExtractionLimits::default()
        };
        let limited_destination = directory.path().join("limited-destination");
        fs::create_dir(&limited_destination).unwrap();
        assert!(LinuxRuntimeArchiveExtractor
            .extract(
                &component(),
                &artifact,
                &limited_destination,
                &inventory,
                &limits,
            )
            .is_err());
    }

    #[test]
    fn rejects_hostile_tar_traversal_before_writing() {
        let directory = tempdir().unwrap();
        let artifact = directory.path().join("hostile.tar");
        let file = File::create(&artifact).unwrap();
        let mut builder = tar::Builder::new(file);
        let mut header = tar::Header::new_gnu();
        header.set_path_absolute("/outside").unwrap();
        header.set_size(7);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append(&header, &b"blocked"[..]).unwrap();
        builder.finish().unwrap();
        let destination = directory.path().join("destination");
        fs::create_dir(&destination).unwrap();
        assert!(LinuxRuntimeArchiveExtractor
            .extract(
                &component(),
                &artifact,
                &destination,
                &[],
                &ExtractionLimits::default(),
            )
            .is_err());
        assert!(!directory.path().join("outside").exists());
    }
}
