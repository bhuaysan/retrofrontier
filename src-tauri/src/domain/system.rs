use crate::domain::bios::{BiosModelError, BiosPolicy, BiosRequirement};
use crate::domain::core::{CoreDefinition, CorePolicy, CorePolicyDecision};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use thiserror::Error;

/// Stable application identifier. Display names and aliases are intentionally not used as IDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum SystemId {
    #[serde(rename = "nes")]
    Nes,
    #[serde(rename = "snes")]
    Snes,
    #[serde(rename = "nintendo_64")]
    Nintendo64,
    #[serde(rename = "game_boy")]
    GameBoy,
    #[serde(rename = "game_boy_color")]
    GameBoyColor,
    #[serde(rename = "game_boy_advance")]
    GameBoyAdvance,
    #[serde(rename = "mega_drive")]
    MegaDrive,
    #[serde(rename = "playstation")]
    PlayStation,
    #[serde(rename = "sega_saturn")]
    SegaSaturn,
    #[serde(rename = "sega_dreamcast")]
    SegaDreamcast,
    #[serde(rename = "nintendo_gamecube")]
    NintendoGameCube,
}

impl SystemId {
    pub const ALL_V1: &'static [Self] = &[
        Self::Nes,
        Self::Snes,
        Self::Nintendo64,
        Self::GameBoy,
        Self::GameBoyColor,
        Self::GameBoyAdvance,
        Self::MegaDrive,
        Self::PlayStation,
        Self::SegaSaturn,
        Self::SegaDreamcast,
        Self::NintendoGameCube,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Nes => "nes",
            Self::Snes => "snes",
            Self::Nintendo64 => "nintendo_64",
            Self::GameBoy => "game_boy",
            Self::GameBoyColor => "game_boy_color",
            Self::GameBoyAdvance => "game_boy_advance",
            Self::MegaDrive => "mega_drive",
            Self::PlayStation => "playstation",
            Self::SegaSaturn => "sega_saturn",
            Self::SegaDreamcast => "sega_dreamcast",
            Self::NintendoGameCube => "nintendo_gamecube",
        }
    }
}

impl fmt::Display for SystemId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemDefinition {
    pub id: SystemId,
    pub display_name: String,
    pub manufacturer: String,
    pub aliases: Vec<String>,
    pub supported_extensions: Vec<String>,
    pub core_policy: CorePolicy,
    pub bios_policy: BiosPolicy,
    pub bios_requirements: Vec<BiosRequirement>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemCatalog {
    systems: Vec<SystemDefinition>,
    cores: Vec<CoreDefinition>,
}

impl SystemCatalog {
    pub fn new(systems: Vec<SystemDefinition>, cores: Vec<CoreDefinition>) -> Self {
        Self { systems, cores }
    }

