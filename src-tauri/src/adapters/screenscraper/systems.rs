//! RetroFrontier-to-ScreenScraper system mapping.
//!
//! This table belongs to the adapter, not to the provider-neutral `SystemCatalog`: the catalog must
//! never acquire provider identifiers. Every mapping below comes from the first-party system list
//! recorded in `docs/SCREENSCRAPER_SPIKE.md`; anything not listed there fails conservatively rather
//! than being guessed.

use crate::domain::system::SystemId;

/// ScreenScraper's `romtype` parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderRomType {
    /// Cartridge-style whole-file content.
    Rom,
    /// Disc-image content.
    Iso,
}

impl ProviderRomType {
    pub const fn as_parameter(self) -> &'static str {
        match self {
            Self::Rom => "rom",
            Self::Iso => "iso",
        }
    }
}

/// One verified provider system mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderSystemMapping {
    pub provider_system_id: u32,
    pub rom_type: ProviderRomType,
}

/// Resolves the single ScreenScraper system for a RetroFrontier system.
///
/// Returns `None` only for a system with no verified unambiguous mapping. Callers must then persist
/// an unsupported/deferred state rather than searching the provider globally.
pub const fn provider_system_mapping(system: SystemId) -> Option<ProviderSystemMapping> {
    let (provider_system_id, rom_type) = match system {
        SystemId::Nes => (3, ProviderRomType::Rom),
        SystemId::Snes => (4, ProviderRomType::Rom),
        SystemId::Nintendo64 => (14, ProviderRomType::Rom),
        SystemId::GameBoy => (9, ProviderRomType::Rom),
        SystemId::GameBoyColor => (10, ProviderRomType::Rom),
        SystemId::GameBoyAdvance => (12, ProviderRomType::Rom),
        SystemId::MegaDrive => (1, ProviderRomType::Rom),
        SystemId::PlayStation => (57, ProviderRomType::Iso),
        SystemId::SegaSaturn => (22, ProviderRomType::Iso),
        SystemId::SegaDreamcast => (23, ProviderRomType::Iso),
        SystemId::NintendoGameCube => (13, ProviderRomType::Iso),
    };
    Some(ProviderSystemMapping {
        provider_system_id,
        rom_type,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// The mapping recorded by the finalized spike, asserted value by value.
    const EXPECTED: &[(SystemId, u32, ProviderRomType)] = &[
        (SystemId::Nes, 3, ProviderRomType::Rom),
        (SystemId::Snes, 4, ProviderRomType::Rom),
        (SystemId::Nintendo64, 14, ProviderRomType::Rom),
        (SystemId::GameBoy, 9, ProviderRomType::Rom),
        (SystemId::GameBoyColor, 10, ProviderRomType::Rom),
        (SystemId::GameBoyAdvance, 12, ProviderRomType::Rom),
        (SystemId::MegaDrive, 1, ProviderRomType::Rom),
        (SystemId::PlayStation, 57, ProviderRomType::Iso),
        (SystemId::SegaSaturn, 22, ProviderRomType::Iso),
        (SystemId::SegaDreamcast, 23, ProviderRomType::Iso),
        (SystemId::NintendoGameCube, 13, ProviderRomType::Iso),
    ];

    #[test]
    fn every_expected_mapping_resolves_to_its_verified_provider_system() {
        for (system, provider_system_id, rom_type) in EXPECTED {
            let mapping = provider_system_mapping(*system)
                .unwrap_or_else(|| panic!("{system} must have a verified provider mapping"));
            assert_eq!(
                mapping.provider_system_id, *provider_system_id,
                "{system} maps to the wrong provider system"
            );
            assert_eq!(
                mapping.rom_type, *rom_type,
                "{system} uses the wrong provider ROM type"
            );
        }
    }

    #[test]
    fn every_v1_system_is_covered_exactly_once() {
        assert_eq!(EXPECTED.len(), SystemId::ALL_V1.len());
        for system in SystemId::ALL_V1 {
            assert!(
                provider_system_mapping(*system).is_some(),
                "{system} has no adapter-level mapping"
            );
        }

        let provider_ids: BTreeSet<u32> = EXPECTED
            .iter()
            .map(|(_, provider_system_id, _)| *provider_system_id)
            .collect();
        assert_eq!(
            provider_ids.len(),
            EXPECTED.len(),
            "two RetroFrontier systems must never share one provider system"
        );
    }

    #[test]
    fn disc_systems_use_the_iso_rom_type_and_cartridge_systems_use_rom() {
        assert_eq!(ProviderRomType::Rom.as_parameter(), "rom");
        assert_eq!(ProviderRomType::Iso.as_parameter(), "iso");

        for system in [
            SystemId::PlayStation,
            SystemId::SegaSaturn,
            SystemId::SegaDreamcast,
            SystemId::NintendoGameCube,
        ] {
            assert_eq!(
                provider_system_mapping(system).unwrap().rom_type,
                ProviderRomType::Iso
            );
        }
        for system in [
            SystemId::Nes,
            SystemId::Snes,
            SystemId::Nintendo64,
            SystemId::GameBoy,
            SystemId::GameBoyColor,
            SystemId::GameBoyAdvance,
            SystemId::MegaDrive,
        ] {
            assert_eq!(
                provider_system_mapping(system).unwrap().rom_type,
                ProviderRomType::Rom
            );
        }
    }
}
