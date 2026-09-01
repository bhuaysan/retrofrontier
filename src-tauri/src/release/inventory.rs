//! Derive the authenticated installed-file inventory from a component's target artefact.
//!
//! ADR-012 requires the release manifest to declare the exact installed tree. That inventory is a
//! *description of what extraction will produce*, so it is derived here by reading the artefact
//! with the same archive readers the client extractor uses, rather than by extracting first and
//! then describing whatever happened to land on disk.
//!
//! Construction still verifies the result the honest way: `crate::release::construct` extracts
//! every component through the real reviewed extractor against this inventory and then runs the
//! client's own installed-tree verification over the outcome.

use crate::adapters::runtime_archive::{appimage_squashfs_path, find_squashfs_offset};
use crate::adapters::runtime_integrity::sha256_bytes;
use crate::domain::runtime::{
    ArchiveFormat, InstalledEntry, InstalledEntryType, RelativePath, RuntimeError, SymlinkTarget,
};
use backhand::{FilesystemReader, InnerNode};
use sevenz_rust2::{ArchiveReader, Password};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use zip::ZipArchive;

/// One entry of a derived inventory, still relative to the component's install path.
struct DerivedEntry {
    entry_type: InstalledEntryType,
    size_bytes: u64,
    sha256: Option<crate::domain::runtime::Sha256Digest>,
    executable: bool,
    link_target: Option<SymlinkTarget>,
}

/// Derive the inventory a component contributes, prefixed with its install path.
///
/// The install-path directory itself is always emitted: the manifest contract requires every
/// component to own a directory entry at its install path.
pub fn derive_component_inventory(
    install_path: &RelativePath,
    archive_format: ArchiveFormat,
    artifact: &Path,
) -> Result<Vec<InstalledEntry>, RuntimeError> {
    let derived =
        match archive_format {
            ArchiveFormat::AppImage => derive_from_appimage(artifact)?,
            ArchiveFormat::Zip => derive_from_zip(artifact)?,
            ArchiveFormat::Tar => derive_from_tar(artifact)?,
            ArchiveFormat::SevenZip => return Err(RuntimeError::Manifest(
                "release construction expects the AppImage to be lifted out of its 7z container"
                    .to_owned(),
            )),
        };

    let mut entries = vec![InstalledEntry {
        path: install_path.clone(),
        entry_type: InstalledEntryType::Directory,
        size_bytes: 0,
        sha256: None,
        executable: false,
        link_target: None,
    }];
    // Archives do not reliably list every ancestor directory: the libretro core zips contain one
    // bare file. Missing ancestors are synthesised so the inventory describes a complete tree.
    let mut all: BTreeMap<RelativePath, DerivedEntry> = BTreeMap::new();
    for (relative, entry) in derived {
        for ancestor in ancestors(&relative) {
            all.entry(ancestor).or_insert(DerivedEntry {
                entry_type: InstalledEntryType::Directory,
                size_bytes: 0,
                sha256: None,
                executable: false,
                link_target: None,
            });
        }
        if all.insert(relative.clone(), entry).is_some() {
            return Err(RuntimeError::Manifest(format!(
                "artefact contains duplicate path '{relative}'"
            )));
        }
    }

    for (relative, entry) in all {
        entries.push(InstalledEntry {
            path: install_path.join(relative.as_str())?,
            entry_type: entry.entry_type,
            size_bytes: entry.size_bytes,
            sha256: entry.sha256,
            executable: entry.executable,
            link_target: entry.link_target,
        });
    }
    Ok(entries)
}

fn ancestors(path: &RelativePath) -> Vec<RelativePath> {
    let mut result = Vec::new();
    let mut current = path.parent();
    while let Some(parent) = current {
        current = parent.parent();
        result.push(parent);
    }
    result.reverse();
    result
}

