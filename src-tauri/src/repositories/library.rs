use crate::domain::library::{
    ContentFile, ContentFileAvailability, ContentFileId, ContentFileMembership, ContentFileRole,
    ContentRoot, ContentRootAvailability, ContentRootId, ContentRootKind, ContentUnit,
    ContentUnitAvailability, ContentUnitId, ContentUnitKind, Game, GameAvailability, GameId,
    GameSnapshot, LibrarySnapshot, ScanCounters, ScanIssue, ScanIssueId, ScanIssueKind, ScanPhase,
    ScanProgress, ScanRunId, ScanRunState, ScanStatus, ScanSummary, ScannedRoot,
};
use crate::domain::system::SystemId;
use crate::error::AppError;
use sqlx::{Row, Sqlite, SqlitePool, Transaction};
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
                return Err(AppError::Library(
                    "the managed content root cannot be configured as external".to_owned(),
                ));
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
        let root = self.content_root(root_id).await?.ok_or_else(|| {
            AppError::Library("the requested content root does not exist".to_owned())
        })?;
        if root.kind == ContentRootKind::Managed {
            return Err(AppError::Library(
                "the managed content root cannot be removed".to_owned(),
            ));
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
            return Err(AppError::Library(
                "the requested content root does not exist".to_owned(),
            ));
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
        let mut used_existing_file_ids = BTreeSet::new();
        let mut consumed_move_identities = BTreeSet::new();
        let mut generated_issues = Vec::new();
        let discovered_paths: BTreeSet<_> = snapshot
            .files
            .iter()
            .map(|file| file.relative_path.as_str())
            .collect();

        for file in &snapshot.files {
            let file_id = if let Some(existing) = files_by_path.get(&file.relative_path).cloned() {
                update_file(&mut transaction, &existing, file, now).await?;
                let updated = existing.updated_from_scanned(file);
                put_live_file(&mut files_by_id, &mut files_by_path, updated);
                used_existing_file_ids.insert(existing.id);
                existing.id
            } else {
                let candidates: Vec<_> = files_by_id
                    .values()
                    .filter(|candidate| {
                        !used_existing_file_ids.contains(&candidate.id)
                            && (candidate.availability == ContentFileAvailability::Missing
                                || !discovered_paths.contains(candidate.relative_path.as_str()))
                            && hashes_match_file(candidate, file)
                    })
                    .cloned()
                    .collect();
                if candidates.len() == 1 {
                    let candidate = candidates
                        .into_iter()
                        .next()
                        .expect("one file candidate exists");
                    update_file_path_and_content(
                        &mut transaction,
                        &candidate,
                        &file.relative_path,
                        file,
                        now,
                    )
                    .await?;
                    used_existing_file_ids.insert(candidate.id);
                    if let Some(identity) = file_identity(file) {
                        consumed_move_identities.insert(identity);
                    }
                    let updated = candidate.updated_from_scanned_at_path(file, &file.relative_path);
                    put_live_file(&mut files_by_id, &mut files_by_path, updated);
                    candidate.id
                } else {
                    let consumed_match = file_identity(file)
                        .is_some_and(|identity| consumed_move_identities.contains(&identity));
                    if candidates.len() > 1 || consumed_match {
                        generated_issues.push(ScanIssue {
                            id: None,
                            scan_run_id: Some(run_id),
                            root_id: Some(root_id),
                            kind: ScanIssueKind::AmbiguousReconciliation,
                            relative_path: Some(file.relative_path.clone()),
                            related_path: None,
                            detail: Some(if candidates.len() > 1 {
                                "more than one missing file has the same content fingerprint"
                                    .to_owned()
                            } else {
                                "one existing file identity matched more than one discovered path"
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

                let duplicate_game = scanned_unit.fingerprint.as_ref().and_then(|fingerprint| {
                    let entries = known_fingerprints.get(fingerprint)?;
                    let game_ids: BTreeSet<_> = entries.iter().map(|(_, game)| *game).collect();
                    if game_ids.len() == 1 {
                        Some(*game_ids.first().expect("one game id exists"))
                    } else {
                        None
                    }
                });
                if duplicate_game.is_some() {
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
                } else if scanned_unit.fingerprint.is_some()
                    && known_fingerprints.contains_key(
                        scanned_unit
                            .fingerprint
                            .as_ref()
                            .expect("fingerprint exists"),
                    )
                {
                    generated_issues.push(ScanIssue {
                        id: None,
                        scan_run_id: Some(run_id),
                        root_id: Some(root_id),
                        kind: ScanIssueKind::AmbiguousReconciliation,
                        relative_path: Some(scanned_unit.primary_relative_path.clone()),
                        related_path: None,
                        detail: Some(
                            "an exact content fingerprint belongs to more than one logical game"
                                .to_owned(),
                        ),
                        created_at: now,
                    });
                }

                let game_id = if let Some(game_id) = duplicate_game {
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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct FileIdentity {
    size_bytes: u64,
    crc32: String,
    md5: String,
    sha1: String,
}

fn file_identity(file: &crate::domain::library::ScannedFile) -> Option<FileIdentity> {
    let hashes = file.hashes.as_ref()?;
    Some(FileIdentity {
        size_bytes: file.size_bytes,
        crc32: hashes.crc32.clone(),
        md5: hashes.md5.clone(),
        sha1: hashes.sha1.clone(),
    })
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
