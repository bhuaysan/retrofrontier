use crate::domain::library::{
    roots_overlap, ContentFileRole, ContentFormat, ContentHashes, ContentRoot,
    ContentRootAvailability, ContentRootKind, ContentUnitKind, ScanAuthority, ScanCounters,
    ScanIssue, ScanIssueKind, ScanPhase, ScanProgress, ScanRunId, ScanRunState, ScanSummary,
    ScannedFile, ScannedMember, ScannedRoot, ScannedUnit,
};
use crate::domain::system::{SystemCatalog, SystemId};
use crate::error::AppError;
use crate::repositories::library::LibraryRepository;
use crc32fast::Hasher as Crc32Hasher;
use md5::Digest;
use sha1::Sha1;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const LIBRARY_SCAN_PROGRESS_EVENT: &str = "library-scan-progress";
pub const LIBRARY_SCAN_COMPLETED_EVENT: &str = "library-scan-completed";
/// Maximum UTF-8 descriptor text loaded into memory for one CUE, GDI, or M3U file.
pub const MAX_DESCRIPTOR_SIZE: usize = 256 * 1024;

pub trait ScanEventSink: Send + Sync {
    fn progress(&self, progress: ScanProgress);
    fn completed(&self, summary: ScanSummary);
}

#[allow(dead_code)]
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopScanEventSink;

impl ScanEventSink for NoopScanEventSink {
    fn progress(&self, _progress: ScanProgress) {}

    fn completed(&self, _summary: ScanSummary) {}
}

#[derive(Clone)]
pub struct ScanService {
    repository: LibraryRepository,
    catalog: SystemCatalog,
    sink: Arc<dyn ScanEventSink>,
}

impl ScanService {
    pub fn new(
        repository: LibraryRepository,
        catalog: SystemCatalog,
        sink: Arc<dyn ScanEventSink>,
    ) -> Self {
        Self {
            repository,
            catalog,
            sink,
        }
    }

    pub async fn scan_once(&self) -> Result<ScanSummary, AppError> {
        let run_id = self.repository.start_scan_run().await?;
        let started = Instant::now();
        let mut counters = ScanCounters::default();
        let reporter = ProgressReporter::new(self.sink.clone());
        reporter.emit(run_id, ScanPhase::Discovery, counters, true);

        let result = self.scan_inner(run_id, &mut counters, &reporter).await;
        let final_error = match result {
            Ok(()) => match self
                .repository
                .finish_scan_run(run_id, ScanRunState::Completed, counters)
                .await
            {
                Ok(()) => None,
                Err(error) => {
                    counters.issues_found = counters.issues_found.saturating_add(1);
                    Some(error)
                }
            },
            Err(error) => {
                counters.issues_found = counters.issues_found.saturating_add(1);
                let _ = self
                    .repository
                    .finish_scan_run(run_id, ScanRunState::Failed, counters)
                    .await;
                Some(error)
            }
        };
        let state = if final_error.is_some() {
            ScanRunState::Failed
        } else {
            ScanRunState::Completed
        };
        let summary = ScanSummary {
            run_id,
            state,
            counters,
            duration_ms: started.elapsed().as_millis().min(u64::MAX as u128) as u64,
        };
        reporter.emit(run_id, ScanPhase::Completed, counters, true);
        self.sink.completed(summary.clone());
        match final_error {
            Some(error) => Err(error),
            None => Ok(summary),
        }
    }

    async fn scan_inner(
        &self,
        run_id: ScanRunId,
        counters: &mut ScanCounters,
        reporter: &ProgressReporter,
    ) -> Result<(), AppError> {
        let roots = self.repository.get_content_roots().await?;
        let planned = plan_roots(roots);
        counters.roots_discovered = planned.len() as u64;

        let mut discovered_roots = Vec::with_capacity(planned.len());
        for plan in planned {
            if plan.blocked_by_overlap {
                let mut root = plan.root;
                root.availability = ContentRootAvailability::Unsafe;
                discovered_roots.push(DiscoveredRoot {
                    root: root.clone(),
                    root_path: PathBuf::from(&root.path),
                    canonical_root: None,
                    authority: ScanAuthority::default(),
                    candidates: BTreeMap::new(),
                    issues: vec![issue(
                        Some(root.id),
                        ScanIssueKind::OverlappingContentRoot,
                        None,
                        None,
                        Some(
                            "this root overlaps another enabled root and was not scanned"
                                .to_owned(),
                        ),
                    )],
                });
                continue;
            }
            let discovered = discover_root(&plan.root, &self.catalog);
            counters.files_discovered = counters
                .files_discovered
                .saturating_add(discovered.candidates.len() as u64);
            counters.issues_found = counters
                .issues_found
                .saturating_add(discovered.issues.len() as u64);
            discovered_roots.push(discovered);
        }

        reporter.emit(run_id, ScanPhase::RelationshipResolution, *counters, true);
        let mut resolved_roots = Vec::with_capacity(discovered_roots.len());
        for discovered in discovered_roots {
            let discovery_issue_count = discovered.issues.len();
            let resolved = resolve_root(discovered, &self.catalog);
            counters.issues_found = counters
                .issues_found
                .saturating_add(resolved.issues.len().saturating_sub(discovery_issue_count) as u64);
            resolved_roots.push(resolved);
        }

        reporter.emit(run_id, ScanPhase::Hashing, *counters, true);
        let mut scanned_roots = Vec::with_capacity(resolved_roots.len());
        for resolved in resolved_roots {
            let (scanned, hash_issue_count, processed, hashed, bytes) =
                hash_resolved_root(resolved);
            counters.files_processed = counters.files_processed.saturating_add(processed);
            counters.files_hashed = counters.files_hashed.saturating_add(hashed);
            counters.bytes_hashed = counters.bytes_hashed.saturating_add(bytes);
            counters.issues_found = counters.issues_found.saturating_add(hash_issue_count);
            reporter.emit(run_id, ScanPhase::Hashing, *counters, false);
            scanned_roots.push(scanned);
        }

        reporter.emit(run_id, ScanPhase::Reconciliation, *counters, true);
        for root in &scanned_roots {
            let result = self.repository.reconcile_root(run_id, root).await?;
            counters.issues_found = counters.issues_found.saturating_add(result.issues_found);
            counters.roots_completed = counters.roots_completed.saturating_add(1);
            reporter.emit(run_id, ScanPhase::Reconciliation, *counters, false);
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct RootPlan {
    root: ContentRoot,
    blocked_by_overlap: bool,
}

fn plan_roots(mut roots: Vec<ContentRoot>) -> Vec<RootPlan> {
    roots.retain(|root| root.enabled);
    roots.sort_by(|left, right| {
        let left_priority = if left.kind == ContentRootKind::Managed {
            0
        } else {
            1
        };
        let right_priority = if right.kind == ContentRootKind::Managed {
            0
        } else {
            1
        };
        left_priority
            .cmp(&right_priority)
            .then_with(|| path_depth(&left.path).cmp(&path_depth(&right.path)))
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| left.id.cmp(&right.id))
    });

    let mut accepted = Vec::<String>::new();
    roots
        .into_iter()
        .map(|root| {
            let blocked = accepted
                .iter()
                .any(|accepted| roots_overlap(accepted, &root.path));
            if !blocked {
                accepted.push(root.path.clone());
            }
            RootPlan {
                root,
                blocked_by_overlap: blocked,
            }
        })
        .collect()
}

fn path_depth(path: &str) -> usize {
    Path::new(path).components().count()
}

#[derive(Debug, Clone)]
struct FileMetadata {
    size_bytes: u64,
    modified_at: i64,
}

#[derive(Debug, Clone)]
struct Candidate {
    path: PathBuf,
    relative_path: String,
    extension: String,
    format: ContentFormat,
    system_id: Option<SystemId>,
    classification_issue: Option<ClassificationIssue>,
    metadata: FileMetadata,
    relevant: bool,
    hashes: Option<ContentHashes>,
    hash_available: bool,
    hash_failed: bool,
}

#[derive(Debug, Clone)]
struct ClassificationIssue {
    kind: ScanIssueKind,
    detail: String,
}

#[derive(Debug)]
struct DiscoveredRoot {
    root: ContentRoot,
    root_path: PathBuf,
    canonical_root: Option<PathBuf>,
    authority: ScanAuthority,
    candidates: BTreeMap<String, Candidate>,
    issues: Vec<ScanIssue>,
}

fn discover_root(root: &ContentRoot, catalog: &SystemCatalog) -> DiscoveredRoot {
    let root_path = PathBuf::from(&root.path);
    let mut discovered = DiscoveredRoot {
        root: root.clone(),
        root_path: root_path.clone(),
        canonical_root: None,
        authority: ScanAuthority::default(),
        candidates: BTreeMap::new(),
        issues: Vec::new(),
    };

    if !root_path.is_absolute() {
        discovered.root.availability = ContentRootAvailability::Unsafe;
        discovered.authority.mark_incomplete("");
        discovered.issues.push(issue(
            Some(root.id),
            ScanIssueKind::UnsafePath,
            None,
            None,
            Some("configured content root is not absolute".to_owned()),
        ));
        return discovered;
    }

    match fs::symlink_metadata(&root_path) {
        Ok(metadata) if is_symlink_or_reparse_point(&metadata) => {
            discovered.root.availability = ContentRootAvailability::Unsafe;
            discovered.authority.mark_incomplete("");
            discovered.issues.push(issue(
                Some(root.id),
                ScanIssueKind::UnsafePath,
                None,
                None,
                Some("configured content root is a symbolic link".to_owned()),
            ));
            return discovered;
        }
        Ok(metadata) if !metadata.is_dir() => {
            discovered.root.availability = ContentRootAvailability::Unavailable;
            discovered.authority.mark_incomplete("");
            discovered.issues.push(issue(
                Some(root.id),
                ScanIssueKind::RootUnavailable,
                None,
                None,
                Some("configured content root is not a directory".to_owned()),
            ));
            return discovered;
        }
        Err(error) => {
            discovered.root.availability = ContentRootAvailability::Unavailable;
            discovered.authority.mark_incomplete("");
            discovered.issues.push(issue(
                Some(root.id),
                ScanIssueKind::RootUnavailable,
                None,
                None,
                Some(error.to_string()),
            ));
            return discovered;
        }
        Ok(_) => {}
    }

    let canonical_root = match fs::canonicalize(&root_path) {
        Ok(path) if path.is_dir() => path,
        Ok(_) => {
            discovered.root.availability = ContentRootAvailability::Unavailable;
            discovered.authority.mark_incomplete("");
            discovered.issues.push(issue(
                Some(discovered.root.id),
                ScanIssueKind::RootUnavailable,
                None,
                None,
                Some("configured content root is not a directory".to_owned()),
            ));
            return discovered;
        }
        Err(error) => {
            discovered.root.availability = ContentRootAvailability::Unavailable;
            discovered.authority.mark_incomplete("");
            discovered.issues.push(issue(
                Some(discovered.root.id),
                ScanIssueKind::RootUnavailable,
                None,
                None,
                Some(error.to_string()),
            ));
            return discovered;
        }
    };
    discovered.canonical_root = Some(canonical_root);

    let mut relative = Vec::new();
    walk_directory(&mut discovered, catalog, &mut relative);
    if discovered.authority.is_fully_authoritative() {
        discovered.root.availability = ContentRootAvailability::Available;
    } else if discovered.root.availability == ContentRootAvailability::Available {
        discovered.root.availability = ContentRootAvailability::PartiallyAvailable;
    }
    discovered
}

fn walk_directory(
    discovered: &mut DiscoveredRoot,
    catalog: &SystemCatalog,
    relative_components: &mut Vec<String>,
) {
    let directory = components_to_path(&discovered.root_path, relative_components);
    let entries = match fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(error) => {
            let relative_directory =
                relative_path_from_components(relative_components).unwrap_or_default();
            discovered.authority.mark_incomplete(&relative_directory);
            discovered.root.availability = ContentRootAvailability::PartiallyAvailable;
            discovered.issues.push(issue(
                Some(discovered.root.id),
                ScanIssueKind::UnreadablePath,
                relative_path_from_components(relative_components),
                None,
                Some(error.to_string()),
            ));
            return;
        }
    };

    let relative_directory = relative_path_from_components(relative_components).unwrap_or_default();
    discovered
        .authority
        .mark_directory_enumerated(&relative_directory);

    let mut named_entries = Vec::new();
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                discovered.authority.mark_incomplete(&relative_directory);
                discovered.root.availability = ContentRootAvailability::PartiallyAvailable;
                discovered.issues.push(issue(
                    Some(discovered.root.id),
                    ScanIssueKind::UnreadablePath,
                    relative_path_from_components(relative_components),
                    None,
                    Some(error.to_string()),
                ));
                continue;
            }
        };
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            discovered.authority.mark_unrepresentable_entry();
            discovered.root.availability = ContentRootAvailability::PartiallyAvailable;
            discovered.issues.push(issue(
                Some(discovered.root.id),
                ScanIssueKind::UnrepresentablePath,
                relative_path_from_components(relative_components),
                None,
                Some("filesystem name is not representable as UTF-8".to_owned()),
            ));
            continue;
        };
        if name.contains('\\') {
            discovered.authority.mark_unrepresentable_entry();
            discovered.root.availability = ContentRootAvailability::PartiallyAvailable;
            discovered.issues.push(issue(
                Some(discovered.root.id),
                ScanIssueKind::UnrepresentablePath,
                relative_path_from_components(relative_components),
                None,
                Some("filesystem name contains a path-separator character".to_owned()),
            ));
            continue;
        }
        named_entries.push((name.to_owned(), entry));
    }
    named_entries.sort_by(|left, right| left.0.cmp(&right.0));

    for (name, entry) in named_entries {
        let path = entry.path();
        relative_components.push(name.clone());
        let relative_path = relative_path_from_components(relative_components)
            .expect("validated UTF-8 path components should be representable");
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) => {
                discovered.authority.mark_incomplete(&relative_path);
                discovered.root.availability = ContentRootAvailability::PartiallyAvailable;
                discovered.issues.push(issue(
                    Some(discovered.root.id),
                    ScanIssueKind::UnreadablePath,
                    Some(relative_path),
                    None,
                    Some(error.to_string()),
                ));
                relative_components.pop();
                continue;
            }
        };
        if is_symlink_or_reparse_point(&metadata) {
            discovered.authority.mark_incomplete(&relative_path);
            discovered.root.availability = ContentRootAvailability::PartiallyAvailable;
            discovered.issues.push(issue(
                Some(discovered.root.id),
                ScanIssueKind::UnsafePath,
                Some(relative_path),
                None,
                Some("symbolic links and reparse points are not followed".to_owned()),
            ));
            relative_components.pop();
            continue;
        }
        if metadata.is_dir() {
            walk_directory(discovered, catalog, relative_components);
            relative_components.pop();
            continue;
        }
        if !metadata.is_file() {
            discovered.authority.mark_incomplete(&relative_path);
            discovered.root.availability = ContentRootAvailability::PartiallyAvailable;
            discovered.issues.push(issue(
                Some(discovered.root.id),
                ScanIssueKind::UnsafePath,
                Some(relative_path),
                None,
                Some("special filesystem objects are not scanned".to_owned()),
            ));
            relative_components.pop();
            continue;
        }

        let Some(extension) = path.extension().and_then(|extension| extension.to_str()) else {
            relative_components.pop();
            continue;
        };
        let extension = format!(".{}", extension.to_ascii_lowercase());
        if catalog.systems_for_extension(&extension).is_empty() {
            relative_components.pop();
            continue;
        }
        let (system_id, classification_issue) =
            classify_system(&discovered.root, catalog, &relative_path, &extension);
        discovered.candidates.insert(
            relative_path.clone(),
            Candidate {
                path,
                relative_path,
                extension: extension.clone(),
                format: ContentFormat::from_extension(&extension),
                system_id,
                classification_issue,
                metadata: FileMetadata {
                    size_bytes: metadata.len(),
                    modified_at: modified_timestamp(&metadata),
                },
                relevant: false,
                hashes: None,
                hash_available: false,
                hash_failed: false,
            },
        );
        relative_components.pop();
    }
}

