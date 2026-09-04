# RetroFrontier Domain Model

## Purpose

This document defines the core domain language for RetroFrontier. The model must avoid treating a game as a single filesystem path.

## Core Concepts

### System

An emulated platform supported by RetroFrontier.

Owns configuration such as:

- stable identifier
- display name
- managed ROM folder name
- supported content formats
- BIOS requirements
- default core policy
- launch capabilities

The V1 system catalog is application-owned static product knowledge. A stable
system identifier is distinct from its display name and aliases, so names such
as Mega Drive and Genesis resolve to one logical system. Catalog validation
checks identifiers, aliases, extensions, BIOS requirements, and core mappings
before the application exposes them.

### System Readiness

The application-level readiness result explains whether a system can be used by
combining its resolved approved default-core policy, availability from the
verified managed runtime, and required BIOS status. It does not include game or
ROM availability. Reasons such as an unresolved core policy, missing verified
core, missing required BIOS, invalid BIOS, or an identity not covered by the
catalog remain inspectable.

### Game

A logical library entry presented to the user.

May contain:

- title
- sort title
- system
- release year
- developer
- publisher
- genre
- region information
- description
- favorite state
- metadata-source identifiers

A Game is not a filesystem file.

### Content Unit

A playable representation associated with a Game.

May represent:

- a single ROM file
- a CHD image
- a CUE/BIN set
- an M3U playlist
- a multi-disc set
- another system-specific launchable content representation

A Game may have more than one Content Unit, for example different regions or revisions.

### Content File

A physical file on disk that belongs to a Content Unit.

Examples:

- `.sfc`
- `.gba`
- `.cue`
- `.bin`
- `.chd`
- `.m3u`
- `.iso`
- `.rvz`

Conceptual fields:

- content unit
- content root
- relative/canonical path
- size
- modified timestamp
- hashes where available
- file role
- availability status

### Content Root

A directory scanned by RetroFrontier.

Kinds:

- managed ROM root
- external ROM root

Conceptual fields:

- path
- kind
- enabled
- optional system hint
- last scan timestamp

### Disc

An ordered disc within a multi-disc Content Unit.

The persistence model may use playlist ordering where appropriate, but the domain must preserve disc ordering.

### Metadata Record

Metadata obtained from a provider.

Provider-specific payloads must not leak throughout the application.

M5 models this as a provider-neutral set of concepts, all downstream of local identity:

- `ProviderIdentity` — the provider's own game and content identifiers for a local `GameId`. It is a
  replaceable relationship and never becomes a `GameId`.
- `MatchEvidence` — the content unit, system, kind, hashes, size, unit fingerprint, and evidence
  schema version that justified the relationship. Local identifiers alone prove nothing, because M4
  keeps them stable across same-path byte replacement.
- `MatchType` — which evidence carried the agreement, or that a user confirmed it manually.
- `NormalizedMetadata` — the small provider-independent V1 field set: title, sort title, synopsis,
  release date, developer, publisher, genre, players, region.
- `MetadataProvenance` — provider identity, provider game ID, available source credit, fetch time.
- `MetadataJob` — persistent provider intent with state, attempts, failure class, and next attempt.
- `ProviderFailureClass` and `ProviderQuotaSnapshot` — typed failure and dynamic quota vocabulary.
- `UserProviderSelection` — a user-owned pinned provider game, stored apart from provider-derived
  data so a refresh can never overwrite a user decision.

Provider-specific state is one of `pending`, `matched`, `no_match`, `ambiguous`, `deferred`,
`failed`, or `stale`.

### Media Asset

Local cached artwork/media associated with a Game, such as cover, screenshot, logo, or background.

V1 caches exactly one primary front cover per game and provider, with its provider media type,
region, provider checksums, source credit, and an app-owned cache-relative path. Downloaded media is
never stored beside user ROMs, in managed ROM or BIOS roots, or in source-controlled paths.

### Core

A RetroArch/libretro core available to the managed runtime.

Tracks:

- stable identifier
- libretro/core name
- display name
- supported systems
- platform/architecture support
- managed component identity

Static core policy is distinct from runtime availability. Only approved cores
from an authenticated managed runtime can become available; system-installed
cores, arbitrary user paths, and user downloads are outside the V1 model.

### BIOS Requirement

A known firmware requirement for a System or Core.

Tracks:

- expected filename(s)
- accepted hashes and known size where authoritative
- required/optional status
- user-facing description

A filename match without an authoritative identity is not a valid BIOS result.
BIOS files remain user-owned data and are never downloaded, executed, or
modified by RetroFrontier.

### BIOS File

A user-supplied local firmware file discovered by RetroFrontier.

RetroFrontier must not download copyrighted BIOS files.

### Runtime Release

A RetroFrontier-approved set of emulation-runtime components.

May include:

- RetroArch version
- core versions
- support assets
- platform/architecture
- download locations
- integrity metadata
- compatibility constraints

