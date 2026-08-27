# Library Scanner

This document describes the M4 local-library pipeline. It is deliberately independent of
metadata providers: a local `Game` is created from filesystem evidence and can exist without a
provider match, artwork, or region/revision data.

## Ownership and roots

Rust owns root configuration, filesystem access, discovery, relationship parsing, hashing,
reconciliation, SQLite persistence, and watcher coordination. React receives typed snapshots and
events through Tauri IPC; it never scans a directory or queries SQLite.

The managed root is the OS-resolved `Documents/RetroFrontier/ROMs`. Initialization creates it and
one canonical subdirectory for every `SystemCatalog` entry. The canonical names are stored in the
catalog, including `Mega Drive` for the `Mega Drive / Genesis` system. External roots are persisted
absolute paths and may carry an explicit `SystemId` hint. Scanning never writes inside an external
root.

Enabled roots may not be equal, nested, or overlapping. The application rejects a new or re-enabled
overlap. A legacy or directly persisted overlap is handled defensively during scanning: the first
deterministic root (managed roots first, then shallower paths and lexical path order) is scanned and
later roots are skipped with an `overlappingContentRoot` issue. Disabled roots are not part of the
active scan plan. Removing an external root disables it instead of deleting its root, file, unit, or
game history.

Configured paths and durable relative paths must be representable as UTF-8 in the current SQLite
and IPC model. Non-representable names are skipped with an `unrepresentablePath` issue. Directory
symlinks/reparse points, file symlinks, special files, and references that leave a root are not
followed. Descriptor and playlist members use one canonical containment rule: the reference is
normalized relative to the configured root, every path component is checked without following
symlinks/reparse points, the target is canonicalized, and the canonical target must remain below
the canonical root. Rejected references produce an `unsafeDescriptorReference` issue before any
descriptor contents are read.

Descriptor text is loaded incrementally with a 256 KiB maximum for each CUE, GDI, or M3U file.
Files over that cap produce the corresponding malformed-descriptor issue and are not treated as
healthy relationship units.

## Scan phases

Every manual or watcher-triggered scan records a `scan_runs` row and progresses through:

1. **Discovery** — recursively enumerate deterministic, sorted directory entries and identify
   supported extensions.
2. **Relationship resolution** — classify systems, parse descriptors/playlists, and construct
   ordered logical units before standalone files are surfaced.
3. **Hashing** — stream each relevant physical file through CRC32, MD5, and SHA-1 in one pass.
4. **Reconciliation** — transactionally update the selected root's files, units, games, ordered
   memberships, root scan state, and issues.
5. **Completed** — publish the final summary even when a scan encountered typed issues. A scanner
   failure publishes a failed completion summary and does not perform absence reconciliation for
   an uncompleted root.

Absence reconciliation uses a granular authority snapshot rather than one root-wide boolean. The
scanner records successfully enumerated directory prefixes, protected/incomplete prefixes, and
unrepresentable entries. Once the root has been successfully enumerated, a prior file can be marked
missing when walking upward from its prior parent reaches a successfully enumerated ancestor and no
protected prefix covers the file. This means a missing directory and all of its missing intermediate
directories are authoritative absences: if one still existed, recursive enumeration would have
reached it. An explicitly incomplete or unsafe prefix protects its subtree instead. A root that
cannot itself be enumerated protects the whole root; an unreadable directory protects only that
subtree; a known unsafe sibling such as a dangling symlink does not disable reconciliation for clean
siblings. Unrepresentable entries prevent the root from being considered fully successful but do not
mask representable sibling absence. A malformed but discovered descriptor can still produce an
incomplete unit and its issue; it does not turn an unreadable root into an authoritative empty root.

## System evidence

Evidence is applied in this order:

1. an explicit `ContentRoot.system_hint`;
2. a managed root's recognized top-level catalog folder (canonical names and catalog aliases are
   accepted for lookup);
3. a unique extension mapping in `SystemCatalog`.

