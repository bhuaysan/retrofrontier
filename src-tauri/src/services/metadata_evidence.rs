//! Shared live-evidence validation for provider matches.
//!
//! M4 keeps local IDs stable across same-path byte replacement, so a persisted provider match is
//! trusted only while its evidence still agrees with current content. This module is the shared
//! read-side authority for that rule. It deliberately performs only local SQLite reads; provider
//! networking and metadata writes remain in `MetadataApplicationService`.

use crate::domain::library::GameId;
use crate::domain::metadata::{evidence_for_unit, MatchEvidence, MatchType, ProviderMatchStatus};
use crate::error::AppError;
use crate::repositories::library::LibraryRepository;
use std::collections::BTreeMap;

#[derive(Clone)]
pub struct MetadataEvidenceService {
    library: LibraryRepository,
}

impl MetadataEvidenceService {
    pub fn new(library: LibraryRepository) -> Self {
        Self { library }
    }

    /// Returns the first eligible current M4 evidence for one game.
    pub async fn current_evidence(
        &self,
        game_id: GameId,
    ) -> Result<Option<MatchEvidence>, AppError> {
        let mut evidence = self.current_evidence_for_games(&[game_id]).await?;
        Ok(evidence.remove(&game_id).flatten())
    }

    /// Loads current evidence for a bounded set of games in bulk.
    ///
    /// The library list is capped at 60 items. Keeping the content-unit and membership reads
    /// bulk-shaped avoids turning that page into one repository read per matched game.
    pub async fn current_evidence_for_games(
        &self,
        game_ids: &[GameId],
    ) -> Result<BTreeMap<GameId, Option<MatchEvidence>>, AppError> {
        let units_by_game = self.library.game_content_units_for_games(game_ids).await?;
        Ok(game_ids
            .iter()
            .copied()
            .map(|game_id| {
                let evidence = units_by_game
                    .get(&game_id)
                    .and_then(|units| units.iter().find_map(|unit| evidence_for_unit(unit).ok()));
                (game_id, evidence)
            })
            .collect())
    }
}

/// Applies the M5 read invariant to a persisted provider match.
///
/// A deterministic match needs a stored snapshot that agrees with current evidence. A
/// user-confirmed relationship may have no comparable snapshot, so it remains current under the
/// same semantics used by `get_metadata_state`. Missing evidence for a deterministic match is
/// untrusted and therefore stale.
pub fn evidence_is_current(
    stored_evidence: Option<&MatchEvidence>,
    match_type: Option<MatchType>,
    current_evidence: Option<&MatchEvidence>,
) -> bool {
    match (stored_evidence, match_type) {
        (Some(stored_evidence), _) => {
            current_evidence.is_some_and(|current| stored_evidence.agrees_with(current))
        }
        (None, Some(match_type)) => !match_type.is_deterministic(),
        (None, None) => true,
    }
}

/// Returns the externally visible status after applying the live-evidence read check.
pub fn effective_match_status(
    status: ProviderMatchStatus,
    match_type: Option<MatchType>,
    stored_evidence: Option<&MatchEvidence>,
    current_evidence: Option<&MatchEvidence>,
) -> ProviderMatchStatus {
    if status == ProviderMatchStatus::Matched
        && !evidence_is_current(stored_evidence, match_type, current_evidence)
    {
        ProviderMatchStatus::Stale
    } else {
        status
    }
}
