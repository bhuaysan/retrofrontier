//! Reproducible construction of a real managed Runtime Release.
//!
//! Construction turns a committed [`ReleaseDefinition`] plus pinned upstream inputs into the exact
//! artefacts a client authenticates: one target file per component, one canonical release
//! manifest, and one runtime policy. Nothing is trusted because a download succeeded — every
//! upstream input and every derived artefact must equal its pinned length and SHA-256 before it is
//! used, and the emitted manifest is then validated and proven by a real extraction.

use crate::adapters::runtime_archive::{LinuxRuntimeArchiveExtractor, RuntimeArchiveExtractor};
use crate::adapters::runtime_installed::{
    apply_inventory_permissions, validate_app_run, verify_tree,
};
use crate::adapters::runtime_integrity::{sha256_bytes, sha256_file};
use crate::domain::runtime::{
    ExtractionLimits, InstalledEntry, RuntimeArchitecture, RuntimeCompatibility, RuntimeComponent,
    RuntimeError, RuntimeManifest, RuntimePlatform, RuntimePolicy, RuntimeRelease, Sha256Digest,
    RUNTIME_MANIFEST_SCHEMA_VERSION,
};
use crate::release::definition::{
    ComponentDerivation, ReleaseComponentDefinition, ReleaseDefinition, ReleaseInput,
};
use crate::release::inventory::{
    derive_component_inventory, read_seven_zip_member, repackage_zip_subtree_as_tar,
};
use std::fs;
use std::path::{Path, PathBuf};

/// The largest upstream input construction will download or read into memory.
pub const MAX_INPUT_BYTES: u64 = 1024 * 1024 * 1024;

/// Where pinned upstream inputs are read from and cached.
///
/// A cache is not an optimisation here: an upstream host that rotates a "latest" artefact would
/// otherwise make an approved release unreconstructable, so a maintainer can rebuild an existing
/// release entirely from previously verified bytes.
#[derive(Debug, Clone)]
pub struct InputCache {
    directory: PathBuf,
    allow_download: bool,
}

impl InputCache {
    pub fn new(directory: PathBuf, allow_download: bool) -> Self {
        Self {
            directory,
            allow_download,
        }
    }

    fn path_for(&self, input_id: &str) -> PathBuf {
        self.directory.join(input_id)
    }
}

/// What construction produced, for reporting and for the TUF publication step.
#[derive(Debug, Clone)]
pub struct ConstructedRelease {
    pub targets_directory: PathBuf,
    pub manifest_target_name: String,
    pub policy_target_name: String,
    pub manifest_bytes: Vec<u8>,
    pub manifest_sha256: Sha256Digest,
    pub manifest: RuntimeManifest,
    /// Every published target, including the manifest and the runtime policy.
    pub targets: Vec<PublishedTarget>,
}

#[derive(Debug, Clone)]
pub struct PublishedTarget {
    pub name: String,
    pub path: PathBuf,
    pub size_bytes: u64,
    pub sha256: Sha256Digest,
}

