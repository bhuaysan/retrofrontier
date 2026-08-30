use crate::domain::bios::{
    BiosDigest, BiosFileIdentity, BiosModelError, BiosPolicy, BiosRequirement,
};
use crate::domain::core::{CoreDefinition, CoreId, CorePolicy, CorePolicyDecision, CoreTarget};
use crate::domain::runtime::{RuntimeArchitecture, RuntimePlatform, SafeIdentifier};
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

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "nes" => Some(Self::Nes),
            "snes" => Some(Self::Snes),
            "nintendo_64" => Some(Self::Nintendo64),
            "game_boy" => Some(Self::GameBoy),
            "game_boy_color" => Some(Self::GameBoyColor),
            "game_boy_advance" => Some(Self::GameBoyAdvance),
            "mega_drive" => Some(Self::MegaDrive),
            "playstation" => Some(Self::PlayStation),
            "sega_saturn" => Some(Self::SegaSaturn),
            "sega_dreamcast" => Some(Self::SegaDreamcast),
            "nintendo_gamecube" => Some(Self::NintendoGameCube),
            _ => None,
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
    pub managed_rom_folder_name: String,
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

        // M7 resolves policy for exactly four reference systems. Every identifier, libretro core
        // name, licence, and upstream source below was verified against the libretro core
        // documentation; see docs/superpowers/specs/2026-08-30-m7-retroarch-launch-design.md.
        let resolved = |core_id: &str| {
            let core_id = CoreId::new(core_id).expect("static approved core id must be valid");
            CorePolicy::resolved(core_id.clone(), vec![core_id])
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
                "NES",
                &[".nes"],
                resolved("nestopia"),
                BiosPolicy::NotRequired,
                Vec::new(),
            ),
            system(
                SystemId::Snes,
                "Super Nintendo Entertainment System",
                "Nintendo",
                &["SNES", "Super Famicom"],
                "SNES",
                &[".sfc", ".smc"],
                resolved("bsnes-mercury-balanced"),
                BiosPolicy::NotRequired,
                Vec::new(),
            ),
            system(
                SystemId::Nintendo64,
                "Nintendo 64",
                "Nintendo",
                &["N64"],
                "Nintendo 64",
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
                "Game Boy",
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
                "Game Boy Color",
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
                "Game Boy Advance",
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
                "Mega Drive",
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
                "PlayStation",
                &[".cue", ".chd", ".pbp", ".bin", ".iso", ".m3u"],
                resolved("beetle-psx"),
                BiosPolicy::Required,
                vec![playstation_bios_requirement()],
            ),
            system(
                SystemId::SegaSaturn,
                "Sega Saturn",
                "Sega",
                &["Saturn"],
                "Sega Saturn",
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
                "Sega Dreamcast",
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
                "Nintendo GameCube",
                &[".iso", ".gcm", ".rvz"],
                resolved("dolphin"),
                BiosPolicy::NotRequired,
                Vec::new(),
            ),
        ];

        Self::new(systems, v1_cores())
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

    pub fn core(&self, id: &CoreId) -> Option<&CoreDefinition> {
        self.cores.iter().find(|core| &core.id == id)
    }

    /// Translate an authenticated managed runtime component identifier into the approved core it
    /// installs. A component with no catalog definition is never approved and never launchable.
    pub fn core_for_component(&self, component_id: &SafeIdentifier) -> Option<&CoreDefinition> {
        self.cores
            .iter()
            .find(|core| core.managed_component_id.as_str() == component_id.as_str())
    }

    /// Static approval only. Installed availability stays a RuntimeManager decision.
    pub fn approves_core_for_system(&self, system_id: SystemId, core_id: &CoreId) -> bool {
        self.system(system_id).is_some_and(|system| {
            system.core_policy.approved_core_ids.contains(core_id)
                && self
                    .core(core_id)
                    .is_some_and(|core| core.systems.contains(&system_id))
        })
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

    /// Resolve the explicit managed-folder name. Existing catalog aliases are accepted for
    /// lookup, while creation always uses `managed_rom_folder_name`.
    pub fn system_for_managed_folder_name(&self, value: &str) -> Option<&SystemDefinition> {
        let normalized = normalize_lookup_name(value);
        self.systems.iter().find(|system| {
            normalize_lookup_name(&system.managed_rom_folder_name) == normalized
                || normalize_lookup_name(&system.display_name) == normalized
                || system
                    .aliases
                    .iter()
                    .any(|alias| normalize_lookup_name(alias) == normalized)
        })
    }

    pub fn systems_for_extension(&self, extension: &str) -> Vec<SystemId> {
        let extension = extension.to_ascii_lowercase();
        self.systems
            .iter()
            .filter(|system| {
                system
                    .supported_extensions
                    .iter()
                    .any(|candidate| candidate == &extension)
            })
            .map(|system| system.id)
            .collect()
    }

    pub fn supports_extension(&self, system_id: SystemId, extension: &str) -> bool {
        let extension = extension.to_ascii_lowercase();
        self.system(system_id).is_some_and(|system| {
            system
                .supported_extensions
                .iter()
                .any(|candidate| candidate == &extension)
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
        let mut managed_folder_names = BTreeMap::new();
        let mut requirement_ids = BTreeSet::new();
        for system in &self.systems {
            if !system_ids.insert(system.id) {
                return Err(CatalogError::DuplicateSystem(system.id));
            }
            if system.display_name.trim().is_empty() || system.manufacturer.trim().is_empty() {
                return Err(CatalogError::MissingSystemMetadata(system.id));
            }
            validate_managed_folder_name(system, &mut managed_folder_names)?;
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

/// The approved managed cores for the M7 reference systems.
///
/// Licences and upstream sources are recorded from the libretro core documentation. Only the
/// qualified Linux x86_64 target is declared; other targets are added when their runtime
/// distribution is qualified.
fn v1_cores() -> Vec<CoreDefinition> {
    let linux_x86_64 = || {
        vec![CoreTarget {
            platform: RuntimePlatform::Linux,
            architecture: RuntimeArchitecture::X86_64,
        }]
    };
    let core = |id: &str,
                libretro_name: &str,
                display_name: &str,
                system_id: SystemId,
                license: &str,
                source_url: &str| {
        let id = CoreId::new(id).expect("static core identifier must be valid");
        CoreDefinition {
            managed_component_id: id.clone(),
            id,
            libretro_name: libretro_name.to_owned(),
            display_name: display_name.to_owned(),
            systems: vec![system_id],
            targets: linux_x86_64(),
            default_for_systems: vec![system_id],
            license: license.to_owned(),
            source_url: source_url.to_owned(),
        }
    };

    vec![
        core(
            "nestopia",
            "nestopia_libretro",
            "Nestopia UE",
            SystemId::Nes,
            "GPL-2.0",
            "https://github.com/libretro/nestopia",
        ),
        core(
            "bsnes-mercury-balanced",
            "bsnes_mercury_balanced_libretro",
            "bsnes-mercury Balanced",
            SystemId::Snes,
            "GPL-3.0",
            "https://github.com/libretro/bsnes-mercury",
        ),
        core(
            "beetle-psx",
            "mednafen_psx_libretro",
            "Beetle PSX",
            SystemId::PlayStation,
            "GPL-2.0",
            "https://github.com/libretro/beetle-psx-libretro",
        ),
        core(
            "dolphin",
            "dolphin_libretro",
            "Dolphin",
            SystemId::NintendoGameCube,
            "GPL-2.0",
            "https://github.com/libretro/dolphin",
        ),
    ]
}

/// The PlayStation BIOS dumps the approved Beetle PSX core loads, identified per filename.
///
/// The identities are the MD5 values published by the core's own libretro documentation; see
/// `docs/CORE_MATRIX.md`. `scph1001.bin` is deliberately absent because the approved core does not
/// look that filename up, and no expected size is asserted because the digest already pins
/// identity exactly. The core can fall back to a bundled OpenBIOS; RetroFrontier deliberately does
/// not rely on that and keeps this requirement `Required`.
fn playstation_bios_requirement() -> BiosRequirement {
    let identity = |filename: &str, md5: &str| {
        BiosFileIdentity::new(
            filename,
            None,
            vec![BiosDigest::md5(md5).expect("static BIOS digest must be valid")],
        )
        .expect("static BIOS file identity must be valid")
    };

    BiosRequirement::with_files(
        "playstation-bios",
        SystemId::PlayStation,
        vec![
            identity("scph5500.bin", "8dd7d5296a650fac7319bce665a6a53c"),
            identity("scph5501.bin", "490f666e1afb15b7362b406ed1cea246"),
            identity("scph5502.bin", "32736f17079d0b2b7024407c39bd3050"),
        ],
        crate::domain::bios::BiosRequirementKind::Required,
        "A PlayStation BIOS dump recognized by the approved core \
         (scph5500.bin, scph5501.bin, or scph5502.bin).",
    )
    .expect("static PlayStation BIOS requirement must be valid")
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
    managed_rom_folder_name: &str,
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
        managed_rom_folder_name: managed_rom_folder_name.to_owned(),
        supported_extensions: extensions.iter().map(|value| (*value).to_owned()).collect(),
        core_policy,
        bios_policy,
        bios_requirements,
    }
}

fn validate_managed_folder_name(
    system: &SystemDefinition,
    lookup_names: &mut BTreeMap<String, SystemId>,
) -> Result<(), CatalogError> {
    let folder = system.managed_rom_folder_name.trim();
    if folder.is_empty()
        || folder != system.managed_rom_folder_name.as_str()
        || folder == "."
        || folder == ".."
        || folder.contains('/')
        || folder.contains('\\')
        || folder.contains(':')
        || folder.chars().any(char::is_control)
        || folder.ends_with('.')
        || folder.ends_with(' ')
    {
        return Err(CatalogError::InvalidManagedRomFolderName {
            system: system.id,
            folder: system.managed_rom_folder_name.clone(),
        });
    }

    let key = normalize_lookup_name(folder);
    if let Some(existing) = lookup_names.get(&key) {
        if *existing != system.id {
            return Err(CatalogError::DuplicateManagedRomFolderName {
                folder: system.managed_rom_folder_name.clone(),
                first: *existing,
                second: system.id,
            });
        }
    } else {
        lookup_names.insert(key, system.id);
    }
    Ok(())
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
    #[error("system {system} declares an invalid managed ROM folder name '{folder}'")]
    InvalidManagedRomFolderName { system: SystemId, folder: String },
    #[error("managed ROM folder '{folder}' is used by both {first} and {second}")]
    DuplicateManagedRomFolderName {
        folder: String,
        first: SystemId,
        second: SystemId,
    },
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
    fn v1_managed_rom_folders_are_explicit_and_catalog_resolvable() {
        let catalog = SystemCatalog::v1();
        let expected = [
            (SystemId::Nes, "NES"),
            (SystemId::Snes, "SNES"),
            (SystemId::Nintendo64, "Nintendo 64"),
            (SystemId::GameBoy, "Game Boy"),
            (SystemId::GameBoyColor, "Game Boy Color"),
            (SystemId::GameBoyAdvance, "Game Boy Advance"),
            (SystemId::MegaDrive, "Mega Drive"),
            (SystemId::PlayStation, "PlayStation"),
            (SystemId::SegaSaturn, "Sega Saturn"),
            (SystemId::SegaDreamcast, "Sega Dreamcast"),
            (SystemId::NintendoGameCube, "Nintendo GameCube"),
        ];

        for (system_id, folder) in expected {
            let system = catalog.system(system_id).unwrap();
            assert_eq!(system.managed_rom_folder_name, folder);
            assert_eq!(
                catalog.system_for_managed_folder_name(folder).unwrap().id,
                system_id
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
            id.as_str(),
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
            license: "GPL-2.0".to_owned(),
            source_url: "https://example.invalid/synthetic-core".to_owned(),
        };
        let nes = system(
            SystemId::Nes,
            "Nintendo Entertainment System",
            "Nintendo",
            &["NES"],
            "NES",
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
    fn m7_reference_systems_resolve_only_to_their_approved_default_core() {
        let catalog = SystemCatalog::v1();
        let expected = [
            (SystemId::Nes, "nestopia", "nestopia_libretro"),
            (
                SystemId::Snes,
                "bsnes-mercury-balanced",
                "bsnes_mercury_balanced_libretro",
            ),
            (SystemId::PlayStation, "beetle-psx", "mednafen_psx_libretro"),
            (SystemId::NintendoGameCube, "dolphin", "dolphin_libretro"),
        ];

        for (system_id, core_id, libretro_name) in expected {
            let system = catalog.system(system_id).unwrap();
            let core_id = CoreId::new(core_id).unwrap();
            assert!(matches!(
                system.core_policy.decision,
                crate::domain::core::CorePolicyDecision::Resolved
            ));
            assert_eq!(system.core_policy.default_core_id.as_ref(), Some(&core_id));
            assert_eq!(system.core_policy.approved_core_ids, vec![core_id.clone()]);

            let core = catalog.core(&core_id).expect("approved core definition");
            assert_eq!(core.libretro_name, libretro_name);
            assert_eq!(core.managed_component_id, core_id);
            assert!(core.systems.contains(&system_id));
            assert!(core.default_for_systems.contains(&system_id));
            assert!(!core.license.trim().is_empty());
            assert!(core.source_url.starts_with("https://"));
            assert!(core.supports_target(CoreTarget {
                platform: RuntimePlatform::Linux,
                architecture: RuntimeArchitecture::X86_64,
            }));
        }
    }

    #[test]
    fn the_remaining_v1_systems_stay_explicitly_unresolved() {
        let catalog = SystemCatalog::v1();
        let resolved = [
            SystemId::Nes,
            SystemId::Snes,
            SystemId::PlayStation,
            SystemId::NintendoGameCube,
        ];

        let mut unresolved = 0;
        for system in catalog.systems() {
            if resolved.contains(&system.id) {
                continue;
            }
            unresolved += 1;
            assert!(system.core_policy.default_core_id.is_none());
            assert!(system.core_policy.approved_core_ids.is_empty());
            assert!(matches!(
                system.core_policy.decision,
                crate::domain::core::CorePolicyDecision::Unresolved { .. }
            ));
        }
        assert_eq!(unresolved, 7);
        assert_eq!(catalog.cores().len(), 4);
        catalog.validate().unwrap();
    }

    #[test]
    fn managed_component_identifiers_resolve_to_one_approved_core() {
        let catalog = SystemCatalog::v1();
        let component: crate::domain::runtime::SafeIdentifier = "beetle-psx".try_into().unwrap();

        let core = catalog.core_for_component(&component).unwrap();

        assert_eq!(core.id, CoreId::new("beetle-psx").unwrap());
        assert!(catalog.approves_core_for_system(SystemId::PlayStation, &core.id));
        assert!(!catalog.approves_core_for_system(SystemId::Nes, &core.id));
        let unknown: crate::domain::runtime::SafeIdentifier = "some-other-core".try_into().unwrap();
        assert!(catalog.core_for_component(&unknown).is_none());
    }

    #[test]
    fn an_unresolved_system_approves_no_core_at_all() {
        let catalog = SystemCatalog::v1();
        let nestopia = CoreId::new("nestopia").unwrap();

        for system_id in [
            SystemId::Nintendo64,
            SystemId::GameBoy,
            SystemId::GameBoyColor,
            SystemId::GameBoyAdvance,
            SystemId::MegaDrive,
            SystemId::SegaSaturn,
            SystemId::SegaDreamcast,
        ] {
            assert!(!catalog.approves_core_for_system(system_id, &nestopia));
        }
    }

    #[test]
    fn a_core_is_rejected_for_a_platform_it_does_not_declare() {
        let catalog = SystemCatalog::v1();
        let core = catalog.core(&CoreId::new("dolphin").unwrap()).unwrap();

        assert!(!core.supports_target(CoreTarget {
            platform: RuntimePlatform::Windows,
            architecture: RuntimeArchitecture::X86_64,
        }));
        assert!(!core.supports_target(CoreTarget {
            platform: RuntimePlatform::Linux,
            architecture: RuntimeArchitecture::Aarch64,
        }));
    }
}
