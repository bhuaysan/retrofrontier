//! The declarative Runtime Release definition.
//!
//! A release is described by a committed JSON document rather than by hand-created filesystem
//! state, so the same release can be reconstructed from the same pinned upstream inputs. Every
//! identity that can affect what is downloaded, extracted, or launched is pinned here: upstream
//! URL, upstream digest and length, the derived component artifact's digest and length, install
//! paths, executable paths, approved system mappings, and licences.
//!
//! The definition is *input* to construction. It is not itself a trust anchor: the authenticated
//! artefacts a client sees are the TUF metadata and the release manifest that construction emits.

use crate::domain::runtime::{
    parse_strict_json, ArchiveFormat, ComponentKind, RelativePath, ReleaseChannel, RuntimeError,
    SafeIdentifier, Sha256Digest,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const RELEASE_DEFINITION_SCHEMA_VERSION: u32 = 1;

/// The maximum accepted size of a release definition document.
pub const MAX_DEFINITION_BYTES: u64 = 512 * 1024;

/// One pinned upstream download.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseInput {
    pub id: SafeIdentifier,
    /// Must be HTTPS. The construction tool never follows a URL that is not pinned here.
    pub url: String,
    pub sha256: Sha256Digest,
    pub size_bytes: u64,
    pub license: String,
    /// Human-readable provenance, recorded so a reviewer can retrace where the bytes came from.
    pub provenance: String,
}

impl ReleaseInput {
    fn validate(&self) -> Result<(), RuntimeError> {
        if !self.url.starts_with("https://") || self.url.contains('@') {
            return Err(RuntimeError::Manifest(format!(
                "input '{}' has an unsafe source URL",
                self.id
            )));
        }
        if self.size_bytes == 0 {
            return Err(RuntimeError::Manifest(format!(
                "input '{}' has an empty length",
                self.id
            )));
        }
        if self.license.trim().is_empty() || self.provenance.trim().is_empty() {
            return Err(RuntimeError::Manifest(format!(
                "input '{}' is missing licence or provenance metadata",
                self.id
            )));
        }
        Ok(())
    }
}

/// How a component's target artefact is derived from a pinned upstream input.
///
/// Deriving rather than always redistributing the upstream file verbatim is necessary because
/// upstream packaging does not always match the installed layout an approved core needs. Each
/// derivation is deterministic and its result is pinned by digest, so construction either
/// reproduces the approved bytes exactly or fails.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ComponentDerivation {
    /// The upstream bytes become the target artefact unchanged. Strongest provenance.
    UpstreamFile { input: SafeIdentifier },
    /// One named member of a 7z container becomes the target artefact unchanged.
    ///
    /// The official Linux RetroArch artefact is a 7z containing the AppImage next to a large
    /// portable-home asset tree RetroFrontier deliberately does not use, so the AppImage is
    /// lifted out rather than redistributed inside its container.
    SevenZipMember {
        input: SafeIdentifier,
        member: String,
    },
    /// One subtree of a zip is repackaged as a deterministic tar whose root is that subtree.
    ///
    /// Extraction never rewrites archive paths, so a support component whose upstream archive
    /// nests the needed directory (Dolphin's `dolphin-emu/Sys`) has to be re-rooted here.
    ZipSubtreeTar {
        input: SafeIdentifier,
        subtree: String,
    },
}

impl ComponentDerivation {
    pub fn input(&self) -> &SafeIdentifier {
        match self {
            Self::UpstreamFile { input }
            | Self::SevenZipMember { input, .. }
            | Self::ZipSubtreeTar { input, .. } => input,
        }
    }
}

/// One component of the release.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseComponentDefinition {
    pub id: SafeIdentifier,
    pub kind: ComponentKind,
    /// The TUF target name the client resolves. Flat names keep target publication unambiguous.
    pub target_name: String,
    pub archive_format: ArchiveFormat,
    pub install_path: RelativePath,
    pub executable_relative_path: Option<RelativePath>,
    pub display_version: Option<String>,
    pub source_revision: Option<String>,
    pub license: String,
    #[serde(default)]
    pub systems: Vec<SafeIdentifier>,
    pub derivation: ComponentDerivation,
    /// The pinned digest and length of the derived artefact, not of the upstream input.
    pub artifact_sha256: Sha256Digest,
    pub artifact_size_bytes: u64,
}

