use crate::domain::bios::{
    BiosDiscovery, BiosRequirement, BiosRequirementStatus, BiosRequirementStatusState,
    BiosRootStatus,
};
use crate::domain::system::SystemCatalog;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use thiserror::Error;

const HASH_BUFFER_BYTES: usize = 128 * 1024;

/// Read-only BIOS discovery. The service knows only the default user-data root and a per-call
/// explicit override; it never owns, creates, repairs, or removes files below either root.
#[derive(Debug, Clone)]
pub struct BiosService {
    default_root: PathBuf,
    requirements: Vec<BiosRequirement>,
}

impl BiosService {
    pub fn from_catalog(
        default_root: impl Into<PathBuf>,
        catalog: &SystemCatalog,
    ) -> Result<Self, BiosError> {
        Self::new(default_root, catalog.bios_requirements().cloned().collect())
    }

    pub fn new(
        default_root: impl Into<PathBuf>,
        requirements: Vec<BiosRequirement>,
    ) -> Result<Self, BiosError> {
        let default_root = default_root.into();
        validate_root_path(&default_root)?;
        for requirement in &requirements {
            requirement
                .validate()
                .map_err(|source| BiosError::InvalidRequirement(source.to_string()))?;
        }
        Ok(Self {
            default_root,
            requirements,
        })
    }

    pub fn default_root(&self) -> &Path {
        &self.default_root
    }

    /// Discover expected BIOS files directly below the selected root. Unrelated files are never
    /// opened or hashed. `root_override` is the explicit development/test path used by the local
    /// real-BIOS integration test; production callers use `None`.
    pub fn discover(&self, root_override: Option<&Path>) -> Result<BiosDiscovery, BiosError> {
        let root = root_override.unwrap_or(self.default_root());
        validate_root_path(root)?;
        let root_status = inspect_root(root)?;
        let requirements = if root_status == BiosRootStatus::Ready {
            // Opening the directory once makes permission/filesystem failures explicit while
            // keeping the actual matching non-recursive and limited to expected filenames.
            fs::read_dir(root).map_err(|source| BiosError::ReadRoot {
                path: root.to_path_buf(),
                source,
            })?;
            self.requirements
                .iter()
                .map(|requirement| inspect_requirement(root, requirement))
                .collect::<Result<Vec<_>, _>>()?
        } else {
            self.requirements.iter().map(missing_status).collect()
        };

        Ok(BiosDiscovery {
            root: root.to_string_lossy().into_owned(),
            root_status,
            requirements,
        })
    }
}

fn validate_root_path(path: &Path) -> Result<(), BiosError> {
    if !path.is_absolute() {
        return Err(BiosError::UnsafeRoot {
            path: path.to_path_buf(),
        });
    }
    Ok(())
}

fn inspect_root(root: &Path) -> Result<BiosRootStatus, BiosError> {
    match fs::symlink_metadata(root) {
        Ok(metadata) if metadata.file_type().is_symlink() => Ok(BiosRootStatus::Unsafe),
        Ok(metadata) if !metadata.is_dir() => Ok(BiosRootStatus::NotDirectory),
        Ok(_) => Ok(BiosRootStatus::Ready),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(BiosRootStatus::Missing),
        Err(source) => Err(BiosError::ReadRoot {
            path: root.to_path_buf(),
            source,
        }),
    }
}

fn missing_status(requirement: &BiosRequirement) -> BiosRequirementStatus {
    BiosRequirementStatus {
        requirement_id: requirement.id.clone(),
        system_id: requirement.system_id,
        required: requirement.kind.is_required(),
        state: if requirement.kind.is_required() {
            BiosRequirementStatusState::Missing
        } else {
            BiosRequirementStatusState::OptionalMissing
        },
        expected_filenames: requirement.expected_filenames.clone(),
        expected_size_bytes: requirement.expected_size_bytes,
        description: requirement.description.clone(),
        matched_filename: None,
        file_size_bytes: None,
        sha256: None,
    }
}

