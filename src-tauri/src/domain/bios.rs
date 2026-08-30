use crate::domain::system::SystemId;
use serde::Serialize;
use std::fmt;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BiosPolicy {
    NotRequired,
    Optional,
    Required,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BiosRequirementKind {
    Required,
    Optional,
}

impl BiosRequirementKind {
    pub fn is_required(self) -> bool {
        matches!(self, Self::Required)
    }
}

impl BiosPolicy {
    pub fn from_requirements(requirements: &[BiosRequirement]) -> Self {
        if requirements
            .iter()
            .any(|requirement| requirement.kind.is_required())
        {
            Self::Required
        } else if requirements.is_empty() {
            Self::NotRequired
        } else {
            Self::Optional
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BiosRequirementId(String);

impl BiosRequirementId {
    pub fn new(value: impl Into<String>) -> Result<Self, BiosModelError> {
        let value = value.into();
        if value.is_empty() {
            return Err(BiosModelError::InvalidRequirementId(
                "requirement identifier must not be empty".to_owned(),
            ));
        }
        if value.len() > 128
            || !value.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
            })
        {
            return Err(BiosModelError::InvalidRequirementId(format!(
                "requirement identifier '{value}' contains an unsafe character"
            )));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for BiosRequirementId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl Serialize for BiosRequirementId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BiosHashAlgorithm {
    Sha256,
    /// Several approved cores publish their accepted BIOS dumps as MD5 only. Recording the
    /// published algorithm is safer than inventing an unverifiable SHA-256 value.
    Md5,
}

impl BiosHashAlgorithm {
    const fn hex_length(self) -> usize {
        match self {
            Self::Sha256 => 64,
            Self::Md5 => 32,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BiosDigest {
    pub algorithm: BiosHashAlgorithm,
    pub value: String,
}

impl BiosDigest {
    pub fn sha256(value: impl Into<String>) -> Result<Self, BiosModelError> {
        Self::new(BiosHashAlgorithm::Sha256, value)
    }

    pub fn md5(value: impl Into<String>) -> Result<Self, BiosModelError> {
        Self::new(BiosHashAlgorithm::Md5, value)
    }

    fn new(algorithm: BiosHashAlgorithm, value: impl Into<String>) -> Result<Self, BiosModelError> {
        let digest = Self {
            algorithm,
            value: value.into().to_ascii_lowercase(),
        };
        digest.validate()?;
        Ok(digest)
    }

    pub fn validate(&self) -> Result<(), BiosModelError> {
        let expected = self.algorithm.hex_length();
        if self.value.len() == expected && self.value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Ok(());
        }
        Err(BiosModelError::InvalidDigest(format!(
            "{:?} digest must contain {expected} hexadecimal characters",
            self.algorithm
        )))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BiosModelError {
    #[error("invalid BIOS requirement id: {0}")]
    InvalidRequirementId(String),
    #[error("invalid BIOS filename '{0}'")]
    InvalidFilename(String),
    #[error("BIOS requirement must declare at least one expected filename")]
    NoExpectedFilenames,
    #[error("BIOS requirement description must not be empty")]
    EmptyDescription,
    #[error("BIOS expected size must be greater than zero")]
    InvalidExpectedSize,
    #[error("invalid BIOS digest: {0}")]
    InvalidDigest(String),
}

/// One BIOS dump an approved core accepts, identified by its own filename.
///
/// Identity is per file on purpose: a genuine dump stored under a different filename is not the
/// file the core loads, so accepting it would report a valid BIOS that still fails at launch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BiosFileIdentity {
    pub filename: String,
    pub size_bytes: Option<u64>,
    pub digests: Vec<BiosDigest>,
}

impl BiosFileIdentity {
    pub fn new(
        filename: impl Into<String>,
        size_bytes: Option<u64>,
        digests: Vec<BiosDigest>,
    ) -> Result<Self, BiosModelError> {
        let identity = Self {
            filename: filename.into(),
            size_bytes,
            digests,
        };
        identity.validate()?;
        Ok(identity)
    }

    pub fn validate(&self) -> Result<(), BiosModelError> {
        let filename = &self.filename;
        if filename.is_empty()
            || filename == "."
            || filename == ".."
            || filename.contains('/')
            || filename.contains('\\')
            || filename.contains(':')
            || filename.chars().any(char::is_control)
        {
            return Err(BiosModelError::InvalidFilename(filename.clone()));
        }
        for digest in &self.digests {
            digest.validate()?;
        }
        if self.size_bytes == Some(0) {
            return Err(BiosModelError::InvalidExpectedSize);
        }
        Ok(())
    }

    /// A filename alone is not an identity. Until an authoritative digest exists, a present file
    /// is explicitly uncovered by the catalog rather than valid.
    pub fn has_authoritative_identity(&self) -> bool {
        !self.digests.is_empty()
    }

    /// Compare the observed digests against every accepted identity for this filename.
    ///
    /// Size is validated separately, because a known size disproves identity even where an
    /// authoritative digest is still unresolved.
    pub fn matches_digest(&self, sha256: &str, md5: &str) -> bool {
        self.digests.iter().any(|digest| {
            let observed = match digest.algorithm {
                BiosHashAlgorithm::Sha256 => sha256,
                BiosHashAlgorithm::Md5 => md5,
            };
            digest.value.eq_ignore_ascii_case(observed)
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BiosRequirement {
    pub id: BiosRequirementId,
    pub system_id: SystemId,
    pub accepted_files: Vec<BiosFileIdentity>,
    pub kind: BiosRequirementKind,
    pub description: String,
}

impl BiosRequirement {
    /// Convenience constructor for requirements whose accepted dumps share one identity set, and
    /// for requirements whose authoritative identity is still unresolved.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: impl Into<String>,
        system_id: SystemId,
        expected_filenames: Vec<String>,
        expected_hashes: Vec<BiosDigest>,
        expected_size_bytes: Option<u64>,
        kind: BiosRequirementKind,
        description: impl Into<String>,
    ) -> Result<Self, BiosModelError> {
        let accepted_files = expected_filenames
            .into_iter()
            .map(|filename| BiosFileIdentity {
                filename,
                size_bytes: expected_size_bytes,
                digests: expected_hashes.clone(),
            })
            .collect();
        Self::with_files(id, system_id, accepted_files, kind, description)
    }

    pub fn with_files(
        id: impl Into<String>,
        system_id: SystemId,
        accepted_files: Vec<BiosFileIdentity>,
        kind: BiosRequirementKind,
        description: impl Into<String>,
    ) -> Result<Self, BiosModelError> {
        let requirement = Self {
            id: BiosRequirementId::new(id)?,
            system_id,
            accepted_files,
            kind,
            description: description.into(),
        };
        requirement.validate()?;
        Ok(requirement)
    }

    pub fn validate(&self) -> Result<(), BiosModelError> {
        if self.accepted_files.is_empty() {
            return Err(BiosModelError::NoExpectedFilenames);
        }
        let mut filenames = std::collections::BTreeSet::new();
        for file in &self.accepted_files {
            file.validate()?;
            if !filenames.insert(&file.filename) {
                return Err(BiosModelError::InvalidFilename(file.filename.clone()));
            }
        }
        if self.description.trim().is_empty() {
            return Err(BiosModelError::EmptyDescription);
        }
        Ok(())
    }

    pub fn expected_filenames(&self) -> Vec<String> {
        self.accepted_files
            .iter()
            .map(|file| file.filename.clone())
            .collect()
    }

    /// Reported only when every accepted dump agrees on a size, so the status stays truthful for
    /// requirements whose accepted files differ.
    pub fn expected_size_bytes(&self) -> Option<u64> {
        let mut sizes = self.accepted_files.iter().map(|file| file.size_bytes);
        let first = sizes.next().flatten()?;
        sizes.all(|size| size == Some(first)).then_some(first)
    }

    /// True only when every accepted dump carries an authoritative digest.
    pub fn has_authoritative_identity(&self) -> bool {
        self.accepted_files
            .iter()
            .all(BiosFileIdentity::has_authoritative_identity)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BiosRootStatus {
    Ready,
    Missing,
    NotDirectory,
    Unsafe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BiosRequirementStatusState {
    PresentValid,
    Missing,
    PresentInvalid,
    OptionalMissing,
    NotCoveredByCatalog,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BiosRequirementStatus {
    pub requirement_id: BiosRequirementId,
    pub system_id: SystemId,
    pub required: bool,
    pub state: BiosRequirementStatusState,
    pub expected_filenames: Vec<String>,
    pub expected_size_bytes: Option<u64>,
    pub description: String,
    pub matched_filename: Option<String>,
    pub file_size_bytes: Option<u64>,
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BiosDiscovery {
    pub root: String,
    pub root_status: BiosRootStatus,
    pub requirements: Vec<BiosRequirementStatus>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemBiosStatus {
    pub policy: BiosPolicy,
    pub ready: bool,
    pub requirements: Vec<BiosRequirementStatus>,
}

impl SystemBiosStatus {
    pub fn from_requirements(policy: BiosPolicy, requirements: Vec<BiosRequirementStatus>) -> Self {
        let ready = requirements.iter().all(|requirement| {
            !requirement.required || requirement.state == BiosRequirementStatusState::PresentValid
        });
        Self {
            policy,
            ready,
            requirements,
        }
    }
}