    /// The application-owned V1 catalog. Core choices remain unresolved because the current
    /// CORE_MATRIX and ADR-009 deliberately do not approve any defaults yet.
    pub fn v1() -> Self {
        let unresolved = || {
            CorePolicy::unresolved(
                "Default and approved cores remain unresolved in docs/CORE_MATRIX.md.",
            )
        };

        let requirements = |system_id,
                            definitions: Vec<(
            &str,
            Vec<&str>,
            crate::domain::bios::BiosRequirementKind,
            &str,
        )>| {
            definitions
                .into_iter()
                .map(|(id, filenames, kind, description)| {
                    BiosRequirement::new(
                        id,
                        system_id,
                        filenames.into_iter().map(str::to_owned).collect(),
                        Vec::new(),
                        None,
                        kind,
                        description,
                    )
                    .expect("static BIOS catalog entry must be valid")
                })
                .collect::<Vec<_>>()
        };

        let systems = vec![
            system(
                SystemId::Nes,
                "Nintendo Entertainment System",
                "Nintendo",
                &["NES"],
                &[".nes"],
                unresolved(),
                BiosPolicy::NotRequired,
                Vec::new(),
            ),
            system(
                SystemId::Snes,
                "Super Nintendo Entertainment System",
                "Nintendo",
                &["SNES", "Super Famicom"],
                &[".sfc", ".smc"],
                unresolved(),
                BiosPolicy::NotRequired,
                Vec::new(),
            ),
            system(
                SystemId::Nintendo64,
                "Nintendo 64",
                "Nintendo",
                &["N64"],
                &[".n64", ".z64", ".v64"],
                unresolved(),
                BiosPolicy::NotRequired,
                Vec::new(),
            ),
            system(
                SystemId::GameBoy,
                "Game Boy",
                "Nintendo",
                &["GB", "DMG"],
                &[".gb"],
                unresolved(),
                BiosPolicy::NotRequired,
                Vec::new(),
            ),
            system(
                SystemId::GameBoyColor,
                "Game Boy Color",
                "Nintendo",
                &["GBC"],
                &[".gbc"],
                unresolved(),
                BiosPolicy::NotRequired,
                Vec::new(),
            ),
            system(
                SystemId::GameBoyAdvance,
                "Game Boy Advance",
                "Nintendo",
                &["GBA"],
                &[".gba"],
                unresolved(),
                BiosPolicy::Optional,
                requirements(
                    SystemId::GameBoyAdvance,
                    vec![(
                        "game_boy_advance-bios",
                        vec!["gba_bios.bin"],
                        crate::domain::bios::BiosRequirementKind::Optional,
                        "Optional Game Boy Advance BIOS; an authoritative identity is still unresolved.",
                    )],
                ),
            ),
            system(
                SystemId::MegaDrive,
                "Mega Drive / Genesis",
                "Sega",
                &["Mega Drive", "Genesis", "MD"],
                &[".md", ".gen", ".smd", ".bin"],
                unresolved(),
                BiosPolicy::NotRequired,
                Vec::new(),
            ),
            system(
                SystemId::PlayStation,
                "PlayStation",
                "Sony",
                &["PS1", "PlayStation 1", "PSX"],
                &[".cue", ".chd", ".pbp", ".bin", ".iso", ".m3u"],
                unresolved(),
                BiosPolicy::Required,
                requirements(
                    SystemId::PlayStation,
                    vec![(
                        "playstation-bios",
                        vec!["scph1001.bin", "scph5500.bin", "scph5501.bin", "scph5502.bin"],
                        crate::domain::bios::BiosRequirementKind::Required,
                        "A PlayStation BIOS dump recognized by the approved core.",
                    )],
                ),
            ),
            system(
                SystemId::SegaSaturn,
                "Sega Saturn",
                "Sega",
                &["Saturn"],
                &[".cue", ".chd", ".iso", ".bin", ".m3u"],
                unresolved(),
                BiosPolicy::Required,
                requirements(
                    SystemId::SegaSaturn,
                    vec![(
                        "sega_saturn-bios",
                        vec!["sega_101.bin", "mpr-17933.bin"],
                        crate::domain::bios::BiosRequirementKind::Required,
                        "A Sega Saturn BIOS dump recognized by the approved core.",
                    )],
                ),
            ),
            system(
                SystemId::SegaDreamcast,
                "Sega Dreamcast",
                "Sega",
                &["Dreamcast", "DC"],
                &[".gdi", ".cdi", ".chd", ".m3u"],
                unresolved(),
                BiosPolicy::Required,
                requirements(
                    SystemId::SegaDreamcast,
                    vec![
                        (
                            "sega_dreamcast-boot-bios",
                            vec!["dc_boot.bin"],
                            crate::domain::bios::BiosRequirementKind::Required,
                            "The Dreamcast boot BIOS recognized by the approved core.",
                        ),
                        (
                            "sega_dreamcast-flash-bios",
                            vec!["dc_flash.bin"],
                            crate::domain::bios::BiosRequirementKind::Required,
                            "The Dreamcast flash BIOS recognized by the approved core.",
                        ),
                    ],
                ),
            ),
            system(
                SystemId::NintendoGameCube,
                "Nintendo GameCube",
                "Nintendo",
                &["GameCube", "GC"],
                &[".iso", ".gcm", ".rvz"],
                unresolved(),
                BiosPolicy::NotRequired,
                Vec::new(),
            ),
        ];

        Self::new(systems, Vec::new())
    }

    pub fn systems(&self) -> &[SystemDefinition] {
        &self.systems
    }

    pub fn cores(&self) -> &[CoreDefinition] {
        &self.cores
    }

    pub fn system(&self, id: SystemId) -> Option<&SystemDefinition> {
        self.systems.iter().find(|system| system.id == id)
    }

