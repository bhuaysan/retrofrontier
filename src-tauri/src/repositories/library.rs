use crate::domain::library::{
    cached_cover_reference, ContentFile, ContentFileAvailability, ContentFileId,
    ContentFileMembership, ContentFileRole, ContentRoot, ContentRootAvailability, ContentRootId,
    ContentRootKind, ContentUnit, ContentUnitAvailability, ContentUnitId, ContentUnitKind, Game,
    GameAvailability, GameId, GameSnapshot, LibraryContentUnitSummary, LibraryGameDetail,
    LibraryListItem, LibraryMetadataMatchState, LibraryPage, LibraryQuery, LibraryShelf,
    LibraryShelfQuery, LibraryShelves, LibrarySnapshot, LibrarySummary, LibrarySystemCount,
    ScanCounters, ScanIssue, ScanIssueId, ScanIssueKind, ScanIssuePage, ScanPhase, ScanProgress,
    ScanRunId, ScanRunState, ScanStatus, ScanSummary, ScannedRoot, DEFAULT_SCAN_ISSUE_PAGE_SIZE,
    MAX_SCAN_ISSUE_PAGE_SIZE,
};
use crate::domain::metadata::MetadataProviderId;
use crate::domain::system::SystemId;
use crate::error::AppError;
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool, Transaction};
use std::collections::{BTreeMap, BTreeSet};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone)]
pub struct LibraryRepository {
    pool: SqlitePool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReconciliationResult {
    pub issues_found: u64,
}

macro_rules! bind_library_query {
    ($query:expr, $request:expr, $provider_id:expr, $search:expr, $genre:expr, $region:expr, $availability:expr) => {{
        $query
            // The three provider joins are all constrained to the provider selected by the
            // application service. No provider payload is returned by this query.
            .bind($provider_id)
            .bind($provider_id)
            .bind($provider_id)
            .bind($request.system_id.map(SystemId::as_str))
            .bind($request.system_id.map(SystemId::as_str))
            .bind(if $request.favorites_only {
                1_i64
            } else {
                0_i64
            })
            .bind(if $request.needs_metadata_review {
                1_i64
            } else {
                0_i64
            })
            .bind($genre)
            .bind($genre)
            .bind($region)
            .bind($region)
            .bind($availability)
            .bind($availability)
            .bind($search)
            .bind($search)
            .bind($search)
    }};
}

impl LibraryRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn get_content_roots(&self) -> Result<Vec<ContentRoot>, AppError> {
        let rows = sqlx::query(
            "SELECT id, path, kind, enabled, system_hint, availability, last_scan_at, \
             last_successful_scan_at, created_at, updated_at \
             FROM content_roots ORDER BY kind = 'managed' DESC, path ASC, id ASC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::Database)?;

        rows.into_iter().map(content_root_from_row).collect()
    }

    pub async fn find_content_root_by_path(
        &self,
        path: &str,
    ) -> Result<Option<ContentRoot>, AppError> {
        let row = sqlx::query(
            "SELECT id, path, kind, enabled, system_hint, availability, last_scan_at, \
             last_successful_scan_at, created_at, updated_at \
             FROM content_roots WHERE path = ?",
        )
        .bind(path)
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::Database)?;

        row.map(content_root_from_row).transpose()
    }

    pub async fn upsert_managed_root(&self, path: &str) -> Result<ContentRoot, AppError> {
        let now = now_timestamp();
        sqlx::query(
            "INSERT INTO content_roots \
             (path, kind, enabled, system_hint, availability, created_at, updated_at) \
             VALUES (?, 'managed', 1, NULL, 'available', ?, ?) \
             ON CONFLICT(path) DO UPDATE SET \
             kind = 'managed', enabled = 1, system_hint = NULL, availability = 'available', \
             updated_at = excluded.updated_at",
        )
        .bind(path)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(AppError::Database)?;

        self.find_content_root_by_path(path)
            .await?
            .ok_or_else(|| AppError::Library("managed content root was not persisted".to_owned()))
    }

    pub async fn upsert_external_root(
        &self,
        path: &str,
        system_hint: Option<SystemId>,
    ) -> Result<ContentRoot, AppError> {
        if let Some(existing) = self.find_content_root_by_path(path).await? {
            if existing.kind == ContentRootKind::Managed {
                return Err(AppError::ContentRootInvalidOperation);
            }
        }

        let now = now_timestamp();
        sqlx::query(
            "INSERT INTO content_roots \
             (path, kind, enabled, system_hint, availability, created_at, updated_at) \
             VALUES (?, 'external', 1, ?, 'unavailable', ?, ?) \
             ON CONFLICT(path) DO UPDATE SET \
             kind = 'external', enabled = 1, system_hint = excluded.system_hint, \
             availability = 'unavailable', updated_at = excluded.updated_at",
        )
        .bind(path)
        .bind(system_hint.map(|system| system.as_str()))
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(AppError::Database)?;

        self.find_content_root_by_path(path)
            .await?
            .ok_or_else(|| AppError::Library("external content root was not persisted".to_owned()))
    }

    pub async fn remove_external_root(&self, root_id: ContentRootId) -> Result<(), AppError> {
        let root = self
            .content_root(root_id)
            .await?
            .ok_or_else(|| AppError::ContentRootInvalidOperation)?;
        if root.kind == ContentRootKind::Managed {
            return Err(AppError::ContentRootInvalidOperation);
        }

        sqlx::query(
            "UPDATE content_roots SET enabled = 0, availability = 'disabled', updated_at = ? \
             WHERE id = ?",
        )
        .bind(now_timestamp())
        .bind(root_id.0)
        .execute(&self.pool)
        .await
        .map_err(AppError::Database)?;
        self.mark_root_content_missing(root_id).await?;
        Ok(())
    }

    pub async fn set_content_root_enabled(
        &self,
        root_id: ContentRootId,
        enabled: bool,
    ) -> Result<ContentRoot, AppError> {
        let changed = sqlx::query(
            "UPDATE content_roots SET enabled = ?, \
             availability = CASE WHEN ? = 1 THEN 'unavailable' ELSE 'disabled' END, \
             updated_at = ? WHERE id = ?",
        )
        .bind(enabled)
        .bind(enabled)
        .bind(now_timestamp())
        .bind(root_id.0)
        .execute(&self.pool)
        .await
        .map_err(AppError::Database)?;
        if changed.rows_affected() == 0 {
            return Err(AppError::ContentRootInvalidOperation);
        }
        if !enabled {
            self.mark_root_content_missing(root_id).await?;
        }
        self.content_root(root_id)
            .await?
            .ok_or_else(|| AppError::Library("content root disappeared after update".to_owned()))
    }

    pub async fn content_root(
        &self,
        root_id: ContentRootId,
    ) -> Result<Option<ContentRoot>, AppError> {
        let row = sqlx::query(
            "SELECT id, path, kind, enabled, system_hint, availability, last_scan_at, \
             last_successful_scan_at, created_at, updated_at \
             FROM content_roots WHERE id = ?",
        )
        .bind(root_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::Database)?;
        row.map(content_root_from_row).transpose()
    }

    async fn mark_root_content_missing(&self, root_id: ContentRootId) -> Result<(), AppError> {
        let mut transaction = self.pool.begin().await.map_err(AppError::Database)?;
        let now = now_timestamp();
        sqlx::query(
            "UPDATE content_files SET availability = 'missing', updated_at = ? WHERE root_id = ?",
        )
        .bind(now)
        .bind(root_id.0)
        .execute(&mut *transaction)
        .await
        .map_err(AppError::Database)?;
        sqlx::query(
            "UPDATE content_units SET availability = 'missing', updated_at = ? WHERE root_id = ?",
        )
        .bind(now)
        .bind(root_id.0)
        .execute(&mut *transaction)
        .await
        .map_err(AppError::Database)?;
        sqlx::query(
            "UPDATE games SET availability = CASE WHEN EXISTS (\
                SELECT 1 FROM content_units WHERE content_units.game_id = games.id \
                AND content_units.availability = 'available'\
             ) THEN 'available' ELSE 'unavailable' END, updated_at = ?",
        )
        .bind(now)
        .execute(&mut *transaction)
        .await
        .map_err(AppError::Database)?;
        transaction.commit().await.map_err(AppError::Database)
    }

    pub async fn start_scan_run(&self) -> Result<ScanRunId, AppError> {
        let result = sqlx::query("INSERT INTO scan_runs (state, started_at) VALUES ('running', ?)")
            .bind(now_timestamp())
            .execute(&self.pool)
            .await
            .map_err(AppError::Database)?;
        Ok(ScanRunId(result.last_insert_rowid()))
    }

    pub async fn recover_interrupted_scan_runs(&self) -> Result<(), AppError> {
        sqlx::query(
            "UPDATE scan_runs SET state = 'failed', completed_at = ? \
             WHERE state = 'running'",
        )
        .bind(now_timestamp())
        .execute(&self.pool)
        .await
        .map_err(AppError::Database)?;
        Ok(())
    }

    pub async fn finish_scan_run(
        &self,
        run_id: ScanRunId,
        state: ScanRunState,
        counters: ScanCounters,
    ) -> Result<(), AppError> {
        sqlx::query(
            "UPDATE scan_runs SET state = ?, completed_at = ?, roots_discovered = ?, \
             roots_completed = ?, files_discovered = ?, files_processed = ?, files_hashed = ?, \
             bytes_hashed = ?, issues_found = ? WHERE id = ?",
        )
        .bind(state.as_db())
        .bind(now_timestamp())
        .bind(i64_counter(counters.roots_discovered))
        .bind(i64_counter(counters.roots_completed))
        .bind(i64_counter(counters.files_discovered))
        .bind(i64_counter(counters.files_processed))
        .bind(i64_counter(counters.files_hashed))
        .bind(i64_counter(counters.bytes_hashed))
        .bind(i64_counter(counters.issues_found))
        .bind(run_id.0)
        .execute(&self.pool)
        .await
        .map_err(AppError::Database)?;
        Ok(())
    }

    pub async fn latest_scan_status(&self) -> Result<Option<ScanStatus>, AppError> {
        let row = sqlx::query(
            "SELECT id, state, started_at, completed_at, roots_discovered, roots_completed, \
             files_discovered, files_processed, files_hashed, bytes_hashed, issues_found \
             FROM scan_runs ORDER BY id DESC LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::Database)?;
        let Some(row) = row else { return Ok(None) };

        let run_id = ScanRunId(row.get("id"));
        let state = scan_state(&row.get::<String, _>("state"))?;
        let counters = scan_counters_from_row(&row);
        let summary = ScanSummary {
            run_id,
            state,
            counters,
            duration_ms: duration_ms(row.get("started_at"), row.try_get("completed_at").ok()),
        };
        Ok(Some(ScanStatus {
            running: state == ScanRunState::Running,
            progress: if state == ScanRunState::Running {
                Some(ScanProgress {
                    run_id,
                    phase: ScanPhase::Discovery,
                    counters,
                })
            } else {
                None
            },
            last_result: if state == ScanRunState::Running {
                None
            } else {
                Some(summary)
            },
        }))
    }

    pub async fn list_latest_scan_issues(&self) -> Result<Vec<ScanIssue>, AppError> {
        let rows = sqlx::query(
            "SELECT i.id, i.scan_run_id, i.root_id, i.kind, i.relative_path, i.related_path, \
             i.detail, i.created_at FROM scan_issues i \
             WHERE i.scan_run_id = (SELECT id FROM scan_runs ORDER BY id DESC LIMIT 1) \
             ORDER BY i.id ASC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::Database)?;
        rows.into_iter().map(scan_issue_from_row).collect()
    }

    /// Loads one bounded page from the latest persisted scan run. The legacy unbounded method
    /// above remains for the M4 diagnostic contract; M6 UI callers use this method exclusively.
    pub async fn list_latest_scan_issues_page(
        &self,
        offset: u64,
        requested_limit: u32,
    ) -> Result<ScanIssuePage, AppError> {
        let limit = bounded_scan_issue_limit(requested_limit);
        let offset = sqlite_offset(offset)?;
        let scan_run_id: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM scan_runs WHERE state IN ('completed', 'failed') \
             ORDER BY id DESC LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::Database)?;
        let Some(scan_run_id) = scan_run_id else {
            return Ok(ScanIssuePage {
                issues: Vec::new(),
                scan_run_id: None,
                total: 0,
                offset: u64::try_from(offset).map_err(|_| {
                    AppError::Library("scan issue offset could not be represented".to_owned())
                })?,
                limit,
            });
        };
        let total: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM scan_issues WHERE scan_run_id = ?")
                .bind(scan_run_id)
                .fetch_one(&self.pool)
                .await
                .map_err(AppError::Database)?;
        let rows = sqlx::query(
            "SELECT i.id, i.scan_run_id, i.root_id, i.kind, i.relative_path, i.related_path, \
             i.detail, i.created_at FROM scan_issues i \
             WHERE i.scan_run_id = ? \
             ORDER BY i.created_at DESC, i.id DESC LIMIT ? OFFSET ?",
        )
        .bind(scan_run_id)
        .bind(i64::from(limit))
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::Database)?;

        Ok(ScanIssuePage {
            issues: rows
                .into_iter()
                .map(scan_issue_from_row)
                .collect::<Result<_, _>>()?,
            scan_run_id: Some(ScanRunId(scan_run_id)),
            total: u64_value(total)?,
            offset: u64::try_from(offset).map_err(|_| {
                AppError::Library("scan issue offset could not be represented".to_owned())
            })?,
            limit,
        })
    }