An ambiguous extension such as `.bin`, `.iso`, `.cue`, `.chd`, or `.m3u` never guesses. It produces
an `ambiguousSystem` issue and is excluded from final standalone content. A conflicting root or
managed-folder hint produces `incompatibleSystemHint`. Descriptor and playlist members are allowed
to use physical extensions that are not standalone formats for the already established system;
their relationship supplies the established system context.

## Content relationships

The durable hierarchy is:

```text
Game
└── ContentUnit
    └── ordered ContentFile memberships
```

`ContentFile` identity is `(ContentRoot, relative path)` in the durable store, with hashes and a
stat fingerprint persisted for safe reconciliation. It is never an absolute path identity alone.

- Ordinary supported files become `singleFile` units. `.chd` becomes an explicit `chd` unit.
- CUE parsing accepts quoted and unquoted single-token `FILE` filenames, strips a leading UTF-8
  BOM, preserves backslash separators for safe normalization, and stores the descriptor before
  ordered track members. Missing or unsafe references make the unit incomplete and create typed
  issues. Referenced tracks are not standalone units.
- GDI parsing strips a leading UTF-8 BOM and tolerates harmless text after the declared track rows
  while still requiring the declared count and every required track record. It follows the same
  containment, missing-member, role, and no-double-counting rules as CUE.
- M3U parsing ignores blank lines and comment/directive lines, preserves remaining entries in exact
  order, and accepts supported disc content including CUE, CHD, and GDI. Nested playlists and
  descriptor dependencies are recursively resolved. Unsupported present playlist members become
  `unsupportedSystem` issues. Cycles become `referenceCycle` issues and do not recurse indefinitely;
  each cyclic playlist group is represented by an incomplete/unavailable playlist unit. An M3U is
  never emitted as a generic standalone unit, including when parsing fails or it is the remaining
  member of a cycle. Playlist-owned discs are not independently surfaced.

The membership `ordinal` and `role` are normalized in `content_unit_files`, so order survives a
database round trip rather than being stored as an opaque descriptor blob.

## Hashing and reconciliation

Hashing uses a fixed 64 KiB buffer and computes CRC32, MD5, and SHA-1 concurrently. A file that
changes while it is being read is reported as `hashReadFailure`; it is not treated as healthy
identified content. If a previously hashed file cannot be refreshed, its last known hashes and
the affected unit's last known fingerprint are retained while availability is degraded. A later
successful refresh replaces them. Unit fingerprints are deterministic SHA-1 values over system,
unit kind, and ordered member hash/role data, never absolute paths.

Repeated scans update existing rows in place. A disappeared file is marked `missing`, its unit is
`missing` or `incomplete`, and its game becomes unavailable only when no other unit remains
available. Games and their user-facing history are not deleted. A temporarily unavailable or
partially scanned root does not mark the undiscovered remainder missing.

A move is preserved only when exactly one old file has an exact content fingerprint and the unit
relationship remains compatible. The one-to-one decision is computed across all old candidates and
new paths before any move is applied. If an old identity has several possible destinations, or a
new path has several possible predecessors, no candidate is selected: a new physical identity is
retained and an `ambiguousReconciliation` issue is emitted. Exact duplicate copies remain separate
physical units; when the fingerprint-to-game match is unambiguous they share one provisional game,
and no copy is deleted or cleaned. Filename/title normalization is never used to merge content.

When a newly discovered M3U takes ownership of content that prior standalone units surfaced, its
non-playlist member `ContentFileId` values are resolved through persisted unit membership. If that
evidence and any exact unit-fingerprint evidence identify exactly one previous `GameId`, the new
M3U unit is attached to that game. The historical standalone units remain; they are not deleted to
hide the ownership change. If member ownership names multiple games or conflicts with exact
fingerprint evidence, the games remain distinct, a new game owns the playlist, and an
`ambiguousReconciliation` issue explains the contested playlist path. An unchanged follow-up scan
reuses the decided M3U unit and does not repeat the one-time ownership decision.