### Runtime Installation

The locally installed managed runtime.

Possible states:

- not installed
- installing
- ready
- update available
- updating
- damaged
- repairing
- rollback available

### Play Session

A recorded game execution with game/content/core/runtime/time/exit information.

M7 persists this in `play_sessions` with a constrained outcome: `running`, `completed`,
`failedToStart`, `crashed`, or `interrupted`. A session is open exactly while it has no end time,
and a closed session keeps its first verdict. It stores no raw process output or log blob.

Play-session history is product data. It is never the authority on whether a managed process is
alive; that answer comes only from the durable process record plus operating-system process
identity.

### Save Data

Normal emulator-managed persistent save data such as SRAM or memory-card data.

Save Data is user data and must survive runtime replacement. M9 keeps it RetroFrontier-*located*
and otherwise **opaque**: it is never interpreted, never enumerated as individual files, never
assigned slots, never deleted by RetroFrontier, and never subjected to save-state compatibility
logic.

### Save State

An emulator snapshot bound to the exact game, content unit, play session, core binary, and physical
bytes that produced it.

M9 makes this a first-class domain object with a hard rule: a Save State exists because a
*controlled launch proved its provenance* and RetroFrontier subsequently verified the exact physical
content. Provenance is never derived from a filename, a slot suffix, a directory name, a content
basename, or timestamp proximity.

- `SaveStateId` is the semantic identity, and the only one that crosses IPC. A filename, an absolute
  path, a slot, a content basename, a RetroArch core directory name, and a timestamp are explicitly
  *not* identity.
- The slot is scoped metadata bounded to 1–999. RetroArch's slot 0 and automatic slot are out of
  scope and cannot be expressed at all.
- `core_binary_sha256` — the authenticated digest of the core executable, from the release
  manifest's installed-file inventory — is the decisive core identity and is **immutable**. The same
  slot under different core-binary provenance is a different Save State, and both are representable.
- Lifecycle is `available`, `missing`, `superseded`, or `deleted`. Only `available` states are shown.
- A human-readable core version may be recorded for description; it is never a load identity.

M9 calculates whether a controlled load is currently *permitted* (`loadable`), never whether a state
is compatible. Even with the identical binary, deserialization is not guaranteed, and one failed
load never marks a state corrupt. See [`docs/SAVE_STATES.md`](docs/SAVE_STATES.md).

### Game Override

Optional per-game launch configuration such as:

- core override
- video behavior
- aspect/integer scaling
- shader choice
- input behavior

Overrides must not mutate unrelated user RetroArch configuration.

M7 implements only the core override, in `game_launch_overrides`. It stores a `CoreId`, never a
filesystem path, and only a core the catalog approves for that game's system may be stored, so an
override can never become an escape hatch out of approved policy. It is user-owned state in its own
table, so scanner reconciliation and provider refresh cannot overwrite it.

## Relationships

```text
System
  ├── Core mappings
  └── BIOS requirements

ContentRoot
  └── ContentFile
        └── ContentUnit
              └── Game

Game
  ├── MetadataRecord
  ├── MediaAsset
  ├── PlaySession
  ├── SaveData
  ├── SaveState
  └── GameOverride

RuntimeRelease
  └── RuntimeInstallation
        └── installed Core components
```

## Domain Rules

1. A Game must never be identified solely by an absolute file path.
2. Removing a file must not automatically delete user metadata without reconciliation.
3. Runtime replacement must not delete ROMs, BIOS files, saves, states, metadata, or the database.
4. External ROM roots are read-only from the perspective of automatic V1 maintenance.
5. A scan must be safe to repeat.
6. Filesystem discovery and metadata identification are separate concerns.
7. A game may exist before metadata lookup succeeds.
8. BIOS validation should produce a user-actionable state rather than a cryptic launch failure.
9. Launching resolves an explicit managed runtime, core, content unit, and configuration. A game is
   launchable only through a RetroFrontier identity: a `GameId` and optionally a `ContentUnitId`,
   never a filesystem path supplied by the presentation layer. The launch target is the primary
   descriptor, playlist, or standalone file of an available content unit, never a member track, and
   multi-file content is never collapsed into a single-file model.
10. Provider-specific metadata identifiers remain behind a provider abstraction.
11. A logical `GameId` may be preserved across content ownership or reconciliation changes only
    when exact content evidence establishes one predecessor game. Ambiguous ownership is retained
    as history and reported; it is never guessed.
12. Metadata-provider state is downstream of local-library identity. Provider failure, no-match,
    ambiguity, deferral, or stale evidence must not delete/hide a Game, change local availability,
    or alter Game/ContentUnit/ContentFile identity or ownership.
13. A provider relationship is trusted only while the evidence snapshot that established it still
    agrees with current content. When it stops agreeing, the local game, its availability, the
    last-known-good metadata, and the cached cover are all retained, and the relationship becomes
    stale rather than being deleted or silently kept.