    /// Resolve a human-entered display name or alias without making that text the persisted
    /// identity. Stable IDs should be used for storage and IPC whenever possible.
    pub fn system_for_name_or_alias(&self, value: &str) -> Option<&SystemDefinition> {
        let normalized = normalize_lookup_name(value);
        self.systems.iter().find(|system| {
            system.id.as_str() == normalized
                || normalize_lookup_name(&system.display_name) == normalized
                || system
                    .aliases
                    .iter()
                    .any(|alias| normalize_lookup_name(alias) == normalized)
        })
    }

    pub fn bios_requirements(&self) -> impl Iterator<Item = &BiosRequirement> {
        self.systems
            .iter()
            .flat_map(|system| system.bios_requirements.iter())
    }

    pub fn validate(&self) -> Result<(), CatalogError> {
        if self.systems.is_empty() {
            return Err(CatalogError::EmptySystems);
        }

        let mut system_ids = BTreeSet::new();
        let mut lookup_names = BTreeMap::new();
        let mut requirement_ids = BTreeSet::new();
        for system in &self.systems {
            if !system_ids.insert(system.id) {
                return Err(CatalogError::DuplicateSystem(system.id));
            }
            if system.display_name.trim().is_empty() || system.manufacturer.trim().is_empty() {
                return Err(CatalogError::MissingSystemMetadata(system.id));
            }
            validate_extensions(system)?;
            register_lookup_name(&mut lookup_names, system.id.as_str(), system.id)?;
            register_lookup_name(&mut lookup_names, &system.display_name, system.id)?;
            let mut aliases = BTreeSet::new();
            for alias in &system.aliases {
                let key = normalize_lookup_name(alias);
                if key.is_empty() {
                    return Err(CatalogError::EmptyAlias(system.id));
                }
                if !aliases.insert(key.clone()) {
                    return Err(CatalogError::DuplicateAlias {
                        alias: alias.clone(),
                        first: system.id,
                        second: system.id,
                    });
                }
                if let Some(existing) = lookup_names.get(&key) {
                    if *existing != system.id {
                        return Err(CatalogError::DuplicateSystemName {
                            name: alias.clone(),
                            first: *existing,
                            second: system.id,
                        });
                    }
                } else {
                    lookup_names.insert(key, system.id);
                }
            }
            if BiosPolicy::from_requirements(&system.bios_requirements) != system.bios_policy {
                return Err(CatalogError::BiosPolicyMismatch(system.id));
            }
            for requirement in &system.bios_requirements {
                requirement
                    .validate()
                    .map_err(|source| CatalogError::InvalidBiosRequirement {
                        requirement: requirement.id.to_string(),
                        source,
                    })?;
                if requirement.system_id != system.id {
                    return Err(CatalogError::BiosRequirementSystemMismatch(
                        requirement.id.to_string(),
                    ));
                }
                if !requirement_ids.insert(requirement.id.clone()) {
                    return Err(CatalogError::DuplicateBiosRequirement(
                        requirement.id.to_string(),
                    ));
                }
            }
            validate_core_policy(system, &self.cores)?;
        }

        let mut core_ids = BTreeSet::new();
        for core in &self.cores {
            if !core_ids.insert(core.id.clone()) {
                return Err(CatalogError::DuplicateCore(core.id.to_string()));
            }
            if core.libretro_name.trim().is_empty() || core.display_name.trim().is_empty() {
                return Err(CatalogError::MissingCoreMetadata(core.id.to_string()));
            }
            if core.systems.is_empty() {
                return Err(CatalogError::CoreHasNoSystems(core.id.to_string()));
            }
            if core.targets.is_empty() {
                return Err(CatalogError::CoreHasNoTargets(core.id.to_string()));
            }
            for system_id in &core.systems {
                if !system_ids.contains(system_id) {
                    return Err(CatalogError::CoreReferencesUnknownSystem {
                        core: core.id.to_string(),
                        system: *system_id,
                    });
                }
            }
            for system_id in &core.default_for_systems {
                if !core.systems.contains(system_id) {
                    return Err(CatalogError::CoreDefaultSystemMismatch {
                        core: core.id.to_string(),
                        system: *system_id,
                    });
                }
                let Some(system) = self.system(*system_id) else {
                    return Err(CatalogError::CoreReferencesUnknownSystem {
                        core: core.id.to_string(),
                        system: *system_id,
                    });
                };
                if system.core_policy.default_core_id.as_ref() != Some(&core.id) {
                    return Err(CatalogError::CoreDefaultPolicyMismatch {
                        core: core.id.to_string(),
                        system: *system_id,
                    });
                }
            }
        }

        for system in &self.systems {
            if let Some(default_core_id) = &system.core_policy.default_core_id {
                let Some(core) = self.cores.iter().find(|core| &core.id == default_core_id) else {
                    return Err(CatalogError::UnknownCoreReference {
                        system: system.id,
                        core: default_core_id.to_string(),
                    });
                };
                if !system
                    .core_policy
                    .approved_core_ids
                    .contains(default_core_id)
                    || !core.systems.contains(&system.id)
                {
                    return Err(CatalogError::DefaultCoreNotApproved {
                        system: system.id,
                        core: default_core_id.to_string(),
                    });
                }
                if !core.default_for_systems.contains(&system.id) {
                    return Err(CatalogError::DefaultCoreNotMarked {
                        system: system.id,
                        core: default_core_id.to_string(),
                    });
                }
            }
        }
        Ok(())
    }
}

fn normalize_lookup_name(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn register_lookup_name(
    lookup_names: &mut BTreeMap<String, SystemId>,
    name: &str,
    system: SystemId,
) -> Result<(), CatalogError> {
    let key = normalize_lookup_name(name);
    if let Some(existing) = lookup_names.get(&key) {
        if *existing != system {
            return Err(CatalogError::DuplicateSystemName {
                name: name.to_owned(),
                first: *existing,
                second: system,
            });
        }
    } else {
        lookup_names.insert(key, system);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn system(
    id: SystemId,
    display_name: &str,
    manufacturer: &str,
    aliases: &[&str],
    extensions: &[&str],
    core_policy: CorePolicy,
    bios_policy: BiosPolicy,
    bios_requirements: Vec<BiosRequirement>,
) -> SystemDefinition {
    SystemDefinition {
        id,
        display_name: display_name.to_owned(),
        manufacturer: manufacturer.to_owned(),
        aliases: aliases.iter().map(|value| (*value).to_owned()).collect(),
        supported_extensions: extensions.iter().map(|value| (*value).to_owned()).collect(),
        core_policy,
        bios_policy,
        bios_requirements,
    }
}

fn validate_extensions(system: &SystemDefinition) -> Result<(), CatalogError> {
    let mut extensions = BTreeSet::new();
    for extension in &system.supported_extensions {
        let valid_suffix = extension.as_bytes().get(1..).is_some_and(|suffix| {
            suffix
                .iter()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        });
        if extension.is_empty()
            || extension != &extension.to_ascii_lowercase()
            || !extension.starts_with('.')
            || extension.len() == 1
            || !valid_suffix
            || !extensions.insert(extension)
        {
            return Err(CatalogError::InvalidExtension {
                system: system.id,
                extension: extension.clone(),
            });
        }
    }
    Ok(())
}

fn validate_core_policy(
    system: &SystemDefinition,
    cores: &[CoreDefinition],
) -> Result<(), CatalogError> {
    let policy = &system.core_policy;
    let mut approved = BTreeSet::new();
    for core_id in &policy.approved_core_ids {
        if !approved.insert(core_id) {
            return Err(CatalogError::DuplicateApprovedCore {
                system: system.id,
                core: core_id.to_string(),
            });
        }
        let Some(core) = cores.iter().find(|core| &core.id == core_id) else {
            return Err(CatalogError::UnknownCoreReference {
                system: system.id,
                core: core_id.to_string(),
            });
        };
        if !core.systems.contains(&system.id) {
            return Err(CatalogError::IncompatibleCoreReference {
                system: system.id,
                core: core_id.to_string(),
            });
        }
    }
    match &policy.decision {
        CorePolicyDecision::Resolved if policy.default_core_id.is_none() => {
            Err(CatalogError::ResolvedPolicyWithoutDefault(system.id))
        }
        CorePolicyDecision::Unresolved { research_item } if research_item.trim().is_empty() => {
            Err(CatalogError::UnresolvedPolicyWithoutResearch(system.id))
        }
        _ => Ok(()),
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CatalogError {
    #[error("system catalog contains no systems")]
    EmptySystems,
    #[error("duplicate system identifier: {0}")]
    DuplicateSystem(SystemId),
    #[error("system metadata is incomplete for {0}")]
    MissingSystemMetadata(SystemId),
    #[error("system lookup name '{name}' is used by both {first} and {second}")]
    DuplicateSystemName {
        name: String,
        first: SystemId,
        second: SystemId,
    },
    #[error("alias '{alias}' is used by both {first} and {second}")]
    DuplicateAlias {
        alias: String,
        first: SystemId,
        second: SystemId,
    },
    #[error("system {0} has an empty alias")]
    EmptyAlias(SystemId),
    #[error("system {system} declares an invalid extension '{extension}'")]
    InvalidExtension { system: SystemId, extension: String },
    #[error("BIOS policy does not match requirements for {0}")]
    BiosPolicyMismatch(SystemId),
    #[error("BIOS requirement '{0}' is attached to an incompatible system")]
    BiosRequirementSystemMismatch(String),
    #[error("duplicate BIOS requirement: {0}")]
    DuplicateBiosRequirement(String),
    #[error("invalid BIOS requirement '{requirement}': {source}")]
    InvalidBiosRequirement {
        requirement: String,
        source: BiosModelError,
    },
    #[error("duplicate core identifier: {0}")]
    DuplicateCore(String),
    #[error("core metadata is incomplete for {0}")]
    MissingCoreMetadata(String),
    #[error("core {0} has no system compatibility mappings")]
    CoreHasNoSystems(String),
    #[error("core {0} has no platform targets")]
    CoreHasNoTargets(String),
    #[error("core {core} references unknown system {system}")]
    CoreReferencesUnknownSystem { core: String, system: SystemId },
    #[error("core {core} marks incompatible system {system} as a default")]
    CoreDefaultSystemMismatch { core: String, system: SystemId },
    #[error("core {core} is not the configured default for system {system}")]
    CoreDefaultPolicyMismatch { core: String, system: SystemId },
    #[error("system {system} references unknown core {core}")]
    UnknownCoreReference { system: SystemId, core: String },
    #[error("system {system} references incompatible core {core}")]
    IncompatibleCoreReference { system: SystemId, core: String },
    #[error("system {system} has duplicate approved core {core}")]
    DuplicateApprovedCore { system: SystemId, core: String },
    #[error("system {system} has a default core that is not approved: {core}")]
    DefaultCoreNotApproved { system: SystemId, core: String },
    #[error("system {system} default core is not marked as a default by core {core}")]
    DefaultCoreNotMarked { system: SystemId, core: String },
    #[error("system {0} has a resolved core policy without a default")]
    ResolvedPolicyWithoutDefault(SystemId),
    #[error("system {0} has an unresolved core policy without a research item")]
    UnresolvedPolicyWithoutResearch(SystemId),
}

#[cfg(test)]
mod tests {
    use super::{system, CatalogError, SystemCatalog, SystemId};
    use crate::domain::bios::BiosPolicy;
    use crate::domain::core::{CoreDefinition, CoreId, CorePolicy, CoreTarget};
    use crate::domain::runtime::{RuntimeArchitecture, RuntimePlatform};
    use std::collections::BTreeSet;

    #[test]
    fn v1_contains_all_stable_systems_and_validates() {
        let catalog = SystemCatalog::v1();

        catalog.validate().unwrap();
        assert_eq!(catalog.systems().len(), SystemId::ALL_V1.len());
        assert_eq!(
            SystemId::ALL_V1
                .iter()
                .copied()
                .collect::<BTreeSet<_>>()
                .len(),
            SystemId::ALL_V1.len()
        );
        for id in SystemId::ALL_V1 {
            assert!(
                catalog.system(*id).is_some(),
                "missing catalog entry for {id}"
            );
        }
    }

    #[test]
    fn aliases_resolve_to_one_logical_system() {
        let catalog = SystemCatalog::v1();

        assert_eq!(
            catalog.system_for_name_or_alias("Mega Drive").unwrap().id,
            SystemId::MegaDrive
        );
        assert_eq!(
            catalog.system_for_name_or_alias("Genesis").unwrap().id,
            SystemId::MegaDrive
        );
        assert_eq!(
            catalog
                .system_for_name_or_alias("Mega Drive / Genesis")
                .unwrap()
                .id,
            SystemId::MegaDrive
        );
        assert_eq!(catalog.systems().len(), 11);
    }

    fn synthetic_system(
        id: SystemId,
        display_name: &str,
        aliases: &[&str],
        extensions: &[&str],
    ) -> super::SystemDefinition {
        system(
            id,
            display_name,
            "Synthetic",
            aliases,
            extensions,
            CorePolicy::unresolved("synthetic core research"),
            BiosPolicy::NotRequired,
            Vec::new(),
        )
    }

    fn assert_duplicate_lookup_name(systems: Vec<super::SystemDefinition>) {
        assert!(matches!(
            SystemCatalog::new(systems, Vec::new()).validate(),
            Err(CatalogError::DuplicateSystemName { .. })
        ));
    }

    #[test]
    fn aliases_cannot_collide_with_other_system_lookup_names() {
        assert_duplicate_lookup_name(vec![
            synthetic_system(SystemId::Nes, "Nintendo", &[], &[".nes"]),
            synthetic_system(SystemId::Snes, "Super Nintendo", &["NES"], &[".sfc"]),
        ]);
        assert_duplicate_lookup_name(vec![
            synthetic_system(SystemId::Nes, "Nintendo", &[], &[".nes"]),
            synthetic_system(SystemId::Snes, "Super Nintendo", &["nInTeNdO"], &[".sfc"]),
        ]);
        assert_duplicate_lookup_name(vec![
            synthetic_system(SystemId::Nes, "Nintendo", &["Shared"], &[".nes"]),
            synthetic_system(SystemId::Snes, "Super Nintendo", &["sHaReD"], &[".sfc"]),
        ]);
    }

    #[test]
    fn extensions_reject_path_like_and_unsafe_values() {
        for extension in [
            ".NES",
            "nes",
            ".",
            ".nes/other",
            r".nes\other",
            ".nes.other",
            ".nes with-space",
            ".nes!",
            ".é",
        ] {
            let catalog = SystemCatalog::new(
                vec![synthetic_system(
                    SystemId::Nes,
                    "Nintendo",
                    &[],
                    &[extension],
                )],
                Vec::new(),
            );
            assert!(matches!(
                catalog.validate(),
                Err(CatalogError::InvalidExtension {
                    extension: actual,
                    ..
                }) if actual == extension
            ));
        }
    }

    #[test]
    fn v1_extensions_are_normalized_and_requirement_ids_are_unique() {
        let catalog = SystemCatalog::v1();
        let mut requirement_ids = BTreeSet::new();

        for system in catalog.systems() {
            let mut extensions = BTreeSet::new();
            for extension in &system.supported_extensions {
                assert!(extension.starts_with('.'));
                assert_eq!(extension, &extension.to_ascii_lowercase());
                assert!(extensions.insert(extension));
            }
            for requirement in &system.bios_requirements {
                assert!(requirement_ids.insert(requirement.id.clone()));
            }
        }
        assert_eq!(requirement_ids.len(), 5);
    }

    #[test]
    fn core_policy_references_are_checked_against_compatible_definitions() {
        let core_id = CoreId::new("synthetic-core").unwrap();
        let core = CoreDefinition {
            id: core_id.clone(),
            libretro_name: "synthetic_core".to_owned(),
            display_name: "Synthetic Core".to_owned(),
            systems: vec![SystemId::Nes],
            targets: vec![CoreTarget {
                platform: RuntimePlatform::Linux,
                architecture: RuntimeArchitecture::X86_64,
            }],
            managed_component_id: CoreId::new("synthetic-core-component").unwrap(),
            default_for_systems: vec![SystemId::Nes],
        };
        let nes = system(
            SystemId::Nes,
            "Nintendo Entertainment System",
            "Nintendo",
            &["NES"],
            &[".nes"],
            CorePolicy::resolved(core_id.clone(), vec![core_id.clone()]),
            BiosPolicy::NotRequired,
            Vec::new(),
        );

        SystemCatalog::new(vec![nes.clone()], vec![core.clone()])
            .validate()
            .unwrap();

        let incompatible = CoreDefinition {
            systems: vec![SystemId::Snes],
            default_for_systems: Vec::new(),
            ..core
        };
        assert!(matches!(
            SystemCatalog::new(vec![nes], vec![incompatible]).validate(),
            Err(CatalogError::IncompatibleCoreReference { .. })
        ));
    }

    #[test]
    fn unresolved_v1_core_policy_is_explicit_for_every_system() {
        let catalog = SystemCatalog::v1();
        assert!(catalog.cores().is_empty());
        for system in catalog.systems() {
            assert!(system.core_policy.default_core_id.is_none());
            assert!(system.core_policy.approved_core_ids.is_empty());
            assert!(matches!(
                system.core_policy.decision,
                crate::domain::core::CorePolicyDecision::Unresolved { .. }
            ));
        }
    }
}