fn derive_from_appimage(
    artifact: &Path,
) -> Result<Vec<(RelativePath, DerivedEntry)>, RuntimeError> {
    let offset = find_squashfs_offset(artifact)?;
    let filesystem =
        FilesystemReader::from_reader_with_offset(BufReader::new(File::open(artifact)?), offset)
            .map_err(|error| {
                RuntimeError::Extraction(format!("AppImage SquashFS is invalid: {error}"))
            })?;
    let mut entries = Vec::new();
    for node in filesystem.files() {
        let Some(path) = appimage_squashfs_path(&node.fullpath)? else {
            continue;
        };
        let executable = node.header.permissions & 0o111 != 0;
        let derived = match &node.inner {
            InnerNode::Dir(_) => DerivedEntry {
                entry_type: InstalledEntryType::Directory,
                size_bytes: 0,
                sha256: None,
                executable: false,
                link_target: None,
            },
            InnerNode::File(file) => {
                let mut bytes = Vec::new();
                filesystem.file(file).reader().read_to_end(&mut bytes)?;
                if bytes.len() as u64 != file.file_len() as u64 {
                    return Err(RuntimeError::Extraction(format!(
                        "AppImage file '{path}' was truncated"
                    )));
                }
                DerivedEntry {
                    entry_type: InstalledEntryType::File,
                    size_bytes: bytes.len() as u64,
                    sha256: Some(sha256_bytes(&bytes)),
                    executable,
                    link_target: None,
                }
            }
            InnerNode::Symlink(link) => {
                let target = link.link.to_str().ok_or_else(|| {
                    RuntimeError::Extraction("AppImage link is not UTF-8".to_owned())
                })?;
                DerivedEntry {
                    entry_type: InstalledEntryType::Symlink,
                    size_bytes: 0,
                    sha256: None,
                    executable: false,
                    link_target: Some(SymlinkTarget::new(target.to_owned())?),
                }
            }
            InnerNode::CharacterDevice(_)
            | InnerNode::BlockDevice(_)
            | InnerNode::NamedPipe
            | InnerNode::Socket => {
                return Err(RuntimeError::Extraction(
                    "AppImage contains a special filesystem entry".to_owned(),
                ))
            }
        };
        entries.push((path, derived));
    }
    Ok(entries)
}

fn derive_from_zip(artifact: &Path) -> Result<Vec<(RelativePath, DerivedEntry)>, RuntimeError> {
    let mut archive = ZipArchive::new(File::open(artifact)?)
        .map_err(|error| RuntimeError::Extraction(format!("zip could not be read: {error}")))?;
    let mut entries = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| {
            RuntimeError::Extraction(format!("zip entry could not be read: {error}"))
        })?;
        let is_directory = entry.is_dir();
        let raw = entry.name().to_owned();
        let path = relative_path(if is_directory {
            raw.trim_end_matches('/')
        } else {
            &raw
        })?;
        let derived = if is_directory {
            DerivedEntry {
                entry_type: InstalledEntryType::Directory,
                size_bytes: 0,
                sha256: None,
                executable: false,
                link_target: None,
            }
        } else {
            let executable = entry.unix_mode().is_some_and(|mode| mode & 0o111 != 0);
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes)?;
            DerivedEntry {
                entry_type: InstalledEntryType::File,
                size_bytes: bytes.len() as u64,
                sha256: Some(sha256_bytes(&bytes)),
                executable,
                link_target: None,
            }
        };
        entries.push((path, derived));
    }
    Ok(entries)
}

fn derive_from_tar(artifact: &Path) -> Result<Vec<(RelativePath, DerivedEntry)>, RuntimeError> {
    let mut archive = tar::Archive::new(File::open(artifact)?);
    let mut entries = Vec::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        let header = entry.header().clone();
        let raw = entry
            .path()?
            .to_str()
            .ok_or_else(|| RuntimeError::Extraction("tar path is not UTF-8".to_owned()))?
            .to_owned();
        let is_directory = header.entry_type().is_dir();
        let path = relative_path(if is_directory {
            raw.trim_end_matches('/')
        } else {
            &raw
        })?;
        if !is_directory && !header.entry_type().is_file() {
            return Err(RuntimeError::Extraction(format!(
                "tar entry '{path}' is not a regular file or directory"
            )));
        }
        let derived = if is_directory {
            DerivedEntry {
                entry_type: InstalledEntryType::Directory,
                size_bytes: 0,
                sha256: None,
                executable: false,
                link_target: None,
            }
        } else {
            let executable = header.mode().unwrap_or(0o644) & 0o111 != 0;
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes)?;
            DerivedEntry {
                entry_type: InstalledEntryType::File,
                size_bytes: bytes.len() as u64,
                sha256: Some(sha256_bytes(&bytes)),
                executable,
                link_target: None,
            }
        };
        entries.push((path, derived));
    }
    Ok(entries)
}

fn relative_path(raw: &str) -> Result<RelativePath, RuntimeError> {
    RelativePath::new(raw.to_owned())
        .map_err(|_| RuntimeError::Extraction(format!("archive path '{raw}' is unsafe")))
}