14. Provider-derived data is replaceable and user-owned decisions are not. A provider refresh may
    overwrite normalized metadata and media, and may never overwrite a user-owned record.
15. A system with an unresolved core policy approves no core at all, so no override, fallback, or
    installed component can make it launchable.
16. Operating-system process identity, not persisted history, decides whether a managed emulator is
    running. When identity cannot be established, RetroFrontier fails closed: runtime mutation stays
    blocked, launches are refused, and the durable record is preserved rather than deleted.
17. A Save State is a managed object only when a controlled launch proved its game, content unit,
    core provenance, and exact file identity. Attribution is fail-closed: a state whose provenance
    cannot be proved is left untouched on disk, never imported, never displayed, and never offered
    for load or delete. Filename resemblance proves nothing.
18. A `SaveStateId` never authorizes a filesystem path. It identifies the expected domain object,
    and the exact current target must be proved again immediately before any read, load, or
    destructive action. The filesystem is authoritative for bytes; SQLite is authoritative for
    provenance and lifecycle; neither alone makes a Save State usable.

## Identification Inputs

May combine:

- managed-folder system context
- extension/content-format knowledge
- filename
- file size
- CRC32
- MD5
- SHA-1
- playlist relationships
- provider results

Exact matching rules remain an implementation task.

## Persistence

SQLite is the store. M4 introduces the normalized schema through a forward/down migration rather
than mirroring UI components. `content_roots` persist managed and external roots; `games`,
`content_units`, and `content_files` keep logical, launchable, and physical identities distinct;
`content_unit_files` stores ordered membership and roles; `scan_runs` and `scan_issues` preserve
reconciliation evidence. Foreign keys use restrictive delete behavior so missing files and removed
external roots do not cascade-delete logical library history. Static `SystemCatalog` knowledge is
not duplicated in SQLite; persisted library rows store stable `SystemId` strings.

The scanner marks files and units missing only for locations covered by a granular authoritative
snapshot: after the root is successfully enumerated, a successfully enumerated ancestor establishes
authority for a vanished descendant, provided no incomplete or unsafe prefix covers that path. This
allows a deleted directory tree to reconcile while an existing unreadable or otherwise protected
subtree remains protected from false absence. A root that cannot be enumerated protects all prior
rows, while an unreadable subtree or unsafe sibling protects only its affected location. Exact,
content fingerprints may preserve a file/unit identity across a move and may relate duplicate
physical copies under one provisional game. Ambiguous matches create a new physical identity and
an inspectable issue. A transient hash read failure degrades availability but preserves previously
verified file hashes and the unit fingerprint for later reconciliation. Local titles are derived
from the primary path only when a unit is first created; M5 metadata matching is separate.

M7 adds two further tables alongside this schema without altering it: `game_launch_overrides` holds
the user-owned per-game core choice and `play_sessions` holds execution history. Both reference
`games (id)` with restrictive delete behaviour, `play_sessions` also references
`content_units (id)`, and neither is written by the scanner or by a metadata provider.

M9 adds three more on the same terms. `save_states` holds proved provenance and the registered file
identity, with restrictive foreign keys to `games`, `content_units`, and `play_sessions`; a unique
`(play_session_id, state_relative_path, state_sha256)` index makes reconciliation replay a no-op,
and a partial unique index on `state_relative_path` lets exactly one `available` row claim a
physical file while superseded predecessors remain as history. `launch_state_baselines` and
`launch_state_baseline_entries` hold the durable pre-launch state-tree snapshot that makes
attribution possible at all; it is persisted before RetroArch is spawned and survives a restart.
None of the three is written by the scanner or by a metadata provider.

M5 adds metadata tables alongside this schema without altering it: `provider_matches` and
`provider_match_evidence` hold provider identity and the evidence bound to it,
`provider_match_candidates` records heuristic suggestions, `provider_metadata` holds the replaceable
normalized record, `provider_media_assets` holds the one primary cover, `metadata_jobs` is the
restart-safe queue, `provider_scheduler_state` holds the dynamic quota snapshot and deferral,
`provider_user_accounts` holds a non-secret opaque reference to an optional personal account, and
`user_provider_selections` holds user-owned decisions. Every one of them references `games (id)`
with restrictive delete behaviour, and none stores a credential value, an authenticated provider URL,
or a raw provider payload.

When a newly discovered M3U absorbs files from prior standalone units, persisted file membership
may transfer the new playlist representation to the prior `GameId` only if every applicable owner
resolves to the same game and exact fingerprint evidence does not conflict. Multiple predecessor
games remain distinct, the playlist receives a new game, and reconciliation records the ambiguity.
Move identity follows the same rule in both directions: one prior file and one discovered path may
reuse physical identity, while contested candidates receive new identities without an ordering
tie-break.