    pub async fn reconcile_root(
        &self,
        run_id: ScanRunId,
        snapshot: &ScannedRoot,
    ) -> Result<ReconciliationResult, AppError> {
        let mut transaction = self.pool.begin().await.map_err(AppError::Database)?;
        let root_id = snapshot.root.id;
        let now = now_timestamp();
        let existing_files = load_existing_files(&mut transaction, root_id).await?;
        let existing_by_path: BTreeMap<_, _> = existing_files
            .iter()
            .cloned()
            .map(|file| (file.relative_path.clone(), file))
            .collect();
        let existing_units = load_existing_units(&mut transaction, root_id).await?;

        let mut files_by_id: BTreeMap<ContentFileId, ExistingFile> = existing_files
            .iter()
            .cloned()
            .map(|file| (file.id, file))
            .collect();
        let mut files_by_path = existing_by_path.clone();
        let mut file_ids_by_path = BTreeMap::<String, ContentFileId>::new();
        let mut seen_file_ids = BTreeSet::new();
        let mut generated_issues = Vec::new();
        let discovered_paths: BTreeSet<_> = snapshot
            .files
            .iter()
            .map(|file| file.relative_path.as_str())
            .collect();
        let mut move_candidates_by_path = BTreeMap::<String, Vec<ContentFileId>>::new();
        let mut discovered_matches_by_candidate = BTreeMap::<ContentFileId, usize>::new();
        for file in &snapshot.files {
            if existing_by_path.contains_key(&file.relative_path) {
                continue;
            }
            let candidates: Vec<_> = existing_files
                .iter()
                .filter(|candidate| {
                    (candidate.availability == ContentFileAvailability::Missing
                        || !discovered_paths.contains(candidate.relative_path.as_str()))
                        && hashes_match_file(candidate, file)
                })
                .map(|candidate| candidate.id)
                .collect();
            for candidate_id in &candidates {
                *discovered_matches_by_candidate
                    .entry(*candidate_id)
                    .or_default() += 1;
            }
            move_candidates_by_path.insert(file.relative_path.clone(), candidates);
        }
        let existing_file_ids: BTreeSet<_> = existing_files.iter().map(|file| file.id).collect();

        for file in &snapshot.files {
            let file_id = if let Some(existing) = files_by_path.get(&file.relative_path).cloned() {
                update_file(&mut transaction, &existing, file, now).await?;
                let updated = existing.updated_from_scanned(file);
                put_live_file(&mut files_by_id, &mut files_by_path, updated);
                existing.id
            } else {
                let candidate_ids = move_candidates_by_path
                    .get(&file.relative_path)
                    .expect("move candidates were collected for every new path");
                let unique_candidate =
                    (candidate_ids.len() == 1)
                        .then(|| candidate_ids[0])
                        .filter(|candidate_id| {
                            discovered_matches_by_candidate.get(candidate_id) == Some(&1)
                        });
                if let Some(candidate_id) = unique_candidate {
                    let candidate = files_by_id
                        .get(&candidate_id)
                        .cloned()
                        .expect("move candidate exists");
                    update_file_path_and_content(
                        &mut transaction,
                        &candidate,
                        &file.relative_path,
                        file,
                        now,
                    )
                    .await?;
                    let updated = candidate.updated_from_scanned_at_path(file, &file.relative_path);
                    put_live_file(&mut files_by_id, &mut files_by_path, updated);
                    candidate.id
                } else {
                    let contested_candidate = candidate_ids
                        .first()
                        .and_then(|candidate_id| discovered_matches_by_candidate.get(candidate_id));
                    if candidate_ids.len() > 1
                        || contested_candidate.is_some_and(|count| *count > 1)
                    {
                        generated_issues.push(ScanIssue {
                            id: None,
                            scan_run_id: Some(run_id),
                            root_id: Some(root_id),
                            kind: ScanIssueKind::AmbiguousReconciliation,
                            relative_path: Some(file.relative_path.clone()),
                            related_path: None,
                            detail: Some(if candidate_ids.len() > 1 {
                                "more than one missing file has the same content fingerprint"
                                    .to_owned()
                            } else {
                                "one previous file identity matches more than one discovered path"
                                    .to_owned()
                            }),
                            created_at: now,
                        });
                    }
                    let file_id = insert_file(&mut transaction, root_id, file, now).await?;
                    put_live_file(
                        &mut files_by_id,
                        &mut files_by_path,
                        ExistingFile::from_scanned(file_id, file),
                    );
                    file_id
                }
            };
            seen_file_ids.insert(file_id);
            file_ids_by_path.insert(file.relative_path.clone(), file_id);
        }

        let mut seen_unit_ids = BTreeSet::new();
        let mut used_existing_unit_ids = BTreeSet::new();
        let mut known_fingerprints = BTreeMap::<String, Vec<(ContentUnitId, GameId)>>::new();
        for unit in &existing_units {
            if let Some(fingerprint) = &unit.fingerprint {
                known_fingerprints
                    .entry(fingerprint.clone())
                    .or_default()
                    .push((unit.id, unit.game_id));
            }
        }
        let fingerprint_rows = sqlx::query(
            "SELECT id, game_id, fingerprint FROM content_units \
             WHERE fingerprint IS NOT NULL ORDER BY id ASC",
        )
        .fetch_all(&mut *transaction)
        .await
        .map_err(AppError::Database)?;
        for row in fingerprint_rows {
            known_fingerprints
                .entry(row.get::<String, _>("fingerprint"))
                .or_default()
                .push((ContentUnitId(row.get("id")), GameId(row.get("game_id"))));
        }

        for scanned_unit in &snapshot.units {
            let mut member_ids = Vec::with_capacity(scanned_unit.members.len());
            for member in &scanned_unit.members {
                let file_id = if let Some(file_id) = file_ids_by_path.get(&member.relative_path) {
                    *file_id
                } else if let Some(existing) = files_by_path.get(&member.relative_path).cloned() {
                    mark_file_missing(&mut transaction, existing.id, now).await?;
                    let mut updated = existing;
                    updated.availability = ContentFileAvailability::Missing;
                    let file_id = updated.id;
                    put_live_file(&mut files_by_id, &mut files_by_path, updated);
                    file_id
                } else {
                    let file_id =
                        insert_missing_file(&mut transaction, root_id, &member.relative_path, now)
                            .await?;
                    put_live_file(
                        &mut files_by_id,
                        &mut files_by_path,
                        ExistingFile::missing(file_id, &member.relative_path),
                    );
                    file_ids_by_path.insert(member.relative_path.clone(), file_id);
                    file_id
                };
                member_ids.push(file_id);
            }

            let primary_file_id = file_ids_by_path
                .get(&scanned_unit.primary_relative_path)
                .copied()
                .or_else(|| {
                    files_by_path
                        .get(&scanned_unit.primary_relative_path)
                        .map(|file| file.id)
                });
            let matching_existing: Vec<_> = existing_units
                .iter()
                .filter(|candidate| {
                    candidate.system_id == scanned_unit.system_id
                        && candidate.kind == scanned_unit.kind
                        && !used_existing_unit_ids.contains(&candidate.id)
                        && (candidate.primary_relative_path == scanned_unit.primary_relative_path
                            || primary_file_id.is_some_and(|file_id| {
                                candidate
                                    .members
                                    .iter()
                                    .any(|member| member.file_id == file_id)
                            }))
                })
                .collect();

            let (unit_id, game_id, is_new_unit) = if matching_existing.len() == 1 {
                let existing = matching_existing[0];
                used_existing_unit_ids.insert(existing.id);
                (existing.id, existing.game_id, false)
            } else {
                if matching_existing.len() > 1 {
                    generated_issues.push(ScanIssue {
                        id: None,
                        scan_run_id: Some(run_id),
                        root_id: Some(root_id),
                        kind: ScanIssueKind::AmbiguousReconciliation,
                        relative_path: Some(scanned_unit.primary_relative_path.clone()),
                        related_path: None,
                        detail: Some(
                            "more than one content unit matches the primary file".to_owned(),
                        ),
                        created_at: now,
                    });
                }

                let fingerprint_game_ids: BTreeSet<_> = scanned_unit
                    .fingerprint
                    .as_ref()
                    .and_then(|fingerprint| known_fingerprints.get(fingerprint))
                    .into_iter()
                    .flatten()
                    .map(|(_, game_id)| *game_id)
                    .collect();
                let predecessor_game_ids =
                    if scanned_unit.kind == ContentUnitKind::M3u && matching_existing.is_empty() {
                        m3u_predecessor_game_ids(
                            scanned_unit,
                            &member_ids,
                            &existing_file_ids,
                            &existing_units,
                        )
                    } else {
                        BTreeSet::new()
                    };
                let game_evidence: BTreeSet<_> = fingerprint_game_ids
                    .union(&predecessor_game_ids)
                    .copied()
                    .collect();
                let identity_is_ambiguous = fingerprint_game_ids.len() > 1
                    || predecessor_game_ids.len() > 1
                    || game_evidence.len() > 1;
                let reconciled_game = (!identity_is_ambiguous && game_evidence.len() == 1)
                    .then(|| *game_evidence.first().expect("one game id exists"));

                if fingerprint_game_ids.len() == 1 && !identity_is_ambiguous {
                    generated_issues.push(ScanIssue {
                        id: None,
                        scan_run_id: Some(run_id),
                        root_id: Some(root_id),
                        kind: ScanIssueKind::DuplicateContent,
                        relative_path: Some(scanned_unit.primary_relative_path.clone()),
                        related_path: None,
                        detail: Some(
                            "an exact content-identical copy is represented as another content unit"
                                .to_owned(),
                        ),
                        created_at: now,
                    });
                }
                if identity_is_ambiguous {
                    generated_issues.push(ScanIssue {
                        id: None,
                        scan_run_id: Some(run_id),
                        root_id: Some(root_id),
                        kind: ScanIssueKind::AmbiguousReconciliation,
                        relative_path: Some(scanned_unit.primary_relative_path.clone()),
                        related_path: None,
                        detail: Some(if predecessor_game_ids.len() > 1 {
                            "playlist content belongs to more than one previous logical game"
                                .to_owned()
                        } else if !predecessor_game_ids.is_empty()
                            && !fingerprint_game_ids.is_empty()
                        {
                            "playlist ownership and exact fingerprint evidence identify different logical games"
                                .to_owned()
                        } else {
                            "an exact content fingerprint belongs to more than one logical game"
                                .to_owned()
                        }),
                        created_at: now,
                    });
                }

                let game_id = if let Some(game_id) = reconciled_game {
                    game_id
                } else {
                    insert_game(
                        &mut transaction,
                        scanned_unit.system_id,
                        &scanned_unit.local_title,
                        now,
                    )
                    .await?
                };
                let unit_id =
                    insert_unit(&mut transaction, game_id, root_id, scanned_unit, now).await?;
                (unit_id, game_id, true)
            };

            if !is_new_unit {
                let existing = matching_existing
                    .first()
                    .copied()
                    .expect("matched content unit exists");
                let fingerprint = if scanned_unit.hash_failed {
                    scanned_unit
                        .fingerprint
                        .as_deref()
                        .or(existing.fingerprint.as_deref())
                } else {
                    scanned_unit.fingerprint.as_deref()
                };
                update_unit(&mut transaction, unit_id, scanned_unit, fingerprint, now).await?;
            }
            replace_unit_members(
                &mut transaction,
                unit_id,
                &scanned_unit.members,
                &member_ids,
            )
            .await?;
            seen_unit_ids.insert(unit_id);
            let persisted_fingerprint = if !is_new_unit && scanned_unit.hash_failed {
                matching_existing
                    .first()
                    .and_then(|existing| existing.fingerprint.as_ref())
                    .or(scanned_unit.fingerprint.as_ref())
            } else {
                scanned_unit.fingerprint.as_ref()
            };
            if let Some(fingerprint) = persisted_fingerprint {
                known_fingerprints
                    .entry(fingerprint.clone())
                    .or_default()
                    .push((unit_id, game_id));
            }
        }

        if snapshot.authority.root_enumerated {
            for existing in &existing_files {
                if !seen_file_ids.contains(&existing.id)
                    && snapshot
                        .authority
                        .can_reconcile_file(&existing.relative_path)
                {
                    mark_file_missing(&mut transaction, existing.id, now).await?;
                    if let Some(file) = files_by_id.get_mut(&existing.id) {
                        file.availability = ContentFileAvailability::Missing;
                    }
                    if let Some(file) = files_by_path.get_mut(&existing.relative_path) {
                        if file.id == existing.id {
                            file.availability = ContentFileAvailability::Missing;
                        }
                    }
                }
            }
            for existing in &existing_units {
                if !seen_unit_ids.contains(&existing.id)
                    && existing_unit_is_authoritative(existing, &files_by_id, &snapshot.authority)
                {
                    let availability = if existing.members.iter().any(|member| {
                        files_by_id.get(&member.file_id).is_some_and(|file| {
                            file.availability != ContentFileAvailability::Missing
                        })
                    }) {
                        ContentUnitAvailability::Incomplete
                    } else {
                        ContentUnitAvailability::Missing
                    };
                    sqlx::query(
                        "UPDATE content_units SET availability = ?, updated_at = ? WHERE id = ?",
                    )
                    .bind(availability.as_db())
                    .bind(now)
                    .bind(existing.id.0)
                    .execute(&mut *transaction)
                    .await
                    .map_err(AppError::Database)?;
                }
            }
        }

        let root_fully_authoritative = snapshot.authority.is_fully_authoritative();
        let root_availability = if root_fully_authoritative {
            ContentRootAvailability::Available
        } else {
            snapshot.root.availability
        };
        sqlx::query(
            "UPDATE content_roots SET availability = ?, last_scan_at = ?, \
             last_successful_scan_at = CASE WHEN ? = 1 THEN ? ELSE last_successful_scan_at END, \
             updated_at = ? WHERE id = ?",
        )
        .bind(root_availability.as_db())
        .bind(now)
        .bind(root_fully_authoritative)
        .bind(now)
        .bind(now)
        .bind(root_id.0)
        .execute(&mut *transaction)
        .await
        .map_err(AppError::Database)?;

        let mut all_issues = snapshot.issues.clone();
        let generated_issue_count = generated_issues.len();
        all_issues.extend(generated_issues);
        for issue in &all_issues {
            insert_issue(&mut transaction, run_id, root_id, issue).await?;
        }

        sqlx::query(
            "UPDATE games SET availability = CASE WHEN EXISTS (\
                SELECT 1 FROM content_units WHERE content_units.game_id = games.id \
                AND content_units.availability = 'available'\
             ) THEN 'available' ELSE 'unavailable' END, updated_at = ?",
        )
        .bind(now)
        .execute(&mut *transaction)
        .await
        .map_err(AppError::Database)?;

        transaction.commit().await.map_err(AppError::Database)?;
        Ok(ReconciliationResult {
            issues_found: generated_issue_count as u64,
        })
    }