fn inspect_requirement(
    root: &Path,
    requirement: &BiosRequirement,
) -> Result<BiosRequirementStatus, BiosError> {
    let mut invalid_candidate: Option<(String, Option<u64>, Option<String>)> = None;

    for filename in &requirement.expected_filenames {
        let path = root.join(filename);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(source) => {
                return Err(BiosError::ReadFile { path, source });
            }
        };

        if metadata.file_type().is_symlink() || !metadata.is_file() {
            invalid_candidate = Some((filename.clone(), None, None));
            continue;
        }

        let (size, digest) = hash_file(&path)?;
        let status = base_status(requirement, size, &digest);
        if status == BiosRequirementStatusState::NotCoveredByCatalog
            || status == BiosRequirementStatusState::PresentValid
        {
            return Ok(status_response(
                requirement,
                status,
                filename,
                Some(size),
                Some(digest),
            ));
        }
        invalid_candidate = Some((filename.clone(), Some(size), Some(digest)));
    }

    if let Some((filename, size, digest)) = invalid_candidate {
        return Ok(status_response(
            requirement,
            BiosRequirementStatusState::PresentInvalid,
            &filename,
            size,
            digest,
        ));
    }
    Ok(missing_status(requirement))
}

fn base_status(
    requirement: &BiosRequirement,
    size: u64,
    digest: &str,
) -> BiosRequirementStatusState {
    if requirement
        .expected_size_bytes
        .is_some_and(|expected| expected != size)
    {
        return BiosRequirementStatusState::PresentInvalid;
    }
    if !requirement.has_authoritative_identity() {
        return BiosRequirementStatusState::NotCoveredByCatalog;
    }
    if requirement
        .expected_hashes
        .iter()
        .any(|expected| expected.value.eq_ignore_ascii_case(digest))
    {
        BiosRequirementStatusState::PresentValid
    } else {
        BiosRequirementStatusState::PresentInvalid
    }
}

fn status_response(
    requirement: &BiosRequirement,
    state: BiosRequirementStatusState,
    filename: &str,
    size: Option<u64>,
    digest: Option<String>,
) -> BiosRequirementStatus {
    BiosRequirementStatus {
        requirement_id: requirement.id.clone(),
        system_id: requirement.system_id,
        required: requirement.kind.is_required(),
        state,
        expected_filenames: requirement.expected_filenames.clone(),
        expected_size_bytes: requirement.expected_size_bytes,
        description: requirement.description.clone(),
        matched_filename: Some(filename.to_owned()),
        file_size_bytes: size,
        sha256: digest,
    }
}

fn hash_file(path: &Path) -> Result<(u64, String), BiosError> {
    let before = fs::symlink_metadata(path).map_err(|source| BiosError::ReadFile {
        path: path.to_path_buf(),
        source,
    })?;
    if before.file_type().is_symlink() || !before.is_file() {
        return Err(BiosError::ReadFile {
            path: path.to_path_buf(),
            source: std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "BIOS candidate is not a regular file",
            ),
        });
    }

    let mut file = File::open(path).map_err(|source| BiosError::ReadFile {
        path: path.to_path_buf(),
        source,
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; HASH_BUFFER_BYTES];
    let mut size = 0_u64;
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|source| BiosError::ReadFile {
                path: path.to_path_buf(),
                source,
            })?;
        if read == 0 {
            break;
        }
        size = size
            .checked_add(read as u64)
            .ok_or_else(|| BiosError::HashOverflow(path.to_path_buf()))?;
        hasher.update(&buffer[..read]);
    }
    let digest = hasher.finalize();
    let after = fs::symlink_metadata(path).map_err(|source| BiosError::ReadFile {
        path: path.to_path_buf(),
        source,
    })?;
    if after.file_type().is_symlink() || !after.is_file() || after.len() != size {
        return Err(BiosError::ChangedDuringRead(path.to_path_buf()));
    }

    Ok((size, bytes_to_hex(&digest)))
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[derive(Debug, Error)]
pub enum BiosError {
    #[error("BIOS root must be an absolute path: {path}")]
    UnsafeRoot { path: PathBuf },
    #[error("BIOS root could not be read: {path}")]
    ReadRoot {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("BIOS file could not be read: {path}")]
    ReadFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("BIOS file changed while it was being read: {0}")]
    ChangedDuringRead(PathBuf),
    #[error("BIOS file size overflowed while it was being read: {0}")]
    HashOverflow(PathBuf),
    #[error("BIOS requirement is invalid: {0}")]
    InvalidRequirement(String),
}

#[cfg(test)]
mod tests {
    use super::{BiosError, BiosService};
    use crate::domain::bios::{
        BiosDigest, BiosRequirement, BiosRequirementKind, BiosRequirementStatusState,
        BiosRootStatus,
    };
    use crate::domain::system::{SystemCatalog, SystemId};
    use sha2::{Digest, Sha256};
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::SystemTime;
    use tempfile::tempdir;

    fn digest(bytes: &[u8]) -> String {
        let digest = Sha256::digest(bytes);
        let mut output = String::with_capacity(64);
        for byte in digest {
            output.push_str(&format!("{byte:02x}"));
        }
        output
    }