/// The complete release definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseDefinition {
    pub schema_version: u32,
    pub manifest_id: SafeIdentifier,
    pub release_id: SafeIdentifier,
    pub release_sequence: u64,
    pub channel: ReleaseChannel,
    pub min_retrofrontier_version: String,
    pub retrofrontier_runtime_version: String,
    pub retroarch_version: String,
    pub retroarch_core_api: String,
    pub save_state_policy: String,
    /// The TUF target name of the emitted release manifest.
    pub manifest_target_name: String,
    /// The TUF target name of the emitted runtime policy.
    pub policy_target_name: String,
    pub minimum_safe_release_sequence: u64,
    pub app_run_path: RelativePath,
    pub inputs: Vec<ReleaseInput>,
    pub components: Vec<ReleaseComponentDefinition>,
}

impl ReleaseDefinition {
    pub fn parse(bytes: &[u8]) -> Result<Self, RuntimeError> {
        if bytes.len() as u64 > MAX_DEFINITION_BYTES {
            return Err(RuntimeError::Manifest(
                "release definition is too large".to_owned(),
            ));
        }
        let definition: Self =
            parse_strict_json(bytes).map_err(|error| RuntimeError::Manifest(error.to_owned()))?;
        definition.validate()?;
        Ok(definition)
    }

    /// Structural checks the construction tool needs before it touches the network.
    ///
    /// This deliberately does not repeat `RuntimeManifest::validate_for_linux_x86_64`. The emitted
    /// manifest is validated by that same client-side function during construction, so a
    /// definition that would produce an unacceptable manifest fails there rather than being
    /// re-implemented here.
    pub fn validate(&self) -> Result<(), RuntimeError> {
        if self.schema_version != RELEASE_DEFINITION_SCHEMA_VERSION {
            return Err(RuntimeError::Manifest(format!(
                "unsupported release definition schema version {}",
                self.schema_version
            )));
        }
        if self.release_sequence == 0 {
            return Err(RuntimeError::Manifest(
                "release sequence must be positive".to_owned(),
            ));
        }
        if self.minimum_safe_release_sequence > self.release_sequence {
            return Err(RuntimeError::Manifest(
                "release is below its own minimum safe sequence".to_owned(),
            ));
        }
        if self.components.is_empty() {
            return Err(RuntimeError::Manifest(
                "release definition has no components".to_owned(),
            ));
        }

        let mut input_ids = BTreeSet::new();
        for input in &self.inputs {
            input.validate()?;
            if !input_ids.insert(input.id.clone()) {
                return Err(RuntimeError::Manifest(format!(
                    "duplicate input id '{}'",
                    input.id
                )));
            }
        }

        let mut target_names = BTreeSet::new();
        for reserved in [&self.manifest_target_name, &self.policy_target_name] {
            validate_target_name(reserved)?;
            if !target_names.insert(reserved.clone()) {
                return Err(RuntimeError::Manifest(
                    "manifest and policy cannot share a target name".to_owned(),
                ));
            }
        }

        let mut component_ids = BTreeSet::new();
        for component in &self.components {
            if !component_ids.insert(component.id.clone()) {
                return Err(RuntimeError::Manifest(format!(
                    "duplicate component id '{}'",
                    component.id
                )));
            }
            validate_target_name(&component.target_name)?;
            if !target_names.insert(component.target_name.clone()) {
                return Err(RuntimeError::Manifest(format!(
                    "duplicate target name '{}'",
                    component.target_name
                )));
            }
            if component.artifact_size_bytes == 0 {
                return Err(RuntimeError::Manifest(format!(
                    "component '{}' has an empty artefact length",
                    component.id
                )));
            }
            if !input_ids.contains(component.derivation.input()) {
                return Err(RuntimeError::Manifest(format!(
                    "component '{}' derives from unknown input '{}'",
                    component.id,
                    component.derivation.input()
                )));
            }
            if let ComponentDerivation::ZipSubtreeTar { subtree, .. } = &component.derivation {
                RelativePath::new(subtree.clone()).map_err(|_| {
                    RuntimeError::Manifest(format!(
                        "component '{}' declares an unsafe subtree",
                        component.id
                    ))
                })?;
            }
            if let ComponentDerivation::SevenZipMember { member, .. } = &component.derivation {
                RelativePath::new(member.clone()).map_err(|_| {
                    RuntimeError::Manifest(format!(
                        "component '{}' declares an unsafe 7z member",
                        component.id
                    ))
                })?;
            }
        }
        Ok(())
    }

    pub fn input(&self, id: &SafeIdentifier) -> Result<&ReleaseInput, RuntimeError> {
        self.inputs
            .iter()
            .find(|input| &input.id == id)
            .ok_or_else(|| RuntimeError::Manifest(format!("unknown input '{id}'")))
    }
}