/// Construct a release into `output_directory/targets`.
pub async fn construct_release(
    definition_path: &Path,
    output_directory: &Path,
    cache: &InputCache,
) -> Result<ConstructedRelease, RuntimeError> {
    let definition_bytes = fs::read(definition_path)?;
    let definition = ReleaseDefinition::parse(&definition_bytes)?;

    let targets_directory = output_directory.join("targets");
    fs::create_dir_all(&targets_directory)?;
    fs::create_dir_all(&cache.directory)?;

    let mut components = Vec::new();
    let mut inventory: Vec<InstalledEntry> = Vec::new();
    let mut targets = Vec::new();

    for component in &definition.components {
        let input = definition.input(component.derivation.input())?;
        let artifact = acquire_component_artifact(component, input, cache).await?;
        let published = targets_directory.join(&component.target_name);
        write_new_file(&published, &artifact)?;

        let (size_bytes, sha256) = sha256_file(&published)?;
        if size_bytes != component.artifact_size_bytes || sha256 != component.artifact_sha256 {
            // Maintainer tooling: report what was produced so a deliberate pin refresh is a small
            // reviewed edit rather than a guessing game. Construction still refuses to continue.
            return Err(RuntimeError::Integrity(format!(
                "component '{}' artefact does not match its pin; produced {} bytes sha256 {}",
                component.id,
                size_bytes,
                sha256.to_hex()
            )));
        }

        inventory.extend(derive_component_inventory(
            &component.install_path,
            component.archive_format,
            &published,
        )?);
        components.push(RuntimeComponent {
            id: component.id.clone(),
            kind: component.kind,
            target_name: component.target_name.clone(),
            source_id: Some(input.id.clone()),
            source_url: Some(input.url.clone()),
            archive_format: component.archive_format,
            archive_size_bytes: size_bytes,
            sha256,
            install_path: component.install_path.clone(),
            expected_root: None,
            payload_filename: None,
            executable_relative_path: component.executable_relative_path.clone(),
            display_version: component.display_version.clone(),
            source_revision: component.source_revision.clone(),
            source_pinning: Some(format!("sha256:{}", input.sha256.to_hex())),
            license: component.license.clone(),
            systems: component.systems.clone(),
        });
        targets.push(PublishedTarget {
            name: component.target_name.clone(),
            path: published,
            size_bytes,
            sha256,
        });
    }

    // `verify_tree` walks the whole installation and refuses any path the inventory does not
    // list, so the directories that merely lead to a component's install path must be listed too.
    inventory.extend(install_path_ancestors(&definition));
    inventory.sort_by(|left, right| left.path.cmp(&right.path));
    inventory.dedup_by(|left, right| left.path == right.path);

    let manifest = RuntimeManifest {
        schema_version: RUNTIME_MANIFEST_SCHEMA_VERSION,
        manifest_id: definition.manifest_id.clone(),
        channel: definition.channel,
        min_retrofrontier_version: definition.min_retrofrontier_version.clone(),
        release: RuntimeRelease {
            release_id: definition.release_id.clone(),
            release_sequence: definition.release_sequence,
            retrofrontier_runtime_version: definition.retrofrontier_runtime_version.clone(),
            retroarch_version: definition.retroarch_version.clone(),
            platform: RuntimePlatform::Linux,
            architecture: RuntimeArchitecture::X86_64,
            components,
            app_run_path: definition.app_run_path.clone(),
            inventory,
            extraction: ExtractionLimits::default(),
        },
        compatibility: RuntimeCompatibility {
            retroarch_core_api: definition.retroarch_core_api.clone(),
            save_state_policy: definition.save_state_policy.clone(),
        },
    };
    // The client's own validation is the gate. A definition that would produce a manifest the
    // client refuses fails here rather than at install time on a user's machine.
    manifest.validate_for_linux_x86_64()?;

    let manifest_bytes = crate::release::canonical::to_canonical_json(&manifest)?;
    let manifest_sha256 = sha256_bytes(&manifest_bytes);
    let manifest_path = targets_directory.join(&definition.manifest_target_name);
    write_new_file(&manifest_path, &manifest_bytes)?;
    targets.push(PublishedTarget {
        name: definition.manifest_target_name.clone(),
        path: manifest_path,
        size_bytes: manifest_bytes.len() as u64,
        sha256: manifest_sha256,
    });

    let policy = RuntimePolicy {
        minimum_safe_release_sequence: definition.minimum_safe_release_sequence,
        revoked_release_ids: Vec::new(),
    };
    policy.validate()?;
    let policy_bytes = crate::release::canonical::to_canonical_json(&policy)?;
    let policy_path = targets_directory.join(&definition.policy_target_name);
    write_new_file(&policy_path, &policy_bytes)?;
    targets.push(PublishedTarget {
        name: definition.policy_target_name.clone(),
        path: policy_path,
        size_bytes: policy_bytes.len() as u64,
        sha256: sha256_bytes(&policy_bytes),
    });

    // Prove the manifest describes a tree the reviewed client extractor actually produces.
    verify_by_extraction(&manifest, &targets_directory, output_directory)?;

    Ok(ConstructedRelease {
        targets_directory,
        manifest_target_name: definition.manifest_target_name.clone(),
        policy_target_name: definition.policy_target_name.clone(),
        manifest_bytes,
        manifest_sha256,
        manifest,
        targets,
    })
}

/// The directory entries that lead to each component's install path, such as `runtime` and
/// `cores`. A component owns its install path; the shared parents above it belong to the release.
fn install_path_ancestors(definition: &ReleaseDefinition) -> Vec<InstalledEntry> {
    let mut paths = std::collections::BTreeSet::new();
    for component in &definition.components {
        let mut current = component.install_path.parent();
        while let Some(parent) = current {
            current = parent.parent();
            paths.insert(parent);
        }
    }
    paths
        .into_iter()
        .map(|path| InstalledEntry {
            path,
            entry_type: crate::domain::runtime::InstalledEntryType::Directory,
            size_bytes: 0,
            sha256: None,
            executable: false,
            link_target: None,
        })
        .collect()
}

