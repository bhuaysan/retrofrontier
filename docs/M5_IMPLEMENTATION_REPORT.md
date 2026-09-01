# M5 Metadata — Final Implementation Report

Branch: `feat/m5-metadata`
Report date: 2026-08-27

## A. Repository state

- **Starting main commit:** `0eabded docs: finalize ScreenScraper M5 readiness (#13)`
- **Branch:** `feat/m5-metadata`, branched from latest `origin/main`, never committed to `main`,
  not merged, not pushed
- **Commits created:**
  - `76f4f66 feat(metadata): implement M5 ScreenScraper metadata enrichment`
  - `0cc046f docs(metadata): document the M5 metadata architecture`
- **Diff:** 42 files changed, 12,914 insertions, 37 deletions
- **Final working-tree status:** clean apart from the pre-existing untracked files
- **Preserved pre-existing untracked files:** `M3_REVIEW.md`, `M4_REVIEW.md`, `M4_REVIEW_2.md`,
  `M4_REVIEW_3.md` — untouched

**Prerequisite verification.** `origin/main` contained completed M4, the pre-M5 identity cleanup
(unique M3U predecessor preservation, no guessed ambiguous ownership, one-to-one move evidence), the
finalized ScreenScraper readiness work, M0.2 marked complete, ADR-007 with the finalized provider
decisions, and `M5 READY: YES`. No unmerged research branch was used.

**Commit structure note.** Two commits rather than four. The suggested layers are genuinely
interdependent — `repositories` is not `#[allow(dead_code)]`, so a domain-plus-repository-only commit
fails `clippy -D warnings`. Every commit here builds and passes all checks, which was judged more
valuable than a layer split that bisects badly.

## B. M5 architecture implemented

- **Domain** (`domain/metadata.rs`): provider-neutral. `MetadataProviderId`,
  `ProviderMatchStatus`, `MatchType`, `MatchEvidence` (+ `EVIDENCE_SCHEMA_VERSION`),
  `ProviderIdentity`, `ProviderCandidate`, `NormalizedMetadata`, `MetadataProvenance`,
  `MediaAsset`, `MetadataJob`, `ProviderFailureClass`/`FailureDisposition`,
  `ProviderQuotaSnapshot`, `ProviderSchedulerState`, `UnsupportedContentReason`,
  `UserProviderSelection`, plus `application_softname()` and the `evidence_for_unit` capability
  gate. No provider endpoint, provider system ID, provider field name, or HTTP status appears here.
- **Application service** (`application/metadata.rs`): the only place combining provider, match
  policy, queue, scheduler, and cover cache. Owns reads, user requests, the matching pipeline,
  refresh, revalidation, and the credential surface. `MetadataWorker` is the single-start background
  lifecycle.
- **Repositories:** `MetadataRepository` holds all metadata SQL with targeted operations only;
  `LibraryRepository` gained bounded `game()` and `game_content_units()` reads so metadata
  processing never needs the full library snapshot.
- **Provider abstraction** (`services/metadata_provider.rs`): `MetadataProvider` with
  `supports_system`, `identify_content`, `search_candidates`, `fetch_game`, `download_media`.
  Success returns `ProviderResponse<T>` carrying any reported quota; `ProviderMediaLocator` is
  redacted, non-serializable, and never persisted.
- **ScreenScraper adapter** (`adapters/screenscraper/`): `mod.rs` (endpoints, credential injection,
  status classification, redaction), `systems.rs` (V1 mapping), `parse.rs` (tolerant parsing, media
  selection, quota extraction).
- **Queue/scheduler** (`services/metadata_queue.rs`): injectable `Clock`/`JitterSource`,
  `failure_action`, bounded backoff, `MinuteBudget` (peek vs. consume), `plan`.
- **Cache/media** (`services/metadata_media.rs`, `adapters/metadata_paths.rs`): validation, atomic
  publication, traversal-proof path resolution.
- **IPC** (`commands/metadata.rs`): nine thin commands.

## C. Persistence and migrations

**Migration added:** `20260827000000_metadata.up.sql` / `.down.sql`.

