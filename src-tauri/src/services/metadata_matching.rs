//! Deterministic-match policy.
//!
//! Pure functions only: no I/O, no clock, no database. The rule they implement is that a provider
//! relationship becomes trusted only when the provider returned a concrete content record whose
//! own evidence agrees with the current local M4 evidence. A successful response is not a match, a
//! result's position in a list is not a match, and an equal filename or similar title is not a
//! match.

use crate::domain::metadata::{MatchEvidence, MatchType};
use crate::services::metadata_provider::{ProviderGameRecord, ProviderRomRecord};

/// Why returned provider evidence was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceConflict {
    SizeMismatch,
    Sha1Mismatch,
    Md5Mismatch,
    Crc32Mismatch,
    /// Two distinct provider content records share our CRC32, so CRC32 alone cannot identify one.
    Crc32Collision,
}

/// Why no comparison could be made at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsufficientEvidence {
    /// The provider reported no concrete content record for the submitted evidence.
    ProviderRecordMissing,
    /// The provider's record carries no hash, so nothing can be compared.
    HashUnavailable,
    /// The provider's record carries no size, and every accepted rule requires size agreement.
    SizeUnavailable,
    /// No hash is present on both sides.
    HashesNotComparable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeterministicOutcome {
    /// Returned evidence agrees. This is the only result that may attach automatically.
    Accepted {
        match_type: MatchType,
        provider_rom_id: Option<String>,
    },
    /// Returned evidence contradicts local evidence. Never attach; record ambiguity.
    Conflicting(EvidenceConflict),
    /// Nothing comparable came back. Never attach; heuristic candidates may still be offered.
    Insufficient(InsufficientEvidence),
}

/// Classifies the provider's answer against the local evidence snapshot.
pub fn classify_deterministic_match(
    evidence: &MatchEvidence,
    record: &ProviderGameRecord,
) -> DeterministicOutcome {
    let Some(matched) = record.matched_rom.as_ref() else {
        return DeterministicOutcome::Insufficient(InsufficientEvidence::ProviderRecordMissing);
    };
    if !matched.has_any_hash() {
        return DeterministicOutcome::Insufficient(InsufficientEvidence::HashUnavailable);
    }
    let Some(provider_size) = matched.size_bytes else {
        return DeterministicOutcome::Insufficient(InsufficientEvidence::SizeUnavailable);
    };
    if provider_size != evidence.size_bytes {
        return DeterministicOutcome::Conflicting(EvidenceConflict::SizeMismatch);
    }

    // Every hash present on both sides must agree. A single disagreement means the provider is
    // describing different bytes, whatever else matched.
    if let Some(conflict) = first_hash_conflict(evidence, matched) {
        return DeterministicOutcome::Conflicting(conflict);
    }

    // Strongest available agreement wins, so a CRC32-only acceptance can only happen when neither
    // SHA-1 nor MD5 was comparable.
    if compares(evidence.sha1.as_deref(), matched.sha1.as_deref()) {
        return DeterministicOutcome::Accepted {
            match_type: MatchType::DeterministicSha1,
            provider_rom_id: matched.provider_rom_id.clone(),
        };
    }
    if compares(evidence.md5.as_deref(), matched.md5.as_deref()) {
        return DeterministicOutcome::Accepted {
            match_type: MatchType::DeterministicMd5,
            provider_rom_id: matched.provider_rom_id.clone(),
        };
    }
    if compares(evidence.crc32.as_deref(), matched.crc32.as_deref()) {
        // CRC32 is collision-prone, so it is only accepted when the provider itself lists no
        // second record with the same CRC32 for this game.
        if crc32_is_ambiguous(evidence, record, matched) {
            return DeterministicOutcome::Conflicting(EvidenceConflict::Crc32Collision);
        }
        return DeterministicOutcome::Accepted {
            match_type: MatchType::DeterministicCrc32,
            provider_rom_id: matched.provider_rom_id.clone(),
        };
    }

    DeterministicOutcome::Insufficient(InsufficientEvidence::HashesNotComparable)
}