fn classify_system(
    root: &ContentRoot,
    catalog: &SystemCatalog,
    relative_path: &str,
    extension: &str,
) -> (Option<SystemId>, Option<ClassificationIssue>) {
    if let Some(system_id) = root.system_hint {
        if catalog.supports_extension(system_id, extension) {
            return (Some(system_id), None);
        }
        return (
            None,
            Some(ClassificationIssue {
                kind: ScanIssueKind::IncompatibleSystemHint,
                detail: format!("root hint {system_id} does not support extension {extension}"),
            }),
        );
    }

    if root.kind == ContentRootKind::Managed {
        if let Some(top_level) = relative_path.split('/').next() {
            if let Some(system) = catalog.system_for_managed_folder_name(top_level) {
                if catalog.supports_extension(system.id, extension) {
                    return (Some(system.id), None);
                }
                return (
                    None,
                    Some(ClassificationIssue {
                        kind: ScanIssueKind::IncompatibleSystemHint,
                        detail: format!(
                            "managed folder hint {} does not support extension {extension}",
                            system.id
                        ),
                    }),
                );
            }
        }
    }

    let systems = catalog.systems_for_extension(extension);
    if systems.len() == 1 {
        (Some(systems[0]), None)
    } else if systems.len() > 1 {
        (
            None,
            Some(ClassificationIssue {
                kind: ScanIssueKind::AmbiguousSystem,
                detail: format!("extension {extension} is supported by multiple systems"),
            }),
        )
    } else {
        (
            None,
            Some(ClassificationIssue {
                kind: ScanIssueKind::UnsupportedSystem,
                detail: format!("no catalog system supports extension {extension}"),
            }),
        )
    }
}

#[derive(Debug, Clone)]
struct RawUnit {
    system_id: SystemId,
    kind: ContentUnitKind,
    primary_relative_path: String,
    members: Vec<ScannedMember>,
    complete: bool,
}

#[derive(Debug)]
struct ResolvedRoot {
    root: ContentRoot,
    root_path: PathBuf,
    authority: ScanAuthority,
    candidates: BTreeMap<String, Candidate>,
    units: Vec<RawUnit>,
    issues: Vec<ScanIssue>,
}

fn resolve_root(discovered: DiscoveredRoot, catalog: &SystemCatalog) -> ResolvedRoot {
    let mut resolver = RelationshipResolver::new(discovered, catalog);
    resolver.resolve();
    resolver.into_resolved_root()
}

struct RelationshipResolver<'a> {
    root: ContentRoot,
    root_path: PathBuf,
    canonical_root: Option<PathBuf>,
    authority: ScanAuthority,
    catalog: &'a SystemCatalog,
    candidates: BTreeMap<String, Candidate>,
    units: Vec<RawUnit>,
    issues: Vec<ScanIssue>,
    owned_paths: BTreeSet<String>,
    m3u_specs: BTreeMap<String, M3uSpec>,
    m3u_incoming: BTreeSet<String>,
}

#[derive(Debug, Clone)]
struct M3uSpec {
    entries: Vec<String>,
    valid: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EnsureCandidateResult {
    Present,
    Missing,
    Rejected,
}

#[derive(Debug)]
enum ContainedPathError {
    Missing,
    Unsafe(&'static str),
    Io(io::Error),
}

#[derive(Debug)]
enum DescriptorReadError {
    Unsafe,
    TooLarge,
    Io(io::Error),
}

impl<'a> RelationshipResolver<'a> {
    fn new(discovered: DiscoveredRoot, catalog: &'a SystemCatalog) -> Self {
        Self {
            root: discovered.root,
            root_path: discovered.root_path,
            canonical_root: discovered.canonical_root,
            authority: discovered.authority,
            catalog,
            candidates: discovered.candidates,
            units: Vec::new(),
            issues: discovered.issues,
            owned_paths: BTreeSet::new(),
            m3u_specs: BTreeMap::new(),
            m3u_incoming: BTreeSet::new(),
        }
    }

    fn resolve(&mut self) {
        self.parse_m3u_specs();
        self.collect_m3u_incoming_edges();

        let m3u_paths: Vec<_> = self
            .candidates
            .values()
            .filter(|candidate| {
                candidate.format == ContentFormat::M3u && candidate.system_id.is_some()
            })
            .map(|candidate| candidate.relative_path.clone())
            .collect();
        for relative_path in m3u_paths {
            if self.m3u_incoming.contains(&relative_path) {
                continue;
            }
            self.build_m3u_unit(&relative_path);
        }

        // A cycle can make every playlist appear to have an incoming edge. Keep breaking one
        // remaining playlist at a time until every playlist has an owning unit, so independent
        // cycles are all surfaced rather than falling through to standalone handling.
        loop {
            let next_unowned = self
                .candidates
                .values()
                .find(|candidate| {
                    candidate.format == ContentFormat::M3u
                        && candidate.system_id.is_some()
                        && !self.owned_paths.contains(&candidate.relative_path)
                })
                .map(|candidate| candidate.relative_path.clone());
            let Some(relative_path) = next_unowned else {
                break;
            };
            self.build_m3u_unit(&relative_path);
        }

        let descriptor_paths: Vec<_> = self
            .candidates
            .values()
            .filter(|candidate| {
                matches!(candidate.format, ContentFormat::Cue | ContentFormat::Gdi)
                    && candidate.system_id.is_some()
            })
            .map(|candidate| candidate.relative_path.clone())
            .collect();
        for relative_path in descriptor_paths {
            if self.owned_paths.contains(&relative_path) {
                continue;
            }
            self.build_descriptor_unit(&relative_path);
        }

        let standalone_paths: Vec<_> = self.candidates.keys().cloned().collect();
        for relative_path in standalone_paths {
            if self.owned_paths.contains(&relative_path) {
                continue;
            }
            let Some(candidate) = self.candidates.get_mut(&relative_path) else {
                continue;
            };
            let Some(system_id) = candidate.system_id else {
                continue;
            };
            if candidate.format == ContentFormat::M3u {
                continue;
            }
            candidate.relevant = true;
            self.owned_paths.insert(relative_path.clone());
            self.units.push(RawUnit {
                system_id,
                kind: unit_kind_for_format(candidate.format),
                primary_relative_path: relative_path.clone(),
                members: vec![ScannedMember {
                    relative_path,
                    ordinal: 0,
                    role: ContentFileRole::Standalone,
                    present: true,
                }],
                complete: true,
            });
        }

        let deferred_classification: Vec<_> = self
            .candidates
            .values()
            .filter_map(|candidate| {
                candidate
                    .classification_issue
                    .as_ref()
                    .map(|classification| (candidate.relative_path.clone(), classification.clone()))
            })
            .collect();
        for (relative_path, classification) in deferred_classification {
            if self.owned_paths.contains(&relative_path) {
                // A descriptor/playlist may legitimately reference a physical member whose
                // extension is not itself a standalone format for the hinted system.
                continue;
            }
            self.authority.mark_incomplete(&relative_path);
            self.root.availability = ContentRootAvailability::PartiallyAvailable;
            self.issues.push(issue(
                Some(self.root.id),
                classification.kind,
                Some(relative_path),
                None,
                Some(classification.detail),
            ));
        }
    }

    fn parse_m3u_specs(&mut self) {
        let paths: Vec<_> = self
            .candidates
            .values()
            .filter(|candidate| {
                candidate.format == ContentFormat::M3u && candidate.system_id.is_some()
            })
            .map(|candidate| candidate.relative_path.clone())
            .collect();
        for relative_path in paths {
            let spec = match self.read_text_file(&relative_path) {
                Ok(contents) => {
                    let entries = contents
                        .lines()
                        .map(str::trim)
                        .filter(|line| !line.is_empty() && !line.starts_with('#'))
                        .map(str::to_owned)
                        .collect::<Vec<_>>();
                    if entries.is_empty() {
                        self.issues.push(issue(
                            Some(self.root.id),
                            ScanIssueKind::MalformedM3u,
                            Some(relative_path.clone()),
                            None,
                            Some("playlist contains no disc entries".to_owned()),
                        ));
                        M3uSpec {
                            entries,
                            valid: false,
                        }
                    } else {
                        M3uSpec {
                            entries,
                            valid: true,
                        }
                    }
                }
                Err(DescriptorReadError::Unsafe) => {
                    self.issues.push(issue(
                        Some(self.root.id),
                        ScanIssueKind::UnsafeDescriptorReference,
                        Some(relative_path.clone()),
                        None,
                        Some("descriptor path is outside the configured content root".to_owned()),
                    ));
                    M3uSpec {
                        entries: Vec::new(),
                        valid: false,
                    }
                }
                Err(DescriptorReadError::TooLarge) => {
                    self.issues.push(issue(
                        Some(self.root.id),
                        ScanIssueKind::MalformedM3u,
                        Some(relative_path.clone()),
                        None,
                        Some(format!(
                            "descriptor exceeds the maximum size of {MAX_DESCRIPTOR_SIZE} bytes"
                        )),
                    ));
                    M3uSpec {
                        entries: Vec::new(),
                        valid: false,
                    }
                }
                Err(DescriptorReadError::Io(error)) => {
                    self.issues.push(issue(
                        Some(self.root.id),
                        ScanIssueKind::MalformedM3u,
                        Some(relative_path.clone()),
                        None,
                        Some(error.to_string()),
                    ));
                    M3uSpec {
                        entries: Vec::new(),
                        valid: false,
                    }
                }
            };
            self.m3u_specs.insert(relative_path, spec);
        }
    }

    fn collect_m3u_incoming_edges(&mut self) {
        let specs: Vec<_> = self
            .m3u_specs
            .iter()
            .filter_map(|(path, spec)| {
                if spec.valid {
                    Some((path.clone(), spec.entries.clone()))
                } else {
                    None
                }
            })
            .collect();
        for (playlist_path, entries) in specs {
            let Some(system_id) = self
                .candidates
                .get(&playlist_path)
                .and_then(|candidate| candidate.system_id)
            else {
                continue;
            };
            for raw in entries {
                let Ok(relative_path) = safe_relative_reference(&playlist_path, &raw) else {
                    continue;
                };
                if self
                    .candidates
                    .get(&relative_path)
                    .is_some_and(|candidate| {
                        candidate.format == ContentFormat::M3u
                            && self.catalog.supports_extension(system_id, ".m3u")
                    })
                {
                    self.m3u_incoming.insert(relative_path);
                }
            }
        }
    }

    fn build_m3u_unit(&mut self, relative_path: &str) {
        let Some(system_id) = self.candidate_system(relative_path) else {
            return;
        };
        let mut members = Vec::new();
        self.add_member(&mut members, relative_path, ContentFileRole::Playlist, true);
        let mut stack = Vec::new();
        let complete =
            self.collect_m3u_contents(relative_path, system_id, &mut stack, &mut members);
        self.units.push(RawUnit {
            system_id,
            kind: ContentUnitKind::M3u,
            primary_relative_path: relative_path.to_owned(),
            members,
            complete,
        });
    }

    fn collect_m3u_contents(
        &mut self,
        relative_path: &str,
        system_id: SystemId,
        stack: &mut Vec<String>,
        members: &mut Vec<ScannedMember>,
    ) -> bool {
        if stack.iter().any(|path| path == relative_path) {
            self.issues.push(issue(
                Some(self.root.id),
                ScanIssueKind::ReferenceCycle,
                Some(relative_path.to_owned()),
                None,
                Some("playlist reference cycle detected".to_owned()),
            ));
            return false;
        }
        let Some(spec) = self.m3u_specs.get(relative_path).cloned() else {
            self.issues.push(issue(
                Some(self.root.id),
                ScanIssueKind::MalformedM3u,
                Some(relative_path.to_owned()),
                None,
                Some("referenced playlist could not be parsed".to_owned()),
            ));
            return false;
        };
        if !spec.valid {
            return false;
        }
        stack.push(relative_path.to_owned());
        let mut complete = true;
        for raw in spec.entries {
            let referenced = match safe_relative_reference(relative_path, &raw) {
                Ok(path) => path,
                Err(error) => {
                    self.issues.push(issue(
                        Some(self.root.id),
                        ScanIssueKind::UnsafeDescriptorReference,
                        Some(relative_path.to_owned()),
                        Some(raw.clone()),
                        Some(error),
                    ));
                    complete = false;
                    continue;
                }
            };
            let present = self.ensure_candidate(&referenced);
            if present != EnsureCandidateResult::Present {
                if present == EnsureCandidateResult::Rejected {
                    complete = false;
                    continue;
                }
                self.issues.push(issue(
                    Some(self.root.id),
                    ScanIssueKind::MissingReferencedFile,
                    Some(relative_path.to_owned()),
                    Some(referenced.clone()),
                    Some("playlist member is missing".to_owned()),
                ));
                self.add_member(&mut *members, &referenced, ContentFileRole::Disc, false);
                complete = false;
                continue;
            }

            let valid_for_system = self.validate_playlist_member(system_id, &referenced);
            let format = self
                .candidates
                .get(&referenced)
                .map(|candidate| candidate.format);
            self.add_member(
                members,
                &referenced,
                if format == Some(ContentFormat::M3u) {
                    ContentFileRole::Playlist
                } else if matches!(format, Some(ContentFormat::Cue | ContentFormat::Gdi)) {
                    ContentFileRole::DiscDescriptor
                } else {
                    ContentFileRole::Disc
                },
                true,
            );
            complete &= valid_for_system;
            match format {
                Some(ContentFormat::M3u) => {
                    complete &= self.collect_m3u_contents(&referenced, system_id, stack, members);
                }
                Some(ContentFormat::Cue) | Some(ContentFormat::Gdi) => {
                    complete &= self.collect_descriptor_members(
                        &referenced,
                        system_id,
                        ContentFileRole::DiscDescriptor,
                        ContentFileRole::DiscTrack,
                        members,
                    );
                }
                _ => {}
            }
        }
        stack.pop();
        complete
    }