/// Target names are flat, relative, and safe: a target name is used as a filename in the published
/// repository, so a nested or traversing name would make publication ambiguous.
fn validate_target_name(name: &str) -> Result<(), RuntimeError> {
    let path = RelativePath::new(name.to_owned())
        .map_err(|_| RuntimeError::Manifest(format!("target name '{name}' is unsafe")))?;
    if path.as_str().contains('/') {
        return Err(RuntimeError::Manifest(format!(
            "target name '{name}' must be a flat filename"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ReleaseDefinition, RELEASE_DEFINITION_SCHEMA_VERSION};

    fn definition_json(mutate: impl FnOnce(&mut serde_json::Value)) -> Vec<u8> {
        let mut value: serde_json::Value = serde_json::json!({
            "schema_version": RELEASE_DEFINITION_SCHEMA_VERSION,
            "manifest_id": "test-manifest",
            "release_id": "test-release",
            "release_sequence": 1,
            "channel": "stable",
            "min_retrofrontier_version": "0.1.0",
            "retrofrontier_runtime_version": "1",
            "retroarch_version": "1.22.2",
            "retroarch_core_api": "1",
            "save_state_policy": "isolated",
            "manifest_target_name": "release.json",
            "policy_target_name": "runtime-policy.json",
            "minimum_safe_release_sequence": 1,
            "app_run_path": "runtime/retroarch/AppRun",
            "inputs": [{
                "id": "retroarch-archive",
                "url": "https://example.invalid/RetroArch.7z",
                "sha256": "a".repeat(64),
                "size_bytes": 10,
                "license": "GPL-3.0-only",
                "provenance": "test"
            }],
            "components": [{
                "id": "retroarch",
                "kind": "runtime",
                "target_name": "retroarch.AppImage",
                "archive_format": "app_image",
                "install_path": "runtime/retroarch",
                "executable_relative_path": "usr/bin/retroarch",
                "display_version": "1.22.2",
                "source_revision": "abc1234",
                "license": "GPL-3.0-only",
                "systems": [],
                "derivation": {
                    "kind": "seven_zip_member",
                    "input": "retroarch-archive",
                    "member": "RetroArch-Linux-x86_64/RetroArch-Linux-x86_64.AppImage"
                },
                "artifact_sha256": "b".repeat(64),
                "artifact_size_bytes": 10
            }]
        });
        mutate(&mut value);
        serde_json::to_vec(&value).unwrap()
    }

    #[test]
    fn a_complete_definition_parses() {
        let definition = ReleaseDefinition::parse(&definition_json(|_| {})).unwrap();
        assert_eq!(definition.components.len(), 1);
        assert_eq!(definition.release_sequence, 1);
    }

    #[test]
    fn an_unpinned_or_unsafe_input_is_refused() {
        let plaintext = definition_json(|value| {
            value["inputs"][0]["url"] = serde_json::json!("http://example.invalid/RetroArch.7z");
        });
        assert!(ReleaseDefinition::parse(&plaintext).is_err());

        let credentials = definition_json(|value| {
            value["inputs"][0]["url"] =
                serde_json::json!("https://user@example.invalid/RetroArch.7z");
        });
        assert!(ReleaseDefinition::parse(&credentials).is_err());
    }

    #[test]
    fn a_component_cannot_derive_from_an_undeclared_input() {
        let orphan = definition_json(|value| {
            value["components"][0]["derivation"]["input"] = serde_json::json!("not-declared");
        });
        assert!(ReleaseDefinition::parse(&orphan).is_err());
    }

    #[test]
    fn target_names_stay_flat_and_unique() {
        let nested = definition_json(|value| {
            value["components"][0]["target_name"] = serde_json::json!("targets/retroarch.AppImage");
        });
        assert!(ReleaseDefinition::parse(&nested).is_err());

        let duplicate = definition_json(|value| {
            value["components"][0]["target_name"] = serde_json::json!("runtime-policy.json");
        });
        assert!(ReleaseDefinition::parse(&duplicate).is_err());
    }

    #[test]
    fn a_release_below_its_own_security_floor_is_refused() {
        let regressed = definition_json(|value| {
            value["minimum_safe_release_sequence"] = serde_json::json!(2);
        });
        assert!(ReleaseDefinition::parse(&regressed).is_err());
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let extra = definition_json(|value| {
            value["surprise"] = serde_json::json!(true);
        });
        assert!(ReleaseDefinition::parse(&extra).is_err());
    }
}