| Table                        | Purpose                                                             |
| ---------------------------- | ------------------------------------------------------------------- |
| `provider_matches`           | provider identity + status/match type, unique per (game, provider)  |
| `provider_match_evidence`    | 1:1 evidence snapshot: hashes, size, fingerprint, evidence version  |
| `provider_match_candidates`  | ordered heuristic suggestions                                       |
| `provider_metadata`          | replaceable normalized record + provenance                          |
| `provider_media_assets`      | the one cover, unique per (game, provider, kind='cover')            |
| `metadata_jobs`              | queue, unique per (game, provider, kind)                            |
| `provider_scheduler_state`   | dynamic quota snapshot + deferral, one row per provider             |
| `provider_user_accounts`     | opaque vault reference + state, no secret                           |
| `user_provider_selections`   | user-owned pinned provider game                                     |

**Invariants:** every table references `games (id)` `ON DELETE RESTRICT`; explicit
`created_at`/`updated_at`; `CHECK` constraints on every enum column; indexes for provider/status and
job readiness; no credential value, authenticated URL, or raw provider payload stored.

**Upgrade behaviour.** Tested from a populated M4 database. `GameId`, `ContentUnitId`,
`ContentFileId`, membership, and scan history are unchanged; all nine metadata tables start empty;
`PRAGMA foreign_key_check` is clean; a provider match against an unknown game is rejected. The down
migration drops only metadata tables and preserves local data; reapplying works.

**Restart behaviour.** Matches, evidence, candidates, metadata, media rows, jobs, quota, deferrals,
and account records all persist. Startup returns `running` jobs to `pending` and deletes partial
cover downloads.

## D. Credential model

- **Developer credential source:** `option_env!` build-time injection for releases; otherwise the
  process environment, populated in debug builds from an ignored local `.env` (`.env.example` added,
  empty). Never committed, never returned via IPC, never logged — only the *origin* is logged.
  Documented as recoverable from a distributed binary, not a secret boundary. Without credentials the
  application starts and the local library is unaffected.
- **User credential storage boundary:** `CredentialVault` trait; `KeyringCredentialVault` (OS
  keychain via `keyring` v4, pure-Rust secret-service on Linux); `InMemoryCredentialVault` as the
  session-only fallback and the injectable test implementation. Username and password are stored as
  one opaque vault secret. SQLite holds only `provider_id`, `vault_reference`, and `state`.
- **Vault abstraction:** injectable everywhere. No test requires a real OS keychain.
- **IPC behaviour:** write-only submission; reads return `notConfigured`, `configured`, `invalid`, or
  `vaultUnavailable`, plus the account name when safely readable. `ProviderAccountStatus` has no
  password field, asserted by test.
- **Secret redaction:** `SecretString` and all credential types have redacted `Debug`/`Display`, no
  `Serialize`, and best-effort drop-scrubbing. `redact_url` strips `devid`/`devpassword`/`ssid`/
  `sspassword` values while keeping parameter names; `HttpRequest`'s `Debug` renders only the
  redacted form; `HttpTransportError` carries no URL, host, header, or body; `redact_text` covers
  free text. A test asserts no secret reaches SQLite.

No credential value is printed anywhere in this report.

## E. Provider and quota behaviour

- **Endpoints used:** `jeuInfos.php` (identification and game-ID retrieval), `jeuRecherche.php`
  (heuristic search), plus the provider's own media URL for the selected cover.
- **softname:** one centralized `application_softname()` —
  `RetroFrontier/<version> (<os>-<arch>)`. The frontend cannot supply it; the adapter test asserts
  equality. HTTP 426 is treated as non-retryable.
- **System mapping:** all eleven V1 systems, adapter-owned, verified value by value against the
  spike, with `romtype` rom/iso. Tests assert full coverage and that no two RetroFrontier systems
  share a provider ID. An unmapped system yields `deferred`/`system_not_mapped` with zero requests.
- **Dynamic quota handling:** `max_threads`, both documented spellings of the per-minute maximum,
  the daily and negative-lookup maxima, and both counters are read from every response and
  persisted. No researched value is hard-coded. Absent quota keeps concurrency at 1.
- **429** → `capacity_deferred`, 1 min base, 15 min cap. **430** → `daily_quota_exceeded`, 30 min
  base, 6 h cap. **431** → `negative_quota_exceeded`, same family, evaluated independently. All
  persist a provider-wide deferral and cost no retry attempt.