fn first_hash_conflict(
    evidence: &MatchEvidence,
    matched: &ProviderRomRecord,
) -> Option<EvidenceConflict> {
    if disagrees(evidence.sha1.as_deref(), matched.sha1.as_deref()) {
        return Some(EvidenceConflict::Sha1Mismatch);
    }
    if disagrees(evidence.md5.as_deref(), matched.md5.as_deref()) {
        return Some(EvidenceConflict::Md5Mismatch);
    }
    if disagrees(evidence.crc32.as_deref(), matched.crc32.as_deref()) {
        return Some(EvidenceConflict::Crc32Mismatch);
    }
    None
}

fn crc32_is_ambiguous(
    evidence: &MatchEvidence,
    record: &ProviderGameRecord,
    matched: &ProviderRomRecord,
) -> bool {
    let Some(local_crc32) = evidence.crc32.as_deref() else {
        return true;
    };
    record
        .roms
        .iter()
        .filter(|candidate| {
            candidate
                .crc32
                .as_deref()
                .is_some_and(|crc32| crc32.eq_ignore_ascii_case(local_crc32))
        })
        .filter(|candidate| candidate.provider_rom_id != matched.provider_rom_id)
        .count()
        > 0
}

/// True when both sides carry the value and they are equal.
fn compares(local: Option<&str>, provider: Option<&str>) -> bool {
    matches!((local, provider), (Some(local), Some(provider)) if local.eq_ignore_ascii_case(provider))
}

