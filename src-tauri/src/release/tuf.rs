//! Publish a constructed release into a TUF 1.0 repository.
//!
//! This is the qualification-grade publication path. It builds the same repository profile
//! ADR-012 specifies — Ed25519 only, SHA-256 digests, consistent snapshots, separately scoped
//! snapshot and timestamp keys, and 2-of-3 thresholds for the offline root and targets roles — so
//! the client authenticates a qualification release through the *production* verification code in
//! `ToughTrustedReleaseSource` rather than through a relaxed development path.
//!
//! What it is not: a production key ceremony. Keys are generated on the maintainer's machine into
//! a directory outside this repository and are held by one person, so the independent-custody and
//! offline-storage requirements of ADR-012 are not met. Public production distribution therefore
//! still needs the M10 key ceremony and hosting decision.

use crate::domain::runtime::RuntimeError;
use crate::release::construct::ConstructedRelease;
use std::collections::HashMap;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};
use tough::editor::signed::SignedRole;
use tough::editor::RepositoryEditor;
use tough::key_source::{KeySource, LocalKeySource};
use tough::schema::decoded::{Decoded, Hex};
use tough::schema::{KeyHolder, RoleKeys, RoleType, Root, Signed, Target};

/// ADR-012's initial maximum metadata lifetimes, in days.
const ROOT_LIFETIME_DAYS: i64 = 366;
const TARGETS_LIFETIME_DAYS: i64 = 90;
const SNAPSHOT_LIFETIME_DAYS: i64 = 31;
const TIMESTAMP_LIFETIME_DAYS: i64 = 7;

/// ADR-012 requires independent custody at a 2-of-3 threshold for the two offline roles.
const OFFLINE_ROLE_KEYS: usize = 3;
const OFFLINE_ROLE_THRESHOLD: u64 = 2;

/// Where the signing keys live. Always outside the source repository.
#[derive(Debug, Clone)]
pub struct KeyDirectory {
    directory: PathBuf,
}

impl KeyDirectory {
    pub fn new(directory: PathBuf) -> Self {
        Self { directory }
    }

    fn key_path(&self, role: &str, index: usize) -> PathBuf {
        self.directory.join(format!("{role}-{index}.pk8"))
    }
}

#[derive(Debug, Clone)]
pub struct PublishedRepository {
    pub metadata_directory: PathBuf,
    pub targets_directory: PathBuf,
    pub root_json: PathBuf,
    pub manifest_target_name: String,
    pub policy_target_name: String,
}

/// Generate any missing Ed25519 signing keys, then publish the constructed release.
pub async fn publish_release(
    release: &ConstructedRelease,
    output_directory: &Path,
    keys: &KeyDirectory,
) -> Result<PublishedRepository, RuntimeError> {
    std::fs::create_dir_all(&keys.directory)?;
    restrict_directory(&keys.directory)?;

    let root_keys = ensure_role_keys(keys, "root", OFFLINE_ROLE_KEYS)?;
    let targets_keys = ensure_role_keys(keys, "targets", OFFLINE_ROLE_KEYS)?;
    let snapshot_keys = ensure_role_keys(keys, "snapshot", 1)?;
    let timestamp_keys = ensure_role_keys(keys, "timestamp", 1)?;

    let metadata_directory = output_directory.join("metadata");
    let published_targets = output_directory.join("repository-targets");
    std::fs::create_dir_all(&metadata_directory)?;
    std::fs::create_dir_all(&published_targets)?;

    let root_json = build_root(
        &metadata_directory,
        &root_keys,
        &targets_keys,
        &snapshot_keys,
        &timestamp_keys,
    )
    .await?;

    let mut editor = RepositoryEditor::new(&root_json)
        .await
        .map_err(|error| tuf_error("repository editor could not start", error))?;
    let now = jiff::Timestamp::now();
    editor
        .targets_version(one())
        .map_err(|error| tuf_error("targets version could not be set", error))?
        .targets_expires(expires(now, TARGETS_LIFETIME_DAYS)?)
        .map_err(|error| tuf_error("targets expiry could not be set", error))?
        .snapshot_version(one())
        .snapshot_expires(expires(now, SNAPSHOT_LIFETIME_DAYS)?)
        .timestamp_version(one())
        .timestamp_expires(expires(now, TIMESTAMP_LIFETIME_DAYS)?);

    for target in &release.targets {
        let descriptor = Target::from_path(&target.path)
            .await
            .map_err(|error| tuf_error("target could not be described", error))?;
        editor
            .add_target(target.name.clone(), descriptor)
            .map_err(|error| tuf_error("target could not be added", error))?;
    }

    let mut signing_keys: Vec<Box<dyn KeySource>> = Vec::new();
    for source in targets_keys
        .iter()
        .chain(&snapshot_keys)
        .chain(&timestamp_keys)
    {
        signing_keys.push(Box::new(LocalKeySource {
            path: source.path.clone(),
        }));
    }
    let signed = editor
        .sign(&signing_keys)
        .await
        .map_err(|error| tuf_error("repository could not be signed", error))?;
    signed
        .write(&metadata_directory)
        .await
        .map_err(|error| tuf_error("metadata could not be written", error))?;
    signed
        .copy_targets(
            &release.targets_directory,
            &published_targets,
            tough::editor::signed::PathExists::Replace,
        )
        .await
        .map_err(|error| tuf_error("targets could not be published", error))?;

    Ok(PublishedRepository {
        metadata_directory,
        targets_directory: published_targets,
        root_json,
        manifest_target_name: release.manifest_target_name.clone(),
        policy_target_name: release.policy_target_name.clone(),
    })
}