- **Retry/defer:** transient faults use exponential backoff (5 s → 30 min cap) with jitter, at most
  5 attempts, then parked. Permanent failures never retry automatically. A deferral never becomes a
  permanent failure.

## F. Matching pipeline

- **Deterministic evidence:** requires an unambiguous mapping, a concrete returned content record,
  size agreement, and no contradicting hash. Preference SHA-1 → MD5 → CRC32; CRC32 only when nothing
  stronger was comparable on both sides and no second provider record shares that CRC32. Filename,
  title, and result order are never deterministic.
- **Heuristic path:** candidates persisted in provider order, no invented score or threshold, never
  auto-attached. A user pin yields `heuristic_user_confirmed`, never reported as deterministic.
- **No match:** a 404 is recorded as a deterministic negative answer and deliberately *not*
  escalated to a title search.
- **Ambiguity:** conflicting returned evidence, or a response with no comparable evidence, yields
  `ambiguous` with candidates and no attachment.
- **Unsupported/deferred formats:** CHD, CUE/BIN, GDI, M3U, RVZ/GCM, and disc-system single-file
  images. The eligible set is an allowlist. Playlist bytes are never submitted as identity.
- **Stale evidence:** the local game, its availability, the last-known-good metadata, and the cover
  are all retained; status becomes `stale`; a read reports stale immediately, before any sweep;
  re-identification is enqueued; a refresh on stale evidence re-identifies instead of re-fetching the
  previous provider ID.

## G. Normalized metadata

Implemented fields: `title`, `sort_title`, `synopsis`, `release_date`, `developer`, `publisher`,
`genre`, `players`, `region`. `sort_title` is a normalization of the provider's own title (leading
English article moved to the end), not invented metadata.

Provenance per record: provider identity, provider game ID, the provider's available source credit,
and the fetch timestamp. Provider-derived data is replaceable and atomically overwritten on success;
a failed refresh leaves the previous snapshot untouched.

## H. Media/cache

Exactly one primary front cover (`box-2D`), game-parented only, ordered by region preference.
Storage: `<app data>/metadata/media/covers/<provider>/<game id>.<ext>`, with the *relative* path
persisted and absolute/traversal paths refused. Atomic publication: bounded download (8 MiB) →
content-type check → container-signature check → temp file in the target directory plus `fsync` →
rename → commit row → delete superseded file. Cache hit: unchanged provider checksums skip the
download entirely. Any failure retains the last-known-good file and records a failure marker without
blanking the cached asset. A cover file that has vanished reports `missing` without changing stored
state or the match.

## I. Offline behaviour

The local library stays fully usable and no local state changes. Cached normalized metadata and the
cached cover remain readable. New enrichment and refresh become and stay `deferred`, with the
deferral persisted. After the first transport failure the provider itself is deferred, so no further
request is issued — asserted as an unchanged provider call count across ten further scheduling
rounds. Consecutive transport failures lengthen the deferral. `get_metadata_provider_status` reports
`offline`. Restarting offline is consistent and still issues nothing.

## J. IPC

| Command                               | Responsibility                                               |
| ------------------------------------- | ------------------------------------------------------------ |
| `get_game_metadata`                   | Full metadata state for one game                             |
| `request_game_metadata`               | Enqueue identification                                       |
| `refresh_game_metadata`               | Enqueue refresh, or identification when no match is accepted |
| `get_metadata_provider_status`        | Quota, deferral, offline flag, job counts, account state     |
| `select_game_metadata_candidate`      | Record a user-owned pinned provider game                     |
| `clear_game_metadata_candidate`       | Remove that user-owned decision                              |
| `set_metadata_provider_credentials`   | Write-only personal credential submission                    |
| `clear_metadata_provider_credentials` | Remove the personal account                                  |
| `get_metadata_provider_account`       | Configured yes/no, state, account name — never a password    |

Tauri commands remain thin: each deserializes input, calls one application service method, logs
failures, and returns a typed DTO. None contains provider logic, SQL, filesystem access, or retry
logic. The DTOs have no field for a developer credential, password, raw provider payload,
authenticated URL, or domain internal. The cover is exposed as a cache-relative reference, so React
never resolves or owns filesystem paths. Matching TypeScript types and wrappers were added to
`src/platform/ipc.ts`, with a Rust test pinning the serialized names.