/// Read the named member of a 7z container into memory.
///
/// Used only by release construction, and bounded: the member must be listed and must not exceed
/// the caller's limit, so a hostile container cannot exhaust memory during construction.
pub fn read_seven_zip_member(
    artifact: &Path,
    member: &str,
    max_bytes: u64,
) -> Result<Vec<u8>, RuntimeError> {
    let mut reader = ArchiveReader::open(artifact, Password::empty())
        .map_err(|error| RuntimeError::Extraction(format!("7z could not be read: {error}")))?;
    reader.set_thread_count(1);
    let declared = reader
        .archive()
        .files
        .iter()
        .find(|entry| entry.name == member && !entry.is_directory)
        .ok_or_else(|| RuntimeError::Extraction(format!("7z member '{member}' is missing")))?;
    if declared.size > max_bytes {
        return Err(RuntimeError::Extraction(format!(
            "7z member '{member}' exceeds the construction size limit"
        )));
    }
    let expected = declared.size;
    let bytes = reader.read_file(member).map_err(|error| {
        RuntimeError::Extraction(format!("7z member '{member}' could not be read: {error}"))
    })?;
    if bytes.len() as u64 != expected {
        return Err(RuntimeError::Extraction(format!(
            "7z member '{member}' was truncated"
        )));
    }
    Ok(bytes)
}

/// Repackage one zip subtree as a deterministic tar rooted at that subtree.
pub fn repackage_zip_subtree_as_tar(
    artifact: &Path,
    subtree: &str,
    max_bytes: u64,
) -> Result<Vec<u8>, RuntimeError> {
    let mut archive = ZipArchive::new(File::open(artifact)?)
        .map_err(|error| RuntimeError::Extraction(format!("zip could not be read: {error}")))?;
    let prefix = format!("{}/", subtree.trim_end_matches('/'));
    let mut files: BTreeMap<RelativePath, Vec<u8>> = BTreeMap::new();
    let mut directories: std::collections::BTreeSet<RelativePath> = Default::default();
    let mut total = 0_u64;

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| {
            RuntimeError::Extraction(format!("zip entry could not be read: {error}"))
        })?;
        let raw = entry.name().to_owned();
        let Some(stripped) = raw.strip_prefix(&prefix) else {
            continue;
        };
        let trimmed = stripped.trim_end_matches('/');
        if trimmed.is_empty() {
            continue;
        }
        let path = relative_path(trimmed)?;
        if entry.is_dir() {
            directories.insert(path);
            continue;
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        total = total
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| RuntimeError::Extraction("repackaged size overflow".to_owned()))?;
        if total > max_bytes {
            return Err(RuntimeError::Extraction(
                "repackaged subtree exceeds the construction size limit".to_owned(),
            ));
        }
        for ancestor in ancestors(&path) {
            directories.insert(ancestor);
        }
        if files.insert(path.clone(), bytes).is_some() {
            return Err(RuntimeError::Extraction(format!(
                "zip subtree contains duplicate path '{path}'"
            )));
        }
    }

    if files.is_empty() {
        return Err(RuntimeError::Extraction(format!(
            "zip subtree '{subtree}' is empty"
        )));
    }

    // Controller profiles and emulator support data are read-only text and binary blobs, never
    // code the runtime loads, so nothing in a repackaged subtree is marked executable.
    build_deterministic_tar(&directories, &files, 0o644)
}

/// Package one member of a 7z container as a deterministic single-entry tar.
///
/// The official version-addressed RetroArch stable core bundle ships each libretro core as a bare
/// `.so` inside one large 7z, so a core taken from it has no upstream archive of its own that could
/// be redistributed verbatim. The member is lifted out and packaged under `entry_name`, which is
/// what the component's `executable_relative_path` then resolves against.
pub fn repackage_seven_zip_member_as_tar(
    artifact: &Path,
    member: &str,
    entry_name: &str,
    max_bytes: u64,
) -> Result<Vec<u8>, RuntimeError> {
    let bytes = read_seven_zip_member(artifact, member, max_bytes)?;
    package_executable_as_tar(bytes, entry_name)
}