    fn build_descriptor_unit(&mut self, relative_path: &str) {
        let Some(system_id) = self.candidate_system(relative_path) else {
            return;
        };
        let format = self
            .candidates
            .get(relative_path)
            .map(|candidate| candidate.format);
        let mut members = Vec::new();
        self.add_member(
            &mut members,
            relative_path,
            ContentFileRole::Descriptor,
            true,
        );
        let complete = match format {
            Some(ContentFormat::Cue) | Some(ContentFormat::Gdi) => self.collect_descriptor_members(
                relative_path,
                system_id,
                ContentFileRole::Descriptor,
                ContentFileRole::Track,
                &mut members,
            ),
            _ => false,
        };
        self.units.push(RawUnit {
            system_id,
            kind: unit_kind_for_format(format.expect("descriptor has a supported format")),
            primary_relative_path: relative_path.to_owned(),
            members,
            complete,
        });
    }

    fn collect_descriptor_members(
        &mut self,
        relative_path: &str,
        _system_id: SystemId,
        descriptor_role: ContentFileRole,
        track_role: ContentFileRole,
        members: &mut Vec<ScannedMember>,
    ) -> bool {
        let format = self
            .candidates
            .get(relative_path)
            .map(|candidate| candidate.format);
        let result = match format {
            Some(ContentFormat::Cue) => self
                .read_text_file(relative_path)
                .map(|contents| parse_cue_file(&contents)),
            Some(ContentFormat::Gdi) => self
                .read_text_file(relative_path)
                .map(|contents| parse_gdi_file(&contents)),
            _ => Ok(Err("unsupported descriptor format".to_owned())),
        };
        let references = match result {
            Ok(Ok(references)) => references,
            Ok(Err(error)) => {
                self.issues.push(issue(
                    Some(self.root.id),
                    if format == Some(ContentFormat::Cue) {
                        ScanIssueKind::MalformedCue
                    } else {
                        ScanIssueKind::MalformedGdi
                    },
                    Some(relative_path.to_owned()),
                    None,
                    Some(error),
                ));
                return false;
            }
            Err(DescriptorReadError::Unsafe) => {
                self.issues.push(issue(
                    Some(self.root.id),
                    ScanIssueKind::UnsafeDescriptorReference,
                    Some(relative_path.to_owned()),
                    None,
                    Some("descriptor path is outside the configured content root".to_owned()),
                ));
                return false;
            }
            Err(DescriptorReadError::TooLarge) => {
                self.issues.push(issue(
                    Some(self.root.id),
                    if format == Some(ContentFormat::Cue) {
                        ScanIssueKind::MalformedCue
                    } else {
                        ScanIssueKind::MalformedGdi
                    },
                    Some(relative_path.to_owned()),
                    None,
                    Some(format!(
                        "descriptor exceeds the maximum size of {MAX_DESCRIPTOR_SIZE} bytes"
                    )),
                ));
                return false;
            }
            Err(DescriptorReadError::Io(error)) => {
                self.issues.push(issue(
                    Some(self.root.id),
                    if format == Some(ContentFormat::Cue) {
                        ScanIssueKind::MalformedCue
                    } else {
                        ScanIssueKind::MalformedGdi
                    },
                    Some(relative_path.to_owned()),
                    None,
                    Some(error.to_string()),
                ));
                return false;
            }
        };
        let mut complete = true;
        for raw in references {
            let referenced = match safe_relative_reference(relative_path, &raw) {
                Ok(path) => path,
                Err(error) => {
                    self.issues.push(issue(
                        Some(self.root.id),
                        ScanIssueKind::UnsafeDescriptorReference,
                        Some(relative_path.to_owned()),
                        Some(raw.clone()),
                        Some(error),
                    ));
                    complete = false;
                    continue;
                }
            };
            let present = self.ensure_candidate(&referenced);
            if present == EnsureCandidateResult::Rejected {
                complete = false;
                continue;
            }
            let is_present = present == EnsureCandidateResult::Present;
            self.add_member(members, &referenced, track_role, is_present);
            if !is_present {
                self.issues.push(issue(
                    Some(self.root.id),
                    ScanIssueKind::MissingReferencedFile,
                    Some(relative_path.to_owned()),
                    Some(referenced),
                    Some("descriptor member is missing".to_owned()),
                ));
                complete = false;
            }
        }
        let _ = (descriptor_role, _system_id);
        complete
    }

    fn validate_playlist_member(&mut self, system_id: SystemId, relative_path: &str) -> bool {
        let Some(candidate) = self.candidates.get(relative_path) else {
            return false;
        };
        if !self
            .catalog
            .supports_extension(system_id, &candidate.extension)
        {
            self.issues.push(issue(
                Some(self.root.id),
                ScanIssueKind::UnsupportedSystem,
                Some(relative_path.to_owned()),
                None,
                Some(format!(
                    "playlist member extension {} is not supported by {system_id}",
                    candidate.extension
                )),
            ));
            return false;
        }
        if let Some(candidate_system) = candidate.system_id {
            if candidate_system != system_id {
                self.issues.push(issue(
                    Some(self.root.id),
                    ScanIssueKind::IncompatibleSystemHint,
                    Some(relative_path.to_owned()),
                    None,
                    Some(format!(
                        "playlist member is classified as {candidate_system}, not {system_id}"
                    )),
                ));
                return false;
            }
        } else if let Some(candidate) = self.candidates.get_mut(relative_path) {
            candidate.system_id = Some(system_id);
            candidate.classification_issue = None;
        }
        true
    }

    fn candidate_system(&self, relative_path: &str) -> Option<SystemId> {
        self.candidates
            .get(relative_path)
            .and_then(|candidate| candidate.system_id)
    }

    fn add_member(
        &mut self,
        members: &mut Vec<ScannedMember>,
        relative_path: &str,
        role: ContentFileRole,
        present: bool,
    ) {
        self.owned_paths.insert(relative_path.to_owned());
        if let Some(candidate) = self.candidates.get_mut(relative_path) {
            candidate.relevant = true;
        }
        members.push(ScannedMember {
            relative_path: relative_path.to_owned(),
            ordinal: members.len() as i64,
            role,
            present,
        });
    }

    fn ensure_candidate(&mut self, relative_path: &str) -> EnsureCandidateResult {
        let path = match resolve_contained_path(
            &self.root_path,
            self.canonical_root.as_deref(),
            relative_path,
        ) {
            Ok(path) => path,
            Err(ContainedPathError::Missing) => return EnsureCandidateResult::Missing,
            Err(ContainedPathError::Unsafe(detail)) => {
                self.issues.push(issue(
                    Some(self.root.id),
                    ScanIssueKind::UnsafeDescriptorReference,
                    Some(relative_path.to_owned()),
                    None,
                    Some(detail.to_owned()),
                ));
                return EnsureCandidateResult::Rejected;
            }
            Err(ContainedPathError::Io(error)) => {
                self.issues.push(issue(
                    Some(self.root.id),
                    ScanIssueKind::UnreadablePath,
                    Some(relative_path.to_owned()),
                    None,
                    Some(error.to_string()),
                ));
                return EnsureCandidateResult::Rejected;
            }
        };
        if let Some(candidate) = self.candidates.get_mut(relative_path) {
            candidate.path = path;
            return EnsureCandidateResult::Present;
        }
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_file() => metadata,
            Ok(_) => {
                self.issues.push(issue(
                    Some(self.root.id),
                    ScanIssueKind::UnsafeDescriptorReference,
                    Some(relative_path.to_owned()),
                    None,
                    Some("referenced path is not a regular file".to_owned()),
                ));
                return EnsureCandidateResult::Rejected;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return EnsureCandidateResult::Missing;
            }
            Err(error) => {
                self.issues.push(issue(
                    Some(self.root.id),
                    ScanIssueKind::UnreadablePath,
                    Some(relative_path.to_owned()),
                    None,
                    Some(error.to_string()),
                ));
                return EnsureCandidateResult::Rejected;
            }
        };
        let extension = Path::new(relative_path)
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| format!(".{}", extension.to_ascii_lowercase()))
            .unwrap_or_default();
        self.candidates.insert(
            relative_path.to_owned(),
            Candidate {
                path,
                relative_path: relative_path.to_owned(),
                extension: extension.clone(),
                format: ContentFormat::from_extension(&extension),
                system_id: None,
                classification_issue: None,
                metadata: FileMetadata {
                    size_bytes: metadata.len(),
                    modified_at: modified_timestamp(&metadata),
                },
                relevant: false,
                hashes: None,
                hash_available: false,
                hash_failed: false,
            },
        );
        EnsureCandidateResult::Present
    }

    fn read_text_file(&self, relative_path: &str) -> Result<String, DescriptorReadError> {
        let path = resolve_contained_path(
            &self.root_path,
            self.canonical_root.as_deref(),
            relative_path,
        )
        .map_err(|error| match error {
            ContainedPathError::Missing => DescriptorReadError::Io(io::Error::new(
                io::ErrorKind::NotFound,
                "descriptor not found",
            )),
            ContainedPathError::Unsafe(_) => DescriptorReadError::Unsafe,
            ContainedPathError::Io(error) => DescriptorReadError::Io(error),
        })?;
        let file = File::open(path).map_err(DescriptorReadError::Io)?;
        let mut limited = file.take((MAX_DESCRIPTOR_SIZE as u64).saturating_add(1));
        let mut bytes = Vec::new();
        limited
            .read_to_end(&mut bytes)
            .map_err(DescriptorReadError::Io)?;
        if bytes.len() > MAX_DESCRIPTOR_SIZE {
            return Err(DescriptorReadError::TooLarge);
        }
        let contents = String::from_utf8(bytes).map_err(|_| {
            DescriptorReadError::Io(io::Error::new(
                io::ErrorKind::InvalidData,
                "descriptor is not valid UTF-8",
            ))
        })?;
        Ok(strip_utf8_bom(contents))
    }

    fn into_resolved_root(self) -> ResolvedRoot {
        ResolvedRoot {
            root: self.root,
            root_path: self.root_path,
            authority: self.authority,
            candidates: self.candidates,
            units: self.units,
            issues: self.issues,
        }
    }
}

fn hash_resolved_root(mut resolved: ResolvedRoot) -> (ScannedRoot, u64, u64, u64, u64) {
    let mut issue_count = 0_u64;
    let mut processed = 0_u64;
    let mut hashed = 0_u64;
    let mut bytes = 0_u64;

    let relevant_paths: Vec<_> = resolved
        .candidates
        .values()
        .filter(|candidate| candidate.relevant)
        .map(|candidate| candidate.relative_path.clone())
        .collect();
    for relative_path in relevant_paths {
        let Some(candidate) = resolved.candidates.get_mut(&relative_path) else {
            continue;
        };
        processed = processed.saturating_add(1);
        match hash_file(&resolved.root_path, candidate) {
            Ok(hashes) => {
                candidate.hashes = Some(hashes);
                candidate.hash_available = true;
                hashed = hashed.saturating_add(1);
                bytes = bytes.saturating_add(candidate.metadata.size_bytes);
            }
            Err(error) => {
                candidate.hash_available = false;
                candidate.hash_failed = true;
                issue_count = issue_count.saturating_add(1);
                resolved.issues.push(issue(
                    Some(resolved.root.id),
                    ScanIssueKind::HashReadFailure,
                    Some(relative_path),
                    None,
                    Some(error.to_string()),
                ));
            }
        }
    }

    let mut files = Vec::new();
    for candidate in resolved
        .candidates
        .values()
        .filter(|candidate| candidate.relevant)
    {
        files.push(ScannedFile {
            relative_path: candidate.relative_path.clone(),
            size_bytes: candidate.metadata.size_bytes,
            modified_at: candidate.metadata.modified_at,
            hashes: candidate.hashes.clone(),
            available: candidate.hash_available,
            hash_failed: candidate.hash_failed,
        });
    }
    files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));

    let mut units = Vec::with_capacity(resolved.units.len());
    for unit in resolved.units {
        let mut complete = unit.complete;
        for member in &unit.members {
            if !member.present {
                complete = false;
                continue;
            }
            if !resolved
                .candidates
                .get(&member.relative_path)
                .is_some_and(|candidate| candidate.hash_available)
            {
                complete = false;
            }
        }
        let fingerprint = if complete {
            content_fingerprint(
                unit.system_id,
                unit.kind,
                &unit.members,
                &resolved.candidates,
            )
        } else {
            None
        };
        let hash_failed = unit.members.iter().any(|member| {
            resolved
                .candidates
                .get(&member.relative_path)
                .is_some_and(|candidate| candidate.hash_failed)
        });
        units.push(ScannedUnit {
            system_id: unit.system_id,
            kind: unit.kind,
            local_title: local_title(&unit.primary_relative_path),
            primary_relative_path: unit.primary_relative_path,
            fingerprint,
            complete,
            hash_failed,
            members: unit.members,
        });
    }

    (
        ScannedRoot {
            root: resolved.root,
            authority: resolved.authority,
            files,
            units,
            issues: resolved.issues,
        },
        issue_count,
        processed,
        hashed,
        bytes,
    )
}

fn content_fingerprint(
    system_id: SystemId,
    kind: ContentUnitKind,
    members: &[ScannedMember],
    candidates: &BTreeMap<String, Candidate>,
) -> Option<String> {
    let mut hasher = Sha1::new();
    hasher.update(b"retrofrontier-content-v1\0");
    hasher.update(system_id.as_str().as_bytes());
    hasher.update(b"\0");
    hasher.update(kind.as_db().as_bytes());
    for member in members {
        let candidate = candidates.get(&member.relative_path)?;
        let hashes = candidate.hashes.as_ref()?;
        hasher.update(member.ordinal.to_string().as_bytes());
        hasher.update(b"\0");
        hasher.update(member.role.as_db().as_bytes());
        hasher.update(b"\0");
        hasher.update(hashes.crc32.as_bytes());
        hasher.update(b"\0");
        hasher.update(hashes.md5.as_bytes());
        hasher.update(b"\0");
        hasher.update(hashes.sha1.as_bytes());
        hasher.update(b"\0");
    }
    Some(hex_bytes(&hasher.finalize()))
}

fn parse_cue_file(contents: &str) -> Result<Vec<String>, String> {
    let mut references = Vec::new();
    for line in without_utf8_bom(contents).lines() {
        let trimmed = line.trim();
        let Some(prefix) = trimmed.get(..4) else {
            continue;
        };
        if !prefix.eq_ignore_ascii_case("file")
            || trimmed
                .get(4..)
                .and_then(|remainder| remainder.chars().next())
                .is_some_and(|character| !character.is_whitespace())
        {
            continue;
        }
        let remainder = trimmed[4..].trim_start();
        let Some(quote) = remainder.chars().next() else {
            return Err("FILE directive has no filename".to_owned());
        };
        if quote == '"' || quote == '\'' {
            let mut value = String::new();
            let mut closed = false;
            for character in remainder[quote.len_utf8()..].chars() {
                if character == quote {
                    closed = true;
                    break;
                }
                value.push(character);
            }
            if !closed || value.is_empty() {
                return Err("FILE directive has an unterminated or empty filename".to_owned());
            }
            references.push(value);
        } else {
            let mut fields = remainder.split_whitespace();
            let Some(value) = fields.next() else {
                return Err("FILE directive has no filename".to_owned());
            };
            if value.is_empty() {
                return Err("FILE directive has an empty filename".to_owned());
            }
            references.push(value.to_owned());
        }
    }
    if references.is_empty() {
        return Err("CUE sheet contains no FILE directives".to_owned());
    }
    Ok(references)
}