## K. Database concurrency

**Selected strategy:** WAL journaling + `synchronous = NORMAL` + 10 s busy timeout + enforced foreign
keys, combined with short writers, transactional job claiming, no provider request inside a
transaction, and worker concurrency bounded by the provider's advertised thread count.

**Why:** the contention shape is many short reads plus occasional short writes. WAL removes
reader/writer blocking; the busy timeout turns SQLite's single-writer limit into a queue instead of
an error surfaced to the user; `NORMAL` is the documented safe WAL pairing (may lose the newest
commits after power loss, never corrupts, and provider metadata is refetchable). A dedicated
serialized writer task was considered and rejected as premature — it would add a channel and
lifecycle for contention WAL plus a busy timeout already handles, without removing the need for
either setting.

Recorded in ADR-013 and asserted by tests: the pragmas are verified after open, and a regression test
runs a background metadata writer concurrently with interactive library reads and writes, requiring
all of them to succeed.

## L. Tests added

137 new tests (131 → 268 total).

| Group                                                                        | Count  | Location                                     |
| ---------------------------------------------------------------------------- | -----: | -------------------------------------------- |
| Provider adapter (requests, encoding, redaction, softname, mapping, parsing, quota, media selection, error classification) | 33 | `screenscraper/{mod,parse,systems}.rs` |
| HTTP boundary + redaction                                                    |      4 | `adapters/http.rs`                           |
| Auth/credentials                                                             |      6 | `adapters/credentials.rs`                    |
| Domain model + IPC naming                                                    |     10 | `domain/metadata.rs`                         |
| Matching policy                                                              |      9 | `services/metadata_matching.rs`              |
| Queue/quota/retry policy                                                     |     13 | `services/metadata_queue.rs`                 |
| Media cache + paths                                                          |  8 + 3 | `metadata_media.rs`, `metadata_paths.rs`     |
| Provider boundary redaction                                                  |      1 | `services/metadata_provider.rs`              |
| Integration: matching, stale evidence, persistence, media, queue/quota, retry, offline, failure isolation, user-owned state, credentials, scheduling | 43 | `application/metadata.rs` |
| Migration + DB concurrency                                                   |      4 | `adapters/database.rs`                       |
| IPC DTO safety                                                               |      3 | `commands/metadata.rs`                       |

No existing test was weakened. Two pre-existing assertions were updated for the new migration count
(2 → 3), which is a factual consequence of adding a migration.

## M. Verification

| Command                                                                                        | Result                                                            |
| ---------------------------------------------------------------------------------------------- | ----------------------------------------------------------------- |
| `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`                                    | pass                                                              |
| `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings` | pass, no warnings                                                |
| `cargo test --manifest-path src-tauri/Cargo.toml`                                              | **267 passed, 0 failed, 1 ignored** (pre-existing opt-in BIOS test) |
| `cargo build --manifest-path src-tauri/Cargo.toml --release`                                   | pass (1 m 14 s)                                                   |
| `pnpm typecheck`                                                                               | pass                                                              |
| `pnpm lint`                                                                                    | pass                                                              |
| `pnpm format:check`                                                                            | pass                                                              |
| `pnpm test`                                                                                    | 1 file, 2 tests passed                                            |
| `pnpm build`                                                                                   | pass                                                              |
| `pnpm tauri:build`                                                                             | pass, built `target/release/retrofrontier`                        |
| `git diff --check` (worktree + cached)                                                         | clean                                                             |

Targeted test counts: metadata/provider 33, matching 9, queue 13, media 11, migration + concurrency
6, application integration 43, credentials 6, commands 3.

Also verified: no credentials tracked; `.env` still ignored and untracked; no real provider media
committed; no authenticated URLs in tracked files; no unexpected changes to local library semantics;
branch based on latest `origin/main`; both new migration files tracked; documentation consistent.

## N. Documentation changes