    /// Loads one game without materializing the library snapshot.
    ///
    /// Metadata processing works game by game, so it must never depend on reading the whole
    /// library.
    pub async fn game(&self, game_id: GameId) -> Result<Option<Game>, AppError> {
        let row = sqlx::query(
            "SELECT id, system_id, local_title, availability, created_at, updated_at \
             FROM games WHERE id = ?",
        )
        .bind(game_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::Database)?;

        row.as_ref().map(game_from_row).transpose()
    }

    /// Returns the bounded list projection consumed by the M6 library UI.
    ///
    /// Count and page are separate aggregate/limited queries. The page joins normalized metadata,
    /// provider state, user favorites, and durable media identity in bulk; it never joins physical
    /// content files and therefore cannot expose scanner hashes or fingerprints.
    pub async fn query_library(
        &self,
        request: &LibraryQuery,
        provider_id: MetadataProviderId,
    ) -> Result<LibraryPage, AppError> {
        let limit = request.bounded_limit();
        let offset = sqlite_offset(request.offset)?;
        let search = normalized_search_filter(request.search.as_deref());
        let genre = normalized_filter(request.genre.as_deref());
        let region = normalized_filter(request.region.as_deref());
        let availability = request.availability.map(GameAvailability::as_db);
        let provider_id = provider_id.as_db();

        let total: i64 = bind_library_query!(
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM games g \
                 LEFT JOIN game_user_state us ON us.game_id = g.id \
                 LEFT JOIN provider_metadata md ON md.game_id = g.id AND md.provider_id = ? \
                 LEFT JOIN provider_matches pm ON pm.game_id = g.id AND pm.provider_id = ? \
                 LEFT JOIN provider_media_assets ma ON ma.game_id = g.id \
                     AND ma.provider_id = ? AND ma.kind = 'cover' \
                 WHERE (? IS NULL OR g.system_id = ?) \
                   AND (? = 0 OR COALESCE(us.favorite, 0) = 1) \
                   AND (? = 0 OR pm.status = 'ambiguous') \
                   AND (? IS NULL OR lower(COALESCE(md.genre, '')) = lower(?)) \
                   AND (? IS NULL OR lower(COALESCE(md.region, '')) = lower(?)) \
                   AND (? IS NULL OR g.availability = ?) \
                   AND (? IS NULL OR lower(COALESCE(md.title, '')) LIKE '%' || lower(?) || '%' ESCAPE '\\' \
                        OR lower(g.local_title) LIKE '%' || lower(?) || '%' ESCAPE '\\')"
            ),
            request,
            provider_id,
            search.as_deref(),
            genre.as_deref(),
            region.as_deref(),
            availability
        )
        .fetch_one(&self.pool)
        .await
        .map_err(AppError::Database)?;