fn parse_gdi_file(contents: &str) -> Result<Vec<String>, String> {
    let mut lines = without_utf8_bom(contents)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty());
    let track_count = lines
        .next()
        .ok_or_else(|| "GDI descriptor is empty".to_owned())?
        .parse::<usize>()
        .map_err(|_| "GDI track count is invalid".to_owned())?;
    // The declared count is untrusted input. The bounded descriptor read limits the number of
    // rows we can inspect, so do not use an attacker-controlled count as an allocation size.
    let mut references = Vec::new();
    for line in lines {
        if references.len() == track_count {
            break;
        }
        let fields = split_descriptor_fields(line)?;
        if fields.len() < 5 {
            return Err("GDI track line has too few fields".to_owned());
        }
        references.push(fields[4].clone());
    }
    if references.len() != track_count || references.is_empty() {
        return Err("GDI track count does not match its track lines".to_owned());
    }
    Ok(references)
}

fn split_descriptor_fields(line: &str) -> Result<Vec<String>, String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    for character in line.chars() {
        if let Some(expected) = quote {
            if character == expected {
                quote = None;
            } else {
                current.push(character);
            }
        } else if character == '"' || character == '\'' {
            quote = Some(character);
        } else if character.is_whitespace() {
            if !current.is_empty() {
                fields.push(std::mem::take(&mut current));
            }
        } else {
            current.push(character);
        }
    }
    if quote.is_some() {
        return Err("descriptor field has an unterminated quote".to_owned());
    }
    if !current.is_empty() {
        fields.push(current);
    }
    Ok(fields)
}

fn safe_relative_reference(descriptor_path: &str, raw: &str) -> Result<String, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("descriptor reference is empty".to_owned());
    }
    if raw.contains('\0')
        || raw.starts_with('/')
        || raw.starts_with('\\')
        || raw.as_bytes().get(1) == Some(&b':')
        || Path::new(raw).is_absolute()
    {
        return Err("absolute descriptor references are not allowed".to_owned());
    }
    let mut components = descriptor_path.split('/').collect::<Vec<_>>();
    components.pop();
    let normalized_raw = raw.replace('\\', "/");
    for component in normalized_raw.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                return Err("parent traversal in descriptor reference is not allowed".to_owned())
            }
            component => components.push(component),
        }
    }
    if components.is_empty() || components.iter().any(|component| component.is_empty()) {
        return Err("descriptor reference does not name a file".to_owned());
    }
    Ok(components.join("/"))
}

fn resolve_contained_path(
    root: &Path,
    canonical_root: Option<&Path>,
    relative_path: &str,
) -> Result<PathBuf, ContainedPathError> {
    let Some(canonical_root) = canonical_root else {
        return Err(ContainedPathError::Unsafe(
            "configured content root could not be canonicalized",
        ));
    };
    if relative_path.is_empty()
        || relative_path.starts_with('/')
        || relative_path.starts_with('\\')
        || relative_path.as_bytes().get(1) == Some(&b':')
        || Path::new(relative_path).is_absolute()
    {
        return Err(ContainedPathError::Unsafe(
            "absolute descriptor references are not allowed",
        ));
    }

    let mut path = root.to_path_buf();
    for component in relative_path.split('/') {
        if component.is_empty() || component == "." || component == ".." {
            return Err(ContainedPathError::Unsafe(
                "descriptor reference contains an unsafe path component",
            ));
        }
        path.push(component);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(ContainedPathError::Missing);
            }
            Err(error) => return Err(ContainedPathError::Io(error)),
        };
        if is_symlink_or_reparse_point(&metadata) {
            return Err(ContainedPathError::Unsafe(
                "symbolic links and reparse points are not followed for descriptor references",
            ));
        }
    }

    let canonical_path = match fs::canonicalize(&path) {
        Ok(path) => path,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(ContainedPathError::Missing);
        }
        Err(error) => return Err(ContainedPathError::Io(error)),
    };
    if !canonical_path.starts_with(canonical_root) {
        return Err(ContainedPathError::Unsafe(
            "descriptor reference escapes the configured content root",
        ));
    }
    Ok(canonical_path)
}

fn strip_utf8_bom(contents: String) -> String {
    if let Some(stripped) = contents.strip_prefix('\u{feff}') {
        stripped.to_owned()
    } else {
        contents
    }
}

fn is_symlink_or_reparse_point(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;

        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        metadata.file_type().is_symlink()
            || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

fn without_utf8_bom(contents: &str) -> &str {
    contents.strip_prefix('\u{feff}').unwrap_or(contents)
}

fn components_to_path(root: &Path, components: &[String]) -> PathBuf {
    components
        .iter()
        .fold(root.to_path_buf(), |path, component| path.join(component))
}

fn relative_path_from_components(components: &[String]) -> Option<String> {
    if components.is_empty() {
        None
    } else {
        Some(components.join("/"))
    }
}

fn modified_timestamp(metadata: &fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos().min(i64::MAX as u128) as i64)
        .unwrap_or_default()
}

fn unit_kind_for_format(format: ContentFormat) -> ContentUnitKind {
    match format {
        ContentFormat::Chd => ContentUnitKind::Chd,
        ContentFormat::Cue => ContentUnitKind::CueBin,
        ContentFormat::Gdi => ContentUnitKind::Gdi,
        ContentFormat::M3u => ContentUnitKind::M3u,
        ContentFormat::SingleFile => ContentUnitKind::SingleFile,
    }
}

fn local_title(relative_path: &str) -> String {
    Path::new(relative_path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.trim().is_empty())
        .unwrap_or(relative_path)
        .to_owned()
}

fn issue(
    root_id: Option<crate::domain::library::ContentRootId>,
    kind: ScanIssueKind,
    relative_path: Option<String>,
    related_path: Option<String>,
    detail: Option<String>,
) -> ScanIssue {
    ScanIssue {
        id: None,
        scan_run_id: None,
        root_id,
        kind,
        relative_path,
        related_path,
        detail,
        created_at: now_timestamp(),
    }
}

fn now_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or_default()
}

struct ProgressReporter {
    sink: Arc<dyn ScanEventSink>,
    last_emit: Mutex<Option<Instant>>,
}

impl ProgressReporter {
    fn new(sink: Arc<dyn ScanEventSink>) -> Self {
        Self {
            sink,
            last_emit: Mutex::new(None),
        }
    }

    fn emit(&self, run_id: ScanRunId, phase: ScanPhase, counters: ScanCounters, force: bool) {
        let mut last_emit = self
            .last_emit
            .lock()
            .expect("progress mutex is not poisoned");
        let should_emit = force
            || last_emit
                .as_ref()
                .is_none_or(|instant| instant.elapsed() >= Duration::from_millis(100));
        if should_emit {
            *last_emit = Some(Instant::now());
            self.sink.progress(ScanProgress {
                run_id,
                phase,
                counters,
            });
        }
    }
}

fn hash_file(root: &Path, candidate: &Candidate) -> Result<ContentHashes, io::Error> {
    let canonical_root = fs::canonicalize(root)?;
    let canonical_file = fs::canonicalize(&candidate.path)?;
    if !canonical_file.starts_with(&canonical_root) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "file escaped its configured content root",
        ));
    }
    let metadata_before = fs::symlink_metadata(&canonical_file)?;
    if is_symlink_or_reparse_point(&metadata_before) || !metadata_before.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "file is no longer a regular non-link file",
        ));
    }
    let mut file = File::open(&canonical_file)?;
    let mut crc = Crc32Hasher::new();
    let mut md5 = md5::Md5::new();
    let mut sha1 = Sha1::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        crc.update(&buffer[..read]);
        md5.update(&buffer[..read]);
        sha1.update(&buffer[..read]);
    }
    let metadata_after = fs::symlink_metadata(&canonical_file)?;
    if metadata_before.len() != metadata_after.len()
        || modified_timestamp(&metadata_before) != modified_timestamp(&metadata_after)
    {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "file changed while it was being hashed",
        ));
    }
    Ok(ContentHashes {
        crc32: format!("{:08x}", crc.finalize()),
        md5: hex_bytes(&md5.finalize()),
        sha1: hex_bytes(&sha1.finalize()),
    })
}