/// One generated or already-present signing key.
struct RoleKey {
    path: PathBuf,
    key: tough::schema::key::Key,
    key_id: Decoded<Hex>,
}

fn ensure_role_keys(
    keys: &KeyDirectory,
    role: &str,
    count: usize,
) -> Result<Vec<RoleKey>, RuntimeError> {
    let mut generated = Vec::new();
    for index in 0..count {
        let path = keys.key_path(role, index);
        if !path.exists() {
            let pkcs8 = aws_lc_rs::signature::Ed25519KeyPair::generate_pkcs8(
                &aws_lc_rs::rand::SystemRandom::new(),
            )
            .map_err(|error| RuntimeError::Trust(format!("key generation failed: {error}")))?;
            std::fs::write(&path, pkcs8.as_ref())?;
            restrict_file(&path)?;
        }
        let bytes = std::fs::read(&path)?;
        let pair = tough::sign::parse_keypair(&bytes)
            .map_err(|error| tuf_error("signing key could not be parsed", error))?;
        let key = tough::sign::Sign::tuf_key(&pair);
        let key_id = key
            .key_id()
            .map_err(|error| tuf_error("key id could not be computed", error))?;
        generated.push(RoleKey { path, key, key_id });
    }
    Ok(generated)
}

/// Build and self-sign `root.json`, and write both the bare and versioned filenames.
async fn build_root(
    metadata_directory: &Path,
    root_keys: &[RoleKey],
    targets_keys: &[RoleKey],
    snapshot_keys: &[RoleKey],
    timestamp_keys: &[RoleKey],
) -> Result<PathBuf, RuntimeError> {
    let mut keys = HashMap::new();
    for key in root_keys
        .iter()
        .chain(targets_keys)
        .chain(snapshot_keys)
        .chain(timestamp_keys)
    {
        keys.insert(key.key_id.clone(), key.key.clone());
    }

    let mut roles = HashMap::new();
    roles.insert(
        RoleType::Root,
        role_keys(root_keys, OFFLINE_ROLE_THRESHOLD)?,
    );
    roles.insert(
        RoleType::Targets,
        role_keys(targets_keys, OFFLINE_ROLE_THRESHOLD)?,
    );
    // Snapshot and timestamp may be online because neither can authorize target content.
    roles.insert(RoleType::Snapshot, role_keys(snapshot_keys, 1)?);
    roles.insert(RoleType::Timestamp, role_keys(timestamp_keys, 1)?);

    let root = Root {
        spec_version: "1.0.0".to_owned(),
        consistent_snapshot: true,
        version: one(),
        expires: expires(jiff::Timestamp::now(), ROOT_LIFETIME_DAYS)?,
        keys,
        roles,
        _extra: HashMap::new(),
    };

    let key_holder = KeyHolder::Root(root.clone());
    let sources: Vec<Box<dyn KeySource>> = root_keys
        .iter()
        .map(|key| {
            Box::new(LocalKeySource {
                path: key.path.clone(),
            }) as Box<dyn KeySource>
        })
        .collect();
    let signed: SignedRole<Root> = SignedRole::new(
        root,
        &key_holder,
        &sources,
        &aws_lc_rs::rand::SystemRandom::new(),
    )
    .await
    .map_err(|error| tuf_error("root metadata could not be signed", error))?;

    let root_json = metadata_directory.join("root.json");
    std::fs::write(&root_json, signed.buffer())?;
    std::fs::write(metadata_directory.join("1.root.json"), signed.buffer())?;

    // Fail loudly here rather than at install time if the produced root is not self-authenticating.
    let parsed: Signed<Root> = serde_json::from_slice(signed.buffer()).map_err(|error| {
        RuntimeError::Trust(format!("generated root is not valid JSON: {error}"))
    })?;
    parsed.signed.verify_role(&parsed).map_err(|error| {
        RuntimeError::Trust(format!(
            "generated root does not self-authenticate: {error}"
        ))
    })?;

    Ok(root_json)
}

fn role_keys(keys: &[RoleKey], threshold: u64) -> Result<RoleKeys, RuntimeError> {
    Ok(RoleKeys {
        keyids: keys.iter().map(|key| key.key_id.clone()).collect(),
        threshold: NonZeroU64::new(threshold)
            .ok_or_else(|| RuntimeError::Trust("role threshold must be positive".to_owned()))?,
        _extra: HashMap::new(),
    })
}

fn one() -> NonZeroU64 {
    NonZeroU64::new(1).expect("1 is not zero")
}

/// `jiff::Timestamp` arithmetic accepts only hour-or-smaller units, so the ADR's day-based
/// lifetimes are converted rather than expressed as calendar spans.
fn expires(now: jiff::Timestamp, days: i64) -> Result<jiff::Timestamp, RuntimeError> {
    now.checked_add(jiff::Span::new().hours(days.saturating_mul(24)))
        .map_err(|error| RuntimeError::Trust(format!("expiry could not be computed: {error}")))
}

fn tuf_error(context: &str, error: impl std::fmt::Display) -> RuntimeError {
    RuntimeError::Trust(format!("{context}: {error}"))
}

#[cfg(unix)]
fn restrict_directory(path: &Path) -> Result<(), RuntimeError> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(unix)]
fn restrict_file(path: &Path) -> Result<(), RuntimeError> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn restrict_directory(_path: &Path) -> Result<(), RuntimeError> {
    Ok(())
}

#[cfg(not(unix))]
fn restrict_file(_path: &Path) -> Result<(), RuntimeError> {
    Ok(())
}
