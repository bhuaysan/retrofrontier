use crate::domain::runtime::{RuntimeArchitecture, RuntimePlatform};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use thiserror::Error;

/// Stable identity for a libretro core. This is deliberately separate from a display name and
/// from the runtime component's authenticated payload metadata.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CoreId(String);

impl CoreId {
    pub fn new(value: impl Into<String>) -> Result<Self, CoreIdError> {
        let value = value.into();
        validate_identifier(&value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CoreId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl Serialize for CoreId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for CoreId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

impl TryFrom<&str> for CoreId {
    type Error = CoreIdError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<String> for CoreId {
    type Error = CoreIdError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CoreIdError {
    #[error("core identifier must not be empty")]
    Empty,
    #[error("core identifier is too long")]
    TooLong,
    #[error("core identifier contains an unsafe character")]
    UnsafeCharacter,
}

fn validate_identifier(value: &str) -> Result<(), CoreIdError> {
    if value.is_empty() {
        return Err(CoreIdError::Empty);
    }
    if value.len() > 128 {
        return Err(CoreIdError::TooLong);
    }
    let mut characters = value.chars();
    let Some(first) = characters.next() else {
        return Err(CoreIdError::Empty);
    };
    if !first.is_ascii_alphanumeric()
        || !characters.all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
    {
        return Err(CoreIdError::UnsafeCharacter);
    }
    Ok(())
}

/// A platform/architecture pair supported by a managed core component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreTarget {
    pub platform: RuntimePlatform,
    pub architecture: RuntimeArchitecture,
}

/// Authenticated managed support data an approved core needs beside the core itself.
///
/// Dolphin's `Sys` directory is the M7 case: the core refuses to work correctly without it, and it
/// must come from the verified managed runtime rather than from an unrelated user installation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreSupportAsset {
    /// The authenticated `RuntimeComponent::id` that installs the support data.
    pub component_id: CoreId,
    /// Where the core expects it, relative to RetroArch's system directory.
    pub system_directory_path: String,
}

/// A core's static policy identity. It contains no TUF signatures, hashes, or mutable installed
/// state; those remain owned by RuntimeManager and its trusted release boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreDefinition {
    pub id: CoreId,
    pub libretro_name: String,
    pub display_name: String,
    pub systems: Vec<crate::domain::system::SystemId>,
    pub targets: Vec<CoreTarget>,
    /// The authenticated `RuntimeComponent::id` that installs this core. Static policy never
    /// assumes it equals `id`; launch and availability both resolve through this value.
    pub managed_component_id: CoreId,
    pub default_for_systems: Vec<crate::domain::system::SystemId>,
    /// Recorded so an approved core's licence is inspectable without reading a release manifest.
    pub license: String,
    /// Upstream project the approved managed component is built from.
    pub source_url: String,
    /// Authenticated managed support components this core requires.
    pub support_assets: Vec<CoreSupportAsset>,
}

impl CoreDefinition {
    /// Approved cores are platform-specific. A definition that does not declare the running
    /// platform/architecture is not approved there, even when a component happens to be installed.
    pub fn supports_target(&self, target: CoreTarget) -> bool {
        self.targets.contains(&target)
    }

    pub fn supports_current_target(&self) -> bool {
        current_core_target().is_some_and(|target| self.supports_target(target))
    }
}

/// The platform/architecture RetroFrontier is currently running on, when it is one V1 supports.
pub fn current_core_target() -> Option<CoreTarget> {
    let platform = match std::env::consts::OS {
        "linux" => RuntimePlatform::Linux,
        "macos" => RuntimePlatform::Macos,
        "windows" => RuntimePlatform::Windows,
        _ => return None,
    };
    let architecture = match std::env::consts::ARCH {
        "x86_64" => RuntimeArchitecture::X86_64,
        "aarch64" => RuntimeArchitecture::Aarch64,
        _ => return None,
    };
    Some(CoreTarget {
        platform,
        architecture,
    })
}

/// The policy decision is intentionally explicit while the V1 matrix is unresolved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CorePolicyDecision {
    Resolved,
    Unresolved {
        #[serde(rename = "researchItem")]
        research_item: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CorePolicy {
    pub default_core_id: Option<CoreId>,
    pub approved_core_ids: Vec<CoreId>,
    pub decision: CorePolicyDecision,
}

impl CorePolicy {
    pub fn unresolved(research_item: impl Into<String>) -> Self {
        Self {
            default_core_id: None,
            approved_core_ids: Vec::new(),
            decision: CorePolicyDecision::Unresolved {
                research_item: research_item.into(),
            },
        }
    }

    pub fn resolved(default_core_id: CoreId, approved_core_ids: Vec<CoreId>) -> Self {
        Self {
            default_core_id: Some(default_core_id),
            approved_core_ids,
            decision: CorePolicyDecision::Resolved,
        }
    }
}