fn hex_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::{
        hash_file, parse_cue_file, parse_gdi_file, safe_relative_reference,
        split_descriptor_fields, Candidate, FileMetadata, ProgressReporter, ScanEventSink,
        ScanService, MAX_DESCRIPTOR_SIZE,
    };
    use crate::adapters::database::Database;
    use crate::domain::library::{
        ContentFileAvailability, ContentFileRole, ContentFormat, ContentRoot,
        ContentRootAvailability, ContentUnitAvailability, ContentUnitKind, GameId, ScanCounters,
        ScanIssueKind, ScanPhase, ScanProgress, ScanRunId, ScanRunState, ScanSummary,
    };
    use crate::domain::system::{SystemCatalog, SystemId};
    use crate::repositories::library::LibraryRepository;
    use std::collections::BTreeSet;
    use std::fs::{self, File};
    use std::io::Write;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use tempfile::{tempdir, TempDir};

    #[derive(Default)]
    struct CollectingSink {
        progress: Mutex<Vec<ScanProgress>>,
        completed: Mutex<Vec<ScanSummary>>,
    }

    impl ScanEventSink for CollectingSink {
        fn progress(&self, progress: ScanProgress) {
            self.progress.lock().unwrap().push(progress);
        }

        fn completed(&self, summary: ScanSummary) {
            self.completed.lock().unwrap().push(summary);
        }
    }

    struct TestContext {
        _directory: TempDir,
        _database: Database,
        repository: LibraryRepository,
        scanner: ScanService,
        root: ContentRoot,
        sink: Arc<CollectingSink>,
    }

    async fn test_context(system_hint: Option<SystemId>) -> TestContext {
        let directory = tempdir().expect("temporary library fixture should be created");
        let root_path = directory.path().join("library");
        fs::create_dir_all(&root_path).expect("temporary library root should be created");
        let database = Database::open(directory.path().join("database.sqlite3"))
            .await
            .expect("test database should open");
        let repository = LibraryRepository::new(database.pool().clone());
        let root = repository
            .upsert_external_root(root_path.to_str().unwrap(), system_hint)
            .await
            .expect("test content root should persist");
        let sink = Arc::new(CollectingSink::default());
        let scanner = ScanService::new(repository.clone(), SystemCatalog::v1(), sink.clone());
        TestContext {
            _directory: directory,
            _database: database,
            repository,
            scanner,
            root,
            sink,
        }
    }

    async fn reopen_persistence_and_scanner(
        context: &TestContext,
    ) -> (Database, LibraryRepository, ScanService) {
        context._database.pool().close().await;
        let database = Database::open(context._directory.path().join("database.sqlite3"))
            .await
            .expect("test database should reopen");
        let repository = LibraryRepository::new(database.pool().clone());
        let scanner = ScanService::new(
            repository.clone(),
            SystemCatalog::v1(),
            Arc::new(CollectingSink::default()),
        );
        (database, repository, scanner)
    }

    fn write_fixture(root: &ContentRoot, relative_path: &str, contents: &[u8]) {
        let path = PathBuf::from(&root.path).join(relative_path);
        fs::create_dir_all(path.parent().unwrap()).expect("fixture parent should exist");
        fs::write(path, contents).expect("fixture file should be written");
    }

    async fn perform_contested_move(
        context: &TestContext,
        first: &str,
        second: &str,
    ) -> (BTreeSet<GameId>, GameId) {
        for prefix in [first, second] {
            write_fixture(&context.root, &format!("{prefix}.chd"), &[7, 8, 9]);
            let playlist = format!("{prefix}.chd\n");
            write_fixture(&context.root, &format!("{prefix}.m3u"), playlist.as_bytes());
            context.scanner.scan_once().await.unwrap();
        }

        let before = context.repository.get_library_snapshot().await.unwrap();
        assert_eq!(before.games.len(), 2);
        let predecessor_game_ids: BTreeSet<_> =
            before.games.iter().map(|game| game.game.id).collect();
        let predecessor_file_ids: BTreeSet<_> = before
            .games
            .iter()
            .flat_map(|game| &game.content_units)
            .flat_map(|unit| &unit.files)
            .filter(|member| member.file.relative_path.ends_with(".chd"))
            .map(|member| member.file.id)
            .collect();
        assert_eq!(predecessor_file_ids.len(), 2);

        for prefix in [first, second] {
            fs::remove_file(PathBuf::from(&context.root.path).join(format!("{prefix}.m3u")))
                .unwrap();
            fs::remove_file(PathBuf::from(&context.root.path).join(format!("{prefix}.chd")))
                .unwrap();
        }
        write_fixture(&context.root, "moved.chd", &[7, 8, 9]);
        context.scanner.scan_once().await.unwrap();

        let after = context.repository.get_library_snapshot().await.unwrap();
        assert_eq!(after.games.len(), 3);
        let available: Vec<_> = after
            .games
            .iter()
            .filter(|game| {
                game.game.availability == crate::domain::library::GameAvailability::Available
            })
            .collect();
        assert_eq!(available.len(), 1);
        let moved_game_id = available[0].game.id;
        assert!(!predecessor_game_ids.contains(&moved_game_id));
        assert_eq!(available[0].content_units.len(), 1);
        assert!(!predecessor_file_ids.contains(&available[0].content_units[0].files[0].file.id));
        assert!(context
            .repository
            .list_latest_scan_issues()
            .await
            .unwrap()
            .iter()
            .any(|issue| issue.kind == ScanIssueKind::AmbiguousReconciliation));
        (predecessor_game_ids, moved_game_id)
    }

    #[test]
    fn cue_parser_accepts_supported_file_forms() {
        let cases = [
            (
                "quoted",
                "FILE \"track 01.bin\" BINARY\nFILE 'track 02.bin' BINARY",
                vec!["track 01.bin", "track 02.bin"],
            ),
            ("unquoted", "FILE game.bin BINARY", vec!["game.bin"]),
            (
                "windows separators",
                r#"FILE "sub\game.bin" BINARY"#,
                vec!["sub\\game.bin"],
            ),
            (
                "utf8 bom",
                "\u{feff}FILE \"game.bin\" BINARY",
                vec!["game.bin"],
            ),
            (
                "crlf",
                "FILE game.bin BINARY\r\nFILE second.bin BINARY\r\n",
                vec!["game.bin", "second.bin"],
            ),
        ];
        for (name, contents, expected) in cases {
            assert_eq!(parse_cue_file(contents).unwrap(), expected, "{name}");
        }
        assert!(parse_cue_file("FILEFOO \"game.bin\" BINARY").is_err());
    }

    #[test]
    fn gdi_parser_accepts_bom_crlf_and_trailing_non_track_text() {
        let cases = [
            (
                "normal",
                "2\n1 0 4 2352 \"track01.bin\" 0\n2 45000 0 2352 \"track02.bin\" 0",
            ),
            (
                "bom and crlf",
                "\u{feff}2\r\n1 0 4 2352 \"track01.bin\" 0\r\n2 45000 0 2352 \"track02.bin\" 0\r\n",
            ),
            (
                "trailing comment",
                "2\n1 0 4 2352 \"track01.bin\" 0\n2 45000 0 2352 \"track02.bin\" 0\n// generated by tool",
            ),
        ];
        for (name, contents) in cases {
            assert_eq!(
                parse_gdi_file(contents).unwrap(),
                vec!["track01.bin", "track02.bin"],
                "{name}"
            );
        }
        assert!(parse_gdi_file("18446744073709551615").is_err());
    }

    #[test]
    fn descriptor_reference_rejects_absolute_and_parent_paths() {
        assert!(safe_relative_reference("disc/game.cue", "../track.bin").is_err());
        assert!(safe_relative_reference("disc/game.cue", "/tmp/track.bin").is_err());
        assert_eq!(
            safe_relative_reference("disc/game.cue", "tracks/track.bin").unwrap(),
            "disc/tracks/track.bin"
        );
    }

    #[test]
    fn descriptor_field_splitter_keeps_spaces_inside_quotes() {
        assert_eq!(
            split_descriptor_fields(r#"1 0 4 2352 "track one.bin" 0"#).unwrap(),
            vec!["1", "0", "4", "2352", "track one.bin", "0"]
        );
    }

    #[tokio::test]
    async fn scanner_persists_hashes_recursively_and_is_idempotent() {
        let context = test_context(Some(SystemId::Nes)).await;
        write_fixture(&context.root, "nested/Game.NES", &[1, 2, 3]);
        write_fixture(&context.root, "nested/readme.txt", b"ignored");

        let first = context
            .scanner
            .scan_once()
            .await
            .expect("first scan should complete");
        assert_eq!(first.state, ScanRunState::Completed);
        assert_eq!(first.counters.files_discovered, 1);
        assert_eq!(first.counters.files_hashed, 1);

        let first_snapshot = context
            .repository
            .get_library_snapshot()
            .await
            .expect("first snapshot should load");
        assert_eq!(first_snapshot.games.len(), 1);
        let first_game = &first_snapshot.games[0];
        assert_eq!(first_game.game.system_id, SystemId::Nes);
        assert_eq!(first_game.game.local_title, "Game");
        assert_eq!(first_game.content_units.len(), 1);
        assert_eq!(
            first_game.content_units[0].kind,
            ContentUnitKind::SingleFile
        );
        let first_file = &first_game.content_units[0].files[0].file;
        assert_eq!(first_file.relative_path, "nested/Game.NES");
        assert_eq!(first_file.crc32.as_deref(), Some("55bc801d"));
        assert_eq!(
            first_file.md5.as_deref(),
            Some("5289df737df57326fcdd22597afb1fac")
        );
        assert_eq!(
            first_file.sha1.as_deref(),
            Some("7037807198c22a7d2b0807371d763779a84fdfcf")
        );

        context
            .scanner
            .scan_once()
            .await
            .expect("unchanged second scan should complete");
        let second_snapshot = context
            .repository
            .get_library_snapshot()
            .await
            .expect("second snapshot should load");
        assert_eq!(
            second_snapshot.games[0].game.id,
            first_snapshot.games[0].game.id
        );
        assert_eq!(
            second_snapshot.games[0].content_units[0].id,
            first_snapshot.games[0].content_units[0].id
        );
        assert_eq!(
            second_snapshot.games[0].content_units[0].files[0].file.id,
            first_file.id
        );

        write_fixture(&context.root, "nested/Game.NES", &[4, 5, 6]);
        context
            .scanner
            .scan_once()
            .await
            .expect("changed content scan should complete");
        let changed_snapshot = context.repository.get_library_snapshot().await.unwrap();
        assert_eq!(changed_snapshot.games.len(), 1);
        assert_eq!(
            changed_snapshot.games[0].game.id,
            first_snapshot.games[0].game.id
        );
        assert_eq!(
            changed_snapshot.games[0].content_units[0].id,
            first_snapshot.games[0].content_units[0].id
        );
        assert_ne!(
            changed_snapshot.games[0].content_units[0].files[0]
                .file
                .sha1,
            first_file.sha1
        );

        let progress = context.sink.progress.lock().unwrap();
        let phases: Vec<_> = progress.iter().map(|event| event.phase).collect();
        let phase_position = |phase| phases.iter().position(|candidate| *candidate == phase);
        let discovery = phase_position(ScanPhase::Discovery).unwrap();
        let relationships = phase_position(ScanPhase::RelationshipResolution).unwrap();
        let hashing = phase_position(ScanPhase::Hashing).unwrap();
        let reconciliation = phase_position(ScanPhase::Reconciliation).unwrap();
        let completed = phase_position(ScanPhase::Completed).unwrap();
        assert!(discovery < relationships);
        assert!(relationships < hashing);
        assert!(hashing < reconciliation);
        assert!(reconciliation < completed);
        assert_eq!(context.sink.completed.lock().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn cue_bin_relationship_preserves_track_order_without_duplicate_units() {
        let context = test_context(Some(SystemId::PlayStation)).await;
        write_fixture(
            &context.root,
            "disc/game.cue",
            b"FILE \"track 01.bin\" BINARY\nTRACK 01 MODE1/2352\nFILE 'track 02.bin' BINARY\nTRACK 02 AUDIO\n",
        );
        write_fixture(&context.root, "disc/track 01.bin", &[1, 2]);
        write_fixture(&context.root, "disc/track 02.bin", &[3, 4]);

        context
            .scanner
            .scan_once()
            .await
            .expect("CUE scan should complete");
        let snapshot = context.repository.get_library_snapshot().await.unwrap();
        assert_eq!(snapshot.games.len(), 1);
        let unit = &snapshot.games[0].content_units[0];
        assert_eq!(unit.kind, ContentUnitKind::CueBin);
        assert_eq!(unit.files.len(), 3);
        assert_eq!(unit.files[0].role, ContentFileRole::Descriptor);
        assert_eq!(unit.files[1].role, ContentFileRole::Track);
        assert_eq!(unit.files[2].role, ContentFileRole::Track);
        assert_eq!(unit.files[1].file.relative_path, "disc/track 01.bin");
        assert_eq!(unit.files[2].file.relative_path, "disc/track 02.bin");
    }

    #[tokio::test]
    async fn unquoted_cue_file_produces_an_available_ordered_unit() {
        let context = test_context(Some(SystemId::PlayStation)).await;
        write_fixture(
            &context.root,
            "disc/game.cue",
            b"FILE track02.bin BINARY\r\nFILE track01.bin BINARY\r\n",
        );
        write_fixture(&context.root, "disc/track02.bin", &[2]);
        write_fixture(&context.root, "disc/track01.bin", &[1]);

        context.scanner.scan_once().await.unwrap();
        let snapshot = context.repository.get_library_snapshot().await.unwrap();
        assert_eq!(snapshot.games.len(), 1);
        let unit = &snapshot.games[0].content_units[0];
        assert_eq!(unit.kind, ContentUnitKind::CueBin);
        assert_eq!(unit.availability, ContentUnitAvailability::Available);
        assert_eq!(unit.files[1].file.relative_path, "disc/track02.bin");
        assert_eq!(unit.files[2].file.relative_path, "disc/track01.bin");
    }

    #[tokio::test]
    async fn chd_is_a_single_file_unit_with_an_explicit_kind() {
        let context = test_context(Some(SystemId::PlayStation)).await;
        write_fixture(&context.root, "image.ChD", &[7, 8, 9]);

        context.scanner.scan_once().await.unwrap();
        let snapshot = context.repository.get_library_snapshot().await.unwrap();
        assert_eq!(snapshot.games.len(), 1);
        let unit = &snapshot.games[0].content_units[0];
        assert_eq!(unit.kind, ContentUnitKind::Chd);
        assert_eq!(unit.files.len(), 1);
        assert_eq!(unit.files[0].role, ContentFileRole::Standalone);
    }

    #[tokio::test]
    async fn gdi_relationship_accepts_non_standalone_track_extensions() {
        let context = test_context(Some(SystemId::SegaDreamcast)).await;
        write_fixture(
            &context.root,
            "game/game.gdi",
            b"2\n1 0 4 2352 \"track01.bin\" 0\n2 45000 0 2352 \"track02.bin\" 0\n",
        );
        write_fixture(&context.root, "game/track01.bin", &[1, 2, 3]);
        write_fixture(&context.root, "game/track02.bin", &[4, 5, 6]);

        context
            .scanner
            .scan_once()
            .await
            .expect("GDI scan should complete");
        let snapshot = context.repository.get_library_snapshot().await.unwrap();
        assert_eq!(snapshot.games.len(), 1);
        let unit = &snapshot.games[0].content_units[0];
        assert_eq!(unit.kind, ContentUnitKind::Gdi);
        assert_eq!(unit.files.len(), 3);
        assert_eq!(unit.files[1].file.relative_path, "game/track01.bin");
        assert_eq!(unit.files[2].file.relative_path, "game/track02.bin");
        assert!(context
            .repository
            .list_latest_scan_issues()
            .await
            .unwrap()
            .iter()
            .all(|issue| issue.kind != ScanIssueKind::IncompatibleSystemHint));
    }

    #[tokio::test]
    async fn m3u_preserves_order_and_owns_referenced_disc_content() {
        let context = test_context(Some(SystemId::PlayStation)).await;
        write_fixture(
            &context.root,
            "collection.m3u",
            b"#EXTM3U\n\n# disc order\ndisc-2.chd\ndisc-1.chd\n",
        );
        write_fixture(&context.root, "disc-1.chd", &[1]);
        write_fixture(&context.root, "disc-2.chd", &[2]);

        context
            .scanner
            .scan_once()
            .await
            .expect("M3U scan should complete");
        let snapshot = context.repository.get_library_snapshot().await.unwrap();
        assert_eq!(snapshot.games.len(), 1);
        assert_eq!(snapshot.games[0].content_units.len(), 1);
        let unit = &snapshot.games[0].content_units[0];
        assert_eq!(unit.kind, ContentUnitKind::M3u);
        assert_eq!(unit.files.len(), 3);
        assert_eq!(unit.files[0].role, ContentFileRole::Playlist);
        assert_eq!(unit.files[1].file.relative_path, "disc-2.chd");
        assert_eq!(unit.files[2].file.relative_path, "disc-1.chd");
        assert_eq!(unit.files[1].ordinal, 1);
        assert_eq!(unit.files[2].ordinal, 2);
    }

    #[tokio::test]
    async fn adding_one_disc_m3u_preserves_the_standalone_game_identity() {
        let context = test_context(Some(SystemId::PlayStation)).await;
        write_fixture(&context.root, "disc.chd", &[1, 2, 3]);

        context.scanner.scan_once().await.unwrap();
        let before = context.repository.get_library_snapshot().await.unwrap();
        assert_eq!(before.games.len(), 1);
        let original_game_id = before.games[0].game.id;
        let original_unit_id = before.games[0].content_units[0].id;
        let original_file_id = before.games[0].content_units[0].files[0].file.id;
        let original_hashes = (
            before.games[0].content_units[0].files[0].file.crc32.clone(),
            before.games[0].content_units[0].files[0].file.md5.clone(),
            before.games[0].content_units[0].files[0].file.sha1.clone(),
        );
        let original_fingerprint = before.games[0].content_units[0].fingerprint.clone();

        write_fixture(&context.root, "game.m3u", b"disc.chd\n");
        context.scanner.scan_once().await.unwrap();

        let after = context.repository.get_library_snapshot().await.unwrap();
        let available_games: Vec<_> = after
            .games
            .iter()
            .filter(|game| {
                game.game.availability == crate::domain::library::GameAvailability::Available
            })
            .collect();
        assert_eq!(after.games.len(), 1);
        assert_eq!(available_games.len(), 1);
        assert_eq!(available_games[0].game.id, original_game_id);
        assert_eq!(available_games[0].content_units.len(), 2);
        let historical_unit = available_games[0]
            .content_units
            .iter()
            .find(|unit| unit.id == original_unit_id)
            .unwrap();
        assert_eq!(
            historical_unit.availability,
            ContentUnitAvailability::Incomplete
        );
        assert_eq!(historical_unit.fingerprint, original_fingerprint);
        let playlist_unit = available_games[0]
            .content_units
            .iter()
            .find(|unit| unit.kind == ContentUnitKind::M3u)
            .unwrap();
        let absorbed_disc = playlist_unit
            .files
            .iter()
            .find(|member| member.file.relative_path == "disc.chd")
            .unwrap();
        assert_eq!(absorbed_disc.file.id, original_file_id);
        assert_eq!(
            (
                absorbed_disc.file.crc32.clone(),
                absorbed_disc.file.md5.clone(),
                absorbed_disc.file.sha1.clone(),
            ),
            original_hashes
        );
    }

    #[tokio::test]
    async fn adding_m3u_for_multiple_units_under_one_game_preserves_that_game() {
        let context = test_context(Some(SystemId::PlayStation)).await;
        write_fixture(&context.root, "disc-a.chd", &[1, 2, 3]);
        write_fixture(&context.root, "disc-b.chd", &[1, 2, 3]);
        context.scanner.scan_once().await.unwrap();

        let before = context.repository.get_library_snapshot().await.unwrap();
        assert_eq!(before.games.len(), 1);
        assert_eq!(before.games[0].content_units.len(), 2);
        let original_game_id = before.games[0].game.id;

        write_fixture(&context.root, "game.m3u", b"disc-a.chd\ndisc-b.chd\n");
        context.scanner.scan_once().await.unwrap();

        let after = context.repository.get_library_snapshot().await.unwrap();
        assert_eq!(after.games.len(), 1);
        assert_eq!(after.games[0].game.id, original_game_id);
        assert!(after.games[0]
            .content_units
            .iter()
            .any(|unit| unit.kind == ContentUnitKind::M3u));
        assert!(context
            .repository
            .list_latest_scan_issues()
            .await
            .unwrap()
            .iter()
            .all(|issue| issue.kind != ScanIssueKind::AmbiguousReconciliation));
    }

    #[tokio::test]
    async fn adding_m3u_for_different_predecessor_games_refuses_identity_transfer() {
        let context = test_context(Some(SystemId::PlayStation)).await;
        write_fixture(&context.root, "disc-a.chd", &[1]);
        write_fixture(&context.root, "disc-b.chd", &[2]);
        context.scanner.scan_once().await.unwrap();

        let before = context.repository.get_library_snapshot().await.unwrap();
        assert_eq!(before.games.len(), 2);
        let predecessor_ids: BTreeSet<_> = before.games.iter().map(|game| game.game.id).collect();

        write_fixture(&context.root, "game.m3u", b"disc-a.chd\ndisc-b.chd\n");
        context.scanner.scan_once().await.unwrap();

        let after = context.repository.get_library_snapshot().await.unwrap();
        assert_eq!(after.games.len(), 3);
        assert!(predecessor_ids
            .iter()
            .all(|game_id| after.games.iter().any(|game| game.game.id == *game_id)));
        let available: Vec<_> = after
            .games
            .iter()
            .filter(|game| {
                game.game.availability == crate::domain::library::GameAvailability::Available
            })
            .collect();
        assert_eq!(available.len(), 1);
        assert!(!predecessor_ids.contains(&available[0].game.id));
        assert_eq!(available[0].content_units.len(), 1);
        assert_eq!(available[0].content_units[0].kind, ContentUnitKind::M3u);
        assert!(context
            .repository
            .list_latest_scan_issues()
            .await
            .unwrap()
            .iter()
            .any(|issue| {
                issue.kind == ScanIssueKind::AmbiguousReconciliation
                    && issue.relative_path.as_deref() == Some("game.m3u")
            }));

        let decided_game_id = available[0].game.id;
        context.scanner.scan_once().await.unwrap();
        let repeated = context.repository.get_library_snapshot().await.unwrap();
        assert_eq!(repeated.games.len(), 3);
        assert_eq!(
            repeated
                .games
                .iter()
                .find(|game| {
                    game.game.availability == crate::domain::library::GameAvailability::Available
                })
                .unwrap()
                .game
                .id,
            decided_game_id
        );
        assert!(context
            .repository
            .list_latest_scan_issues()
            .await
            .unwrap()
            .iter()
            .all(|issue| issue.kind != ScanIssueKind::AmbiguousReconciliation));
    }

    #[tokio::test]
    async fn new_m3u_without_predecessor_uses_normal_game_creation() {
        let context = test_context(Some(SystemId::PlayStation)).await;
        write_fixture(&context.root, "game.m3u", b"disc.chd\n");
        write_fixture(&context.root, "disc.chd", &[1, 2, 3]);

        context.scanner.scan_once().await.unwrap();

        let snapshot = context.repository.get_library_snapshot().await.unwrap();
        assert_eq!(snapshot.games.len(), 1);
        assert_eq!(snapshot.games[0].content_units.len(), 1);
        assert_eq!(
            snapshot.games[0].content_units[0].kind,
            ContentUnitKind::M3u
        );
        assert!(context
            .repository
            .list_latest_scan_issues()
            .await
            .unwrap()
            .iter()
            .all(|issue| issue.kind != ScanIssueKind::AmbiguousReconciliation));
    }

    #[tokio::test]
    async fn successful_m3u_transfer_is_stable_across_repeated_scan_and_restart() {
        let context = test_context(Some(SystemId::PlayStation)).await;
        write_fixture(&context.root, "disc.chd", &[1, 2, 3]);
        context.scanner.scan_once().await.unwrap();
        let original_game_id = context
            .repository
            .get_library_snapshot()
            .await
            .unwrap()
            .games[0]
            .game
            .id;
        write_fixture(&context.root, "game.m3u", b"disc.chd\n");
        context.scanner.scan_once().await.unwrap();
        context.scanner.scan_once().await.unwrap();

        let before_restart = context.repository.get_library_snapshot().await.unwrap();
        assert_eq!(before_restart.games.len(), 1);
        assert_eq!(before_restart.games[0].game.id, original_game_id);
        let unit_ids: BTreeSet<_> = before_restart.games[0]
            .content_units
            .iter()
            .map(|unit| unit.id)
            .collect();

        let (_database, repository, scanner) = reopen_persistence_and_scanner(&context).await;
        scanner.scan_once().await.unwrap();
        let after_restart = repository.get_library_snapshot().await.unwrap();
        assert_eq!(after_restart.games.len(), 1);
        assert_eq!(after_restart.games[0].game.id, original_game_id);
        assert_eq!(
            after_restart.games[0]
                .content_units
                .iter()
                .map(|unit| unit.id)
                .collect::<BTreeSet<_>>(),
            unit_ids
        );
    }

    #[tokio::test]
    async fn removing_and_readding_transferred_m3u_keeps_the_game_identity() {
        let context = test_context(Some(SystemId::PlayStation)).await;
        write_fixture(&context.root, "disc.chd", &[1, 2, 3]);
        context.scanner.scan_once().await.unwrap();
        let original_game_id = context
            .repository
            .get_library_snapshot()
            .await
            .unwrap()
            .games[0]
            .game
            .id;
        write_fixture(&context.root, "game.m3u", b"disc.chd\n");
        context.scanner.scan_once().await.unwrap();

        fs::remove_file(PathBuf::from(&context.root.path).join("game.m3u")).unwrap();
        context.scanner.scan_once().await.unwrap();
        let without_playlist = context.repository.get_library_snapshot().await.unwrap();
        assert_eq!(without_playlist.games.len(), 1);
        assert_eq!(without_playlist.games[0].game.id, original_game_id);
        assert!(without_playlist.games[0].content_units.iter().any(|unit| {
            unit.kind == ContentUnitKind::Chd
                && unit.availability == ContentUnitAvailability::Available
        }));

        write_fixture(&context.root, "game.m3u", b"disc.chd\n");
        context.scanner.scan_once().await.unwrap();
        let readded = context.repository.get_library_snapshot().await.unwrap();
        assert_eq!(readded.games.len(), 1);
        assert_eq!(readded.games[0].game.id, original_game_id);
        assert!(readded.games[0].content_units.iter().any(|unit| {
            unit.kind == ContentUnitKind::M3u
                && unit.availability == ContentUnitAvailability::Available
        }));
    }

    #[tokio::test]
    async fn m3u_resolves_descriptor_disc_dependencies() {
        let context = test_context(Some(SystemId::PlayStation)).await;
        write_fixture(&context.root, "collection.m3u", b"disc.cue\n");
        write_fixture(
            &context.root,
            "disc.cue",
            b"FILE \"track.bin\" BINARY\nTRACK 01 MODE1/2352\n",
        );
        write_fixture(&context.root, "track.bin", &[1, 2, 3]);

        context.scanner.scan_once().await.unwrap();
        let snapshot = context.repository.get_library_snapshot().await.unwrap();
        assert_eq!(snapshot.games.len(), 1);
        assert_eq!(snapshot.games[0].content_units.len(), 1);
        let unit = &snapshot.games[0].content_units[0];
        assert_eq!(unit.kind, ContentUnitKind::M3u);
        assert_eq!(unit.files.len(), 3);
        assert_eq!(unit.files[0].role, ContentFileRole::Playlist);
        assert_eq!(unit.files[1].role, ContentFileRole::DiscDescriptor);
        assert_eq!(unit.files[2].role, ContentFileRole::DiscTrack);
    }

    #[tokio::test]
    async fn m3u_resolves_gdi_disc_dependencies() {
        let context = test_context(Some(SystemId::SegaDreamcast)).await;
        write_fixture(&context.root, "collection.m3u", b"disc.gdi\n");
        write_fixture(
            &context.root,
            "disc.gdi",
            b"1\n1 0 4 2352 \"track.bin\" 0\n",
        );
        write_fixture(&context.root, "track.bin", &[1, 2, 3]);

        context.scanner.scan_once().await.unwrap();
        let snapshot = context.repository.get_library_snapshot().await.unwrap();
        assert_eq!(snapshot.games.len(), 1);
        assert_eq!(snapshot.games[0].content_units.len(), 1);
        let unit = &snapshot.games[0].content_units[0];
        assert_eq!(unit.kind, ContentUnitKind::M3u);
        assert_eq!(unit.files.len(), 3);
        assert_eq!(unit.files[0].role, ContentFileRole::Playlist);
        assert_eq!(unit.files[1].role, ContentFileRole::DiscDescriptor);
        assert_eq!(unit.files[2].role, ContentFileRole::DiscTrack);
    }

    #[tokio::test]
    async fn gdi_missing_and_unsafe_members_are_incomplete_and_reported() {
        let context = test_context(Some(SystemId::SegaDreamcast)).await;
        write_fixture(
            &context.root,
            "broken/game.gdi",
            b"3\n1 0 4 2352 \"present.bin\" 0\n2 45000 0 2352 \"missing.bin\" 0\n3 90000 0 2352 \"../escape.bin\" 0\n",
        );
        write_fixture(&context.root, "broken/present.bin", &[1, 2, 3]);

        context.scanner.scan_once().await.unwrap();
        let snapshot = context.repository.get_library_snapshot().await.unwrap();
        assert_eq!(snapshot.games.len(), 1);
        assert_eq!(
            snapshot.games[0].content_units[0].availability,
            crate::domain::library::ContentUnitAvailability::Incomplete
        );
        let issues = context.repository.list_latest_scan_issues().await.unwrap();
        assert!(issues
            .iter()
            .any(|issue| issue.kind == ScanIssueKind::MissingReferencedFile));
        assert!(issues
            .iter()
            .any(|issue| issue.kind == ScanIssueKind::UnsafeDescriptorReference));
    }

    #[tokio::test]
    async fn ambiguous_and_incompatible_systems_are_issues_not_guesses() {
        let ambiguous = test_context(None).await;
        write_fixture(&ambiguous.root, "ambiguous.bin", &[9]);
        ambiguous.scanner.scan_once().await.unwrap();
        assert!(ambiguous
            .repository
            .get_library_snapshot()
            .await
            .unwrap()
            .games
            .is_empty());
        assert!(ambiguous
            .repository
            .list_latest_scan_issues()
            .await
            .unwrap()
            .iter()
            .any(|issue| issue.kind == ScanIssueKind::AmbiguousSystem));

        let incompatible = test_context(Some(SystemId::Nes)).await;
        write_fixture(&incompatible.root, "wrong.sfc", &[9]);
        incompatible.scanner.scan_once().await.unwrap();
        assert!(incompatible
            .repository
            .list_latest_scan_issues()
            .await
            .unwrap()
            .iter()
            .any(|issue| issue.kind == ScanIssueKind::IncompatibleSystemHint));
        assert!(incompatible
            .repository
            .get_library_snapshot()
            .await
            .unwrap()
            .games
            .is_empty());
    }

    #[tokio::test]
    async fn missing_descriptor_members_are_incomplete_and_reported() {
        let context = test_context(Some(SystemId::PlayStation)).await;
        write_fixture(
            &context.root,
            "broken.cue",
            b"FILE \"present.bin\" BINARY\nFILE \"missing.bin\" BINARY\nFILE \"../escape.bin\" BINARY\n",
        );
        write_fixture(&context.root, "present.bin", &[1]);

        context.scanner.scan_once().await.unwrap();
        let snapshot = context.repository.get_library_snapshot().await.unwrap();
        assert_eq!(snapshot.games.len(), 1);
        assert_eq!(
            snapshot.games[0].content_units[0].availability,
            crate::domain::library::ContentUnitAvailability::Incomplete
        );
        let issues = context.repository.list_latest_scan_issues().await.unwrap();
        assert!(issues
            .iter()
            .any(|issue| issue.kind == ScanIssueKind::MissingReferencedFile));
        assert!(issues
            .iter()
            .any(|issue| issue.kind == ScanIssueKind::UnsafeDescriptorReference));
    }

    #[tokio::test]
    async fn m3u_reference_cycles_and_unsafe_entries_are_reported() {
        let context = test_context(Some(SystemId::PlayStation)).await;
        write_fixture(&context.root, "a.m3u", b"#EXTM3U\nb.m3u\n../escape.chd\n");
        write_fixture(&context.root, "b.m3u", b"a.m3u\n");

        context.scanner.scan_once().await.unwrap();
        let snapshot = context.repository.get_library_snapshot().await.unwrap();
        assert_eq!(snapshot.games.len(), 1);
        assert_eq!(snapshot.games[0].content_units.len(), 1);
        assert_eq!(
            snapshot.games[0].content_units[0].availability,
            crate::domain::library::ContentUnitAvailability::Incomplete
        );
        let issues = context.repository.list_latest_scan_issues().await.unwrap();
        assert!(issues
            .iter()
            .any(|issue| issue.kind == ScanIssueKind::ReferenceCycle));
        assert!(issues
            .iter()
            .any(|issue| issue.kind == ScanIssueKind::UnsafeDescriptorReference));
    }

    #[tokio::test]
    async fn independent_m3u_cycles_are_all_incomplete_and_never_standalone() {
        let context = test_context(Some(SystemId::PlayStation)).await;
        write_fixture(&context.root, "a.m3u", b"b.m3u\n");
        write_fixture(&context.root, "b.m3u", b"a.m3u\n");
        write_fixture(&context.root, "c.m3u", b"d.m3u\n");
        write_fixture(&context.root, "d.m3u", b"c.m3u\n");

        context.scanner.scan_once().await.unwrap();
        let snapshot = context.repository.get_library_snapshot().await.unwrap();
        let units: Vec<_> = snapshot
            .games
            .iter()
            .flat_map(|game| game.content_units.iter())
            .collect();
        assert_eq!(units.len(), 2);
        assert!(units.iter().all(|unit| {
            unit.kind == ContentUnitKind::M3u
                && unit.availability != ContentUnitAvailability::Available
                && unit
                    .files
                    .iter()
                    .all(|member| member.role != ContentFileRole::Standalone)
        }));
        let issues = context.repository.list_latest_scan_issues().await.unwrap();
        assert_eq!(
            issues
                .iter()
                .filter(|issue| issue.kind == ScanIssueKind::ReferenceCycle)
                .count(),
            2
        );

        context.scanner.scan_once().await.unwrap();
        let repeated = context.repository.get_library_snapshot().await.unwrap();
        assert_eq!(repeated.games.len(), snapshot.games.len());
        assert_eq!(repeated.games[0].game.id, snapshot.games[0].game.id);
        assert_eq!(
            repeated.games[0].content_units[0].id,
            snapshot.games[0].content_units[0].id
        );
        assert_eq!(
            repeated.games[1].content_units[0].files[1].file.id,
            snapshot.games[1].content_units[0].files[1].file.id
        );
    }

    #[tokio::test]
    async fn m3u_missing_member_is_incomplete_and_reported() {
        let context = test_context(Some(SystemId::PlayStation)).await;
        write_fixture(
            &context.root,
            "collection.m3u",
            b"#EXTM3U\ndisc.chd\nmissing.chd\n",
        );
        write_fixture(&context.root, "disc.chd", &[1, 2, 3]);

        context.scanner.scan_once().await.unwrap();
        let snapshot = context.repository.get_library_snapshot().await.unwrap();
        assert_eq!(snapshot.games.len(), 1);
        assert_eq!(
            snapshot.games[0].content_units[0].availability,
            crate::domain::library::ContentUnitAvailability::Incomplete
        );
        let issues = context.repository.list_latest_scan_issues().await.unwrap();
        assert!(issues
            .iter()
            .any(|issue| issue.kind == ScanIssueKind::MissingReferencedFile));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn symlink_escape_and_loop_are_skipped_as_unsafe_paths() {
        use std::os::unix::fs::symlink;

        let context = test_context(Some(SystemId::Nes)).await;
        let outside = context._directory.path().join("outside.nes");
        fs::write(&outside, b"outside").unwrap();
        symlink(
            &outside,
            PathBuf::from(&context.root.path).join("escape.nes"),
        )
        .unwrap();
        symlink(
            &context.root.path,
            PathBuf::from(&context.root.path).join("loop"),
        )
        .unwrap();

        context.scanner.scan_once().await.unwrap();
        assert!(context
            .repository
            .get_library_snapshot()
            .await
            .unwrap()
            .games
            .is_empty());
        assert!(context
            .repository
            .list_latest_scan_issues()
            .await
            .unwrap()
            .iter()
            .any(|issue| issue.kind == ScanIssueKind::UnsafePath));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn descriptor_member_symlink_escape_is_rejected_before_reading() {
        use std::os::unix::fs::symlink;

        let context = test_context(Some(SystemId::PlayStation)).await;
        let outside = context._directory.path().join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::write(
            outside.join("secret.cue"),
            b"FILE \"outside.bin\" BINARY\nTRACK 01 MODE1/2352\n",
        )
        .unwrap();
        fs::write(outside.join("outside.bin"), [9_u8]).unwrap();
        symlink(&outside, PathBuf::from(&context.root.path).join("link")).unwrap();
        write_fixture(&context.root, "list.m3u", b"link/secret.cue\n");

        context.scanner.scan_once().await.unwrap();
        let issues = context.repository.list_latest_scan_issues().await.unwrap();
        assert!(issues
            .iter()
            .any(|issue| issue.kind == ScanIssueKind::UnsafeDescriptorReference));
        assert!(!issues
            .iter()
            .any(|issue| issue.kind == ScanIssueKind::MalformedCue));

        let snapshot = context.repository.get_library_snapshot().await.unwrap();
        assert!(snapshot
            .games
            .iter()
            .flat_map(|game| game.content_units.iter())
            .flat_map(|unit| unit.files.iter())
            .all(|member| member.file.relative_path != "link/secret.cue"));
    }

    #[tokio::test]
    async fn oversized_descriptor_is_bounded_and_incomplete() {
        let context = test_context(Some(SystemId::PlayStation)).await;
        let oversized = vec![b'#'; MAX_DESCRIPTOR_SIZE + 1];
        write_fixture(&context.root, "oversized.m3u", &oversized);

        let summary = context.scanner.scan_once().await.unwrap();
        assert_eq!(summary.state, ScanRunState::Completed);
        let issues = context.repository.list_latest_scan_issues().await.unwrap();
        assert!(issues.iter().any(|issue| {
            issue.kind == ScanIssueKind::MalformedM3u
                && issue
                    .detail
                    .as_deref()
                    .is_some_and(|detail| detail.contains("maximum size"))
        }));
        let snapshot = context.repository.get_library_snapshot().await.unwrap();
        assert_eq!(snapshot.games.len(), 1);
        assert_ne!(
            snapshot.games[0].content_units[0].availability,
            ContentUnitAvailability::Available
        );
    }

    #[tokio::test]
    async fn deleting_a_whole_subdirectory_marks_its_files_and_units_missing() {
        let context = test_context(Some(SystemId::Nes)).await;
        write_fixture(&context.root, "sub/game.nes", &[1, 2, 3]);
        write_fixture(&context.root, "top.nes", &[4, 5, 6]);
        let first = context.scanner.scan_once().await.unwrap();
        assert_eq!(first.state, ScanRunState::Completed);

        let before = context.repository.get_library_snapshot().await.unwrap();
        assert_eq!(before.games.len(), 2);
        let before_sub = before
            .games
            .iter()
            .find(|game| game.game.local_title == "game")
            .unwrap();
        let before_top = before
            .games
            .iter()
            .find(|game| game.game.local_title == "top")
            .unwrap();
        assert_eq!(
            before_sub.game.availability,
            crate::domain::library::GameAvailability::Available
        );
        assert_eq!(
            before_sub.content_units[0].availability,
            ContentUnitAvailability::Available
        );
        assert_eq!(
            before_sub.content_units[0].files[0].file.availability,
            ContentFileAvailability::Available
        );
        assert_eq!(
            before_top.game.availability,
            crate::domain::library::GameAvailability::Available
        );

        fs::remove_dir_all(PathBuf::from(&context.root.path).join("sub")).unwrap();
        let second = context.scanner.scan_once().await.unwrap();
        assert_eq!(second.state, ScanRunState::Completed);

        let after = context.repository.get_library_snapshot().await.unwrap();
        let sub = after
            .games
            .iter()
            .find(|game| game.game.local_title == "game")
            .unwrap();
        assert_eq!(sub.game.id, before_sub.game.id);
        assert_eq!(
            sub.game.availability,
            crate::domain::library::GameAvailability::Unavailable
        );
        assert_eq!(sub.content_units[0].id, before_sub.content_units[0].id);
        assert_eq!(
            sub.content_units[0].availability,
            ContentUnitAvailability::Missing
        );
        assert_eq!(
            sub.content_units[0].files[0].file.id,
            before_sub.content_units[0].files[0].file.id
        );
        assert_eq!(
            sub.content_units[0].files[0].file.availability,
            ContentFileAvailability::Missing
        );

        let top = after
            .games
            .iter()
            .find(|game| game.game.local_title == "top")
            .unwrap();
        assert_eq!(top.game.id, before_top.game.id);
        assert_eq!(
            top.game.availability,
            crate::domain::library::GameAvailability::Available
        );
        assert_eq!(top.content_units[0].id, before_top.content_units[0].id);
        assert_eq!(
            top.content_units[0].availability,
            ContentUnitAvailability::Available
        );
        assert_eq!(
            top.content_units[0].files[0].file.availability,
            ContentFileAvailability::Available
        );
        assert_eq!(
            top.content_units[0].files[0].file.sha1,
            before_top.content_units[0].files[0].file.sha1
        );

        let root = context
            .repository
            .content_root(context.root.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(root.availability, ContentRootAvailability::Available);
        assert!(root.last_successful_scan_at.is_some());
        assert!(context
            .repository
            .list_latest_scan_issues()
            .await
            .unwrap()
            .is_empty());

        let summarize = |snapshot: &crate::domain::library::LibrarySnapshot| {
            snapshot
                .games
                .iter()
                .map(|game| {
                    (
                        game.game.id,
                        game.game.availability,
                        game.content_units
                            .iter()
                            .map(|unit| {
                                (
                                    unit.id,
                                    unit.availability,
                                    unit.files
                                        .iter()
                                        .map(|member| (member.file.id, member.file.availability))
                                        .collect::<Vec<_>>(),
                                )
                            })
                            .collect::<Vec<_>>(),
                    )
                })
                .collect::<Vec<_>>()
        };
        let after_shape = summarize(&after);

        let repeated = context.scanner.scan_once().await.unwrap();
        assert_eq!(repeated.state, ScanRunState::Completed);
        let repeated_snapshot = context.repository.get_library_snapshot().await.unwrap();
        assert_eq!(summarize(&repeated_snapshot), after_shape);
        assert!(context
            .repository
            .list_latest_scan_issues()
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn deleting_a_deeply_nested_subdirectory_reconciles_through_missing_intermediates() {
        let context = test_context(Some(SystemId::Nes)).await;
        write_fixture(&context.root, "a/b/c/game.nes", &[1, 2, 3]);
        context.scanner.scan_once().await.unwrap();
        let before = context.repository.get_library_snapshot().await.unwrap();
        let before_game = &before.games[0];

        fs::remove_dir_all(PathBuf::from(&context.root.path).join("a/b")).unwrap();
        let summary = context.scanner.scan_once().await.unwrap();
        assert_eq!(summary.state, ScanRunState::Completed);

        let after = context.repository.get_library_snapshot().await.unwrap();
        assert_eq!(after.games.len(), 1);
        let game = &after.games[0];
        assert_eq!(game.game.id, before_game.game.id);
        assert_eq!(
            game.game.availability,
            crate::domain::library::GameAvailability::Unavailable
        );
        assert_eq!(game.content_units.len(), 1);
        assert_eq!(game.content_units[0].id, before_game.content_units[0].id);
        assert_eq!(
            game.content_units[0].availability,
            ContentUnitAvailability::Missing
        );
        assert_eq!(
            game.content_units[0].files[0].file.relative_path,
            "a/b/c/game.nes"
        );
        assert_eq!(
            game.content_units[0].files[0].file.availability,
            ContentFileAvailability::Missing
        );
        assert!(context
            .repository
            .list_latest_scan_issues()
            .await
            .unwrap()
            .is_empty());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn unreadable_intermediate_subtree_still_protects_prior_content() {
        use std::os::unix::fs::PermissionsExt;

        let context = test_context(Some(SystemId::Nes)).await;
        write_fixture(&context.root, "a/b/game.nes", &[1, 2, 3]);
        write_fixture(&context.root, "top.nes", &[4, 5, 6]);
        context.scanner.scan_once().await.unwrap();

        let protected = PathBuf::from(&context.root.path).join("a/b");
        let original_permissions = fs::metadata(&protected).unwrap().permissions();
        let mut unreadable_permissions = original_permissions.clone();
        unreadable_permissions.set_mode(0o000);
        fs::set_permissions(&protected, unreadable_permissions).unwrap();
        let unreadable = fs::read_dir(&protected).is_err();
        if !unreadable {
            fs::set_permissions(&protected, original_permissions).unwrap();
            return;
        }

        fs::remove_file(PathBuf::from(&context.root.path).join("top.nes")).unwrap();
        let summary = context.scanner.scan_once().await.unwrap();
        fs::set_permissions(&protected, original_permissions).unwrap();
        assert_eq!(summary.state, ScanRunState::Completed);

        let snapshot = context.repository.get_library_snapshot().await.unwrap();
        let hidden = snapshot
            .games
            .iter()
            .find(|game| game.game.local_title == "game")
            .unwrap();
        assert_eq!(
            hidden.game.availability,
            crate::domain::library::GameAvailability::Available
        );
        assert_eq!(
            hidden.content_units[0].availability,
            ContentUnitAvailability::Available
        );
        assert_eq!(
            hidden.content_units[0].files[0].file.availability,
            ContentFileAvailability::Available
        );

        let removed = snapshot
            .games
            .iter()
            .find(|game| game.game.local_title == "top")
            .unwrap();
        assert_eq!(
            removed.game.availability,
            crate::domain::library::GameAvailability::Unavailable
        );
        assert_eq!(
            removed.content_units[0].availability,
            ContentUnitAvailability::Missing
        );
        assert!(context
            .repository
            .list_latest_scan_issues()
            .await
            .unwrap()
            .iter()
            .any(|issue| issue.kind == ScanIssueKind::UnreadablePath));
    }

    #[tokio::test]
    async fn removal_preserves_logical_game_and_marks_content_missing() {
        let context = test_context(Some(SystemId::Nes)).await;
        write_fixture(&context.root, "game.nes", &[1, 2, 3]);
        context.scanner.scan_once().await.unwrap();
        let path = PathBuf::from(&context.root.path).join("game.nes");
        fs::remove_file(path).unwrap();

        context.scanner.scan_once().await.unwrap();
        let snapshot = context.repository.get_library_snapshot().await.unwrap();
        assert_eq!(snapshot.games.len(), 1);
        assert_eq!(
            snapshot.games[0].game.availability,
            crate::domain::library::GameAvailability::Unavailable
        );
        assert_eq!(
            snapshot.games[0].content_units[0].availability,
            crate::domain::library::ContentUnitAvailability::Missing
        );
        assert_eq!(
            snapshot.games[0].content_units[0].files[0]
                .file
                .availability,
            crate::domain::library::ContentFileAvailability::Missing
        );
    }

    #[tokio::test]
    async fn unique_move_preserves_file_unit_and_game_identity() {
        let context = test_context(Some(SystemId::Nes)).await;
        write_fixture(&context.root, "old.nes", &[1, 2, 3]);
        context.scanner.scan_once().await.unwrap();
        let before = context.repository.get_library_snapshot().await.unwrap();
        let before_game = before.games[0].game.id;
        let before_unit = before.games[0].content_units[0].id;
        let before_file = before.games[0].content_units[0].files[0].file.id;
        fs::rename(
            PathBuf::from(&context.root.path).join("old.nes"),
            PathBuf::from(&context.root.path).join("new.nes"),
        )
        .unwrap();

        context.scanner.scan_once().await.unwrap();
        let after = context.repository.get_library_snapshot().await.unwrap();
        assert_eq!(after.games.len(), 1);
        assert_eq!(after.games[0].game.id, before_game);
        assert_eq!(after.games[0].content_units[0].id, before_unit);
        assert_eq!(
            after.games[0].content_units[0].files[0].file.id,
            before_file
        );
        assert_eq!(
            after.games[0].content_units[0].files[0].file.relative_path,
            "new.nes"
        );
    }

    #[tokio::test]
    async fn one_missing_file_identity_is_not_reused_for_two_new_copies() {
        let context = test_context(Some(SystemId::Nes)).await;
        write_fixture(&context.root, "old.nes", &[1, 2, 3]);
        context.scanner.scan_once().await.unwrap();
        let before = context.repository.get_library_snapshot().await.unwrap();
        let original_game_id = before.games[0].game.id;
        let original_unit_id = before.games[0].content_units[0].id;

        fs::remove_file(PathBuf::from(&context.root.path).join("old.nes")).unwrap();
        write_fixture(&context.root, "a/dup1.nes", &[1, 2, 3]);
        write_fixture(&context.root, "b/dup2.nes", &[1, 2, 3]);
        context.scanner.scan_once().await.unwrap();

        let snapshot = context.repository.get_library_snapshot().await.unwrap();
        assert_eq!(snapshot.games.len(), 1);
        assert_eq!(snapshot.games[0].game.id, original_game_id);
        assert_eq!(snapshot.games[0].content_units.len(), 3);
        let units = &snapshot.games[0].content_units;
        let available_units: Vec<_> = units
            .iter()
            .filter(|unit| unit.availability == ContentUnitAvailability::Available)
            .collect();
        assert_eq!(available_units.len(), 2);
        assert!(units.iter().any(|unit| {
            unit.id == original_unit_id && unit.availability == ContentUnitAvailability::Missing
        }));
        let file_ids: BTreeSet<_> = available_units
            .iter()
            .flat_map(|unit| unit.files.iter().map(|member| member.file.id))
            .collect();
        assert_eq!(file_ids.len(), 2);
        assert!(available_units.iter().all(|unit| {
            unit.primary_relative_path == unit.files[0].file.relative_path
                && unit
                    .files
                    .iter()
                    .any(|member| member.file.relative_path == unit.primary_relative_path)
        }));
        assert!(available_units
            .iter()
            .all(|unit| unit.id != original_unit_id));
        let issues = context.repository.list_latest_scan_issues().await.unwrap();
        assert!(issues
            .iter()
            .any(|issue| issue.kind == ScanIssueKind::AmbiguousReconciliation));

        context.scanner.scan_once().await.unwrap();
        let repeated = context.repository.get_library_snapshot().await.unwrap();
        assert_eq!(repeated.games.len(), 1);
        assert_eq!(
            repeated.games[0]
                .content_units
                .iter()
                .map(|unit| unit.id)
                .collect::<BTreeSet<_>>(),
            units.iter().map(|unit| unit.id).collect::<BTreeSet<_>>()
        );
        assert_eq!(
            repeated.games[0]
                .content_units
                .iter()
                .filter(|unit| unit.availability == ContentUnitAvailability::Available)
                .flat_map(|unit| unit.files.iter().map(|member| member.file.id))
                .collect::<BTreeSet<_>>(),
            file_ids
        );
    }

    #[tokio::test]
    async fn exact_duplicate_copies_share_only_an_unambiguous_provisional_game() {
        let context = test_context(Some(SystemId::Nes)).await;
        write_fixture(&context.root, "copy-a.nes", &[1, 2, 3]);
        write_fixture(&context.root, "copy-b.nes", &[1, 2, 3]);

        context.scanner.scan_once().await.unwrap();
        let snapshot = context.repository.get_library_snapshot().await.unwrap();
        assert_eq!(snapshot.games.len(), 1);
        assert_eq!(snapshot.games[0].content_units.len(), 2);
        assert!(context
            .repository
            .list_latest_scan_issues()
            .await
            .unwrap()
            .iter()
            .any(|issue| issue.kind == ScanIssueKind::DuplicateContent));
    }

    #[tokio::test]
    async fn ambiguous_move_does_not_reuse_one_of_multiple_old_file_ids() {
        let context = test_context(Some(SystemId::Nes)).await;
        write_fixture(&context.root, "copy-a.nes", &[1, 2, 3]);
        write_fixture(&context.root, "copy-b.nes", &[1, 2, 3]);
        context.scanner.scan_once().await.unwrap();
        let before = context.repository.get_library_snapshot().await.unwrap();
        let old_file_ids: Vec<_> = before.games[0]
            .content_units
            .iter()
            .map(|unit| unit.files[0].file.id)
            .collect();

        fs::remove_file(PathBuf::from(&context.root.path).join("copy-a.nes")).unwrap();
        fs::remove_file(PathBuf::from(&context.root.path).join("copy-b.nes")).unwrap();
        write_fixture(&context.root, "moved.nes", &[1, 2, 3]);
        context.scanner.scan_once().await.unwrap();

        let after = context.repository.get_library_snapshot().await.unwrap();
        let current_file_ids: Vec<_> = after
            .games
            .iter()
            .flat_map(|game| game.content_units.iter())
            .filter(|unit| {
                unit.availability == crate::domain::library::ContentUnitAvailability::Available
            })
            .map(|unit| unit.files[0].file.id)
            .collect();
        assert_eq!(current_file_ids.len(), 1);
        assert!(!old_file_ids.contains(&current_file_ids[0]));
        assert!(context
            .repository
            .list_latest_scan_issues()
            .await
            .unwrap()
            .iter()
            .any(|issue| issue.kind == ScanIssueKind::AmbiguousReconciliation));
    }

    #[tokio::test]
    async fn contested_move_between_different_games_remains_unresolved_after_restart() {
        let context = test_context(Some(SystemId::PlayStation)).await;
        let (predecessor_game_ids, moved_game_id) =
            perform_contested_move(&context, "copy-a", "copy-b").await;

        let (_database, repository, scanner) = reopen_persistence_and_scanner(&context).await;
        scanner.scan_once().await.unwrap();
        let restarted = repository.get_library_snapshot().await.unwrap();
        assert_eq!(restarted.games.len(), 3);
        assert!(predecessor_game_ids
            .iter()
            .all(|game_id| restarted.games.iter().any(|game| game.game.id == *game_id)));
        let available: Vec<_> = restarted
            .games
            .iter()
            .filter(|game| {
                game.game.availability == crate::domain::library::GameAvailability::Available
            })
            .collect();
        assert_eq!(available.len(), 1);
        assert_eq!(available[0].game.id, moved_game_id);
        assert!(repository
            .list_latest_scan_issues()
            .await
            .unwrap()
            .iter()
            .all(|issue| issue.kind != ScanIssueKind::AmbiguousReconciliation));
    }

    #[tokio::test]
    async fn contested_move_is_independent_of_predecessor_insertion_order() {
        let forward = test_context(Some(SystemId::PlayStation)).await;
        let (forward_predecessors, forward_result) =
            perform_contested_move(&forward, "copy-a", "copy-b").await;
        assert_eq!(forward_predecessors.len(), 2);
        assert!(!forward_predecessors.contains(&forward_result));

        let reverse = test_context(Some(SystemId::PlayStation)).await;
        let (reverse_predecessors, reverse_result) =
            perform_contested_move(&reverse, "copy-b", "copy-a").await;
        assert_eq!(reverse_predecessors.len(), 2);
        assert!(!reverse_predecessors.contains(&reverse_result));

        let forward_snapshot = forward.repository.get_library_snapshot().await.unwrap();
        let reverse_snapshot = reverse.repository.get_library_snapshot().await.unwrap();
        assert_eq!(forward_snapshot.games.len(), reverse_snapshot.games.len());
        assert_eq!(
            forward_snapshot
                .games
                .iter()
                .filter(|game| {
                    game.game.availability == crate::domain::library::GameAvailability::Available
                })
                .count(),
            reverse_snapshot
                .games
                .iter()
                .filter(|game| {
                    game.game.availability == crate::domain::library::GameAvailability::Available
                })
                .count()
        );
    }

    #[tokio::test]
    async fn same_path_replacement_updates_evidence_without_duplicate_identity() {
        let context = test_context(Some(SystemId::Nes)).await;
        write_fixture(&context.root, "game.nes", &[1, 2, 3]);
        context.scanner.scan_once().await.unwrap();
        let before = context.repository.get_library_snapshot().await.unwrap();
        let game_id = before.games[0].game.id;
        let unit_id = before.games[0].content_units[0].id;
        let file_id = before.games[0].content_units[0].files[0].file.id;
        let old_sha1 = before.games[0].content_units[0].files[0].file.sha1.clone();
        let old_fingerprint = before.games[0].content_units[0].fingerprint.clone();

        write_fixture(&context.root, "game.nes", &[9, 8, 7, 6]);
        context.scanner.scan_once().await.unwrap();
        let after = context.repository.get_library_snapshot().await.unwrap();
        assert_eq!(after.games.len(), 1);
        assert_eq!(after.games[0].game.id, game_id);
        assert_eq!(after.games[0].content_units.len(), 1);
        assert_eq!(after.games[0].content_units[0].id, unit_id);
        assert_eq!(after.games[0].content_units[0].files[0].file.id, file_id);
        assert_ne!(after.games[0].content_units[0].files[0].file.sha1, old_sha1);
        assert_ne!(after.games[0].content_units[0].fingerprint, old_fingerprint);

        context.scanner.scan_once().await.unwrap();
        let repeated = context.repository.get_library_snapshot().await.unwrap();
        assert_eq!(repeated.games.len(), 1);
        assert_eq!(repeated.games[0].game.id, game_id);
        assert_eq!(repeated.games[0].content_units.len(), 1);
    }

    #[tokio::test]
    async fn unavailable_root_preserves_last_known_library_state() {
        let context = test_context(Some(SystemId::Nes)).await;
        write_fixture(&context.root, "game.nes", &[1, 2, 3]);
        context.scanner.scan_once().await.unwrap();
        let before = context.repository.get_library_snapshot().await.unwrap();
        let hidden_root = PathBuf::from(&context.root.path).with_file_name("library-hidden");
        fs::rename(&context.root.path, &hidden_root).unwrap();

        context.scanner.scan_once().await.unwrap();
        let after = context.repository.get_library_snapshot().await.unwrap();
        assert_eq!(after.games.len(), 1);
        assert_eq!(after.games[0].game.id, before.games[0].game.id);
        assert_eq!(
            after.games[0].content_units[0].id,
            before.games[0].content_units[0].id
        );
        let root = context
            .repository
            .content_root(context.root.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(root.availability, ContentRootAvailability::Unavailable);
        assert!(context
            .repository
            .list_latest_scan_issues()
            .await
            .unwrap()
            .iter()
            .any(|issue| issue.kind == ScanIssueKind::RootUnavailable));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn unsafe_sibling_does_not_disable_clean_absence_reconciliation() {
        use std::os::unix::fs::symlink;

        let context = test_context(Some(SystemId::Nes)).await;
        write_fixture(&context.root, "game.nes", &[1]);
        write_fixture(&context.root, "other.nes", &[2]);
        symlink(
            context._directory.path().join("does-not-exist"),
            PathBuf::from(&context.root.path).join("shortcut"),
        )
        .unwrap();
        context.scanner.scan_once().await.unwrap();

        fs::remove_file(PathBuf::from(&context.root.path).join("game.nes")).unwrap();
        context.scanner.scan_once().await.unwrap();

        let snapshot = context.repository.get_library_snapshot().await.unwrap();
        let game = snapshot
            .games
            .iter()
            .find(|game| game.game.local_title == "game")
            .unwrap();
        assert_eq!(
            game.game.availability,
            crate::domain::library::GameAvailability::Unavailable
        );
        assert_eq!(
            game.content_units[0].availability,
            ContentUnitAvailability::Missing
        );
        let other = snapshot
            .games
            .iter()
            .find(|game| game.game.local_title == "other")
            .unwrap();
        assert_eq!(
            other.game.availability,
            crate::domain::library::GameAvailability::Available
        );
        assert!(context
            .repository
            .list_latest_scan_issues()
            .await
            .unwrap()
            .iter()
            .any(|issue| issue.kind == ScanIssueKind::UnsafePath));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn unreadable_subtree_is_protected_from_false_missing_reconciliation() {
        use std::os::unix::fs::PermissionsExt;

        let context = test_context(Some(SystemId::Nes)).await;
        write_fixture(&context.root, "private/hidden.nes", &[1]);
        write_fixture(&context.root, "public.nes", &[2]);
        context.scanner.scan_once().await.unwrap();

        let private = PathBuf::from(&context.root.path).join("private");
        let original_permissions = fs::metadata(&private).unwrap().permissions();
        let mut unreadable_permissions = original_permissions.clone();
        unreadable_permissions.set_mode(0o000);
        fs::set_permissions(&private, unreadable_permissions).unwrap();
        let unreadable = fs::read_dir(&private).is_err();
        if !unreadable {
            fs::set_permissions(&private, original_permissions).unwrap();
            return;
        }

        context.scanner.scan_once().await.unwrap();
        fs::set_permissions(&private, original_permissions).unwrap();

        let snapshot = context.repository.get_library_snapshot().await.unwrap();
        let hidden = snapshot
            .games
            .iter()
            .find(|game| game.game.local_title == "hidden")
            .unwrap();
        assert_eq!(
            hidden.game.availability,
            crate::domain::library::GameAvailability::Available
        );
        assert_eq!(
            hidden.content_units[0].availability,
            ContentUnitAvailability::Available
        );
        assert!(snapshot
            .games
            .iter()
            .any(|game| game.game.local_title == "public"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn failed_hash_preserves_identity_for_a_later_move() {
        use std::os::unix::fs::PermissionsExt;

        let context = test_context(Some(SystemId::Nes)).await;
        write_fixture(&context.root, "game.nes", &[1, 2, 3]);
        context.scanner.scan_once().await.unwrap();
        let before = context.repository.get_library_snapshot().await.unwrap();
        let before_game = before.games[0].game.id;
        let before_unit = before.games[0].content_units[0].id;
        let before_file = before.games[0].content_units[0].files[0].file.clone();
        let path = PathBuf::from(&context.root.path).join("game.nes");
        let original_permissions = fs::metadata(&path).unwrap().permissions();
        let mut unreadable_permissions = original_permissions.clone();
        unreadable_permissions.set_mode(0o000);
        fs::set_permissions(&path, unreadable_permissions).unwrap();
        if File::open(&path).is_ok() {
            fs::set_permissions(&path, original_permissions).unwrap();
            return;
        }

        context.scanner.scan_once().await.unwrap();
        let failed = context.repository.get_library_snapshot().await.unwrap();
        let failed_file = &failed.games[0].content_units[0].files[0].file;
        assert_eq!(
            failed_file.availability,
            ContentFileAvailability::Unavailable
        );
        assert_eq!(failed_file.crc32, before_file.crc32);
        assert_eq!(failed_file.md5, before_file.md5);
        assert_eq!(failed_file.sha1, before_file.sha1);
        assert_eq!(
            failed.games[0].content_units[0].fingerprint,
            before.games[0].content_units[0].fingerprint
        );
        assert!(context
            .repository
            .list_latest_scan_issues()
            .await
            .unwrap()
            .iter()
            .any(|issue| issue.kind == ScanIssueKind::HashReadFailure));

        fs::set_permissions(&path, original_permissions).unwrap();
        fs::rename(&path, PathBuf::from(&context.root.path).join("renamed.nes")).unwrap();
        context.scanner.scan_once().await.unwrap();
        let moved = context.repository.get_library_snapshot().await.unwrap();
        assert_eq!(moved.games.len(), 1);
        assert_eq!(moved.games[0].game.id, before_game);
        assert_eq!(moved.games[0].content_units[0].id, before_unit);
        assert_eq!(
            moved.games[0].content_units[0].files[0].file.id,
            before_file.id
        );
        assert_eq!(
            moved.games[0].content_units[0].files[0].file.relative_path,
            "renamed.nes"
        );
    }

    #[test]
    fn hash_file_uses_streaming_hashes_for_large_synthetic_content() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("large.nes");
        let mut file = File::create(&path).unwrap();
        let chunk = [0x5a_u8; 64 * 1024];
        for _ in 0..3 {
            file.write_all(&chunk).unwrap();
        }
        drop(file);
        let metadata = fs::metadata(&path).unwrap();
        let candidate = Candidate {
            path: path.clone(),
            relative_path: "large.nes".to_owned(),
            extension: ".nes".to_owned(),
            format: ContentFormat::SingleFile,
            system_id: Some(SystemId::Nes),
            classification_issue: None,
            metadata: FileMetadata {
                size_bytes: metadata.len(),
                modified_at: super::modified_timestamp(&metadata),
            },
            relevant: true,
            hashes: None,
            hash_available: false,
            hash_failed: false,
        };
        let hashes = hash_file(directory.path(), &candidate).unwrap();
        assert_eq!(hashes.crc32.len(), 8);
        assert_eq!(hashes.md5.len(), 32);
        assert_eq!(hashes.sha1.len(), 40);
    }

    #[test]
    fn progress_reporter_coalesces_ordinary_updates() {
        let sink = Arc::new(CollectingSink::default());
        let reporter = ProgressReporter::new(sink.clone());
        let counters = ScanCounters::default();
        reporter.emit(ScanRunId(1), ScanPhase::Hashing, counters, true);
        for _ in 0..20 {
            reporter.emit(ScanRunId(1), ScanPhase::Hashing, counters, false);
        }
        assert_eq!(sink.progress.lock().unwrap().len(), 1);
    }
}