        let rows = bind_library_query!(
            sqlx::query(
                "SELECT g.id AS game_id, g.system_id, g.local_title, g.availability, \
                        md.title AS metadata_title, md.sort_title AS metadata_sort_title, \
                        md.release_date, md.genre, md.region, \
                        COALESCE(us.favorite, 0) AS favorite, pm.status AS metadata_status, \
                        CASE WHEN ma.state = 'cached' \
                                  AND ma.cache_relative_path IS NOT NULL \
                                  AND lower(COALESCE(ma.content_type, '')) IN \
                                      ('image/png', 'image/jpeg', 'image/webp') \
                             THEN 1 ELSE 0 END AS cover_cached, \
                        COALESCE(NULLIF(md.sort_title, ''), NULLIF(md.title, ''), g.local_title) \
                            AS effective_sort_title \
                 FROM games g \
                 LEFT JOIN game_user_state us ON us.game_id = g.id \
                 LEFT JOIN provider_metadata md ON md.game_id = g.id AND md.provider_id = ? \
                 LEFT JOIN provider_matches pm ON pm.game_id = g.id AND pm.provider_id = ? \
                 LEFT JOIN provider_media_assets ma ON ma.game_id = g.id \
                     AND ma.provider_id = ? AND ma.kind = 'cover' \
                 WHERE (? IS NULL OR g.system_id = ?) \
                   AND (? = 0 OR COALESCE(us.favorite, 0) = 1) \
                   AND (? = 0 OR pm.status = 'ambiguous') \
                   AND (? IS NULL OR lower(COALESCE(md.genre, '')) = lower(?)) \
                   AND (? IS NULL OR lower(COALESCE(md.region, '')) = lower(?)) \
                   AND (? IS NULL OR g.availability = ?) \
                   AND (? IS NULL OR lower(COALESCE(md.title, '')) LIKE '%' || lower(?) || '%' ESCAPE '\\' \
                        OR lower(g.local_title) LIKE '%' || lower(?) || '%' ESCAPE '\\') \
                 ORDER BY lower(effective_sort_title) ASC, g.id ASC LIMIT ? OFFSET ?"
            ),
            request,
            provider_id,
            search.as_deref(),
            genre.as_deref(),
            region.as_deref(),
            availability
        )
        .bind(i64::from(limit))
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::Database)?;

        Ok(LibraryPage {
            items: rows
                .iter()
                .map(library_list_item_from_row)
                .collect::<Result<_, _>>()?,
            total: u64_value(total)?,
            offset: request.offset,
            limit,
        })
    }

    /// Returns the bounded All Systems shelf projection.
    ///
    /// One set-oriented query, never one per system. `ROW_NUMBER()` ranks each system's matches in
    /// the *same* title order the paginated grid uses and `COUNT(*)` counts them, both partitioned
    /// by system in a single pass; only rows inside the preview rank survive the outer filter. The
    /// response size is therefore bounded by `system count x preview limit` and does not grow with
    /// the library.
    ///
    /// The filter predicate and the projected columns are literally the same text the grid query
    /// uses, bound through the same macro from a `LibraryQuery` derived by the domain. That is what
    /// keeps search, favorites and metadata-review semantics from drifting between the two views.
    ///
    /// Systems are not filtered against a known-system list: every system present in the matching
    /// data gets a shelf, so content whose system this build does not rank first is still returned
    /// rather than silently dropped.
    pub async fn query_library_shelves(
        &self,
        request: &LibraryShelfQuery,
        provider_id: MetadataProviderId,
    ) -> Result<LibraryShelves, AppError> {
        let preview_limit = request.bounded_preview_limit();
        let query = request.as_library_query();
        let search = normalized_search_filter(query.search.as_deref());
        let genre = normalized_filter(query.genre.as_deref());
        let region = normalized_filter(query.region.as_deref());
        let availability = query.availability.map(GameAvailability::as_db);
        let provider_id = provider_id.as_db();

        let rows = bind_library_query!(
            sqlx::query(
                "WITH ranked AS ( \
                    SELECT g.id AS game_id, g.system_id, g.local_title, g.availability, \
                           md.title AS metadata_title, md.sort_title AS metadata_sort_title, \
                           md.release_date, md.genre, md.region, \
                           COALESCE(us.favorite, 0) AS favorite, pm.status AS metadata_status, \
                           CASE WHEN ma.state = 'cached' \
                                     AND ma.cache_relative_path IS NOT NULL \
                                     AND lower(COALESCE(ma.content_type, '')) IN \
                                         ('image/png', 'image/jpeg', 'image/webp') \
                                THEN 1 ELSE 0 END AS cover_cached, \
                           COALESCE(NULLIF(md.sort_title, ''), NULLIF(md.title, ''), \
                                    g.local_title) AS effective_sort_title, \
                           ROW_NUMBER() OVER ( \
                               PARTITION BY g.system_id \
                               ORDER BY lower(COALESCE(NULLIF(md.sort_title, ''), \
                                                       NULLIF(md.title, ''), \
                                                       g.local_title)) ASC, g.id ASC \
                           ) AS shelf_rank, \
                           COUNT(*) OVER (PARTITION BY g.system_id) AS shelf_total \
                    FROM games g \
                    LEFT JOIN game_user_state us ON us.game_id = g.id \
                    LEFT JOIN provider_metadata md ON md.game_id = g.id AND md.provider_id = ? \
                    LEFT JOIN provider_matches pm ON pm.game_id = g.id AND pm.provider_id = ? \
                    LEFT JOIN provider_media_assets ma ON ma.game_id = g.id \
                        AND ma.provider_id = ? AND ma.kind = 'cover' \
                    WHERE (? IS NULL OR g.system_id = ?) \
                      AND (? = 0 OR COALESCE(us.favorite, 0) = 1) \
                      AND (? = 0 OR pm.status = 'ambiguous') \
                      AND (? IS NULL OR lower(COALESCE(md.genre, '')) = lower(?)) \
                      AND (? IS NULL OR lower(COALESCE(md.region, '')) = lower(?)) \
                      AND (? IS NULL OR g.availability = ?) \
                      AND (? IS NULL OR lower(COALESCE(md.title, '')) LIKE '%' || lower(?) || '%' ESCAPE '\\' \
                           OR lower(g.local_title) LIKE '%' || lower(?) || '%' ESCAPE '\\') \
                 ) \
                 SELECT * FROM ranked WHERE shelf_rank <= ? \
                 ORDER BY system_id ASC, shelf_rank ASC"
            ),
            query,
            provider_id,
            search.as_deref(),
            genre.as_deref(),
            region.as_deref(),
            availability
        )
        .bind(i64::from(preview_limit))
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::Database)?;

        let mut shelves: Vec<LibraryShelf> = Vec::new();
        for row in &rows {
            let item = library_list_item_from_row(row)?;
            let total = u64_value(row.get::<i64, _>("shelf_total"))?;
            match shelves.last_mut() {
                // Rows arrive grouped and ordered by system, so the open shelf is always the last.
                Some(shelf) if shelf.system_id == item.system_id => shelf.items.push(item),
                _ => shelves.push(LibraryShelf {
                    system_id: item.system_id,
                    total,
                    items: vec![item],
                }),
            }
        }

        Ok(LibraryShelves { shelves })
    }

    /// Returns aggregate counts without materializing game rows.
    pub async fn get_library_summary(&self) -> Result<LibrarySummary, AppError> {
        let row = sqlx::query(
            "SELECT COUNT(*) AS total_games, \
                    COALESCE(SUM(CASE WHEN COALESCE(us.favorite, 0) = 1 THEN 1 ELSE 0 END), 0) \
                        AS favorite_games \
             FROM games g LEFT JOIN game_user_state us ON us.game_id = g.id",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(AppError::Database)?;
        let system_rows = sqlx::query(
            "SELECT system_id, COUNT(*) AS game_count FROM games \
             GROUP BY system_id ORDER BY system_id ASC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::Database)?;

        Ok(LibrarySummary {
            total_games: u64_value(row.get("total_games"))?,
            favorite_games: u64_value(row.get("favorite_games"))?,
            systems: system_rows
                .iter()
                .map(|row| {
                    Ok(LibrarySystemCount {
                        system_id: system_id(&row.get::<String, _>("system_id"))?,
                        game_count: u64_value(row.get("game_count"))?,
                    })
                })
                .collect::<Result<_, AppError>>()?,
        })
    }

    /// Returns only local content structure for one game. Physical-file hashes and content-unit
    /// fingerprints are intentionally absent from this projection.
    pub async fn get_library_game_detail(
        &self,
        game_id: GameId,
    ) -> Result<Option<LibraryGameDetail>, AppError> {
        let row = sqlx::query(
            "SELECT g.id AS game_id, g.system_id, g.local_title, g.availability, \
                    COALESCE(us.favorite, 0) AS favorite \
             FROM games g LEFT JOIN game_user_state us ON us.game_id = g.id \
             WHERE g.id = ?",
        )
        .bind(game_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::Database)?;
        let Some(row) = row else { return Ok(None) };

        let unit_rows = sqlx::query(
            "SELECT cu.id AS unit_id, cu.root_id, cu.kind, cu.local_title, \
                    cu.primary_relative_path, cu.availability, COUNT(cuf.content_file_id) AS file_count \
             FROM content_units cu \
             LEFT JOIN content_unit_files cuf ON cuf.content_unit_id = cu.id \
             WHERE cu.game_id = ? \
             GROUP BY cu.id, cu.root_id, cu.kind, cu.local_title, cu.primary_relative_path, cu.availability \
             ORDER BY cu.id ASC",
        )
        .bind(game_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::Database)?;

        Ok(Some(LibraryGameDetail {
            game_id: GameId(row.get("game_id")),
            system_id: system_id(&row.get::<String, _>("system_id"))?,
            local_title: row.get("local_title"),
            availability: game_availability(&row.get::<String, _>("availability"))?,
            favorite: row.get::<i64, _>("favorite") != 0,
            content_units: unit_rows
                .iter()
                .map(|row| {
                    Ok(LibraryContentUnitSummary {
                        unit_id: ContentUnitId(row.get("unit_id")),
                        root_id: ContentRootId(row.get("root_id")),
                        kind: unit_kind(&row.get::<String, _>("kind"))?,
                        local_title: row.get("local_title"),
                        primary_relative_path: row.get("primary_relative_path"),
                        file_count: u64_value(row.get("file_count"))?,
                        availability: unit_availability_from_db(
                            &row.get::<String, _>("availability"),
                        )?,
                    })
                })
                .collect::<Result<_, AppError>>()?,
        }))
    }

    /// Upserts the one user-owned state currently required by M6.1.
    pub async fn set_game_favorite(&self, game_id: GameId, favorite: bool) -> Result<(), AppError> {
        let exists: Option<i64> = sqlx::query_scalar("SELECT id FROM games WHERE id = ?")
            .bind(game_id.0)
            .fetch_optional(&self.pool)
            .await
            .map_err(AppError::Database)?;
        if exists.is_none() {
            return Err(AppError::Library(
                "the requested game does not exist".to_owned(),
            ));
        }

        let now = now_timestamp();
        sqlx::query(
            "INSERT INTO game_user_state (game_id, favorite, created_at, updated_at) \
             VALUES (?, ?, ?, ?) \
             ON CONFLICT(game_id) DO UPDATE SET favorite = excluded.favorite, \
                                                updated_at = excluded.updated_at",
        )
        .bind(game_id.0)
        .bind(if favorite { 1_i64 } else { 0_i64 })
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(AppError::Database)?;
        Ok(())
    }

    /// Loads the content units of one game with their ordered file membership.
    ///
    /// This is the bounded read that metadata matching uses to obtain current M4 evidence.
    pub async fn game_content_units(&self, game_id: GameId) -> Result<Vec<ContentUnit>, AppError> {
        let mut units = self.game_content_units_for_games(&[game_id]).await?;
        Ok(units.remove(&game_id).unwrap_or_default())
    }

    /// Loads content units and memberships for several games with two bounded bulk reads.
    ///
    /// Metadata list validation uses this instead of reading each game's content separately. The
    /// caller owns the page bound; this repository method never turns a page into an N+1 query.
    pub async fn game_content_units_for_games(
        &self,
        game_ids: &[GameId],
    ) -> Result<BTreeMap<GameId, Vec<ContentUnit>>, AppError> {
        if game_ids.is_empty() {
            return Ok(BTreeMap::new());
        }

        let mut unit_query = QueryBuilder::<Sqlite>::new(
            "SELECT id, game_id, root_id, system_id, kind, local_title, primary_relative_path, \
             fingerprint, availability, created_at, updated_at FROM content_units \
             WHERE game_id IN (",
        );
        {
            let mut separated = unit_query.separated(", ");
            for game_id in game_ids {
                separated.push_bind(game_id.0);
            }
            separated.push_unseparated(") ORDER BY game_id ASC, id ASC");
        }
        let unit_rows = unit_query
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(AppError::Database)?;

        let mut units_by_game = BTreeMap::<GameId, Vec<ContentUnit>>::new();
        let mut unit_locations = BTreeMap::<ContentUnitId, (GameId, usize)>::new();
        for row in &unit_rows {
            let unit = unit_from_row(row)?;
            let game_id = unit.game_id;
            let units = units_by_game.entry(game_id).or_default();
            let index = units.len();
            unit_locations.insert(unit.id, (game_id, index));
            units.push(unit);
        }

        let mut membership_query = QueryBuilder::<Sqlite>::new(
            "SELECT cuf.content_unit_id, cuf.ordinal, cuf.role, \
             cf.id, cf.root_id, cf.relative_path, cf.size_bytes, cf.modified_at, cf.crc32, \
             cf.md5, cf.sha1, cf.availability FROM content_unit_files cuf \
             JOIN content_files cf ON cf.id = cuf.content_file_id \
             JOIN content_units cu ON cu.id = cuf.content_unit_id \
             WHERE cu.game_id IN (",
        );
        {
            let mut separated = membership_query.separated(", ");
            for game_id in game_ids {
                separated.push_bind(game_id.0);
            }
            separated.push_unseparated(") ORDER BY cuf.content_unit_id ASC, cuf.ordinal ASC");
        }
        let membership_rows = membership_query
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(AppError::Database)?;
        for row in &membership_rows {
            let unit_id = ContentUnitId(row.get("content_unit_id"));
            let Some((game_id, index)) = unit_locations.get(&unit_id).copied() else {
                continue;
            };
            let membership = membership_from_row(row)?;
            if let Some(unit) = units_by_game
                .get_mut(&game_id)
                .and_then(|units| units.get_mut(index))
            {
                unit.files.push(membership);
            }
        }

        Ok(units_by_game)
    }

    pub async fn get_library_snapshot(&self) -> Result<LibrarySnapshot, AppError> {
        let game_rows = sqlx::query(
            "SELECT id, system_id, local_title, availability, created_at, updated_at \
             FROM games ORDER BY id ASC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::Database)?;
        let mut games = BTreeMap::<GameId, Game>::new();
        for row in game_rows {
            let game = game_from_row(&row)?;
            games.insert(game.id, game);
        }

        let unit_rows = sqlx::query(
            "SELECT id, game_id, root_id, system_id, kind, local_title, primary_relative_path, \
             fingerprint, availability, created_at, updated_at FROM content_units ORDER BY id ASC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::Database)?;
        let mut units = BTreeMap::<ContentUnitId, ContentUnit>::new();
        for row in unit_rows {
            let unit = unit_from_row(&row)?;
            units.insert(unit.id, unit);
        }

        let membership_rows = sqlx::query(
            "SELECT cuf.content_unit_id, cuf.ordinal, cuf.role, \
             cf.id, cf.root_id, cf.relative_path, cf.size_bytes, cf.modified_at, cf.crc32, \
             cf.md5, cf.sha1, cf.availability FROM content_unit_files cuf \
             JOIN content_files cf ON cf.id = cuf.content_file_id \
             ORDER BY cuf.content_unit_id ASC, cuf.ordinal ASC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::Database)?;
        for row in membership_rows {
            let unit_id = ContentUnitId(row.get("content_unit_id"));
            let membership = membership_from_row(&row)?;
            let unit = units.get_mut(&unit_id).ok_or_else(|| {
                AppError::Library("content membership refers to an unknown unit".to_owned())
            })?;
            unit.files.push(membership);
        }

        let mut snapshots = BTreeMap::<GameId, GameSnapshot>::new();
        for game in games.values() {
            snapshots.insert(
                game.id,
                GameSnapshot {
                    game: game.clone(),
                    content_units: Vec::new(),
                },
            );
        }
        for unit in units.into_values() {
            let snapshot = snapshots.get_mut(&unit.game_id).ok_or_else(|| {
                AppError::Library("content unit refers to an unknown game".to_owned())
            })?;
            snapshot.content_units.push(unit);
        }
        Ok(LibrarySnapshot {
            games: snapshots.into_values().collect(),
        })
    }
}

#[derive(Debug, Clone)]
struct ExistingFile {
    id: ContentFileId,
    relative_path: String,
    size_bytes: u64,
    crc32: Option<String>,
    md5: Option<String>,
    sha1: Option<String>,
    availability: ContentFileAvailability,
}

impl ExistingFile {
    fn from_scanned(id: ContentFileId, file: &crate::domain::library::ScannedFile) -> Self {
        Self {
            id,
            relative_path: file.relative_path.clone(),
            size_bytes: file.size_bytes,
            crc32: file.hashes.as_ref().map(|hashes| hashes.crc32.clone()),
            md5: file.hashes.as_ref().map(|hashes| hashes.md5.clone()),
            sha1: file.hashes.as_ref().map(|hashes| hashes.sha1.clone()),
            availability: if file.available {
                ContentFileAvailability::Available
            } else {
                ContentFileAvailability::Unavailable
            },
        }
    }

    fn missing(id: ContentFileId, relative_path: &str) -> Self {
        Self {
            id,
            relative_path: relative_path.to_owned(),
            size_bytes: 0,
            crc32: None,
            md5: None,
            sha1: None,
            availability: ContentFileAvailability::Missing,
        }
    }

    fn updated_from_scanned(&self, file: &crate::domain::library::ScannedFile) -> Self {
        self.updated_from_scanned_at_path(file, &file.relative_path)
    }

    fn updated_from_scanned_at_path(
        &self,
        file: &crate::domain::library::ScannedFile,
        relative_path: &str,
    ) -> Self {
        let mut updated = Self::from_scanned(self.id, file);
        if file.hash_failed {
            if updated.crc32.is_none() {
                updated.crc32 = self.crc32.clone();
            }
            if updated.md5.is_none() {
                updated.md5 = self.md5.clone();
            }
            if updated.sha1.is_none() {
                updated.sha1 = self.sha1.clone();
            }
        }
        updated.relative_path = relative_path.to_owned();
        updated
    }
}

fn put_live_file(
    files_by_id: &mut BTreeMap<ContentFileId, ExistingFile>,
    files_by_path: &mut BTreeMap<String, ExistingFile>,
    file: ExistingFile,
) {
    let file_id = file.id;
    if let Some(previous) = files_by_id.insert(file_id, file.clone()) {
        if previous.relative_path != file.relative_path
            && files_by_path
                .get(&previous.relative_path)
                .is_some_and(|candidate| candidate.id == file_id)
        {
            files_by_path.remove(&previous.relative_path);
        }
    }
    files_by_path.insert(file.relative_path.clone(), file);
}

#[derive(Debug, Clone)]
struct ExistingMembership {
    file_id: ContentFileId,
}

#[derive(Debug, Clone)]
struct ExistingUnit {
    id: ContentUnitId,
    game_id: GameId,
    system_id: SystemId,
    kind: ContentUnitKind,
    primary_relative_path: String,
    fingerprint: Option<String>,
    members: Vec<ExistingMembership>,
}

fn m3u_predecessor_game_ids(
    scanned_unit: &crate::domain::library::ScannedUnit,
    member_ids: &[ContentFileId],
    existing_file_ids: &BTreeSet<ContentFileId>,
    existing_units: &[ExistingUnit],
) -> BTreeSet<GameId> {
    let mut predecessor_game_ids = BTreeSet::new();
    for (_, file_id) in scanned_unit
        .members
        .iter()
        .zip(member_ids)
        .filter(|(member, file_id)| {
            member.role != ContentFileRole::Playlist && existing_file_ids.contains(file_id)
        })
    {
        let member_game_ids: BTreeSet<_> = existing_units
            .iter()
            .filter_map(|unit| {
                (unit.system_id == scanned_unit.system_id
                    && unit.members.iter().any(|member| member.file_id == *file_id))
                .then_some(unit.game_id)
            })
            .collect();
        if member_game_ids.is_empty() {
            return BTreeSet::new();
        }
        predecessor_game_ids.extend(member_game_ids);
    }
    predecessor_game_ids
}

fn existing_unit_is_authoritative(
    unit: &ExistingUnit,
    files_by_id: &BTreeMap<ContentFileId, ExistingFile>,
    authority: &crate::domain::library::ScanAuthority,
) -> bool {
    if unit.members.is_empty() {
        return authority.can_reconcile_file(&unit.primary_relative_path);
    }
    unit.members.iter().all(|member| {
        files_by_id
            .get(&member.file_id)
            .is_some_and(|file| authority.can_reconcile_file(&file.relative_path))
    })
}

async fn load_existing_files(
    transaction: &mut Transaction<'_, Sqlite>,
    root_id: ContentRootId,
) -> Result<Vec<ExistingFile>, AppError> {
    let rows = sqlx::query(
        "SELECT id, relative_path, size_bytes, crc32, md5, sha1, availability \
         FROM content_files WHERE root_id = ? ORDER BY id ASC",
    )
    .bind(root_id.0)
    .fetch_all(&mut **transaction)
    .await
    .map_err(AppError::Database)?;
    rows.into_iter()
        .map(|row| {
            Ok(ExistingFile {
                id: ContentFileId(row.get("id")),
                relative_path: row.get("relative_path"),
                size_bytes: u64_value(row.get("size_bytes"))?,
                crc32: row.get("crc32"),
                md5: row.get("md5"),
                sha1: row.get("sha1"),
                availability: file_availability(&row.get::<String, _>("availability"))?,
            })
        })
        .collect()
}

async fn load_existing_units(
    transaction: &mut Transaction<'_, Sqlite>,
    root_id: ContentRootId,
) -> Result<Vec<ExistingUnit>, AppError> {
    let rows = sqlx::query(
        "SELECT id, game_id, system_id, kind, primary_relative_path, fingerprint \
         FROM content_units WHERE root_id = ? ORDER BY id ASC",
    )
    .bind(root_id.0)
    .fetch_all(&mut **transaction)
    .await
    .map_err(AppError::Database)?;
    let mut units = Vec::with_capacity(rows.len());
    for row in rows {
        units.push(ExistingUnit {
            id: ContentUnitId(row.get("id")),
            game_id: GameId(row.get("game_id")),
            system_id: system_id(&row.get::<String, _>("system_id"))?,
            kind: unit_kind(&row.get::<String, _>("kind"))?,
            primary_relative_path: row.get("primary_relative_path"),
            fingerprint: row.get("fingerprint"),
            members: Vec::new(),
        });
    }
    let membership_rows = sqlx::query(
        "SELECT content_unit_id, content_file_id FROM content_unit_files \
         WHERE content_unit_id IN (SELECT id FROM content_units WHERE root_id = ?) \
         ORDER BY content_unit_id, ordinal",
    )
    .bind(root_id.0)
    .fetch_all(&mut **transaction)
    .await
    .map_err(AppError::Database)?;
    for row in membership_rows {
        let unit_id = ContentUnitId(row.get("content_unit_id"));
        if let Some(unit) = units.iter_mut().find(|unit| unit.id == unit_id) {
            unit.members.push(ExistingMembership {
                file_id: ContentFileId(row.get("content_file_id")),
            });
        }
    }
    Ok(units)
}

fn hashes_match_file(
    existing: &ExistingFile,
    scanned: &crate::domain::library::ScannedFile,
) -> bool {
    let Some(hashes) = &scanned.hashes else {
        return false;
    };
    existing.size_bytes == scanned.size_bytes
        && existing.crc32.as_deref() == Some(hashes.crc32.as_str())
        && existing.md5.as_deref() == Some(hashes.md5.as_str())
        && existing.sha1.as_deref() == Some(hashes.sha1.as_str())
}

async fn insert_file(
    transaction: &mut Transaction<'_, Sqlite>,
    root_id: ContentRootId,
    file: &crate::domain::library::ScannedFile,
    now: i64,
) -> Result<ContentFileId, AppError> {
    let result = sqlx::query(
        "INSERT INTO content_files \
         (root_id, relative_path, size_bytes, modified_at, crc32, md5, sha1, availability, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(root_id.0)
    .bind(&file.relative_path)
    .bind(sqlite_size(file.size_bytes)?)
    .bind(file.modified_at)
    .bind(file.hashes.as_ref().map(|hashes| hashes.crc32.as_str()))
    .bind(file.hashes.as_ref().map(|hashes| hashes.md5.as_str()))
    .bind(file.hashes.as_ref().map(|hashes| hashes.sha1.as_str()))
    .bind(if file.available {
        ContentFileAvailability::Available.as_db()
    } else {
        ContentFileAvailability::Unavailable.as_db()
    })
    .bind(now)
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(AppError::Database)?;
    Ok(ContentFileId(result.last_insert_rowid()))
}

async fn insert_missing_file(
    transaction: &mut Transaction<'_, Sqlite>,
    root_id: ContentRootId,
    relative_path: &str,
    now: i64,
) -> Result<ContentFileId, AppError> {
    let result = sqlx::query(
        "INSERT INTO content_files \
         (root_id, relative_path, size_bytes, modified_at, availability, created_at, updated_at) \
         VALUES (?, ?, 0, 0, 'missing', ?, ?)",
    )
    .bind(root_id.0)
    .bind(relative_path)
    .bind(now)
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(AppError::Database)?;
    Ok(ContentFileId(result.last_insert_rowid()))
}

async fn update_file(
    transaction: &mut Transaction<'_, Sqlite>,
    existing: &ExistingFile,
    file: &crate::domain::library::ScannedFile,
    now: i64,
) -> Result<(), AppError> {
    let (crc32, md5, sha1) = persisted_hashes(existing, file);
    sqlx::query(
        "UPDATE content_files SET size_bytes = ?, modified_at = ?, crc32 = ?, md5 = ?, sha1 = ?, \
         availability = ?, updated_at = ? WHERE id = ?",
    )
    .bind(sqlite_size(file.size_bytes)?)
    .bind(file.modified_at)
    .bind(crc32)
    .bind(md5)
    .bind(sha1)
    .bind(if file.available {
        ContentFileAvailability::Available.as_db()
    } else {
        ContentFileAvailability::Unavailable.as_db()
    })
    .bind(now)
    .bind(existing.id.0)
    .execute(&mut **transaction)
    .await
    .map_err(AppError::Database)?;
    Ok(())
}

async fn update_file_path_and_content(
    transaction: &mut Transaction<'_, Sqlite>,
    existing: &ExistingFile,
    relative_path: &str,
    file: &crate::domain::library::ScannedFile,
    now: i64,
) -> Result<(), AppError> {
    let (crc32, md5, sha1) = persisted_hashes(existing, file);
    sqlx::query(
        "UPDATE content_files SET relative_path = ?, size_bytes = ?, modified_at = ?, crc32 = ?, \
         md5 = ?, sha1 = ?, availability = ?, updated_at = ? WHERE id = ?",
    )
    .bind(relative_path)
    .bind(sqlite_size(file.size_bytes)?)
    .bind(file.modified_at)
    .bind(crc32)
    .bind(md5)
    .bind(sha1)
    .bind(if file.available {
        ContentFileAvailability::Available.as_db()
    } else {
        ContentFileAvailability::Unavailable.as_db()
    })
    .bind(now)
    .bind(existing.id.0)
    .execute(&mut **transaction)
    .await
    .map_err(AppError::Database)?;
    Ok(())
}

fn persisted_hashes(
    existing: &ExistingFile,
    file: &crate::domain::library::ScannedFile,
) -> (Option<String>, Option<String>, Option<String>) {
    if let Some(hashes) = &file.hashes {
        return (
            Some(hashes.crc32.clone()),
            Some(hashes.md5.clone()),
            Some(hashes.sha1.clone()),
        );
    }
    if file.hash_failed {
        return (
            existing.crc32.clone(),
            existing.md5.clone(),
            existing.sha1.clone(),
        );
    }
    (None, None, None)
}

async fn mark_file_missing(
    transaction: &mut Transaction<'_, Sqlite>,
    file_id: ContentFileId,
    now: i64,
) -> Result<(), AppError> {
    sqlx::query("UPDATE content_files SET availability = 'missing', updated_at = ? WHERE id = ?")
        .bind(now)
        .bind(file_id.0)
        .execute(&mut **transaction)
        .await
        .map_err(AppError::Database)?;
    Ok(())
}

async fn insert_game(
    transaction: &mut Transaction<'_, Sqlite>,
    system_id: SystemId,
    local_title: &str,
    now: i64,
) -> Result<GameId, AppError> {
    let result = sqlx::query(
        "INSERT INTO games (system_id, local_title, availability, created_at, updated_at) \
         VALUES (?, ?, 'unavailable', ?, ?)",
    )
    .bind(system_id.as_str())
    .bind(local_title)
    .bind(now)
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(AppError::Database)?;
    Ok(GameId(result.last_insert_rowid()))
}

async fn insert_unit(
    transaction: &mut Transaction<'_, Sqlite>,
    game_id: GameId,
    root_id: ContentRootId,
    unit: &crate::domain::library::ScannedUnit,
    now: i64,
) -> Result<ContentUnitId, AppError> {
    let result = sqlx::query(
        "INSERT INTO content_units \
         (game_id, root_id, system_id, kind, local_title, primary_relative_path, fingerprint, availability, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(game_id.0)
    .bind(root_id.0)
    .bind(unit.system_id.as_str())
    .bind(unit.kind.as_db())
    .bind(&unit.local_title)
    .bind(&unit.primary_relative_path)
    .bind(unit.fingerprint.as_deref())
    .bind(unit_availability(unit).as_db())
    .bind(now)
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(AppError::Database)?;
    Ok(ContentUnitId(result.last_insert_rowid()))
}

async fn update_unit(
    transaction: &mut Transaction<'_, Sqlite>,
    unit_id: ContentUnitId,
    unit: &crate::domain::library::ScannedUnit,
    fingerprint: Option<&str>,
    now: i64,
) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE content_units SET system_id = ?, kind = ?, primary_relative_path = ?, \
         fingerprint = ?, availability = ?, updated_at = ? WHERE id = ?",
    )
    .bind(unit.system_id.as_str())
    .bind(unit.kind.as_db())
    .bind(&unit.primary_relative_path)
    .bind(fingerprint)
    .bind(unit_availability(unit).as_db())
    .bind(now)
    .bind(unit_id.0)
    .execute(&mut **transaction)
    .await
    .map_err(AppError::Database)?;
    Ok(())
}

async fn replace_unit_members(
    transaction: &mut Transaction<'_, Sqlite>,
    unit_id: ContentUnitId,
    members: &[crate::domain::library::ScannedMember],
    file_ids: &[ContentFileId],
) -> Result<(), AppError> {
    sqlx::query("DELETE FROM content_unit_files WHERE content_unit_id = ?")
        .bind(unit_id.0)
        .execute(&mut **transaction)
        .await
        .map_err(AppError::Database)?;
    for (member, file_id) in members.iter().zip(file_ids) {
        sqlx::query(
            "INSERT INTO content_unit_files (content_unit_id, content_file_id, ordinal, role) \
             VALUES (?, ?, ?, ?)",
        )
        .bind(unit_id.0)
        .bind(file_id.0)
        .bind(member.ordinal)
        .bind(member.role.as_db())
        .execute(&mut **transaction)
        .await
        .map_err(AppError::Database)?;
    }
    Ok(())
}

fn unit_availability(unit: &crate::domain::library::ScannedUnit) -> ContentUnitAvailability {
    if unit.complete {
        ContentUnitAvailability::Available
    } else if unit.members.iter().any(|member| member.present) {
        ContentUnitAvailability::Incomplete
    } else {
        ContentUnitAvailability::Missing
    }
}

async fn insert_issue(
    transaction: &mut Transaction<'_, Sqlite>,
    run_id: ScanRunId,
    root_id: ContentRootId,
    issue: &ScanIssue,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO scan_issues \
         (scan_run_id, root_id, kind, relative_path, related_path, detail, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(run_id.0)
    .bind(issue.root_id.map(|id| id.0).or(Some(root_id.0)))
    .bind(issue.kind.as_db())
    .bind(&issue.relative_path)
    .bind(&issue.related_path)
    .bind(&issue.detail)
    .bind(issue.created_at)
    .execute(&mut **transaction)
    .await
    .map_err(AppError::Database)?;
    Ok(())
}

fn content_root_from_row(row: sqlx::sqlite::SqliteRow) -> Result<ContentRoot, AppError> {
    Ok(ContentRoot {
        id: ContentRootId(row.get("id")),
        path: row.get("path"),
        kind: root_kind(&row.get::<String, _>("kind"))?,
        enabled: row.get("enabled"),
        system_hint: row
            .try_get::<Option<String>, _>("system_hint")
            .map_err(AppError::Database)?
            .as_deref()
            .map(system_id)
            .transpose()?,
        availability: root_availability(&row.get::<String, _>("availability"))?,
        last_scan_at: row.get("last_scan_at"),
        last_successful_scan_at: row.get("last_successful_scan_at"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

fn library_list_item_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<LibraryListItem, AppError> {
    let game_id = GameId(row.get("game_id"));
    let local_title: String = row.get("local_title");
    let metadata_title = non_empty(row.get("metadata_title"));
    let display_title = metadata_title
        .clone()
        .unwrap_or_else(|| local_title.clone());
    let match_state = library_metadata_match_state(row.get("metadata_status"))?;

    Ok(LibraryListItem {
        game_id,
        system_id: system_id(&row.get::<String, _>("system_id"))?,
        local_title,
        metadata_title,
        display_title,
        sort_title: row.get("effective_sort_title"),
        availability: game_availability(&row.get::<String, _>("availability"))?,
        favorite: row.get::<i64, _>("favorite") != 0,
        metadata_match_state: match_state,
        release_date: non_empty(row.get("release_date")),
        genre: non_empty(row.get("genre")),
        region: non_empty(row.get("region")),
        cover_ref: (row.get::<i64, _>("cover_cached") != 0)
            .then(|| cached_cover_reference(game_id)),
    })
}

fn library_metadata_match_state(
    status: Option<String>,
) -> Result<LibraryMetadataMatchState, AppError> {
    match status.as_deref() {
        // M5 uses `pending` for the absence of an accepted provider match, including work that
        // has not been requested yet. Keep the list/detail projections on that same state.
        None => Ok(LibraryMetadataMatchState::Pending),
        Some("pending") => Ok(LibraryMetadataMatchState::Pending),
        Some("matched") => Ok(LibraryMetadataMatchState::Matched),
        Some("no_match") => Ok(LibraryMetadataMatchState::NoMatch),
        Some("ambiguous") => Ok(LibraryMetadataMatchState::Ambiguous),
        Some("deferred") => Ok(LibraryMetadataMatchState::Deferred),
        Some("failed") => Ok(LibraryMetadataMatchState::Failed),
        Some("stale") => Ok(LibraryMetadataMatchState::Stale),
        Some(value) => Err(AppError::Library(format!(
            "invalid provider match status in database: {value}"
        ))),
    }
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.trim().is_empty())
}

fn normalized_filter(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn normalized_search_filter(value: Option<&str>) -> Option<String> {
    normalized_filter(value).map(|value| {
        value
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    })
}

fn bounded_scan_issue_limit(requested: u32) -> u32 {
    if requested == 0 {
        DEFAULT_SCAN_ISSUE_PAGE_SIZE
    } else {
        requested.min(MAX_SCAN_ISSUE_PAGE_SIZE)
    }
}

fn sqlite_offset(value: u64) -> Result<i64, AppError> {
    i64::try_from(value)
        .map_err(|_| AppError::Library("the requested page offset is too large".to_owned()))
}

fn game_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<Game, AppError> {
    Ok(Game {
        id: GameId(row.get("id")),
        system_id: system_id(&row.get::<String, _>("system_id"))?,
        local_title: row.get("local_title"),
        availability: game_availability(&row.get::<String, _>("availability"))?,
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

fn unit_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<ContentUnit, AppError> {
    Ok(ContentUnit {
        id: ContentUnitId(row.get("id")),
        game_id: GameId(row.get("game_id")),
        root_id: ContentRootId(row.get("root_id")),
        system_id: system_id(&row.get::<String, _>("system_id"))?,
        kind: unit_kind(&row.get::<String, _>("kind"))?,
        local_title: row.get("local_title"),
        primary_relative_path: row.get("primary_relative_path"),
        fingerprint: row.get("fingerprint"),
        availability: unit_availability_from_db(&row.get::<String, _>("availability"))?,
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        files: Vec::new(),
    })
}

fn membership_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<ContentFileMembership, AppError> {
    Ok(ContentFileMembership {
        ordinal: row.get("ordinal"),
        role: file_role(&row.get::<String, _>("role"))?,
        file: ContentFile {
            id: ContentFileId(row.get("id")),
            root_id: ContentRootId(row.get("root_id")),
            relative_path: row.get("relative_path"),
            size_bytes: u64_value(row.get("size_bytes"))?,
            modified_at: row.get("modified_at"),
            crc32: row.get("crc32"),
            md5: row.get("md5"),
            sha1: row.get("sha1"),
            availability: file_availability(&row.get::<String, _>("availability"))?,
        },
    })
}

fn scan_issue_from_row(row: sqlx::sqlite::SqliteRow) -> Result<ScanIssue, AppError> {
    Ok(ScanIssue {
        id: Some(ScanIssueId(row.get("id"))),
        scan_run_id: Some(ScanRunId(row.get("scan_run_id"))),
        root_id: row.get::<Option<i64>, _>("root_id").map(ContentRootId),
        kind: issue_kind(&row.get::<String, _>("kind"))?,
        relative_path: row.get("relative_path"),
        related_path: row.get("related_path"),
        detail: row.get("detail"),
        created_at: row.get("created_at"),
    })
}

fn root_kind(value: &str) -> Result<ContentRootKind, AppError> {
    ContentRootKind::from_db(value)
        .ok_or_else(|| AppError::Library(format!("invalid content-root kind in database: {value}")))
}

fn root_availability(value: &str) -> Result<ContentRootAvailability, AppError> {
    ContentRootAvailability::from_db(value).ok_or_else(|| {
        AppError::Library(format!(
            "invalid content-root availability in database: {value}"
        ))
    })
}

fn game_availability(value: &str) -> Result<GameAvailability, AppError> {
    GameAvailability::from_db(value)
        .ok_or_else(|| AppError::Library(format!("invalid game availability in database: {value}")))
}

fn unit_availability_from_db(value: &str) -> Result<ContentUnitAvailability, AppError> {
    ContentUnitAvailability::from_db(value)
        .ok_or_else(|| AppError::Library(format!("invalid content-unit availability: {value}")))
}

fn file_availability(value: &str) -> Result<ContentFileAvailability, AppError> {
    ContentFileAvailability::from_db(value)
        .ok_or_else(|| AppError::Library(format!("invalid content-file availability: {value}")))
}

fn unit_kind(value: &str) -> Result<ContentUnitKind, AppError> {
    ContentUnitKind::from_db(value)
        .ok_or_else(|| AppError::Library(format!("invalid content-unit kind: {value}")))
}

fn file_role(value: &str) -> Result<ContentFileRole, AppError> {
    ContentFileRole::from_db(value)
        .ok_or_else(|| AppError::Library(format!("invalid content-file role: {value}")))
}

fn issue_kind(value: &str) -> Result<ScanIssueKind, AppError> {
    ScanIssueKind::from_db(value)
        .ok_or_else(|| AppError::Library(format!("invalid scan issue kind: {value}")))
}

fn system_id(value: &str) -> Result<SystemId, AppError> {
    SystemId::from_str(value)
        .ok_or_else(|| AppError::Library(format!("invalid system identifier in database: {value}")))
}

fn scan_state(value: &str) -> Result<ScanRunState, AppError> {
    ScanRunState::from_db(value)
        .ok_or_else(|| AppError::Library(format!("invalid scan-run state in database: {value}")))
}

fn scan_counters_from_row(row: &sqlx::sqlite::SqliteRow) -> ScanCounters {
    ScanCounters {
        roots_discovered: row.get::<i64, _>("roots_discovered").max(0) as u64,
        roots_completed: row.get::<i64, _>("roots_completed").max(0) as u64,
        files_discovered: row.get::<i64, _>("files_discovered").max(0) as u64,
        files_processed: row.get::<i64, _>("files_processed").max(0) as u64,
        files_hashed: row.get::<i64, _>("files_hashed").max(0) as u64,
        bytes_hashed: row.get::<i64, _>("bytes_hashed").max(0) as u64,
        issues_found: row.get::<i64, _>("issues_found").max(0) as u64,
    }
}

fn sqlite_size(value: u64) -> Result<i64, AppError> {
    i64::try_from(value)
        .map_err(|_| AppError::Library("file is too large for SQLite integer storage".to_owned()))
}

fn u64_value(value: i64) -> Result<u64, AppError> {
    u64::try_from(value)
        .map_err(|_| AppError::Library("database contains a negative file size".to_owned()))
}

fn i64_counter(value: u64) -> i64 {
    value.min(i64::MAX as u64) as i64
}

fn now_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or_default()
}

fn duration_ms(started_at: i64, completed_at: Option<i64>) -> u64 {
    completed_at
        .unwrap_or_else(now_timestamp)
        .saturating_sub(started_at)
        .max(0) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::database::Database;
    use crate::domain::library::{
        DEFAULT_LIBRARY_PAGE_SIZE, DEFAULT_LIBRARY_SHELF_PREVIEW, MAX_LIBRARY_PAGE_SIZE,
        MAX_LIBRARY_SHELF_PREVIEW,
    };
    use crate::domain::metadata::MetadataProviderId;
    use crate::domain::system::SystemId;
    use sqlx::SqlitePool;
    use tempfile::TempDir;

    const TEST_TIME: i64 = 1_700_000_000_000;

    async fn fixture() -> (TempDir, SqlitePool) {
        let directory = tempfile::tempdir().expect("temporary directory");
        let database = Database::open(directory.path().join("library.sqlite3"))
            .await
            .expect("database should open");
        (directory, database.pool().clone())
    }

    async fn insert_root(pool: &SqlitePool) {
        sqlx::query(
            "INSERT INTO content_roots \
             (id, path, kind, enabled, availability, created_at, updated_at) \
             VALUES (1, '/synthetic/library', 'managed', 1, 'available', ?, ?)",
        )
        .bind(TEST_TIME)
        .bind(TEST_TIME)
        .execute(pool)
        .await
        .expect("synthetic content root");
    }

    async fn insert_game(
        pool: &SqlitePool,
        game_id: i64,
        system: SystemId,
        title: &str,
        availability: GameAvailability,
    ) {
        sqlx::query(
            "INSERT INTO games \
             (id, system_id, local_title, availability, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(game_id)
        .bind(system.as_str())
        .bind(title)
        .bind(availability.as_db())
        .bind(TEST_TIME)
        .bind(TEST_TIME)
        .execute(pool)
        .await
        .expect("synthetic game");
    }

    async fn insert_metadata(
        pool: &SqlitePool,
        game_id: i64,
        title: &str,
        sort_title: Option<&str>,
        genre: Option<&str>,
        region: Option<&str>,
    ) {
        sqlx::query(
            "INSERT INTO provider_metadata \
             (game_id, provider_id, provider_game_id, title, sort_title, synopsis, release_date, \
              developer, publisher, genre, players, region, source_credit, fetched_at, created_at, updated_at) \
             VALUES (?, 'screenscraper', ?, ?, ?, NULL, ?, NULL, NULL, ?, NULL, ?, NULL, ?, ?, ?)",
        )
        .bind(game_id)
        .bind(format!("provider-{game_id}"))
        .bind(title)
        .bind(sort_title)
        .bind("1990-01-01")
        .bind(genre)
        .bind(region)
        .bind(TEST_TIME)
        .bind(TEST_TIME)
        .bind(TEST_TIME)
        .execute(pool)
        .await
        .expect("synthetic normalized metadata");
    }

    async fn insert_match(pool: &SqlitePool, game_id: i64, status: &str) {
        let matched = status == "matched";
        sqlx::query(
            "INSERT INTO provider_matches \
             (game_id, provider_id, status, match_type, provider_game_id, created_at, updated_at) \
             VALUES (?, 'screenscraper', ?, ?, ?, ?, ?)",
        )
        .bind(game_id)
        .bind(status)
        .bind(matched.then_some("deterministic_sha1"))
        .bind(matched.then(|| format!("provider-{game_id}")))
        .bind(TEST_TIME)
        .bind(TEST_TIME)
        .execute(pool)
        .await
        .expect("synthetic provider match state");
    }

    async fn insert_cached_cover(pool: &SqlitePool, game_id: i64) {
        sqlx::query(
            "INSERT INTO provider_media_assets \
             (game_id, provider_id, kind, state, cache_relative_path, content_type, created_at, updated_at) \
             VALUES (?, 'screenscraper', 'cover', 'cached', ?, 'image/png', ?, ?)",
        )
        .bind(game_id)
        .bind(format!("covers/screenscraper/{game_id}.png"))
        .bind(TEST_TIME)
        .bind(TEST_TIME)
        .execute(pool)
        .await
        .expect("synthetic cached cover row");
    }

    fn ids(page: &LibraryPage) -> Vec<i64> {
        page.items.iter().map(|item| item.game_id.0).collect()
    }

    fn assert_no_physical_identity(serialized: &str) {
        let serialized = serialized.to_ascii_lowercase();
        for field in ["crc32", "md5", "sha1", "fingerprint", "contentfiles"] {
            assert!(
                !serialized.contains(field),
                "UI projection leaked physical identity field {field}: {serialized}"
            );
        }
    }

    #[tokio::test]
    async fn empty_library_returns_bounded_page_and_summary() {
        let (_directory, pool) = fixture().await;
        let repository = LibraryRepository::new(pool);

        let page = repository
            .query_library(&LibraryQuery::default(), MetadataProviderId::ScreenScraper)
            .await
            .expect("empty library query");
        assert!(page.items.is_empty());
        assert_eq!(page.total, 0);
        assert_eq!(page.offset, 0);
        assert_eq!(page.limit, DEFAULT_LIBRARY_PAGE_SIZE);

        let summary = repository
            .get_library_summary()
            .await
            .expect("empty library summary");
        assert_eq!(summary.total_games, 0);
        assert_eq!(summary.favorite_games, 0);
        assert!(summary.systems.is_empty());
        assert!(repository
            .get_library_game_detail(GameId(1))
            .await
            .expect("missing game detail")
            .is_none());
        let issues = repository
            .list_latest_scan_issues_page(0, 0)
            .await
            .expect("empty scan issue page");
        assert_eq!(issues.scan_run_id, None);
        assert_eq!(issues.total, 0);
        assert!(issues.issues.is_empty());
    }

    #[tokio::test]
    async fn the_review_filter_returns_only_games_with_a_candidate_choice_to_make() {
        let (_directory, pool) = fixture().await;
        let repository = LibraryRepository::new(pool.clone());
        for (game_id, title, status) in [
            (1, "Alpha", "matched"),
            (2, "Beta", "ambiguous"),
            (3, "Gamma", "no_match"),
            (4, "Delta", "failed"),
            (5, "Epsilon", "stale"),
            (6, "Zeta", "ambiguous"),
        ] {
            insert_game(
                &pool,
                game_id,
                SystemId::Snes,
                title,
                GameAvailability::Available,
            )
            .await;
            insert_match(&pool, game_id, status).await;
        }
        // A game with no provider relationship at all is not awaiting review either.
        insert_game(&pool, 7, SystemId::Snes, "Eta", GameAvailability::Available).await;

        let review = repository
            .query_library(
                &LibraryQuery {
                    needs_metadata_review: true,
                    ..LibraryQuery::default()
                },
                MetadataProviderId::ScreenScraper,
            )
            .await
            .unwrap();

        assert_eq!(ids(&review), vec![2, 6]);
        assert_eq!(review.total, 2, "the bounded total must respect the filter");
        for item in &review.items {
            assert_eq!(
                item.metadata_match_state,
                LibraryMetadataMatchState::Ambiguous
            );
        }

        // The filter composes with the existing ones rather than replacing them.
        let narrowed = repository
            .query_library(
                &LibraryQuery {
                    needs_metadata_review: true,
                    search: Some("Zeta".to_owned()),
                    ..LibraryQuery::default()
                },
                MetadataProviderId::ScreenScraper,
            )
            .await
            .unwrap();
        assert_eq!(ids(&narrowed), vec![6]);

        // Off by default: an ordinary library read is unchanged.
        let everything = repository
            .query_library(&LibraryQuery::default(), MetadataProviderId::ScreenScraper)
            .await
            .unwrap();
        assert_eq!(everything.total, 7);
    }

    #[tokio::test]
    async fn library_query_supports_bounded_pages_search_filters_and_stable_title_order() {
        let (_directory, pool) = fixture().await;
        let repository = LibraryRepository::new(pool.clone());
        insert_game(
            &pool,
            1,
            SystemId::Snes,
            "Local Alpha",
            GameAvailability::Available,
        )
        .await;
        insert_metadata(
            &pool,
            1,
            "Zeta Metadata",
            Some("Zeta Metadata"),
            Some("RPG"),
            Some("US"),
        )
        .await;
        insert_match(&pool, 1, "matched").await;
        insert_game(
            &pool,
            2,
            SystemId::Snes,
            "Beta Local",
            GameAvailability::Available,
        )
        .await;
        insert_metadata(
            &pool,
            2,
            "Alpha Metadata",
            Some("Alpha Metadata"),
            Some("Action"),
            Some("EU"),
        )
        .await;
        insert_match(&pool, 2, "stale").await;
        insert_game(
            &pool,
            3,
            SystemId::Nes,
            "Gamma Local",
            GameAvailability::Unavailable,
        )
        .await;
        insert_match(&pool, 3, "no_match").await;
        insert_game(
            &pool,
            4,
            SystemId::Snes,
            "Fallback Local",
            GameAvailability::Available,
        )
        .await;
        insert_match(&pool, 4, "ambiguous").await;
        insert_game(
            &pool,
            5,
            SystemId::Snes,
            "Same Title",
            GameAvailability::Available,
        )
        .await;
        insert_match(&pool, 5, "pending").await;
        insert_game(
            &pool,
            6,
            SystemId::Snes,
            "same title",
            GameAvailability::Available,
        )
        .await;

        let all = repository
            .query_library(&LibraryQuery::default(), MetadataProviderId::ScreenScraper)
            .await
            .expect("library page");
        assert_eq!(all.total, 6);
        assert_eq!(ids(&all), vec![2, 4, 3, 5, 6, 1]);
        assert_eq!(all.items[0].display_title, "Alpha Metadata");
        assert_eq!(all.items[1].display_title, "Fallback Local");
        assert_eq!(
            all.items
                .iter()
                .find(|item| item.game_id == GameId(3))
                .unwrap()
                .metadata_match_state,
            LibraryMetadataMatchState::NoMatch
        );
        assert_eq!(
            all.items
                .iter()
                .find(|item| item.game_id == GameId(1))
                .unwrap()
                .metadata_match_state,
            LibraryMetadataMatchState::Matched
        );
        assert_eq!(
            all.items
                .iter()
                .find(|item| item.game_id == GameId(2))
                .unwrap()
                .metadata_match_state,
            LibraryMetadataMatchState::Stale
        );
        assert_eq!(
            all.items
                .iter()
                .find(|item| item.game_id == GameId(4))
                .unwrap()
                .metadata_match_state,
            LibraryMetadataMatchState::Ambiguous
        );
        assert_eq!(
            all.items
                .iter()
                .find(|item| item.game_id == GameId(5))
                .unwrap()
                .metadata_match_state,
            LibraryMetadataMatchState::Pending
        );
        assert_eq!(
            all.items
                .iter()
                .find(|item| item.game_id == GameId(6))
                .unwrap()
                .metadata_match_state,
            LibraryMetadataMatchState::Pending,
            "a game without a provider row uses the same pending state as queued work"
        );

        let mut request = LibraryQuery {
            search: Some(" metadata ".to_owned()),
            ..LibraryQuery::default()
        };
        assert_eq!(
            ids(&repository
                .query_library(&request, MetadataProviderId::ScreenScraper)
                .await
                .unwrap()),
            vec![2, 1]
        );
        request.search = Some("fallback".to_owned());
        assert_eq!(
            ids(&repository
                .query_library(&request, MetadataProviderId::ScreenScraper)
                .await
                .unwrap()),
            vec![4]
        );
        request.search = Some("   ".to_owned());
        assert_eq!(
            repository
                .query_library(&request, MetadataProviderId::ScreenScraper)
                .await
                .unwrap()
                .total,
            6,
            "whitespace-only search is the empty search"
        );

        request = LibraryQuery {
            system_id: Some(SystemId::Nes),
            ..LibraryQuery::default()
        };
        assert_eq!(
            ids(&repository
                .query_library(&request, MetadataProviderId::ScreenScraper)
                .await
                .unwrap()),
            vec![3]
        );
        request = LibraryQuery {
            availability: Some(GameAvailability::Unavailable),
            ..LibraryQuery::default()
        };
        assert_eq!(
            ids(&repository
                .query_library(&request, MetadataProviderId::ScreenScraper)
                .await
                .unwrap()),
            vec![3]
        );
        request = LibraryQuery {
            genre: Some("aCtIoN".to_owned()),
            ..LibraryQuery::default()
        };
        assert_eq!(
            ids(&repository
                .query_library(&request, MetadataProviderId::ScreenScraper)
                .await
                .unwrap()),
            vec![2]
        );
        request = LibraryQuery {
            region: Some("us".to_owned()),
            ..LibraryQuery::default()
        };
        assert_eq!(
            ids(&repository
                .query_library(&request, MetadataProviderId::ScreenScraper)
                .await
                .unwrap()),
            vec![1]
        );

        request = LibraryQuery {
            limit: 2,
            offset: 2,
            ..LibraryQuery::default()
        };
        let page = repository
            .query_library(&request, MetadataProviderId::ScreenScraper)
            .await
            .unwrap();
        assert_eq!(page.total, 6);
        assert_eq!(page.limit, 2);
        assert_eq!(ids(&page), vec![3, 5]);

        request.limit = u32::MAX;
        let capped = repository
            .query_library(&request, MetadataProviderId::ScreenScraper)
            .await
            .unwrap();
        assert_eq!(capped.limit, MAX_LIBRARY_PAGE_SIZE);
        assert!(capped.items.len() <= MAX_LIBRARY_PAGE_SIZE as usize);

        request.offset = u64::MAX;
        assert!(matches!(
            repository
                .query_library(&request, MetadataProviderId::ScreenScraper)
                .await,
            Err(AppError::Library(_))
        ));
    }

    #[tokio::test]
    async fn library_search_treats_like_metacharacters_as_literal_text() {
        let (_directory, pool) = fixture().await;
        let repository = LibraryRepository::new(pool.clone());
        for (game_id, title) in [
            (1, "Literal % Title"),
            (2, "Literal _ Title"),
            (3, "abc"),
            (4, "aXc"),
            (5, "a_c"),
            (6, r"slash\title"),
        ] {
            insert_game(
                &pool,
                game_id,
                SystemId::Nes,
                title,
                GameAvailability::Available,
            )
            .await;
        }

        let mut request = LibraryQuery {
            search: Some("%".to_owned()),
            ..LibraryQuery::default()
        };
        let percent = repository
            .query_library(&request, MetadataProviderId::ScreenScraper)
            .await
            .unwrap();
        assert_eq!(percent.total, 1);
        assert_eq!(ids(&percent), vec![1]);

        request.search = Some("_".to_owned());
        let underscore = repository
            .query_library(&request, MetadataProviderId::ScreenScraper)
            .await
            .unwrap();
        assert_eq!(underscore.total, 2);
        assert_eq!(ids(&underscore), vec![5, 2]);

        request.search = Some("a_c".to_owned());
        assert_eq!(
            ids(&repository
                .query_library(&request, MetadataProviderId::ScreenScraper)
                .await
                .unwrap()),
            vec![5],
            "an underscore in the query must not match an arbitrary character"
        );

        request.search = Some(r"slash\title".to_owned());
        assert_eq!(
            ids(&repository
                .query_library(&request, MetadataProviderId::ScreenScraper)
                .await
                .unwrap()),
            vec![6],
            "a backslash in the query must remain literal"
        );

        request.search = Some("abc".to_owned());
        assert_eq!(
            ids(&repository
                .query_library(&request, MetadataProviderId::ScreenScraper)
                .await
                .unwrap()),
            vec![3],
            "ordinary search text retains its existing semantics"
        );
    }

    #[tokio::test]
    async fn favorites_are_durable_filtered_and_composed_with_cached_cover_state() {
        let (_directory, pool) = fixture().await;
        let repository = LibraryRepository::new(pool.clone());
        insert_game(
            &pool,
            1,
            SystemId::Nes,
            "Favorite Candidate",
            GameAvailability::Available,
        )
        .await;
        insert_cached_cover(&pool, 1).await;

        let default_item = repository
            .query_library(&LibraryQuery::default(), MetadataProviderId::ScreenScraper)
            .await
            .unwrap()
            .items
            .pop()
            .unwrap();
        assert!(!default_item.favorite);
        let expected_cover_ref = cached_cover_reference(GameId(1));
        assert_eq!(
            default_item.cover_ref.as_deref(),
            Some(expected_cover_ref.as_str())
        );

        repository.set_game_favorite(GameId(1), true).await.unwrap();
        let favorites = repository
            .query_library(
                &LibraryQuery {
                    favorites_only: true,
                    ..LibraryQuery::default()
                },
                MetadataProviderId::ScreenScraper,
            )
            .await
            .unwrap();
        assert_eq!(ids(&favorites), vec![1]);
        assert!(favorites.items[0].favorite);
        assert!(
            repository
                .get_library_game_detail(GameId(1))
                .await
                .unwrap()
                .unwrap()
                .favorite
        );
        assert_eq!(
            repository
                .get_library_summary()
                .await
                .unwrap()
                .favorite_games,
            1
        );

        repository
            .set_game_favorite(GameId(1), false)
            .await
            .unwrap();
        let no_favorites = repository
            .query_library(
                &LibraryQuery {
                    favorites_only: true,
                    ..LibraryQuery::default()
                },
                MetadataProviderId::ScreenScraper,
            )
            .await
            .unwrap();
        assert!(no_favorites.items.is_empty());
        let stored: i64 =
            sqlx::query_scalar("SELECT favorite FROM game_user_state WHERE game_id = 1")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(stored, 0, "clearing a favorite remains durable state");

        assert!(
            sqlx::query("UPDATE game_user_state SET favorite = 2 WHERE game_id = 1")
                .execute(&pool)
                .await
                .is_err()
        );
        assert!(
            sqlx::query("DELETE FROM games WHERE id = 1")
                .execute(&pool)
                .await
                .is_err(),
            "user state intentionally restricts game deletion"
        );
    }

    #[tokio::test]
    async fn game_detail_is_bounded_and_never_serializes_physical_hashes() {
        let (_directory, pool) = fixture().await;
        let repository = LibraryRepository::new(pool.clone());
        insert_root(&pool).await;
        insert_game(
            &pool,
            1,
            SystemId::Snes,
            "Disc Summary",
            GameAvailability::Available,
        )
        .await;
        sqlx::query(
            "INSERT INTO content_units \
             (id, game_id, root_id, system_id, kind, local_title, primary_relative_path, \
              fingerprint, availability, created_at, updated_at) \
             VALUES (1, 1, 1, 'snes', 'cue_bin', 'Disc Summary', 'disc/game.cue', \
                     'unit-fingerprint-secret', 'available', ?, ?)",
        )
        .bind(TEST_TIME)
        .bind(TEST_TIME)
        .execute(&pool)
        .await
        .unwrap();
        for (file_id, path, role) in [
            (1_i64, "disc/game.cue", "descriptor"),
            (2, "disc/game.bin", "track"),
        ] {
            sqlx::query(
                "INSERT INTO content_files \
                 (id, root_id, relative_path, size_bytes, modified_at, crc32, md5, sha1, \
                  availability, created_at, updated_at) \
                 VALUES (?, 1, ?, 10, ?, 'AABBCCDD', 'd41d8cd98f00b204e9800998ecf8427e', \
                         'da39a3ee5e6b4b0d3255bfef95601890afd80709', 'available', ?, ?)",
            )
            .bind(file_id)
            .bind(path)
            .bind(TEST_TIME)
            .bind(TEST_TIME)
            .bind(TEST_TIME)
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO content_unit_files \
                 (content_unit_id, content_file_id, ordinal, role) VALUES (1, ?, ?, ?)",
            )
            .bind(file_id)
            .bind(file_id - 1)
            .bind(role)
            .execute(&pool)
            .await
            .unwrap();
        }

        let detail = repository
            .get_library_game_detail(GameId(1))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(detail.content_units.len(), 1);
        assert_eq!(detail.content_units[0].file_count, 2);
        assert_eq!(
            detail.content_units[0].primary_relative_path,
            "disc/game.cue"
        );
        let detail_json = serde_json::to_string(&detail).unwrap();
        assert_no_physical_identity(&detail_json);
        assert!(!detail_json.contains("unit-fingerprint-secret"));
        assert!(!detail_json.contains("AABBCCDD"));

        let page = repository
            .query_library(&LibraryQuery::default(), MetadataProviderId::ScreenScraper)
            .await
            .unwrap();
        assert_no_physical_identity(&serde_json::to_string(&page).unwrap());
    }

    #[tokio::test]
    async fn large_synthetic_library_queries_return_only_the_bounded_page() {
        let (_directory, pool) = fixture().await;
        let repository = LibraryRepository::new(pool.clone());
        let mut transaction = pool.begin().await.unwrap();
        for game_id in 1_i64..=500 {
            sqlx::query(
                "INSERT INTO games \
                 (id, system_id, local_title, availability, created_at, updated_at) \
                 VALUES (?, 'nes', ?, 'available', ?, ?)",
            )
            .bind(game_id)
            .bind(format!("Synthetic Game {game_id:04}"))
            .bind(TEST_TIME)
            .bind(TEST_TIME)
            .execute(&mut *transaction)
            .await
            .unwrap();
        }
        transaction.commit().await.unwrap();

        let capped = repository
            .query_library(
                &LibraryQuery {
                    limit: u32::MAX,
                    ..LibraryQuery::default()
                },
                MetadataProviderId::ScreenScraper,
            )
            .await
            .unwrap();
        assert_eq!(capped.total, 500);
        assert_eq!(capped.items.len(), MAX_LIBRARY_PAGE_SIZE as usize);
        assert_eq!(capped.items.first().unwrap().game_id, GameId(1));
        assert_eq!(capped.items.last().unwrap().game_id, GameId(60));

        let tail = repository
            .query_library(
                &LibraryQuery {
                    offset: 480,
                    limit: 60,
                    ..LibraryQuery::default()
                },
                MetadataProviderId::ScreenScraper,
            )
            .await
            .unwrap();
        assert_eq!(tail.total, 500);
        assert_eq!(tail.items.len(), 20);
        assert_eq!(tail.items.first().unwrap().game_id, GameId(481));
        assert_eq!(tail.items.last().unwrap().game_id, GameId(500));
    }

    /// The shelf query and the grid query must agree about *which* games match. Rather than
    /// restating the expected set by hand for each filter, this asks both surfaces the same
    /// question and compares them, which is the invariant that actually matters.
    async fn assert_shelves_agree_with_grid(
        repository: &LibraryRepository,
        shelf_query: &LibraryShelfQuery,
    ) -> LibraryShelves {
        let shelves = repository
            .query_library_shelves(shelf_query, MetadataProviderId::ScreenScraper)
            .await
            .expect("shelf projection");

        let grid = repository
            .query_library(
                &LibraryQuery {
                    limit: MAX_LIBRARY_PAGE_SIZE,
                    ..shelf_query.as_library_query()
                },
                MetadataProviderId::ScreenScraper,
            )
            .await
            .expect("grid page");

        // Totals: every shelf total summed is the grid's total for the same filters.
        let shelf_total: u64 = shelves.shelves.iter().map(|shelf| shelf.total).sum();
        assert_eq!(
            shelf_total, grid.total,
            "shelves and the grid disagree about how many games match"
        );

        for shelf in &shelves.shelves {
            // Order: each shelf's preview is the grid's order restricted to that system.
            let expected: Vec<i64> = grid
                .items
                .iter()
                .filter(|item| item.system_id == shelf.system_id)
                .map(|item| item.game_id.0)
                .take(shelf.items.len())
                .collect();
            let actual: Vec<i64> = shelf.items.iter().map(|item| item.game_id.0).collect();
            assert_eq!(
                actual, expected,
                "shelf {:?} does not follow the Library's own title order",
                shelf.system_id
            );

            assert!(
                !shelf.items.is_empty(),
                "a system with no match must have no shelf at all"
            );
            assert!(
                shelf.items.len() as u64 <= shelf.total,
                "a preview cannot hold more games than the system matches"
            );
        }

        shelves
    }

    #[tokio::test]
    async fn shelves_preview_each_system_in_the_librarys_own_order_and_omit_empty_systems() {
        let (_directory, pool) = fixture().await;
        let repository = LibraryRepository::new(pool.clone());
        insert_root(&pool).await;

        // Two systems with content, one authoritative system with none.
        for (game_id, title) in [(1_i64, "Zeta"), (2, "Alpha"), (3, "Mid")] {
            insert_game(
                &pool,
                game_id,
                SystemId::Snes,
                title,
                GameAvailability::Available,
            )
            .await;
        }
        insert_game(
            &pool,
            4,
            SystemId::Nes,
            "Kirby",
            GameAvailability::Available,
        )
        .await;

        let shelves = assert_shelves_agree_with_grid(&repository, &LibraryShelfQuery::default())
            .await
            .shelves;

        assert_eq!(shelves.len(), 2, "only systems with a match get a shelf");
        let snes = shelves
            .iter()
            .find(|shelf| shelf.system_id == SystemId::Snes)
            .expect("SNES shelf");
        assert_eq!(snes.total, 3);
        assert_eq!(
            snes.items
                .iter()
                .map(|item| item.display_title.as_str())
                .collect::<Vec<_>>(),
            vec!["Alpha", "Mid", "Zeta"]
        );
        let nes = shelves
            .iter()
            .find(|shelf| shelf.system_id == SystemId::Nes)
            .expect("NES shelf");
        assert_eq!(nes.total, 1);
        assert!(
            !shelves
                .iter()
                .any(|shelf| shelf.system_id == SystemId::NintendoGameCube),
            "a system with no games must not appear as an empty heading"
        );
    }

    #[tokio::test]
    async fn a_shelf_preview_is_bounded_while_its_total_counts_every_match() {
        let (_directory, pool) = fixture().await;
        let repository = LibraryRepository::new(pool.clone());
        insert_root(&pool).await;
        for game_id in 1_i64..=84 {
            insert_game(
                &pool,
                game_id,
                SystemId::Snes,
                &format!("SNES Game {game_id:04}"),
                GameAvailability::Available,
            )
            .await;
        }

        let shelves = assert_shelves_agree_with_grid(&repository, &LibraryShelfQuery::default())
            .await
            .shelves;
        assert_eq!(shelves.len(), 1);
        assert_eq!(shelves[0].total, 84, "the total is the whole system");
        assert_eq!(
            shelves[0].items.len(),
            DEFAULT_LIBRARY_SHELF_PREVIEW as usize,
            "the preview stays bounded"
        );
        assert_eq!(shelves[0].items[0].display_title, "SNES Game 0001");

        // A caller cannot widen the preview past the backend ceiling.
        let capped = repository
            .query_library_shelves(
                &LibraryShelfQuery {
                    preview_limit: u32::MAX,
                    ..LibraryShelfQuery::default()
                },
                MetadataProviderId::ScreenScraper,
            )
            .await
            .expect("shelf projection");
        assert_eq!(
            capped.shelves[0].items.len(),
            MAX_LIBRARY_SHELF_PREVIEW as usize
        );
        assert_eq!(capped.shelves[0].total, 84);
    }

    #[tokio::test]
    async fn shelf_search_favorites_and_review_semantics_match_the_grid_exactly() {
        let (_directory, pool) = fixture().await;
        let repository = LibraryRepository::new(pool.clone());
        insert_root(&pool).await;

        // Deliberately mixed: metadata titles, local-title-only games, favorites, and every
        // provider match state the review filter has to discriminate between.
        let games = [
            (1_i64, SystemId::Snes, "Super Mario World", "matched"),
            (2, SystemId::Snes, "F-Zero", "ambiguous"),
            (3, SystemId::Nes, "Super Mario Bros.", "ambiguous"),
            (4, SystemId::Nes, "Metroid", "no_match"),
            (5, SystemId::Nintendo64, "Super Mario 64", "matched"),
            (6, SystemId::Nintendo64, "GoldenEye 007", "failed"),
            (7, SystemId::NintendoGameCube, "Mario Kart", "pending"),
        ];
        for (game_id, system, title, status) in games {
            insert_game(&pool, game_id, system, title, GameAvailability::Available).await;
            insert_match(&pool, game_id, status).await;
        }
        // Metadata titles must be searched as well as local titles, exactly as the grid does.
        insert_metadata(&pool, 6, "GoldenEye 007 (Mario Cameo)", None, None, None).await;
        for favorite in [1_i64, 3, 5] {
            sqlx::query(
                "INSERT INTO game_user_state (game_id, favorite, created_at, updated_at) \
                 VALUES (?, 1, ?, ?)",
            )
            .bind(favorite)
            .bind(TEST_TIME)
            .bind(TEST_TIME)
            .execute(&pool)
            .await
            .expect("synthetic favorite");
        }

        let search = assert_shelves_agree_with_grid(
            &repository,
            &LibraryShelfQuery {
                search: Some("mario".to_owned()),
                ..LibraryShelfQuery::default()
            },
        )
        .await;
        assert_eq!(
            search.shelves.len(),
            4,
            "systems with no match disappear from a searched shelf view"
        );

        assert_shelves_agree_with_grid(
            &repository,
            &LibraryShelfQuery {
                favorites_only: true,
                ..LibraryShelfQuery::default()
            },
        )
        .await;

        // M8.5's review filter is one narrow ambiguous-only flag; it must not widen here.
        let review = assert_shelves_agree_with_grid(
            &repository,
            &LibraryShelfQuery {
                needs_metadata_review: true,
                ..LibraryShelfQuery::default()
            },
        )
        .await;
        assert_eq!(
            review
                .shelves
                .iter()
                .map(|shelf| (shelf.system_id, shelf.total))
                .collect::<Vec<_>>(),
            vec![(SystemId::Nes, 1), (SystemId::Snes, 1)]
        );

        let combined = assert_shelves_agree_with_grid(
            &repository,
            &LibraryShelfQuery {
                search: Some("mario".to_owned()),
                favorites_only: true,
                ..LibraryShelfQuery::default()
            },
        )
        .await;
        // Grouped by system, so this is the grid's matching set re-partitioned rather than its
        // flat order. The backend orders shelves by system identity for determinism; the catalog's
        // own presentation order is applied by the frontend, which is where it is authoritative.
        assert_eq!(
            combined
                .shelves
                .iter()
                .map(|shelf| (
                    shelf.system_id,
                    shelf.items.iter().map(|item| item.game_id.0).collect()
                ))
                .collect::<Vec<(SystemId, Vec<i64>)>>(),
            vec![
                (SystemId::Nes, vec![3]),
                (SystemId::Nintendo64, vec![5]),
                (SystemId::Snes, vec![1]),
            ],
            "combined filters compose exactly as they do in the grid"
        );

        // Search metacharacters stay literal on both surfaces.
        assert_shelves_agree_with_grid(
            &repository,
            &LibraryShelfQuery {
                search: Some("%".to_owned()),
                ..LibraryShelfQuery::default()
            },
        )
        .await;
    }

    #[tokio::test]
    async fn shelves_project_availability_favorites_and_cover_identity_like_the_grid() {
        let (_directory, pool) = fixture().await;
        let repository = LibraryRepository::new(pool.clone());
        insert_root(&pool).await;
        insert_game(
            &pool,
            1,
            SystemId::Snes,
            "Local Only",
            GameAvailability::Unavailable,
        )
        .await;
        insert_metadata(&pool, 1, "Provider Title", None, Some("RPG"), Some("EU")).await;
        insert_match(&pool, 1, "matched").await;
        insert_cached_cover(&pool, 1).await;

        let shelves = repository
            .query_library_shelves(
                &LibraryShelfQuery::default(),
                MetadataProviderId::ScreenScraper,
            )
            .await
            .expect("shelf projection");
        let item = &shelves.shelves[0].items[0];
        let grid = repository
            .query_library(&LibraryQuery::default(), MetadataProviderId::ScreenScraper)
            .await
            .expect("grid page");

        assert_eq!(
            item, &grid.items[0],
            "the shelf must reuse the grid's own list projection, field for field"
        );
        assert_eq!(item.display_title, "Provider Title");
        assert_eq!(item.availability, GameAvailability::Unavailable);
        assert_eq!(
            item.cover_ref.as_deref(),
            Some("rfmedia://localhost/cover/1")
        );

        let serialized = serde_json::to_string(&shelves).expect("serializable shelves");
        assert_no_physical_identity(&serialized);
    }

    #[tokio::test]
    async fn every_system_present_in_the_data_receives_a_shelf() {
        let (_directory, pool) = fixture().await;
        let repository = LibraryRepository::new(pool.clone());
        insert_root(&pool).await;

        // The shelf SQL groups by whatever `system_id` the data really holds. Nothing is matched
        // against a hard-coded ordering table, so no system's games can be dropped by a projection
        // that has not heard of it yet.
        for (index, system) in SystemId::ALL_V1.iter().enumerate() {
            insert_game(
                &pool,
                index as i64 + 1,
                *system,
                "Any Title",
                GameAvailability::Available,
            )
            .await;
        }

        let shelves = assert_shelves_agree_with_grid(&repository, &LibraryShelfQuery::default())
            .await
            .shelves;
        assert_eq!(shelves.len(), SystemId::ALL_V1.len());
        let mut returned: Vec<&str> = shelves
            .iter()
            .map(|shelf| shelf.system_id.as_str())
            .collect();
        returned.sort_unstable();
        let mut expected: Vec<&str> = SystemId::ALL_V1.iter().map(|id| id.as_str()).collect();
        expected.sort_unstable();
        assert_eq!(returned, expected);
    }

    /// Inserts `games` rows spread over every authoritative system and proves the shelf response
    /// is bounded by system count rather than by library size.
    async fn shelf_scale_run(games: i64) {
        let (_directory, pool) = fixture().await;
        let repository = LibraryRepository::new(pool.clone());
        insert_root(&pool).await;
        let systems = SystemId::ALL_V1;
        let mut transaction = pool.begin().await.unwrap();
        for game_id in 1..=games {
            sqlx::query(
                "INSERT INTO games \
                 (id, system_id, local_title, availability, created_at, updated_at) \
                 VALUES (?, ?, ?, 'available', ?, ?)",
            )
            .bind(game_id)
            .bind(systems[(game_id as usize) % systems.len()].as_str())
            .bind(format!("Synthetic Game {game_id:06}"))
            .bind(TEST_TIME)
            .bind(TEST_TIME)
            .execute(&mut *transaction)
            .await
            .unwrap();
        }
        transaction.commit().await.unwrap();

        let shelves = repository
            .query_library_shelves(
                &LibraryShelfQuery::default(),
                MetadataProviderId::ScreenScraper,
            )
            .await
            .expect("shelf projection")
            .shelves;

        let returned: usize = shelves.iter().map(|shelf| shelf.items.len()).sum();
        let ceiling = systems.len() * DEFAULT_LIBRARY_SHELF_PREVIEW as usize;
        assert!(
            returned <= ceiling,
            "a {games}-game library returned {returned} items, above the \
             system-count x preview ceiling of {ceiling}"
        );
        assert_eq!(shelves.len(), systems.len());
        for shelf in &shelves {
            assert_eq!(shelf.items.len(), DEFAULT_LIBRARY_SHELF_PREVIEW as usize);
            assert!(
                shelf.total > DEFAULT_LIBRARY_SHELF_PREVIEW as u64,
                "the total must keep counting past the preview"
            );
        }
        let counted: u64 = shelves.iter().map(|shelf| shelf.total).sum();
        assert_eq!(
            counted, games as u64,
            "every game is still counted even though almost none are returned"
        );
    }

    #[tokio::test]
    async fn a_five_thousand_game_library_returns_a_bounded_shelf_response() {
        shelf_scale_run(5_000).await;
    }

    #[tokio::test]
    async fn a_twenty_thousand_game_library_returns_a_bounded_shelf_response() {
        shelf_scale_run(20_000).await;
    }

    #[tokio::test]
    async fn scan_issue_page_is_bounded_paginated_and_deterministically_ordered() {
        let (_directory, pool) = fixture().await;
        let repository = LibraryRepository::new(pool.clone());
        let old_run: i64 = sqlx::query_scalar(
            "INSERT INTO scan_runs (state, started_at) VALUES ('completed', ?) RETURNING id",
        )
        .bind(TEST_TIME)
        .fetch_one(&pool)
        .await
        .unwrap();
        let latest_run: i64 = sqlx::query_scalar(
            "INSERT INTO scan_runs (state, started_at) VALUES ('completed', ?) RETURNING id",
        )
        .bind(TEST_TIME + 1)
        .fetch_one(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO scan_issues (scan_run_id, kind, relative_path, created_at) \
             VALUES (?, 'unreadable_path', 'old-issue', ?)",
        )
        .bind(old_run)
        .bind(TEST_TIME + 10_000)
        .execute(&pool)
        .await
        .unwrap();
        for issue_id in 0_i64..205 {
            sqlx::query(
                "INSERT INTO scan_issues (scan_run_id, kind, relative_path, created_at) \
                 VALUES (?, 'unreadable_path', ?, ?)",
            )
            .bind(latest_run)
            .bind(format!("issue-{issue_id}"))
            .bind(if issue_id < 2 {
                TEST_TIME + 20_000
            } else {
                TEST_TIME + issue_id
            })
            .execute(&pool)
            .await
            .unwrap();
        }

        let first_page = repository.list_latest_scan_issues_page(0, 0).await.unwrap();
        assert_eq!(first_page.total, 205);
        assert_eq!(first_page.limit, DEFAULT_SCAN_ISSUE_PAGE_SIZE);
        assert_eq!(
            first_page.issues.len(),
            DEFAULT_SCAN_ISSUE_PAGE_SIZE as usize
        );
        assert_eq!(
            first_page.issues[0].relative_path.as_deref(),
            Some("issue-1")
        );
        assert_eq!(
            first_page.issues[1].relative_path.as_deref(),
            Some("issue-0")
        );
        assert!(!first_page
            .issues
            .iter()
            .any(|issue| issue.relative_path.as_deref() == Some("old-issue")));

        let tail = repository
            .list_latest_scan_issues_page(200, 50)
            .await
            .unwrap();
        assert_eq!(tail.total, 205);
        assert_eq!(tail.offset, 200);
        assert_eq!(tail.issues.len(), 5);
        assert_eq!(tail.issues[0].relative_path.as_deref(), Some("issue-6"));

        let capped = repository
            .list_latest_scan_issues_page(0, u32::MAX)
            .await
            .unwrap();
        assert_eq!(capped.limit, MAX_SCAN_ISSUE_PAGE_SIZE);
        assert_eq!(capped.issues.len(), MAX_SCAN_ISSUE_PAGE_SIZE as usize);

        let running_run: i64 = sqlx::query_scalar(
            "INSERT INTO scan_runs (state, started_at) VALUES ('running', ?) RETURNING id",
        )
        .bind(TEST_TIME + 2)
        .fetch_one(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO scan_issues (scan_run_id, kind, relative_path, created_at) \
             VALUES (?, 'unreadable_path', 'running-issue', ?)",
        )
        .bind(running_run)
        .bind(TEST_TIME + 30_000)
        .execute(&pool)
        .await
        .unwrap();

        let during_new_run = repository
            .list_latest_scan_issues_page(0, 50)
            .await
            .unwrap();
        assert_eq!(
            during_new_run.scan_run_id,
            Some(ScanRunId(latest_run)),
            "a running scan must not replace the latest persisted issue run"
        );
        assert_eq!(during_new_run.total, 205);
        assert_eq!(during_new_run.issues.len(), 50);
        assert!(during_new_run
            .issues
            .iter()
            .all(|issue| issue.scan_run_id == Some(ScanRunId(latest_run))));

        sqlx::query("UPDATE scan_runs SET state = 'completed', completed_at = ? WHERE id = ?")
            .bind(TEST_TIME + 40_000)
            .bind(running_run)
            .execute(&pool)
            .await
            .unwrap();
        let completed_new_run = repository
            .list_latest_scan_issues_page(0, 50)
            .await
            .unwrap();
        assert_eq!(completed_new_run.scan_run_id, Some(ScanRunId(running_run)));
        assert_eq!(completed_new_run.total, 1);
        assert_eq!(completed_new_run.issues.len(), 1);
        assert_eq!(
            completed_new_run.issues[0].scan_run_id,
            Some(ScanRunId(running_run))
        );
        assert_eq!(
            completed_new_run.issues[0].relative_path.as_deref(),
            Some("running-issue")
        );
    }
}