    fn requirement(
        filename: &str,
        hashes: Vec<BiosDigest>,
        size: Option<u64>,
        kind: BiosRequirementKind,
    ) -> BiosRequirement {
        BiosRequirement::new(
            "synthetic-bios",
            SystemId::PlayStation,
            vec![filename.to_owned()],
            hashes,
            size,
            kind,
            "Synthetic BIOS requirement",
        )
        .unwrap()
    }

    fn service(root: PathBuf, requirement: BiosRequirement) -> BiosService {
        BiosService::new(root, vec![requirement]).unwrap()
    }

    #[derive(Debug, PartialEq, Eq)]
    struct TreeEntrySnapshot {
        is_file: bool,
        is_directory: bool,
        is_symlink: bool,
        is_readonly: bool,
        size: u64,
        modified: SystemTime,
        sha256: Option<String>,
    }

    fn snapshot_tree(root: &Path) -> BTreeMap<PathBuf, TreeEntrySnapshot> {
        let mut snapshot = BTreeMap::new();
        snapshot_tree_entry(root, root, &mut snapshot);
        snapshot
    }

    fn snapshot_tree_entry(
        root: &Path,
        path: &Path,
        snapshot: &mut BTreeMap<PathBuf, TreeEntrySnapshot>,
    ) {
        let metadata = fs::symlink_metadata(path).unwrap();
        let relative_path = path.strip_prefix(root).unwrap().to_path_buf();
        let sha256 = metadata.is_file().then(|| digest(&fs::read(path).unwrap()));
        snapshot.insert(
            relative_path,
            TreeEntrySnapshot {
                is_file: metadata.is_file(),
                is_directory: metadata.is_dir(),
                is_symlink: metadata.file_type().is_symlink(),
                is_readonly: metadata.permissions().readonly(),
                size: metadata.len(),
                modified: metadata.modified().unwrap(),
                sha256,
            },
        );
        if metadata.is_dir() {
            for entry in fs::read_dir(path).unwrap() {
                snapshot_tree_entry(root, &entry.unwrap().path(), snapshot);
            }
        }
    }

    #[test]
    fn no_requirements_are_reported_without_opening_unrelated_files() {
        let directory = tempdir().unwrap();
        fs::write(directory.path().join("unrelated.bin"), b"unrelated").unwrap();
        let service = BiosService::new(directory.path().to_path_buf(), Vec::new()).unwrap();

        let report = service.discover(None).unwrap();

        assert_eq!(report.root_status, BiosRootStatus::Ready);
        assert!(report.requirements.is_empty());
    }

    #[test]
    fn required_bios_is_missing() {
        let directory = tempdir().unwrap();
        let service = service(
            directory.path().to_path_buf(),
            requirement(
                "firmware.bin",
                Vec::new(),
                None,
                BiosRequirementKind::Required,
            ),
        );

        let report = service.discover(None).unwrap();

        assert_eq!(
            report.requirements[0].state,
            BiosRequirementStatusState::Missing
        );
    }

    #[test]
    fn known_hash_and_size_are_validated() {
        let directory = tempdir().unwrap();
        let bytes = b"synthetic BIOS";
        fs::write(directory.path().join("firmware.bin"), bytes).unwrap();
        let service = service(
            directory.path().to_path_buf(),
            requirement(
                "firmware.bin",
                vec![BiosDigest::sha256(digest(bytes)).unwrap()],
                Some(bytes.len() as u64),
                BiosRequirementKind::Required,
            ),
        );

        let report = service.discover(None).unwrap();

        assert_eq!(
            report.requirements[0].state,
            BiosRequirementStatusState::PresentValid
        );
        assert_eq!(
            report.requirements[0].file_size_bytes,
            Some(bytes.len() as u64)
        );
        assert_eq!(
            report.requirements[0].sha256.as_deref(),
            Some(digest(bytes).as_str())
        );
    }