/// Package one native-code blob as a deterministic single-entry tar under `entry_name`.
///
/// Split out from [`repackage_seven_zip_member_as_tar`] so the packaging contract — determinism,
/// the flat entry name, and the executable bit — is testable without a 7z container.
pub fn package_executable_as_tar(
    bytes: Vec<u8>,
    entry_name: &str,
) -> Result<Vec<u8>, RuntimeError> {
    let path = relative_path(entry_name)?;
    if path.as_str().contains('/') {
        return Err(RuntimeError::Extraction(format!(
            "entry name '{entry_name}' must be a flat filename"
        )));
    }
    if bytes.is_empty() {
        return Err(RuntimeError::Extraction(format!(
            "entry '{entry_name}' is empty"
        )));
    }
    let mut files = BTreeMap::new();
    files.insert(path, bytes);
    // This derivation exists to carry native code the runtime dlopens, so the single entry is
    // executable. `RuntimeManifest::validate_for_linux_x86_64` requires a component's declared
    // executable to be an executable file, and it must live under an approved code root.
    build_deterministic_tar(&Default::default(), &files, 0o755)
}

/// Build a tar whose bytes depend only on the entries, never on the build host.
///
/// Determinism matters: the produced bytes are pinned by digest in the release definition, so the
/// modification time, ownership, ordering, and mode of every entry are fixed rather than inherited
/// from the build host or the upstream archive.
fn build_deterministic_tar(
    directories: &std::collections::BTreeSet<RelativePath>,
    files: &BTreeMap<RelativePath, Vec<u8>>,
    file_mode: u32,
) -> Result<Vec<u8>, RuntimeError> {
    let mut output = Vec::new();
    {
        let mut builder = tar::Builder::new(&mut output);
        builder.mode(tar::HeaderMode::Deterministic);
        for directory in directories {
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Directory);
            header.set_mode(0o755);
            header.set_size(0);
            header.set_mtime(0);
            header.set_uid(0);
            header.set_gid(0);
            builder
                .append_data(&mut header, directory.to_path_buf(), std::io::empty())
                .map_err(RuntimeError::Io)?;
        }
        for (path, bytes) in files {
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Regular);
            header.set_mode(file_mode);
            header.set_size(bytes.len() as u64);
            header.set_mtime(0);
            header.set_uid(0);
            header.set_gid(0);
            builder
                .append_data(&mut header, path.to_path_buf(), bytes.as_slice())
                .map_err(RuntimeError::Io)?;
        }
        builder.finish().map_err(RuntimeError::Io)?;
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::{derive_component_inventory, repackage_zip_subtree_as_tar};
    use crate::domain::runtime::{ArchiveFormat, InstalledEntryType, RelativePath};
    use std::io::Write;
    use tempfile::tempdir;

    fn core_zip(path: &std::path::Path) {
        let file = std::fs::File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file(
                "example_libretro.so",
                zip::write::SimpleFileOptions::default().unix_permissions(0o755),
            )
            .unwrap();
        writer.write_all(b"native core bytes").unwrap();
        writer.finish().unwrap();
    }

    #[test]
    fn a_bare_core_zip_yields_its_install_directory_and_the_core_file() {
        let directory = tempdir().unwrap();
        let artifact = directory.path().join("core.zip");
        core_zip(&artifact);

        let install_path = RelativePath::new("cores/example").unwrap();
        let inventory =
            derive_component_inventory(&install_path, ArchiveFormat::Zip, &artifact).unwrap();

        assert_eq!(inventory.len(), 2);
        assert_eq!(inventory[0].path, install_path);
        assert_eq!(inventory[0].entry_type, InstalledEntryType::Directory);
        let core = &inventory[1];
        assert_eq!(core.path.as_str(), "cores/example/example_libretro.so");
        assert_eq!(core.entry_type, InstalledEntryType::File);
        assert!(core.executable);
        assert_eq!(core.size_bytes, b"native core bytes".len() as u64);
        assert!(core.sha256.is_some());
    }

    #[test]
    fn repackaging_a_zip_subtree_is_byte_deterministic_and_re_rooted() {
        let directory = tempdir().unwrap();
        let artifact = directory.path().join("support.zip");
        let file = std::fs::File::create(&artifact).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        writer.add_directory("dolphin-emu/", options).unwrap();
        writer
            .start_file("dolphin-emu/license.txt", options)
            .unwrap();
        writer.write_all(b"licence").unwrap();
        writer
            .start_file("dolphin-emu/Sys/GC/font.bin", options)
            .unwrap();
        writer.write_all(b"font").unwrap();
        writer
            .start_file("dolphin-emu/Sys/wiitdb.txt", options)
            .unwrap();
        writer.write_all(b"titles").unwrap();
        writer.finish().unwrap();

        let first =
            repackage_zip_subtree_as_tar(&artifact, "dolphin-emu/Sys", 1024 * 1024).unwrap();
        let second =
            repackage_zip_subtree_as_tar(&artifact, "dolphin-emu/Sys", 1024 * 1024).unwrap();
        assert_eq!(first, second, "repackaging must be reproducible");

        let tar_path = directory.path().join("support.tar");
        std::fs::write(&tar_path, &first).unwrap();
        let install_path = RelativePath::new("runtime/support/dolphin-sys").unwrap();
        let inventory =
            derive_component_inventory(&install_path, ArchiveFormat::Tar, &tar_path).unwrap();
        let paths: Vec<_> = inventory
            .iter()
            .map(|entry| entry.path.as_str().to_owned())
            .collect();

        // The subtree becomes the root, and the sibling licence file outside it is not carried in.
        assert!(paths.contains(&"runtime/support/dolphin-sys/GC/font.bin".to_owned()));
        assert!(paths.contains(&"runtime/support/dolphin-sys/GC".to_owned()));
        assert!(paths.contains(&"runtime/support/dolphin-sys/wiitdb.txt".to_owned()));
        assert!(!paths.iter().any(|path| path.contains("license.txt")));
    }

    /// The core components taken from the version-addressed stable bundle are derived through
    /// this packaging step, so its three guarantees are asserted directly: the bytes are
    /// reproducible, the single entry lands at the component's `executable_relative_path`, and it
    /// is executable — a non-executable core would be refused by the manifest validator.
    #[test]
    fn packaging_a_core_binary_is_deterministic_and_executable() {
        use super::package_executable_as_tar;

        let core = b"\x7fELF native core bytes".to_vec();
        let first = package_executable_as_tar(core.clone(), "nestopia_libretro.so").unwrap();
        let second = package_executable_as_tar(core.clone(), "nestopia_libretro.so").unwrap();
        assert_eq!(first, second, "packaging must be reproducible");

        let directory = tempdir().unwrap();
        let artifact = directory.path().join("nestopia_libretro.so.tar");
        std::fs::write(&artifact, &first).unwrap();
        let install_path = RelativePath::new("cores/nestopia").unwrap();
        let inventory =
            derive_component_inventory(&install_path, ArchiveFormat::Tar, &artifact).unwrap();

        assert_eq!(inventory.len(), 2);
        assert_eq!(inventory[0].path, install_path);
        let entry = &inventory[1];
        assert_eq!(entry.path.as_str(), "cores/nestopia/nestopia_libretro.so");
        assert_eq!(entry.entry_type, InstalledEntryType::File);
        assert!(entry.executable, "a core the runtime dlopens is executable");
        assert_eq!(entry.size_bytes, core.len() as u64);
    }

    /// A nested entry name would put the core somewhere the component does not declare.
    #[test]
    fn a_nested_or_empty_core_entry_is_refused() {
        use super::package_executable_as_tar;

        assert!(package_executable_as_tar(b"core".to_vec(), "cores/nestopia_libretro.so").is_err());
        assert!(package_executable_as_tar(b"core".to_vec(), "../escape.so").is_err());
        assert!(package_executable_as_tar(Vec::new(), "empty_libretro.so").is_err());
    }

    /// Support data is never marked executable, even though both derivations share one tar builder.
    #[test]
    fn a_repackaged_support_subtree_is_not_executable() {
        let directory = tempdir().unwrap();
        let artifact = directory.path().join("support.zip");
        let file = std::fs::File::create(&artifact).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file(
                "root/udev/Pad.cfg",
                zip::write::SimpleFileOptions::default().unix_permissions(0o755),
            )
            .unwrap();
        writer.write_all(b"input_driver = \"udev\"\n").unwrap();
        writer.finish().unwrap();

        let tar = repackage_zip_subtree_as_tar(&artifact, "root", 1024 * 1024).unwrap();
        let tar_path = directory.path().join("support.tar");
        std::fs::write(&tar_path, &tar).unwrap();
        let inventory = derive_component_inventory(
            &RelativePath::new("runtime/support/joypad-autoconfig").unwrap(),
            ArchiveFormat::Tar,
            &tar_path,
        )
        .unwrap();

        assert!(
            inventory.iter().all(|entry| !entry.executable),
            "read-only support data must not become executable"
        );
    }

    #[test]
    fn an_empty_subtree_is_refused_rather_than_producing_an_empty_component() {
        let directory = tempdir().unwrap();
        let artifact = directory.path().join("support.zip");
        core_zip(&artifact);
        assert!(repackage_zip_subtree_as_tar(&artifact, "missing", 1024).is_err());
    }
}