/// Extract every component through the production extractor and verify the resulting tree.
///
/// This is the difference between "the manifest looks plausible" and "installing this release
/// produces exactly the authenticated inventory". It is the same extractor and the same
/// verification the client runs, so a mismatch is caught by the maintainer, not by a user.
pub fn verify_by_extraction(
    manifest: &RuntimeManifest,
    targets_directory: &Path,
    output_directory: &Path,
) -> Result<(), RuntimeError> {
    let proof_root = output_directory.join("construction-proof");
    if proof_root.exists() {
        fs::remove_dir_all(&proof_root)?;
    }
    fs::create_dir_all(&proof_root)?;
    let extractor = LinuxRuntimeArchiveExtractor;
    for component in &manifest.release.components {
        let destination = proof_root.join(component.install_path.to_path_buf());
        fs::create_dir_all(&destination)?;
        extractor.extract(
            component,
            &targets_directory.join(&component.target_name),
            &destination,
            &manifest.release.inventory,
            &manifest.release.extraction,
        )?;
    }
    apply_inventory_permissions(&proof_root, manifest)?;
    verify_tree(&proof_root, manifest)?;
    validate_app_run(&proof_root, manifest)?;
    fs::remove_dir_all(&proof_root)?;
    Ok(())
}

async fn acquire_component_artifact(
    component: &ReleaseComponentDefinition,
    input: &ReleaseInput,
    cache: &InputCache,
) -> Result<Vec<u8>, RuntimeError> {
    let input_path = acquire_input(input, cache).await?;
    match &component.derivation {
        ComponentDerivation::UpstreamFile { .. } => Ok(fs::read(&input_path)?),
        ComponentDerivation::SevenZipMember { member, .. } => {
            read_seven_zip_member(&input_path, member, MAX_INPUT_BYTES)
        }
        ComponentDerivation::ZipSubtreeTar { subtree, .. } => {
            repackage_zip_subtree_as_tar(&input_path, subtree, MAX_INPUT_BYTES)
        }
    }
}

/// Fetch or reuse one pinned upstream input, verifying length and digest before returning it.
async fn acquire_input(input: &ReleaseInput, cache: &InputCache) -> Result<PathBuf, RuntimeError> {
    let path = cache.path_for(input.id.as_str());
    if path.exists() {
        let (size_bytes, sha256) = sha256_file(&path)?;
        if size_bytes == input.size_bytes && sha256 == input.sha256 {
            return Ok(path);
        }
        // A cached file that does not match its pin is evidence of corruption or of an upstream
        // change, never something to silently accept.
        return Err(RuntimeError::Integrity(format!(
            "cached input '{}' does not match its pinned digest or length",
            input.id
        )));
    }
    if !cache.allow_download {
        return Err(RuntimeError::Download(format!(
            "input '{}' is not cached and downloads are disabled",
            input.id
        )));
    }
    if input.size_bytes > MAX_INPUT_BYTES {
        return Err(RuntimeError::Download(format!(
            "input '{}' exceeds the construction size limit",
            input.id
        )));
    }

    let bytes = download(&input.url, input.size_bytes).await?;
    if bytes.len() as u64 != input.size_bytes || sha256_bytes(&bytes) != input.sha256 {
        return Err(RuntimeError::Integrity(format!(
            "downloaded input '{}' does not match its pinned digest or length",
            input.id
        )));
    }
    write_new_file(&path, &bytes)?;
    Ok(path)
}

async fn download(url: &str, expected_size: u64) -> Result<Vec<u8>, RuntimeError> {
    let parsed = url::Url::parse(url)
        .map_err(|error| RuntimeError::Download(format!("input URL is invalid: {error}")))?;
    if parsed.scheme() != "https" {
        return Err(RuntimeError::Download(
            "input URL must use HTTPS".to_owned(),
        ));
    }
    let client = reqwest::Client::builder()
        .https_only(true)
        .build()
        .map_err(|error| RuntimeError::Download(error.to_string()))?;
    let response = client
        .get(parsed)
        .send()
        .await
        .map_err(|error| RuntimeError::Download(error.to_string()))?;
    if !response.status().is_success() {
        return Err(RuntimeError::Download(format!(
            "input download returned HTTP {}",
            response.status().as_u16()
        )));
    }
    if response
        .content_length()
        .is_some_and(|length| length != expected_size)
    {
        return Err(RuntimeError::Download(
            "input download announced an unexpected length".to_owned(),
        ));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|error| RuntimeError::Download(error.to_string()))?;
    Ok(bytes.to_vec())
}

/// Write a file, replacing any earlier construction output rather than appending to it.
fn write_new_file(path: &Path, bytes: &[u8]) -> Result<(), RuntimeError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    if path.exists() {
        fs::remove_file(path)?;
    }
    fs::write(path, bytes)?;
    Ok(())
}