    #[test]
    fn discovery_does_not_modify_a_synthetic_bios_tree() {
        let directory = tempdir().unwrap();
        let bytes = b"synthetic BIOS fixture";
        fs::write(directory.path().join("firmware.bin"), bytes).unwrap();
        fs::write(directory.path().join("unrelated.bin"), b"unrelated fixture").unwrap();
        fs::create_dir(directory.path().join("nested-system")).unwrap();
        fs::write(
            directory.path().join("nested-system").join("ignored.bin"),
            b"nested fixture",
        )
        .unwrap();
        let service = service(
            directory.path().to_path_buf(),
            requirement(
                "firmware.bin",
                vec![BiosDigest::sha256(digest(bytes)).unwrap()],
                Some(bytes.len() as u64),
                BiosRequirementKind::Required,
            ),
        );

        let before = snapshot_tree(directory.path());
        let report = service.discover(None).unwrap();
        let after = snapshot_tree(directory.path());

        assert_eq!(
            report.requirements[0].state,
            BiosRequirementStatusState::PresentValid
        );
        assert_eq!(before, after);
    }

    #[test]
    fn wrong_hash_and_size_are_invalid() {
        let directory = tempdir().unwrap();
        fs::write(directory.path().join("firmware.bin"), b"wrong").unwrap();
        let service = service(
            directory.path().to_path_buf(),
            requirement(
                "firmware.bin",
                vec![BiosDigest::sha256("00".repeat(32)).unwrap()],
                Some(999),
                BiosRequirementKind::Required,
            ),
        );

        let report = service.discover(None).unwrap();

        assert_eq!(
            report.requirements[0].state,
            BiosRequirementStatusState::PresentInvalid
        );
    }

    #[test]
    fn wrong_known_size_is_invalid_even_when_identity_hash_is_unresolved() {
        let directory = tempdir().unwrap();
        fs::write(directory.path().join("firmware.bin"), b"wrong size").unwrap();
        let service = service(
            directory.path().to_path_buf(),
            requirement(
                "firmware.bin",
                Vec::new(),
                Some(999),
                BiosRequirementKind::Required,
            ),
        );

        let report = service.discover(None).unwrap();

        assert_eq!(
            report.requirements[0].state,
            BiosRequirementStatusState::PresentInvalid
        );
    }

    #[test]
    fn unrelated_files_are_ignored() {
        let directory = tempdir().unwrap();
        fs::write(directory.path().join("unrelated.bin"), b"unrelated").unwrap();
        let service = service(
            directory.path().to_path_buf(),
            requirement(
                "firmware.bin",
                vec![BiosDigest::sha256(digest(b"expected")).unwrap()],
                None,
                BiosRequirementKind::Required,
            ),
        );

        let report = service.discover(None).unwrap();

        assert_eq!(
            report.requirements[0].state,
            BiosRequirementStatusState::Missing
        );
    }

    #[test]
    fn optional_missing_does_not_look_required() {
        let directory = tempdir().unwrap();
        let service = service(
            directory.path().to_path_buf(),
            requirement(
                "firmware.bin",
                Vec::new(),
                None,
                BiosRequirementKind::Optional,
            ),
        );

        let report = service.discover(None).unwrap();

        assert!(!report.requirements[0].required);
        assert_eq!(
            report.requirements[0].state,
            BiosRequirementStatusState::OptionalMissing
        );
    }

    #[test]
    fn multiple_accepted_filenames_use_the_first_valid_candidate() {
        let directory = tempdir().unwrap();
        let bytes = b"accepted";
        fs::write(directory.path().join("second.bin"), bytes).unwrap();
        let requirement = BiosRequirement::new(
            "synthetic-bios",
            SystemId::PlayStation,
            vec!["first.bin".to_owned(), "second.bin".to_owned()],
            vec![BiosDigest::sha256(digest(bytes)).unwrap()],
            None,
            BiosRequirementKind::Required,
            "Synthetic BIOS requirement",
        )
        .unwrap();
        let service = service(directory.path().to_path_buf(), requirement);

        let report = service.discover(None).unwrap();

        assert_eq!(
            report.requirements[0].state,
            BiosRequirementStatusState::PresentValid
        );
        assert_eq!(
            report.requirements[0].matched_filename.as_deref(),
            Some("second.bin")
        );
    }

    #[test]
    fn present_file_without_authoritative_hash_is_not_covered() {
        let directory = tempdir().unwrap();
        fs::write(directory.path().join("firmware.bin"), b"unknown dump").unwrap();
        let service = service(
            directory.path().to_path_buf(),
            requirement(
                "firmware.bin",
                Vec::new(),
                None,
                BiosRequirementKind::Required,
            ),
        );

        let report = service.discover(None).unwrap();

        assert_eq!(
            report.requirements[0].state,
            BiosRequirementStatusState::NotCoveredByCatalog
        );
    }

