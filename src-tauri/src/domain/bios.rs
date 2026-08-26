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
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BiosDigest {
    pub algorithm: BiosHashAlgorithm,
    pub value: String,
}

impl BiosDigest {
    pub fn sha256(value: impl Into<String>) -> Result<Self, BiosModelError> {
        let value = value.into().to_ascii_lowercase();
        let digest = Self {
            algorithm: BiosHashAlgorithm::Sha256,
            value,
        };
        digest.validate()?;
        Ok(digest)
    }

    pub fn validate(&self) -> Result<(), BiosModelError> {
        match self.algorithm {
            BiosHashAlgorithm::Sha256
                if self.value.len() == 64
                    && self.value.bytes().all(|byte| byte.is_ascii_hexdigit()) =>
            {
                Ok(())
            }
            BiosHashAlgorithm::Sha256 => Err(BiosModelError::InvalidDigest(
                "SHA-256 digest must contain 64 hexadecimal characters".to_owned(),
            )),
        }
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BiosRequirement {
    pub id: BiosRequirementId,
    pub system_id: SystemId,
    pub expected_filenames: Vec<String>,
    pub expected_hashes: Vec<BiosDigest>,
    pub expected_size_bytes: Option<u64>,
    pub kind: BiosRequirementKind,
    pub description: String,
}

impl BiosRequirement {
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
        let requirement = Self {
            id: BiosRequirementId::new(id)?,
            system_id,
            expected_filenames,
            expected_hashes,
            expected_size_bytes,
            kind,
            description: description.into(),
        };
        requirement.validate()?;
        Ok(requirement)
    }

    pub fn validate(&self) -> Result<(), BiosModelError> {
        if self.expected_filenames.is_empty() {
            return Err(BiosModelError::NoExpectedFilenames);
        }
        let mut filenames = std::collections::BTreeSet::new();
        for filename in &self.expected_filenames {
            if filename.is_empty()
                || filename == "."
                || filename == ".."
                || filename.contains('/')
                || filename.contains('\\')
                || filename.contains(':')
                || filename.chars().any(char::is_control)
                || !filenames.insert(filename)
            {
                return Err(BiosModelError::InvalidFilename(filename.clone()));
            }
        }
        for digest in &self.expected_hashes {
            digest.validate()?;
        }
        if self.expected_size_bytes == Some(0) {
            return Err(BiosModelError::InvalidExpectedSize);
        }
        if self.description.trim().is_empty() {
            return Err(BiosModelError::EmptyDescription);
        }
        Ok(())
    }

    /// A filename alone is not enough to recognize a BIOS dump. Until an authoritative digest is
    /// recorded, a present file remains explicitly uncovered by the catalog.
    pub fn has_authoritative_identity(&self) -> bool {
        !self.expected_hashes.is_empty()
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