/// True when both sides carry the value and they differ.
fn disagrees(local: Option<&str>, provider: Option<&str>) -> bool {
    matches!((local, provider), (Some(local), Some(provider)) if !local.eq_ignore_ascii_case(provider))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::library::{ContentFileId, ContentUnitId, ContentUnitKind, GameId};
    use crate::domain::metadata::{NormalizedMetadata, EVIDENCE_SCHEMA_VERSION};
    use crate::domain::system::SystemId;

    const SHA1: &str = "da39a3ee5e6b4b0d3255bfef95601890afd80709";
    const MD5: &str = "d41d8cd98f00b204e9800998ecf8427e";
    const CRC32: &str = "AABBCCDD";

    fn evidence() -> MatchEvidence {
        MatchEvidence {
            game_id: GameId(1),
            content_unit_id: ContentUnitId(2),
            system_id: SystemId::Snes,
            content_unit_kind: ContentUnitKind::SingleFile,
            content_file_id: Some(ContentFileId(3)),
            size_bytes: 524_288,
            crc32: Some(CRC32.to_owned()),
            md5: Some(MD5.to_owned()),
            sha1: Some(SHA1.to_owned()),
            fingerprint: Some("fingerprint-1".to_owned()),
            evidence_version: EVIDENCE_SCHEMA_VERSION,
        }
    }

    fn record(matched: Option<ProviderRomRecord>) -> ProviderGameRecord {
        let roms = matched.clone().map(|rom| vec![rom]).unwrap_or_default();
        ProviderGameRecord {
            provider_game_id: "3".to_owned(),
            provider_rom_id: Some("77".to_owned()),
            matched_rom: matched,
            roms,
            metadata: NormalizedMetadata::default(),
            source_credit: None,
            primary_cover: None,
        }
    }

    fn full_rom() -> ProviderRomRecord {
        ProviderRomRecord {
            provider_rom_id: Some("101".to_owned()),
            filename: Some("Example (USA).sfc".to_owned()),
            size_bytes: Some(524_288),
            crc32: Some(CRC32.to_owned()),
            md5: Some(MD5.to_owned()),
            sha1: Some(SHA1.to_owned()),
            support_number: Some(1),
            support_count: Some(1),
        }
    }

    #[test]
    fn exact_sha1_and_size_is_accepted_as_the_strongest_evidence() {
        let outcome = classify_deterministic_match(&evidence(), &record(Some(full_rom())));

        assert_eq!(
            outcome,
            DeterministicOutcome::Accepted {
                match_type: MatchType::DeterministicSha1,
                provider_rom_id: Some("101".to_owned())
            }
        );
    }

    #[test]
    fn exact_md5_and_size_is_accepted_when_sha1_is_not_comparable() {
        let mut rom = full_rom();
        rom.sha1 = None;
        let mut local = evidence();
        local.sha1 = None;

        assert_eq!(
            classify_deterministic_match(&local, &record(Some(rom))),
            DeterministicOutcome::Accepted {
                match_type: MatchType::DeterministicMd5,
                provider_rom_id: Some("101".to_owned())
            }
        );
    }

    #[test]
    fn crc32_and_size_is_the_weakest_accepted_fallback() {
        let mut rom = full_rom();
        rom.sha1 = None;
        rom.md5 = None;
        let mut local = evidence();
        local.sha1 = None;
        local.md5 = None;

        assert_eq!(
            classify_deterministic_match(&local, &record(Some(rom))),
            DeterministicOutcome::Accepted {
                match_type: MatchType::DeterministicCrc32,
                provider_rom_id: Some("101".to_owned())
            }
        );
    }

    #[test]
    fn a_second_provider_record_with_the_same_crc32_blocks_crc32_acceptance() {
        let mut rom = full_rom();
        rom.sha1 = None;
        rom.md5 = None;
        let mut collision = rom.clone();
        collision.provider_rom_id = Some("102".to_owned());
        let mut game = record(Some(rom));
        game.roms.push(collision);
        let mut local = evidence();
        local.sha1 = None;
        local.md5 = None;

        assert_eq!(
            classify_deterministic_match(&local, &game),
            DeterministicOutcome::Conflicting(EvidenceConflict::Crc32Collision)
        );
    }

    #[test]
    fn conflicting_returned_hashes_are_rejected() {
        let mut rom = full_rom();
        rom.sha1 = Some("0000000000000000000000000000000000000000".to_owned());
        assert_eq!(
            classify_deterministic_match(&evidence(), &record(Some(rom))),
            DeterministicOutcome::Conflicting(EvidenceConflict::Sha1Mismatch)
        );

        let mut rom = full_rom();
        rom.sha1 = None;
        rom.md5 = Some("00000000000000000000000000000000".to_owned());
        assert_eq!(
            classify_deterministic_match(&evidence(), &record(Some(rom))),
            DeterministicOutcome::Conflicting(EvidenceConflict::Md5Mismatch)
        );

        let mut rom = full_rom();
        rom.sha1 = None;
        rom.md5 = None;
        rom.crc32 = Some("11223344".to_owned());
        assert_eq!(
            classify_deterministic_match(&evidence(), &record(Some(rom))),
            DeterministicOutcome::Conflicting(EvidenceConflict::Crc32Mismatch)
        );
    }

    #[test]
    fn a_size_mismatch_is_rejected_even_when_a_hash_agrees() {
        let mut rom = full_rom();
        rom.size_bytes = Some(1);

        assert_eq!(
            classify_deterministic_match(&evidence(), &record(Some(rom))),
            DeterministicOutcome::Conflicting(EvidenceConflict::SizeMismatch)
        );
    }

    #[test]
    fn a_response_without_a_content_record_is_never_deterministic() {
        assert_eq!(
            classify_deterministic_match(&evidence(), &record(None)),
            DeterministicOutcome::Insufficient(InsufficientEvidence::ProviderRecordMissing)
        );
    }

    #[test]
    fn a_record_without_hashes_or_size_is_never_deterministic() {
        let mut rom = full_rom();
        rom.crc32 = None;
        rom.md5 = None;
        rom.sha1 = None;
        assert_eq!(
            classify_deterministic_match(&evidence(), &record(Some(rom))),
            DeterministicOutcome::Insufficient(InsufficientEvidence::HashUnavailable)
        );

        let mut rom = full_rom();
        rom.size_bytes = None;
        assert_eq!(
            classify_deterministic_match(&evidence(), &record(Some(rom))),
            DeterministicOutcome::Insufficient(InsufficientEvidence::SizeUnavailable)
        );
    }

    #[test]
    fn an_equal_filename_alone_is_not_deterministic_evidence() {
        let rom = ProviderRomRecord {
            provider_rom_id: Some("101".to_owned()),
            filename: Some("Example (USA).sfc".to_owned()),
            size_bytes: Some(524_288),
            crc32: None,
            md5: None,
            sha1: Some(SHA1.to_owned()),
            support_number: None,
            support_count: None,
        };
        let mut local = evidence();
        local.sha1 = None;
        local.md5 = None;
        local.crc32 = None;

        assert_eq!(
            classify_deterministic_match(&local, &record(Some(rom))),
            DeterministicOutcome::Insufficient(InsufficientEvidence::HashesNotComparable),
            "an identical basename and size must not be treated as an exact match"
        );
    }
}