Replacing bytes at the same persisted path continues to preserve `ContentFileId`, `ContentUnitId`,
and `GameId` while updating hashes and the unit fingerprint. M5 match evidence must be bound to
those hashes/fingerprint and made stale when they change; that metadata behavior is outside M4.

Before M5 metadata, a new unit's local title is its primary descriptor/file stem. Existing local
titles and identities survive ordinary reconciliation.

## Watcher and progress contract

The watcher is a signal only. `notify` events are debounced for approximately 250 ms, coalesced,
and submitted to the same real scanner used by manual rescan. The coordinator permits one scan at a
time. Signals during a running scan set one follow-up flag; they cannot launch overlapping scans.
Watcher setup, watch/unwatch, and callback failures become in-memory `watcherFailure` issues and
are logged. Manual rescan remains usable if the watcher cannot start.

The stable Tauri event names are:

- `library-scan-progress` with typed `ScanProgress` payloads;
- `library-scan-completed` with typed `ScanSummary` payloads.

Phase transitions and completion are always emitted. Ordinary updates are coalesced to at most
approximately ten per second; events are never emitted for individual read buffers.

The application boundary exposes `get_content_roots`, `add_external_content_root`,
`remove_external_content_root`, `set_content_root_enabled`, `rescan_library`, `get_scan_status`,
`get_scan_issues`, and `get_library_snapshot`. The snapshot contains the `Game → ContentUnit →
ContentFile` hierarchy without raw SQL rows.

## M6.1 UI backend boundary

M6.1 keeps the full M4 snapshot for diagnostics but adds bounded, query-oriented read contracts for
the later library UI:

- `query_library` returns a page of lightweight list items. It supports normalized-title/local-title
  search, system, favorite, genre, region, and availability filters, with title-ascending ordering
  and a `gameId` tie-breaker. The default and hard maximum page size are both 60. The response also
  includes the total matching count, requested offset, and effective limit. Search treats `\`, `%`,
  and `_` literally through the repository's SQL `ESCAPE` clause. Case-insensitive matching uses
  SQLite `lower()`, which is ASCII-oriented in the current configuration; full Unicode case folding
  is a deferred search-quality item, not an M6.1 claim.
- `get_library_summary` returns total games, favorite games, and counts grouped by system without
  materializing game rows in the frontend.
- `get_library_game_detail` returns one game's local title, system, availability, favorite state,
  and bounded content-unit summaries (root identity, unit kind, primary relative path, file count,
  and unit availability). It does not return physical-file membership or any hash/fingerprint.

The list query composes normalized metadata title, release date, genre, region, coarse provider-match
state, and an opaque cached-cover reference in Rust. It never joins physical content files. A matched
row is also checked against the current M4 evidence in one bounded bulk validation pass; if the
evidence changed, the list reports `stale` while retaining cached metadata and cover data. The M5
detail read applies the same rule. With no accepted match, `pending` covers both not-yet-requested
and queued work; `notRequested` is not a separate UI state. The `game_user_state` table owns the
durable favorite boolean separately from scanner-owned `games`; a scan cannot reset it. Game deletion
is intentionally restricted while user state exists.

`get_scan_issue_page` is the bounded issue-read path for future UI rendering. It reports the total
and rows for one resolved persisted scan run, uses a default limit of 50 and a hard maximum of 100,
and orders by newest `created_at` with `id` as the tie-breaker. By default the resolver selects the
latest `completed` or `failed` run and does not switch to a newer `running` run; the response includes
that run's `scanRunId`, or `null` with an empty page when no terminal run exists. The legacy aggregate
issue command remains available for the existing diagnostic contract and may also include transient
watcher issues.

Root-management commands use safe typed error codes for invalid/unsafe paths, unavailable roots,
non-directory paths, enabled-root overlap, and invalid operations. Raw filesystem and SQL details
remain in Rust logs. Registering an already-configured external path remains idempotent, as in M4;
it updates that root instead of creating a duplicate or returning a new duplicate error.

Metadata providers, provider credentials, matching heuristics, media, launch behavior, archive
import, automatic rename/move/conversion/deletion, and duplicate cleanup are outside M4.
