use crate::adapters::runtime_integrity::{sha256_bytes, sha256_file, verify_file};
use crate::adapters::runtime_paths::fsync_directory;
use crate::domain::runtime::{
    parse_strict_json, RuntimeError, RuntimeManifest, RuntimePolicy, Sha256Digest,
    MAX_MANIFEST_BYTES,
};
use async_trait::async_trait;
use futures::StreamExt;
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tough::schema::{Root, Signed};
use tough::{DefaultTransport, ExpirationEnforcement, Repository, RepositoryLoader, TargetName};
use url::Url;

static DOWNLOAD_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone)]
pub struct TrustedTarget {
    pub name: String,
    pub length: u64,
    pub sha256: Sha256Digest,
}

impl TrustedTarget {
    pub fn validate(&self) -> Result<(), RuntimeError> {
        crate::domain::runtime::RelativePath::new(self.name.clone())
            .map_err(|_| RuntimeError::Trust("trusted target name is unsafe".to_owned()))?;
        if self.length == 0 {
            return Err(RuntimeError::Trust(format!(
                "trusted target '{}' has an empty length",
                self.name
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct TrustedRelease {
    pub manifest_target_name: String,
    pub manifest_bytes: Vec<u8>,
    pub manifest_sha256: Sha256Digest,
    pub manifest: RuntimeManifest,
    pub targets: BTreeMap<String, TrustedTarget>,
    pub policy: RuntimePolicy,
    pub metadata_versions: crate::domain::runtime::MetadataVersions,
}

impl TrustedRelease {
    pub fn validate(&self) -> Result<(), RuntimeError> {
        self.manifest.validate_for_linux_x86_64()?;
        if self.manifest_bytes.len() as u64 > MAX_MANIFEST_BYTES
            || sha256_bytes(&self.manifest_bytes) != self.manifest_sha256
        {
            return Err(RuntimeError::Trust(
                "trusted manifest bytes do not match their digest".to_owned(),
            ));
        }
        if self.manifest_target_name.is_empty() {
            return Err(RuntimeError::Trust(
                "trusted release has no manifest target".to_owned(),
            ));
        }
        crate::domain::runtime::RelativePath::new(self.manifest_target_name.clone())
            .map_err(|_| RuntimeError::Trust("manifest target name is unsafe".to_owned()))?;
        let manifest_target = self
            .targets
            .get(&self.manifest_target_name)
            .ok_or_else(|| {
                RuntimeError::Trust("manifest target is not authenticated".to_owned())
            })?;
        if manifest_target.length != self.manifest_bytes.len() as u64
            || manifest_target.sha256 != self.manifest_sha256
        {
            return Err(RuntimeError::Trust(
                "manifest target metadata does not match manifest bytes".to_owned(),
            ));
        }

        for target in self.targets.values() {
            target.validate()?;
        }
        for component in &self.manifest.release.components {
            let target = self.targets.get(&component.target_name).ok_or_else(|| {
                RuntimeError::Trust(format!(
                    "component '{}' is not present in authenticated targets",
                    component.id
                ))
            })?;
            if target.length != component.archive_size_bytes || target.sha256 != component.sha256 {
                return Err(RuntimeError::Trust(format!(
                    "component '{}' disagrees with authenticated target metadata",
                    component.id
                )));
            }
        }
        if self.policy.minimum_safe_release_sequence > self.manifest.release.release_sequence {
            return Err(RuntimeError::Trust(
                "release is below the authenticated minimum sequence".to_owned(),
            ));
        }
        if self
            .policy
            .revoked_release_ids
            .iter()
            .any(|release_id| release_id == &self.manifest.release.release_id)
        {
            return Err(RuntimeError::Trust("release is revoked".to_owned()));
        }
        Ok(())
    }

    pub fn target(&self, name: &str) -> Result<&TrustedTarget, RuntimeError> {
        self.targets
            .get(name)
            .ok_or_else(|| RuntimeError::Trust(format!("target '{name}' is not trusted")))
    }
}

#[async_trait]
pub trait TrustedReleaseSource: Send + Sync {
    async fn resolve_release(
        &self,
        manifest_target_name: &str,
    ) -> Result<TrustedRelease, RuntimeError>;

    async fn download_target(
        &self,
        target: &TrustedTarget,
        destination: &Path,
        max_size: u64,
    ) -> Result<(), RuntimeError>;
}

/// A local source used by tests and explicitly configured development fixtures.
///
/// It still requires the same authenticated manifest/target relationships as the production
/// source. It is not selected implicitly by the application.
#[derive(Debug, Clone)]
pub struct LocalTrustedReleaseSource {
    manifest_target_name: String,
    release: TrustedRelease,
    target_files: BTreeMap<String, PathBuf>,
}

impl LocalTrustedReleaseSource {
    pub fn from_manifest_bytes(
        manifest_target_name: impl Into<String>,
        manifest_bytes: Vec<u8>,
        target_files: BTreeMap<String, PathBuf>,
    ) -> Result<Self, RuntimeError> {
        let manifest_target_name = manifest_target_name.into();
        let manifest = RuntimeManifest::parse(&manifest_bytes)?;
        let manifest_sha256 = sha256_bytes(&manifest_bytes);
        let mut targets = BTreeMap::new();
        targets.insert(
            manifest_target_name.clone(),
            TrustedTarget {
                name: manifest_target_name.clone(),
                length: manifest_bytes.len() as u64,
                sha256: manifest_sha256,
            },
        );

        for component in &manifest.release.components {
            let path = target_files.get(&component.target_name).ok_or_else(|| {
                RuntimeError::Trust(format!(
                    "local fixture is missing target '{}'",
                    component.target_name
                ))
            })?;
            let (length, sha256) = sha256_file(path)?;
            if length != component.archive_size_bytes || sha256 != component.sha256 {
                return Err(RuntimeError::Trust(format!(
                    "local fixture target '{}' does not match the manifest",
                    component.target_name
                )));
            }
            targets.insert(
                component.target_name.clone(),
                TrustedTarget {
                    name: component.target_name.clone(),
                    length,
                    sha256,
                },
            );
        }

        let release = TrustedRelease {
            manifest_target_name: manifest_target_name.clone(),
            manifest_bytes,
            manifest_sha256,
            manifest,
            targets,
            policy: RuntimePolicy::default(),
            metadata_versions: Default::default(),
        };
        release.validate()?;
        Ok(Self {
            manifest_target_name,
            release,
            target_files,
        })
    }

    pub fn with_policy(mut self, policy: RuntimePolicy) -> Result<Self, RuntimeError> {
        self.release.policy = policy;
        self.release.validate()?;
        Ok(self)
    }

    pub fn release(&self) -> &TrustedRelease {
        &self.release
    }
}

#[async_trait]
impl TrustedReleaseSource for LocalTrustedReleaseSource {
    async fn resolve_release(
        &self,
        manifest_target_name: &str,
    ) -> Result<TrustedRelease, RuntimeError> {
        if manifest_target_name != self.manifest_target_name {
            return Err(RuntimeError::Trust(format!(
                "local source has no approved release target '{manifest_target_name}'"
            )));
        }
        self.release.validate()?;
        Ok(self.release.clone())
    }

    async fn download_target(
        &self,
        target: &TrustedTarget,
        destination: &Path,
        max_size: u64,
    ) -> Result<(), RuntimeError> {
        let source = self.target_files.get(&target.name).ok_or_else(|| {
            RuntimeError::Download(format!("local target '{}' is unavailable", target.name))
        })?;
        stage_copy(source, target, destination, max_size)
    }
}

#[derive(Debug)]
pub struct ToughTrustedReleaseSource {
    trusted_root: Vec<u8>,
    metadata_base_url: Url,
    targets_base_url: Url,
    datastore: PathBuf,
    policy_target_name: String,
}

impl ToughTrustedReleaseSource {
    pub fn new(
        trusted_root: Vec<u8>,
        metadata_base_url: Url,
        targets_base_url: Url,
        datastore: PathBuf,
        policy_target_name: impl Into<String>,
    ) -> Result<Self, RuntimeError> {
        validate_trusted_root(&trusted_root)?;
        validate_repository_url(&metadata_base_url)?;
        validate_repository_url(&targets_base_url)?;
        let policy_target_name = policy_target_name.into();
        validate_policy_target_name(&policy_target_name)?;
        Ok(Self {
            trusted_root,
            metadata_base_url,
            targets_base_url,
            datastore,
            policy_target_name,
        })
    }

    async fn load_repository(&self) -> Result<Repository, RuntimeError> {
        let repository = RepositoryLoader::new(
            &self.trusted_root,
            self.metadata_base_url.clone(),
            self.targets_base_url.clone(),
        )
        .transport(DefaultTransport::new())
        .datastore(self.datastore.clone())
        .expiration_enforcement(ExpirationEnforcement::Safe)
        .load()
        .await
        .map_err(|error| RuntimeError::Trust(format!("TUF repository could not load: {error}")))?;
        Ok(repository)
    }
}

#[async_trait]
impl TrustedReleaseSource for ToughTrustedReleaseSource {
    async fn resolve_release(
        &self,
        manifest_target_name: &str,
    ) -> Result<TrustedRelease, RuntimeError> {
        let repository = self.load_repository().await?;
        let manifest_target = find_target(&repository, manifest_target_name)?;
        let manifest_bytes =
            read_target_bytes(&repository, manifest_target_name, MAX_MANIFEST_BYTES).await?;
        let manifest = RuntimeManifest::parse(&manifest_bytes)?;
        let manifest_sha256 = sha256_bytes(&manifest_bytes);
        let mut targets = BTreeMap::new();
        targets.insert(
            manifest_target_name.to_owned(),
            trusted_target_from_tough(manifest_target_name, &manifest_target)?,
        );
        for component in &manifest.release.components {
            let target = find_target(&repository, &component.target_name)?;
            targets.insert(
                component.target_name.clone(),
                trusted_target_from_tough(&component.target_name, &target)?,
            );
        }
        let policy_bytes =
            read_target_bytes(&repository, &self.policy_target_name, MAX_MANIFEST_BYTES).await?;
        let policy: RuntimePolicy = parse_strict_json(&policy_bytes)
            .map_err(|error| RuntimeError::Trust(error.to_owned()))?;
        policy.validate()?;
        let release = TrustedRelease {
            manifest_target_name: manifest_target_name.to_owned(),
            manifest_bytes,
            manifest_sha256,
            manifest,
            targets,
            policy,
            metadata_versions: crate::domain::runtime::MetadataVersions {
                timestamp: repository.timestamp().signed.version.get(),
                snapshot: repository.snapshot().signed.version.get(),
                targets: repository.targets().signed.version.get(),
            },
        };
        release.validate()?;
        Ok(release)
    }

    async fn download_target(
        &self,
        target: &TrustedTarget,
        destination: &Path,
        max_size: u64,
    ) -> Result<(), RuntimeError> {
        let repository = self.load_repository().await?;
        let metadata = find_target(&repository, &target.name)?;
        let metadata_target = trusted_target_from_tough(&target.name, &metadata)?;
        if metadata_target.length != target.length || metadata_target.sha256 != target.sha256 {
            return Err(RuntimeError::Trust(format!(
                "target '{}' changed between resolution and download",
                target.name
            )));
        }
        stage_tuf_target(&repository, &target.name, target, destination, max_size).await
    }
}

fn validate_repository_url(url: &Url) -> Result<(), RuntimeError> {
    if !matches!(url.scheme(), "https" | "file") {
        return Err(RuntimeError::Trust(format!(
            "TUF repository URL must use HTTPS or file fixtures, got {}",
            url.scheme()
        )));
    }
    Ok(())
}

fn validate_policy_target_name(name: &str) -> Result<(), RuntimeError> {
    crate::domain::runtime::RelativePath::new(name.to_owned())
        .map(|_| ())
        .map_err(|_| RuntimeError::Trust("runtime-policy target name is unsafe".to_owned()))
}

fn validate_trusted_root(bytes: &[u8]) -> Result<(), RuntimeError> {
    if bytes.is_empty() || bytes.len() as u64 > MAX_MANIFEST_BYTES {
        return Err(RuntimeError::Trust(
            "the trusted TUF root is empty or too large".to_owned(),
        ));
    }
    let root: Signed<Root> = serde_json::from_slice(bytes).map_err(|error| {
        RuntimeError::Trust(format!("the trusted TUF root is invalid: {error}"))
    })?;
    root.signed.verify_role(&root).map_err(|error| {
        RuntimeError::Trust(format!(
            "the trusted TUF root is not self-authenticating: {error}"
        ))
    })
}

fn find_target(repository: &Repository, name: &str) -> Result<tough::schema::Target, RuntimeError> {
    let target_name = TargetName::new(name.to_owned())
        .map_err(|error| RuntimeError::Trust(format!("invalid TUF target name: {error}")))?;
    let matches: Vec<_> = repository
        .all_targets()
        .filter(|(candidate, _)| candidate.raw() == target_name.raw())
        .map(|(_, target)| target.clone())
        .collect();
    match matches.as_slice() {
        [target] => Ok(target.clone()),
        [] => Err(RuntimeError::Trust(format!(
            "TUF target '{name}' is not listed"
        ))),
        _ => Err(RuntimeError::Trust(format!(
            "TUF target '{name}' has ambiguous metadata"
        ))),
    }
}

fn trusted_target_from_tough(
    name: &str,
    target: &tough::schema::Target,
) -> Result<TrustedTarget, RuntimeError> {
    let mut digest = [0_u8; 32];
    if target.hashes.sha256.as_ref().len() != digest.len() {
        return Err(RuntimeError::Trust(format!(
            "TUF target '{name}' does not have a SHA-256 digest"
        )));
    }
    digest.copy_from_slice(target.hashes.sha256.as_ref());
    Ok(TrustedTarget {
        name: name.to_owned(),
        length: target.length,
        sha256: Sha256Digest::from_bytes(digest),
    })
}

async fn read_target_bytes(
    repository: &Repository,
    name: &str,
    max_size: u64,
) -> Result<Vec<u8>, RuntimeError> {
    let target_name = TargetName::new(name.to_owned())
        .map_err(|error| RuntimeError::Trust(format!("invalid TUF target name: {error}")))?;
    let mut stream = repository
        .read_target(&target_name)
        .await
        .map_err(|error| RuntimeError::Download(format!("TUF target read failed: {error}")))?
        .ok_or_else(|| RuntimeError::Trust(format!("TUF target '{name}' is not listed")))?;
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| RuntimeError::Download(error.to_string()))?;
        if bytes.len() as u64 + chunk.len() as u64 > max_size {
            return Err(RuntimeError::Download(format!(
                "TUF target '{name}' exceeded its size limit"
            )));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

async fn stage_tuf_target(
    repository: &Repository,
    name: &str,
    target: &TrustedTarget,
    destination: &Path,
    max_size: u64,
) -> Result<(), RuntimeError> {
    target.validate()?;
    if target.length > max_size {
        return Err(RuntimeError::Download(format!(
            "target '{}' exceeds its download size limit",
            target.name
        )));
    }
    let target_name = TargetName::new(name.to_owned())
        .map_err(|error| RuntimeError::Trust(format!("invalid TUF target name: {error}")))?;
    let mut stream = repository
        .read_target(&target_name)
        .await
        .map_err(|error| RuntimeError::Download(format!("TUF target read failed: {error}")))?
        .ok_or_else(|| RuntimeError::Trust(format!("TUF target '{name}' is not listed")))?;
    let temporary = temporary_path(destination)?;
    let result = async {
        reject_existing_destination(destination)?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        let mut total = 0_u64;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|error| RuntimeError::Download(error.to_string()))?;
            total = total
                .checked_add(chunk.len() as u64)
                .ok_or_else(|| RuntimeError::Download("download size overflow".to_owned()))?;
            if total > max_size {
                return Err(RuntimeError::Download(format!(
                    "target '{}' exceeded its download size limit",
                    target.name
                )));
            }
            output.write_all(&chunk)?;
        }
        if total != target.length {
            return Err(RuntimeError::Integrity(format!(
                "TUF target '{}' has size {}, expected {}",
                target.name, total, target.length
            )));
        }
        output.flush()?;
        output.sync_all()?;
        drop(output);
        verify_file(&temporary, target.length, target.sha256)?;
        reject_existing_destination(destination)?;
        fs::rename(&temporary, destination)?;
        fsync_directory(destination.parent().ok_or_else(|| {
            RuntimeError::Download("staging destination has no parent".to_owned())
        })?)?;
        Ok(())
    }
    .await;
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn stage_copy(
    source: &Path,
    target: &TrustedTarget,
    destination: &Path,
    max_size: u64,
) -> Result<(), RuntimeError> {
    target.validate()?;
    let source_metadata = fs::symlink_metadata(source)?;
    if source_metadata.file_type().is_symlink() || !source_metadata.is_file() {
        return Err(RuntimeError::Download(format!(
            "local target '{}' is not a regular file",
            target.name
        )));
    }
    let mut input = File::open(source)?;
    let temporary = temporary_path(destination)?;
    let result = (|| {
        reject_existing_destination(destination)?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        let mut limited = std::io::Read::by_ref(&mut input).take(max_size.saturating_add(1));
        let copied = std::io::copy(&mut limited, &mut output)?;
        if copied > max_size {
            return Err(RuntimeError::Download(format!(
                "target '{}' exceeded its download size limit",
                target.name
            )));
        }
        output.flush()?;
        output.sync_all()?;
        drop(output);
        verify_file(&temporary, target.length, target.sha256)?;
        reject_existing_destination(destination)?;
        fs::rename(&temporary, destination)?;
        fsync_directory(destination.parent().ok_or_else(|| {
            RuntimeError::Download("staging destination has no parent".to_owned())
        })?)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn temporary_path(destination: &Path) -> Result<PathBuf, RuntimeError> {
    let parent = destination.parent().ok_or_else(|| {
        RuntimeError::Download(format!(
            "staging destination has no parent: {}",
            destination.display()
        ))
    })?;
    match fs::symlink_metadata(parent) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err(RuntimeError::Download(format!(
                "staging destination parent is not a private directory: {}",
                parent.display()
            )))
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(parent)?;
        }
        Err(error) => return Err(RuntimeError::Io(error)),
    }
    let name = destination
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| RuntimeError::Download("staging filename is not valid UTF-8".to_owned()))?;
    let counter = DOWNLOAD_COUNTER.fetch_add(1, Ordering::Relaxed);
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    Ok(parent.join(format!(
        ".{name}.partial-{}-{counter}-{stamp}",
        std::process::id()
    )))
}

fn reject_existing_destination(destination: &Path) -> Result<(), RuntimeError> {
    match fs::symlink_metadata(destination) {
        Ok(_) => Err(RuntimeError::Download(format!(
            "staging destination already exists: {}",
            destination.display()
        ))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(RuntimeError::Io(error)),
    }
}

#[cfg(test)]
mod tests {
    use super::{stage_copy, LocalTrustedReleaseSource, TrustedReleaseSource, TrustedTarget};
    use crate::adapters::runtime_integrity::sha256_file;
    use crate::domain::runtime::{
        ArchiveFormat, ComponentKind, InstalledEntry, InstalledEntryType, RelativePath,
        ReleaseChannel, RuntimeArchitecture, RuntimeCompatibility, RuntimeComponent,
        RuntimeManifest, RuntimePlatform, RuntimeRelease, SafeIdentifier, Sha256Digest,
    };
    use std::collections::BTreeMap;
    use std::fs;
    use tempfile::tempdir;

    fn synthetic_manifest(archive_size: u64, hash: Sha256Digest) -> RuntimeManifest {
        RuntimeManifest {
            schema_version: 1,
            manifest_id: SafeIdentifier::new("manifest-1").unwrap(),
            channel: ReleaseChannel::Stable,
            min_retrofrontier_version: "0.1.0".to_owned(),
            release: RuntimeRelease {
                release_id: SafeIdentifier::new("release-1").unwrap(),
                release_sequence: 1,
                retrofrontier_runtime_version: "1".to_owned(),
                retroarch_version: "1".to_owned(),
                platform: RuntimePlatform::Linux,
                architecture: RuntimeArchitecture::X86_64,
                components: vec![RuntimeComponent {
                    id: SafeIdentifier::new("retroarch").unwrap(),
                    kind: ComponentKind::Runtime,
                    target_name: "runtime.tar".to_owned(),
                    source_id: None,
                    source_url: None,
                    archive_format: ArchiveFormat::AppImage,
                    archive_size_bytes: archive_size,
                    sha256: hash,
                    install_path: RelativePath::new("runtime/app").unwrap(),
                    expected_root: None,
                    payload_filename: None,
                    executable_relative_path: None,
                    display_version: None,
                    source_revision: None,
                    source_pinning: None,
                    license: "GPL-3.0-or-later".to_owned(),
                    systems: Vec::new(),
                }],
                app_run_path: RelativePath::new("runtime/app/AppRun").unwrap(),
                inventory: vec![
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
                        size_bytes: 1,
                        sha256: Some(Sha256Digest::from_hex(&"a".repeat(64)).unwrap()),
                        executable: true,
                        link_target: None,
                    },
                ],
                extraction: Default::default(),
            },
            compatibility: RuntimeCompatibility {
                retroarch_core_api: "1".to_owned(),
                save_state_policy: "isolated".to_owned(),
            },
        }
    }

    #[tokio::test]
    async fn local_source_requires_exact_authenticated_target() {
        let directory = tempdir().unwrap();
        let artifact = directory.path().join("runtime.tar");
        fs::write(&artifact, b"artifact").unwrap();
        let (size, hash) = sha256_file(&artifact).unwrap();
        let manifest = synthetic_manifest(size, hash);
        let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
        let mut files = BTreeMap::new();
        files.insert("runtime.tar".to_owned(), artifact);
        let source = LocalTrustedReleaseSource::from_manifest_bytes(
            "manifests/release-1.json",
            manifest_bytes,
            files,
        )
        .unwrap();
        assert!(source
            .resolve_release("manifests/release-1.json")
            .await
            .is_ok());
        assert!(source.resolve_release("other.json").await.is_err());
    }

    #[test]
    fn staged_download_is_size_hash_checked_and_never_overwrites() {
        let directory = tempdir().unwrap();
        let source = directory.path().join("source.bin");
        let destination = directory.path().join("staging").join("target.bin");
        let bytes = b"approved bytes";
        fs::write(&source, bytes).unwrap();
        let (length, sha256) = sha256_file(&source).unwrap();
        let target = TrustedTarget {
            name: "target.bin".to_owned(),
            length,
            sha256,
        };
        stage_copy(&source, &target, &destination, length).unwrap();
        assert_eq!(fs::read(&destination).unwrap(), bytes);
        assert!(stage_copy(&source, &target, &destination, length).is_err());
        assert_eq!(fs::read(&destination).unwrap(), bytes);

        let too_small = directory.path().join("staging").join("too-small.bin");
        assert!(stage_copy(&source, &target, &too_small, length - 1).is_err());
        assert!(!too_small.exists());
    }

    #[test]
    fn tuf_source_accepts_only_https_or_local_fixture_urls() {
        let root = vec![1, 2, 3];
        assert!(super::ToughTrustedReleaseSource::new(
            root.clone(),
            url::Url::parse("http://example.invalid/metadata/").unwrap(),
            url::Url::parse("https://example.invalid/targets/").unwrap(),
            std::path::PathBuf::from("/tmp/retrofrontier-test-tuf"),
            "policy.json",
        )
        .is_err());
        let source = super::ToughTrustedReleaseSource::new(
            root,
            url::Url::parse("file:///tmp/metadata/").unwrap(),
            url::Url::parse("file:///tmp/targets/").unwrap(),
            std::path::PathBuf::from("/tmp/retrofrontier-test-tuf"),
            "policy.json",
        )
        .unwrap_err();
        assert!(source.to_string().contains("trusted TUF root"));
        let unsafe_policy = super::validate_policy_target_name("../policy.json");
        assert!(unsafe_policy.is_err());
    }
}