| File                                                | Why                                                                                                                                                          |
| --------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `docs/METADATA.md` (new)                            | Full implementation record: model, provider boundary, credentials, queue, quotas, error taxonomy, matching, capability matrix, stale evidence, media, offline, IPC, concurrency, testing, limitations |
| `docs/adr/ADR-013-sqlite-write-concurrency.md` (new) | Records the WAL/busy-timeout decision M5 forced, including the rejected alternative                                                                          |
| `docs/adr/README.md`                                | Index ADR-013                                                                                                                                                |
| `docs/adr/ADR-007-metadata-provider.md`             | Implementation-status section                                                                                                                                |
| `docs/SCREENSCRAPER_SPIKE.md`                       | Executive note that M5 is implemented; new §39 recording only the clarifications implementation forced. Research labels and findings unchanged                |
| `ARCHITECTURE.md`                                   | MetadataService contract, database concurrency, `metadata/media` layout, resolved open decision                                                              |
| `DOMAIN.md`                                         | Metadata/media concepts, two new domain rules, metadata persistence                                                                                          |
| `PROJECT_CONTEXT.md`                                | Metadata implementation status and current phase                                                                                                             |
| `PRODUCT.md`                                        | User-facing metadata behaviour and its deliberate limits                                                                                                     |
| `BACKLOG.md`                                        | M5 items completed, status summary, deferred capabilities                                                                                                    |
| `README.md`                                         | Milestone status and cover cache location                                                                                                                    |
| `docs/DEVELOPMENT.md`                               | `.env` setup, credential boundary, test independence, cache location                                                                                         |
| `.env.example` (new)                                | Documented, empty development credential template                                                                                                            |

## O. Deferred capabilities / known limitations

- **CHD deterministic matching** — deferred; canonical bytes undefined by the provider. Heuristic
  candidates only.
- **CUE/BIN deterministic matching** — deferred; descriptor/track identity undefined.
- **GDI deterministic matching** — deferred; canonical member/aggregate rule undefined.
- **M3U deterministic disc-aware matching** — deferred; the playlist is never provider identity.
- **RVZ deterministic matching** — deferred; no established provider representation.
- **Broad media scraping** — not implemented; one front cover only.
- **Provider-cache export** — not implemented.
- **M6 visual attribution presentation** — not implemented. Provider identity and available source
  credit are preserved to support it; no legal attribution sentence is hard-coded.

**Newly discovered during implementation:**

- The eligible set for automatic deterministic matching had to become an **allowlist**, not a
  denylist. Beyond the named formats this also defers GameCube `.gcm` and PlayStation/Saturn/
  Dreamcast single-file images (`.iso`, `.pbp`, `.cdi`, a bare `.bin` track), because no first-party
  material establishes them as a canonical lookup representation. GameCube `.iso` *is* enabled, since
  the provider system list confirms it. This is stricter than the capability matrix required and can
  be relaxed per row without touching M4 or the matching contract.
- A 404 hash miss does not trigger an automatic title search (per spike §12), so heuristic candidates
  arise from unsupported container representations and from responses that return no comparable
  content evidence.
- `maxdownloadspeed` is not used as a scheduling input. Media downloads are bounded by size and share
  the same concurrency and rolling-minute budgets as metadata requests.
- No opt-in live provider test was added. The allowance remains available and unexercised; every
  shipped test uses synthetic or sanitized fixtures and a fake provider.
- The ScreenScraper maintainer questions in spike §31 all remain open. None blocked implementation.
- ScreenScraper Web API v2 remains documented as beta and may change without notice.
- Application developer credentials remain recoverable from a distributed binary.
- The provider publishes no authoritative quota reset instant, so re-probing is conservative rather
  than precisely timed.

## P. Security verification

- `.env` untracked and ignored (`.gitignore:17`). A developer's local `.env` exists on this machine
  and was never read or printed.
- No real credentials committed. The only credential-shaped strings in tracked files are synthetic
  `fake-*` values inside the environment-parser test.
- No credential-bearing URLs committed; all fixtures use `provider.invalid`.
- No provider media committed; no binary or image file appears in either commit.
- No secret leakage detected: redacted `Debug`/`Display`, no `Serialize` on secret types, redaction
  across URLs, errors, and free text, a test asserting no secret appears in SQLite, and DTO-shape
  tests proving no password field exists.
- `.env.example` contains empty values only.

## Q. M5 completion assessment

`None`

## R. Final status

`M5 IMPLEMENTATION: COMPLETE`
