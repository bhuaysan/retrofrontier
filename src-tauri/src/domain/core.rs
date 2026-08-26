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
    pub managed_component_id: CoreId,
    pub default_for_systems: Vec<crate::domain::system::SystemId>,
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