    #[test]
    fn explicit_root_override_is_used_without_changing_the_default() {
        let default_directory = tempdir().unwrap();
        let override_directory = tempdir().unwrap();
        let bytes = b"override";
        fs::write(override_directory.path().join("firmware.bin"), bytes).unwrap();
        let service = service(
            default_directory.path().to_path_buf(),
            requirement(
                "firmware.bin",
                vec![BiosDigest::sha256(digest(bytes)).unwrap()],
                None,
                BiosRequirementKind::Required,
            ),
        );

        let report = service.discover(Some(override_directory.path())).unwrap();

        assert_eq!(report.root_status, BiosRootStatus::Ready);
        assert_eq!(service.default_root(), default_directory.path());
        assert_eq!(
            report.requirements[0].state,
            BiosRequirementStatusState::PresentValid
        );
    }

    #[test]
    fn missing_root_and_file_root_are_reported_safely() {
        let directory = tempdir().unwrap();
        let missing = directory.path().join("missing");
        let missing_service = service(
            missing.clone(),
            requirement(
                "firmware.bin",
                Vec::new(),
                None,
                BiosRequirementKind::Required,
            ),
        );
        assert_eq!(
            missing_service.discover(None).unwrap().root_status,
            BiosRootStatus::Missing
        );

        let file_root = directory.path().join("not-a-directory");
        fs::write(&file_root, b"not a root").unwrap();
        let file_service = service(
            file_root,
            requirement(
                "firmware.bin",
                Vec::new(),
                None,
                BiosRequirementKind::Required,
            ),
        );
        let report = file_service.discover(None).unwrap();
        assert_eq!(report.root_status, BiosRootStatus::NotDirectory);
        assert_eq!(
            report.requirements[0].state,
            BiosRequirementStatusState::Missing
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_bios_candidate_is_invalid_and_not_followed() {
        use std::os::unix::fs::symlink;

        let directory = tempdir().unwrap();
        let outside = directory.path().join("outside.bin");
        fs::write(&outside, b"outside").unwrap();
        symlink(&outside, directory.path().join("firmware.bin")).unwrap();
        let service = service(
            directory.path().to_path_buf(),
            requirement(
                "firmware.bin",
                Vec::new(),
                None,
                BiosRequirementKind::Required,
            ),
        );

        let report = service.discover(None).unwrap();

        assert_eq!(
            report.requirements[0].state,
            BiosRequirementStatusState::PresentInvalid
        );
    }

    #[test]
    fn default_catalog_can_supply_the_bios_service() {
        let directory = tempdir().unwrap();
        let catalog = SystemCatalog::v1();
        let service = BiosService::from_catalog(directory.path().to_path_buf(), &catalog).unwrap();

        let report = service.discover(None).unwrap();

        assert_eq!(report.requirements.len(), 5);
        assert!(report
            .requirements
            .iter()
            .any(|requirement| requirement.system_id == SystemId::PlayStation));
    }

    #[test]
    fn relative_override_is_rejected() {
        let directory = tempdir().unwrap();
        let service = BiosService::new(directory.path().to_path_buf(), Vec::new()).unwrap();

        let error = service.discover(Some(Path::new("BIOS"))).unwrap_err();

        assert!(matches!(error, BiosError::UnsafeRoot { .. }));
    }

    #[ignore = "reads only developer-provided BIOS/; never part of CI"]
    #[test]
    fn inspect_local_real_bios_directory_read_only() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../BIOS");
        if !root.is_dir() {
            eprintln!("local BIOS integration: skipped; developer BIOS/ directory is absent");
            return;
        }

        let catalog = SystemCatalog::v1();
        let service = BiosService::from_catalog(root.clone(), &catalog).unwrap();
        let root_report = service.discover(Some(&root)).unwrap();
        // The developer-provided GBA file is kept in its existing named subdirectory. The
        // second explicit override keeps this opt-in test bounded and avoids enabling a recursive
        // walk in production discovery.
        let gba_root = root.join("Nintendo - Game Boy Advance");
        let gba_report = service.discover(Some(&gba_root)).unwrap();
        for (filename, report) in [
            ("scph1001.bin", &root_report),
            ("gba_bios.bin", &gba_report),
        ] {
            let status = report.requirements.iter().find(|status| {
                status
                    .expected_filenames
                    .iter()
                    .any(|expected| expected == filename)
            });
            match status {
                Some(status) => eprintln!(
                    "local BIOS integration: file={} state={:?} size={:?} sha256={:?}",
                    filename, status.state, status.file_size_bytes, status.sha256
                ),
                None => eprintln!(
                    "local BIOS integration: file={} state=not_covered_by_catalog size=None sha256=None",
                    filename
                ),
            }
        }
    }
}
