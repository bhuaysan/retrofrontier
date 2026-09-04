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

/// The committed *active* Linux x86_64 release definition, relative to the crate manifest.
///
/// Exactly one definition per platform is the build input. Superseded generations are kept beside
/// it as historical records and are never built or published again.
pub const ACTIVE_LINUX_DEFINITION: &str = "../release/linux-x86_64/runtime-release.json";

/// Every superseded Linux x86_64 release generation, oldest first, relative to the crate manifest.
pub const HISTORICAL_LINUX_DEFINITIONS: &[&str] =
    &["../release/linux-x86_64/history/runtime-release-001.json"];

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
    /// One named member of a 7z container is packaged as a deterministic single-entry tar.
    ///
    /// The official *version-addressed* RetroArch stable core bundle ships every libretro core as a
    /// bare `.so` inside one 7z, so a core taken from it has no upstream archive of its own to
    /// redistribute verbatim. Deriving a tar keeps the component a self-describing archive whose
    /// installed layout is declared, rather than introducing a bare-file component format.
    SevenZipMemberTar {
        input: SafeIdentifier,
        member: String,
        /// The flat filename the member is installed as, inside the component's install path.
        entry_name: String,
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
            | Self::SevenZipMemberTar { input, .. }
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

/// How the emitted manifest carries the installed-file inventory.
///
/// ADR-012 permits the complete installed-file inventory to live in a separate immutable target
/// referenced by digest, which is what the growing core matrix needs: the four-core Release 002
/// manifest already uses about 60 % of `MAX_MANIFEST_BYTES`. The representation is a stated
/// property of the definition, never something construction picks by size, so a published release
/// is reconstructible from its committed definition alone.
///
/// The default is `Inline`, so an existing committed definition that omits this field keeps
/// producing byte-identical manifests.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "representation", rename_all = "snake_case", deny_unknown_fields)]
pub enum InventoryPublication {
    #[default]
    Inline,
    DetachedTarget {
        target_name: String,
    },
}

impl InventoryPublication {
    pub fn target_name(&self) -> Option<&str> {
        match self {
            Self::Inline => None,
            Self::DetachedTarget { target_name } => Some(target_name),
        }
    }
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
    /// The installed-file inventory representation the emitted manifest uses.
    #[serde(default)]
    pub inventory: InventoryPublication,
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
        // The detached inventory, when published, is a target in the same namespace as the
        // manifest, the policy, and every component, so it needs the same flat-name and
        // uniqueness rules. A shared name would let one authenticated target stand in for another.
        let reserved_names: Vec<&str> = [
            self.manifest_target_name.as_str(),
            self.policy_target_name.as_str(),
        ]
        .into_iter()
        .chain(self.inventory.target_name())
        .collect();
        for reserved in reserved_names {
            validate_target_name(reserved)?;
            if !target_names.insert(reserved.to_owned()) {
                return Err(RuntimeError::Manifest(
                    "manifest, policy, and inventory cannot share a target name".to_owned(),
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
            if let ComponentDerivation::SevenZipMemberTar {
                member, entry_name, ..
            } = &component.derivation
            {
                RelativePath::new(member.clone()).map_err(|_| {
                    RuntimeError::Manifest(format!(
                        "component '{}' declares an unsafe 7z member",
                        component.id
                    ))
                })?;
                validate_target_name(entry_name).map_err(|_| {
                    RuntimeError::Manifest(format!(
                        "component '{}' declares an unsafe 7z member entry name",
                        component.id
                    ))
                })?;
            }
        }
        Ok(())
    }

    /// The authenticated contents of this release: what a client actually ends up installing.
    ///
    /// Keyed by component id, valued by the target name and the pinned artefact identity, because
    /// those three together are what TUF publishes and what the client verifies. Release-level
    /// metadata is deliberately excluded — this answers "do two definitions ship the same bytes",
    /// not "are two definitions textually equal".
    pub fn authenticated_contents(&self) -> std::collections::BTreeMap<String, String> {
        let mut contents: std::collections::BTreeMap<String, String> = self
            .components
            .iter()
            .map(|component| {
                (
                    component.id.as_str().to_owned(),
                    format!(
                        "{}:sha256:{}:{}",
                        component.target_name,
                        component.artifact_sha256.to_hex(),
                        component.artifact_size_bytes
                    ),
                )
            })
            .collect();
        // The inventory representation is part of what a client authenticates, because it changes
        // the emitted manifest bytes and which targets exist. Switching it under a published
        // release identity would republish a different manifest under an immutable target name, so
        // it counts as changed authenticated contents and forces a new generation. A component id
        // is a `SafeIdentifier` and must begin alphanumeric, so this reserved key cannot collide.
        contents.insert(
            "@inventory-representation".to_owned(),
            match &self.inventory {
                InventoryPublication::Inline => "inline".to_owned(),
                InventoryPublication::DetachedTarget { target_name } => {
                    format!("detached_target:{target_name}")
                }
            },
        );
        contents
    }

    /// Check that `self` is a legitimate successor of the already-published `previous` release.
    ///
    /// ADR-012 gives a Runtime Release an immutable id and a monotonically increasing sequence, and
    /// makes the authenticated targets immutable. Those three rules have one consequence that is
    /// easy to violate by editing a definition in place: changing what a release ships is not a
    /// re-publication of that release, it is a *new* release. Both halves are enforced here.
    ///
    /// This is not the anti-rollback floor. `minimum_safe_release_sequence` is a security
    /// revocation decision about a specific compromised or unsafe release, so it is deliberately
    /// not raised just because a newer generation exists.
    pub fn supersedes(&self, previous: &Self) -> Result<(), RuntimeError> {
        if self.release_sequence <= previous.release_sequence {
            return Err(RuntimeError::Manifest(format!(
                "release '{}' has sequence {} which does not exceed the sequence {} of '{}'",
                self.release_id,
                self.release_sequence,
                previous.release_sequence,
                previous.release_id
            )));
        }
        if self.authenticated_contents() == previous.authenticated_contents() {
            return Ok(());
        }
        for (label, current, earlier) in [
            (
                "release id",
                self.release_id.as_str(),
                previous.release_id.as_str(),
            ),
            (
                "manifest id",
                self.manifest_id.as_str(),
                previous.manifest_id.as_str(),
            ),
            (
                "manifest target name",
                self.manifest_target_name.as_str(),
                previous.manifest_target_name.as_str(),
            ),
        ] {
            if current == earlier {
                return Err(RuntimeError::Manifest(format!(
                    "authenticated contents changed but the {label} '{current}' is unchanged; \
                     a new release generation is required"
                )));
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
    use crate::domain::runtime::ComponentKind;

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

    /// The committed Linux release definition is a reviewed artefact, so its shape is asserted
    /// here rather than only being exercised by a maintainer running the construction tool.
    ///
    /// B2/B4: the managed controller-profile database is a pinned, authenticated component of the
    /// release, taken from an immutable upstream revision, and no host RetroArch location appears
    /// anywhere in the definition.
    #[test]
    fn the_committed_linux_definition_pins_the_managed_controller_profiles() {
        let definition = active_definition();

        let component = definition
            .components
            .iter()
            .find(|component| component.id.as_str() == "joypad-autoconfig")
            .expect("the release ships managed controller profiles");
        assert_eq!(
            component.kind,
            crate::domain::runtime::ComponentKind::SupportAsset
        );
        assert_eq!(
            component.install_path.as_str(),
            "runtime/support/joypad-autoconfig"
        );
        assert!(component.executable_relative_path.is_none());
        // An immutable upstream revision, not a rolling "latest" asset.
        assert_eq!(
            component.source_revision.as_deref(),
            Some("38cf938bba0adbde375972053068f10d955a9d14")
        );
        assert_eq!(component.license, "MIT");
        // The derived artefact is pinned by its own digest and length, not only the upstream input's.
        assert!(component.artifact_size_bytes > 0);

        let input = definition.input(component.derivation.input()).unwrap();
        assert!(input
            .url
            .starts_with("https://codeload.github.com/libretro/retroarch-joypad-autoconfig/zip/"));
        assert!(input
            .url
            .ends_with("38cf938bba0adbde375972053068f10d955a9d14"));
        assert!(input.provenance.contains("libretro"));

        // B4: no host RetroArch autoconfig location is a source for anything in this release.
        for input in &definition.inputs {
            for forbidden in [
                "/usr/share/libretro/autoconfig",
                "/.config/retroarch",
                "/.local/share/retroarch",
            ] {
                assert!(!input.url.contains(forbidden), "{forbidden}");
            }
        }
    }

    fn read_definition(relative: &str) -> ReleaseDefinition {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(relative);
        ReleaseDefinition::parse(&std::fs::read(&path).unwrap())
            .unwrap_or_else(|error| panic!("{relative} must parse: {error}"))
    }

    fn active_definition() -> ReleaseDefinition {
        read_definition(super::ACTIVE_LINUX_DEFINITION)
    }

    /// R1 — changing what a release ships while keeping its identity is refused.
    ///
    /// This is the finding that produced Release 002: the managed controller-profile component
    /// changed the authenticated contents while `rf-runtime-1.22.2-linux-x86_64-001`,
    /// `release_sequence = 1`, and `rf-runtime-linux-x86_64-001.manifest.json` were left in place.
    /// ADR-012 makes an authenticated Runtime Release target immutable, so that is a new
    /// generation, not a re-publication.
    #[test]
    fn changed_contents_cannot_keep_the_previous_release_identity() {
        let active = active_definition();
        let previous = read_definition(super::HISTORICAL_LINUX_DEFINITIONS[0]);

        assert_ne!(
            active.authenticated_contents(),
            previous.authenticated_contents(),
            "Release 002 exists because its authenticated contents differ from Release 001"
        );
        active
            .supersedes(&previous)
            .expect("the committed active definition must be a legitimate successor");

        // The exact defect: same identity, different contents.
        let mut impostor = active.clone();
        impostor.release_id = previous.release_id.clone();
        impostor.manifest_id = previous.manifest_id.clone();
        impostor.manifest_target_name = previous.manifest_target_name.clone();
        impostor.release_sequence = previous.release_sequence + 1;
        let error = impostor
            .supersedes(&previous)
            .expect_err("changed contents under a published identity must be refused");
        assert!(
            format!("{error}").contains("release id"),
            "unexpected error: {error}"
        );

        // One changed component pin is enough; it does not take a whole new component.
        let mut repinned = previous.clone();
        repinned.release_sequence += 1;
        repinned.components[1].artifact_sha256 =
            crate::domain::runtime::Sha256Digest::from_hex(&"c".repeat(64)).unwrap();
        assert!(repinned.supersedes(&previous).is_err());

        // Republishing byte-identical contents under the same identity is not what this forbids.
        let mut reissued = previous.clone();
        reissued.release_sequence += 1;
        reissued.supersedes(&previous).unwrap();
    }

    /// R2 — a new generation must advance the release sequence.
    #[test]
    fn a_new_release_sequence_must_exceed_the_previous_one() {
        let active = active_definition();
        let previous = read_definition(super::HISTORICAL_LINUX_DEFINITIONS[0]);

        assert!(
            active.release_sequence > previous.release_sequence,
            "the active definition must advance the sequence"
        );

        for regressed in [previous.release_sequence, previous.release_sequence - 1] {
            let mut candidate = active.clone();
            candidate.release_sequence = regressed;
            candidate.minimum_safe_release_sequence = regressed;
            let error = candidate
                .supersedes(&previous)
                .expect_err("a non-advancing sequence must be refused");
            assert!(
                format!("{error}").contains("does not exceed"),
                "unexpected error: {error}"
            );
        }

        // The anti-rollback floor is a security decision, not a version selector, so a new
        // generation on its own must not raise it. Release 001 is superseded, not revoked.
        assert_eq!(
            active.minimum_safe_release_sequence, previous.minimum_safe_release_sequence,
            "minimum_safe_release_sequence is an anti-rollback floor and needs its own decision"
        );
    }

    /// R3 — no rolling core input may reach the active release definition.
    ///
    /// The four `buildbot.libretro.com/nightly/linux/x86_64/latest/` core URLs Release 001 pinned
    /// had already been rotated upstream, which made a committed, published release impossible to
    /// reconstruct. Re-pinning a rolling URL only resets the clock, so the active definition must
    /// name no rolling path at all.
    #[test]
    fn the_active_definition_pins_no_rolling_upstream_url() {
        let active = active_definition();
        for input in &active.inputs {
            for rolling in ["/nightly/", "/latest/"] {
                assert!(
                    !input.url.contains(rolling),
                    "input '{}' pins the rolling path '{rolling}': {}",
                    input.id,
                    input.url
                );
            }
        }

        // Every core is derived from the version-addressed stable bundle for the pinned RetroArch
        // version, so the core bytes are addressed by that version rather than by a moving pointer.
        let bundle = "https://buildbot.libretro.com/stable/1.22.2/linux/x86_64/RetroArch_cores.7z";
        for component in &active.components {
            if component.kind != ComponentKind::Core {
                continue;
            }
            let input = active.input(component.derivation.input()).unwrap();
            assert_eq!(
                input.url, bundle,
                "core '{}' must come from the version-addressed stable bundle",
                component.id
            );
            assert!(
                matches!(
                    component.derivation,
                    super::ComponentDerivation::SevenZipMemberTar { .. }
                ),
                "core '{}' must be derived from a named bundle member",
                component.id
            );
        }
    }

    /// R4 — the active release must be complete, `joypad-autoconfig` included.
    ///
    /// A release built without the managed controller-profile component installs a runtime whose
    /// controller does not work inside RetroArch, which is the M8 acceptance blocker.
    #[test]
    fn the_active_definition_declares_every_required_component() {
        let active = active_definition();
        let declared: Vec<&str> = active
            .components
            .iter()
            .map(|component| component.id.as_str())
            .collect();
        assert_eq!(
            declared,
            vec![
                "retroarch",
                "nestopia",
                "bsnes-mercury-balanced",
                "beetle-psx",
                "dolphin",
                "dolphin-sys",
                "joypad-autoconfig",
            ]
        );
    }

    /// R5 — qualification must select the active manifest, never a superseded one.
    ///
    /// The manifest target is chosen by `RETROFRONTIER_RUNTIME_MANIFEST_TARGET`, so the documented
    /// qualification configuration *is* the selection. A superseded target name left behind there
    /// sends an operator's requalification at the old release.
    #[test]
    fn every_documented_qualification_target_is_the_active_manifest() {
        let active = active_definition();
        let superseded: Vec<String> = super::HISTORICAL_LINUX_DEFINITIONS
            .iter()
            .map(|path| read_definition(path).manifest_target_name)
            .collect();

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let sources = [
            "src/release/qualification.rs",
            "../docs/M7_5_RUNTIME_QUALIFICATION.md",
            "../docs/M8_FINAL_HARDWARE_INPUT_REPORT.md",
        ];
        let variable = crate::adapters::runtime_release_source::MANIFEST_TARGET_VARIABLE;
        let mut found = 0_usize;
        for source in sources {
            let text = std::fs::read_to_string(root.join(source))
                .unwrap_or_else(|error| panic!("{source} must be readable: {error}"));
            for line in text.lines() {
                let Some((_, selected)) = line.split_once(&format!("{variable}=")) else {
                    continue;
                };
                // Shell snippets continue lines with a trailing backslash, and Markdown quotes
                // values in backticks; neither is part of the selected target name.
                let selected = selected
                    .trim()
                    .trim_end_matches('\\')
                    .trim()
                    .trim_matches('`')
                    .trim_matches('"');
                assert_eq!(
                    selected, active.manifest_target_name,
                    "{source} selects '{selected}' rather than the active manifest"
                );
                assert!(
                    !superseded.iter().any(|name| name == selected),
                    "{source} still selects the superseded manifest '{selected}'"
                );
                found += 1;
            }
        }
        assert!(
            found >= 2,
            "the qualification selection must stay documented; found {found} occurrences"
        );
    }

    /// R6 — both committed Linux definitions still publish the inline representation.
    ///
    /// Release 002 is the active real Runtime Release and it is immutable. Introducing the detached
    /// option must not change what it publishes, so the committed definitions omit the field
    /// entirely and get `Inline` by default.
    #[test]
    fn the_committed_definitions_still_publish_an_inline_inventory() {
        for relative in std::iter::once(super::ACTIVE_LINUX_DEFINITION)
            .chain(super::HISTORICAL_LINUX_DEFINITIONS.iter().copied())
        {
            let definition = read_definition(relative);
            assert_eq!(
                definition.inventory,
                super::InventoryPublication::Inline,
                "{relative} must keep publishing an inline inventory"
            );
            assert!(definition.inventory.target_name().is_none());
        }

        // The field is genuinely absent from the committed JSON, not merely equal to the default.
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(super::ACTIVE_LINUX_DEFINITION);
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert!(
            value.get("inventory").is_none(),
            "the active definition must not have been edited to declare a representation"
        );
    }

    /// R7 — a definition may publish the inventory as a separate immutable target, and that target
    /// is held to the same naming and uniqueness rules as every other target.
    #[test]
    fn a_detached_inventory_target_is_declared_explicitly_and_must_be_unique() {
        let detached = definition_json(|value| {
            value["inventory"] = serde_json::json!({
                "representation": "detached_target",
                "target_name": "test-release.inventory.json",
            });
        });
        let definition = ReleaseDefinition::parse(&detached).unwrap();
        assert_eq!(
            definition.inventory.target_name(),
            Some("test-release.inventory.json")
        );

        // Nested, traversing, and URL-shaped names are refused.
        for name in [
            "targets/inventory.json",
            "../inventory.json",
            "https://example.invalid/inventory.json",
        ] {
            let unsafe_name = definition_json(|value| {
                value["inventory"] = serde_json::json!({
                    "representation": "detached_target",
                    "target_name": name,
                });
            });
            assert!(
                ReleaseDefinition::parse(&unsafe_name).is_err(),
                "'{name}' must not be accepted"
            );
        }

        // Sharing a name with the manifest, the policy, or a component is refused.
        for taken in ["release.json", "runtime-policy.json", "retroarch.AppImage"] {
            let collision = definition_json(|value| {
                value["inventory"] = serde_json::json!({
                    "representation": "detached_target",
                    "target_name": taken,
                });
            });
            assert!(
                ReleaseDefinition::parse(&collision).is_err(),
                "'{taken}' is already a target name"
            );
        }

        // The representation must be tagged, and the tag must be one this tool implements.
        for inventory in [
            serde_json::json!({ "target_name": "x.inventory.json" }),
            serde_json::json!({ "representation": "detached_url", "target_name": "x.json" }),
            serde_json::json!({ "representation": "detached_target" }),
            serde_json::json!({
                "representation": "detached_target",
                "target_name": "x.json",
                "url": "https://example.invalid/x.json",
            }),
        ] {
            let malformed = definition_json(|value| {
                value["inventory"] = inventory.clone();
            });
            assert!(ReleaseDefinition::parse(&malformed).is_err());
        }
    }

    /// R8 — switching the inventory representation is a new release generation.
    ///
    /// The component bytes do not change, but the emitted manifest does, and a published manifest
    /// target is immutable. Republishing a different manifest under the same release identity is
    /// exactly what `supersedes` exists to refuse.
    #[test]
    fn changing_the_inventory_representation_requires_a_new_release_identity() {
        let inline = ReleaseDefinition::parse(&definition_json(|_| {})).unwrap();
        let mut detached = inline.clone();
        detached.inventory = super::InventoryPublication::DetachedTarget {
            target_name: "test-release.inventory.json".to_owned(),
        };

        assert_ne!(
            inline.authenticated_contents(),
            detached.authenticated_contents(),
            "the representation is part of what a client authenticates"
        );

        let mut impostor = detached.clone();
        impostor.release_sequence = inline.release_sequence + 1;
        let error = impostor
            .supersedes(&inline)
            .expect_err("a changed representation under the same identity must be refused");
        assert!(
            format!("{error}").contains("release id"),
            "unexpected error: {error}"
        );

        // A genuinely new generation is accepted.
        let mut successor = detached;
        successor.release_sequence = inline.release_sequence + 1;
        successor.release_id = "test-release-002".try_into().unwrap();
        successor.manifest_id = "test-manifest-002".try_into().unwrap();
        successor.manifest_target_name = "release-002.json".to_owned();
        successor.supersedes(&inline).unwrap();
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let extra = definition_json(|value| {
            value["surprise"] = serde_json::json!(true);
        });
        assert!(ReleaseDefinition::parse(&extra).is_err());
    }
}
