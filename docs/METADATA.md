# RetroFrontier metadata (M5, M6.1, and M8.5 boundary)

This document describes the metadata implementation that ships with M5 and the M6.1 boundary that
allows the later library UI to consume it. It is an implementation
record, not new research. The provider evidence, ES-DE precedent, project decisions, and residual
risks behind these choices are recorded in [`SCREENSCRAPER_SPIKE.md`](SCREENSCRAPER_SPIKE.md) and
[ADR-007](adr/ADR-007-metadata-provider.md); nothing here upgrades a research finding into a
provider guarantee.

## Scope

M5 adds provider-backed enrichment on top of the M4 local library:

- a provider-neutral `MetadataProvider` boundary with one implementation, `ScreenScraperProvider`;
- deterministic matching for ordinary single-file ROM content, validated against returned provider
  evidence;
- candidate-only heuristic search;
- a small normalized metadata record with provider/source provenance;
- exactly one cached primary front cover per game;
- a restart-safe job queue with provider-aware scheduling, dynamic quota handling, and typed
  retry/defer semantics;
- optional personal provider credentials in the OS credential vault;
- a thin typed IPC surface.

M5 adds no visual library UI. M6.1 adds only the native/IPC boundaries that a later UI can consume;
it does not implement broad media scraping, a metadata editor, arbitrary manual metadata entry,
provider-cache export, or additional providers.

## Layering

```text
thin Tauri commands (commands/metadata.rs)
        |
MetadataApplicationService (application/metadata.rs)
        |
match policy            persistent queue + scheduler        cover cache
(services/               (services/metadata_queue.rs)        (services/
 metadata_matching.rs)                                        metadata_media.rs)
        |                          |                              |
MetadataProvider trait      MetadataRepository            MetadataPaths
(services/                  (repositories/                (adapters/
 metadata_provider.rs)       metadata.rs)                   metadata_paths.rs)
        |                          |                              |
ScreenScraperProvider        SQLite migrations         app-owned media directory
(adapters/screenscraper/)
        |
redacting HTTP client (adapters/http.rs)
credential boundary  (adapters/credentials.rs)
```

Rust owns provider networking, authentication, credentials, the media cache, SQLite persistence,
scheduling, rate limiting, retries, matching, normalization, offline behaviour, and refresh. React
receives normalized state only.

## Domain model

`domain/metadata.rs` is provider-neutral. Provider endpoint names, provider system identifiers,
provider field names, and HTTP status codes do not appear in it.

| Concept                     | Purpose                                                                       |
| --------------------------- | ----------------------------------------------------------------------------- |
| `MetadataProviderId`        | Stable provider identifier persisted in SQLite (`screenscraper`)              |
| `ProviderMatchStatus`       | `pending`/`matched`/`no_match`/`ambiguous`/`deferred`/`failed`/`stale`        |
| `MatchType`                 | Which evidence carried the agreement, or `heuristic_user_confirmed`           |
| `MatchEvidence`             | Content unit, system, kind, hashes, size, unit fingerprint, evidence version  |
| `ProviderIdentity`          | Provider game ID and provider ROM/content ID, stored separately               |
| `ProviderCandidate`         | One heuristic suggestion; never an attachment                                 |
| `NormalizedMetadata`        | The provider-independent V1 field set                                         |
| `MetadataProvenance`        | Provider, provider game ID, available source credit, fetch timestamp          |
| `MediaAsset`                | The one primary cover, its cache identity and provenance                      |
| `MetadataJob`               | Persistent intent, state, attempts, failure class, earliest next attempt      |
| `ProviderFailureClass`      | The typed failure taxonomy every adapter must map onto                        |
| `ProviderQuotaSnapshot`     | The provider's own latest quota maxima and counters                           |
| `ProviderSchedulerState`    | Persisted quota snapshot plus any provider-wide deferral                      |
| `UnsupportedContentReason`  | Why automatic matching is deliberately not attempted                          |
| `UserProviderSelection`     | A user-owned pinned provider game, stored apart from provider-derived data     |

`EVIDENCE_SCHEMA_VERSION` is stored with every accepted match, so a future change to the evidence
rules invalidates old matches instead of silently reinterpreting them.

## Five kinds of state, persisted separately

| State                | Tables                                                             |
| -------------------- | ------------------------------------------------------------------ |
| Local game identity  | `games`, `content_units`, `content_files`, `content_unit_files` (M4, unchanged) |
| Provider identity    | `provider_matches`                                                 |
| Match evidence       | `provider_match_evidence`, plus `provider_match_candidates`         |
| Normalized metadata  | `provider_metadata`                                                |
| Media                | `provider_media_assets`                                            |

Supporting tables: `metadata_jobs` (queue), `provider_scheduler_state` (quota and deferral),
`provider_user_accounts` (non-secret account record), `user_provider_selections` (user-owned).

Every metadata table references `games (id)` with `ON DELETE RESTRICT`, so provider rows can never
cascade into local identity. No metadata table stores a credential value, an authenticated provider
URL, or a raw provider payload.

## Provider boundary

`MetadataProvider` exposes semantic operations rather than HTTP endpoints:

- `supports_system` — is there exactly one unambiguous provider mapping?
- `identify_content` — submit local hash/size evidence and receive a provider record;
- `search_candidates` — heuristic title search;
- `fetch_game` — identity retrieval for an existing relationship, never new matching evidence;
- `download_media` — fetch the previously selected cover bytes.

Successful calls return `ProviderResponse<T>`, which carries any quota the provider reported, so the
scheduler always consumes the provider's current numbers. Failures are `ProviderFailureClass`.

Media locators are wrapped in `ProviderMediaLocator`, which has a redacted `Debug`, cannot be
serialized, and is never persisted or returned through IPC.

## ScreenScraper adapter

`adapters/screenscraper/` owns endpoint construction (`jeuInfos.php`, `jeuRecherche.php`), query
parameter names, the V1 system mapping, response parsing, media selection, quota extraction,
provider error classification, credential injection, and URL redaction. Parsing is deliberately
tolerant: the API v2 schema is documented as beta, mixes string and numeric encodings, and has at
least one inconsistent field spelling, so unknown fields are ignored, missing optional fields are
accepted, both observed spellings of the per-minute quota are read, and a structurally unusable body
is rejected as a protocol failure rather than producing empty metadata.

### V1 system mapping

Verified from the first-party system list recorded in the spike. The mapping lives in the adapter;
the provider-neutral `SystemCatalog` never acquires provider identifiers.

| RetroFrontier `SystemId` | ScreenScraper ID | `romtype` |
| ------------------------ | ---------------: | --------- |
| `nes`                    |                3 | `rom`     |
| `snes`                   |                4 | `rom`     |
| `nintendo_64`            |               14 | `rom`     |
| `game_boy`               |                9 | `rom`     |
| `game_boy_color`         |               10 | `rom`     |
| `game_boy_advance`       |               12 | `rom`     |
| `mega_drive`             |                1 | `rom`     |
| `playstation`            |               57 | `iso`     |
| `sega_saturn`            |               22 | `iso`     |
| `sega_dreamcast`         |               23 | `iso`     |
| `nintendo_gamecube`      |               13 | `iso`     |

An unmapped system is never searched globally: it is persisted as `deferred` with
`system_not_mapped`.

### softname

`domain::metadata::application_softname()` is the single source of the provider `softname` and of
the HTTP user agent. It is a stable product/version/platform identity
(`RetroFrontier/<version> (<os>-<arch>)`), contains nothing about the user, and cannot be supplied by
the frontend. HTTP 426 is treated as a non-retryable client-lifecycle signal.

## Credential architecture

Two different kinds of secret, deliberately separated.

**Application developer credentials** identify RetroFrontier to the provider.

- Development: an ignored local `.env` (see `.env.example`) or the process environment, read only in
  debug builds for the file case.
- Release: the same variables injected at compile time through `option_env!` from a protected
  CI/release secret. No real value or generated credential-bearing source is committed.
- A distributed desktop binary makes an application credential recoverable. This is an application
  identity, not a cryptographic secret boundary, and no obfuscation is claimed as security.
- A build without credentials still starts. The provider reports `credentials_unavailable` and the
  local library is unaffected.

**Optional personal user credentials** belong to one individual and are optional.

- `CredentialVault` is the persistence boundary. `KeyringCredentialVault` uses the OS
  keychain/credential vault; `InMemoryCredentialVault` is the session-only fallback when the host has
  no usable vault, and the injectable implementation every test uses.
- Username and password travel together as one opaque vault secret. SQLite stores only the provider
  identifier, an opaque `vault_reference`, and a state.
- `SecretString`, `DeveloperCredentials`, and `UserCredentials` have redacted `Debug`/`Display`, no
  `Serialize`, and best-effort scrubbing on drop.
- Credential submission is write-only IPC. Read IPC reports `notConfigured`, `configured`,
  `invalid`, or `vaultUnavailable`, plus the account name when it can be read safely — never a
  password.

**Redaction.** `adapters::http::redact_url` removes the values of `devid`, `devpassword`, `ssid`, and
`sspassword` while keeping the parameter names, and `HttpRequest`'s `Debug` renders only the redacted
form. `HttpTransportError` variants carry no URL, host, header, or body.
`adapters::screenscraper::redact_text` is the fallback for free text. No authenticated URL is ever
logged or persisted.

## HTTP client

One bounded HTTPS GET behind the `HttpClient` trait, so tests never need a network. The production
`ReqwestHttpClient` enforces HTTPS, a 10 s connect timeout, a 30 s request timeout, at most three
redirects, an explicit user agent, and a response cap checked both against `Content-Length` and while
reading. This is deliberately not a general networking framework.

## Queue and scheduler

`metadata_jobs` is the authoritative queue; there is no in-memory-only state.

- Job kinds: `identify`, `refresh_metadata`, `refresh_cover`. At most one live job per game,
  provider, and kind.
- States: `pending`, `running`, `deferred`, `failed`, `completed`, with attempts, last failure
  class, earliest next attempt, and timestamps.
- Claiming happens inside a transaction, so concurrent workers and a restarted process never
  process the same job twice.
- Startup returns every `running` job to `pending`, so a crash mid-request cannot leave work
  claimed forever, and leftover partial cover downloads are deleted.
- A claim also carries a 10 minute lease. If a scheduling round is abandoned part-way through — a
  storage failure, for example — it hands its unprocessed jobs straight back to `pending`, and the
  worker's lease sweep re-arms anything that still leaked. Recovery therefore does not depend on
  restarting the process.
- Jobs run in two scheduling bands. Explicit per-game user actions and the evidence-integrity sweep
  use the interactive band; whole-library scrape-run work uses the bulk band, one full band-span
  behind it. The slowest interactive job therefore still outranks the fastest bulk one, so bulk work
  can never delay something the user just asked for by hand.
- Enqueueing interactive work promotes any compatible job already in the queue instead of creating a
  second one: the queue's uniqueness constraint means there is only ever one row per game, provider
  and kind. Promotion also clears the job's bulk attribution, so stopping the run that created it
  cannot cancel work the user has claimed. A job already running keeps its state and attempt budget.
- `MetadataWorker` starts exactly once, runs on the async runtime rather than the UI thread, and
  needs no frontend listener: progress lives in SQLite. It sleeps between rounds (250 ms when busy,
  60 s when idle, at most 300 s while deferred) and stops cleanly on request.
- Explicitly requested work raises a work signal the sleep selects on, so a user action is not left
  waiting out an idle minute. A wake-up only shortens the pause: the next round still consults
  provider deferral, quota, the rolling minute budget and per-job retry timing, so it can bring work
  forward but never past a wait the provider asked for.

## Library discovery does not scrape

> Library discovery is local and does not automatically trigger first-time metadata scraping.

A library scan finds content on this machine. It does not decide to spend the user's provider budget
on what it found. First-time metadata for newly discovered games is fetched only by a scrape run the
user starts in Settings (see below).

> Accepted metadata relationships are still automatically revalidated for evidence integrity.

These are different mechanisms and only the first was removed. Revalidation protects relationships
the user already has: M4 keeps every local identifier stable across same-path byte replacement, so
the evidence sweep is the only thing that can notice replaced content, and without it a stale match
would keep being presented as trusted. It continues to run on its own, in the interactive band.

Work queued by an earlier version is preserved. The change is that no *new* implicit work appears,
not that existing work is destroyed.

## User-initiated scrape runs (M8.5)

A `MetadataScrapeRun` and a `MetadataJob` answer different questions. The run is *which
user-initiated batch operation is in progress*; the job is *which concrete provider operation the M5
pipeline must execute*. The run layer sits above the queue and feeds it; it owns no provider logic,
no matching policy and no scheduler of its own.

Two whole-library modes:

- **Missing Metadata** — games the provider has never answered about. Usually that means untouched:
  no provider relationship and no metadata job. It also covers a game whose only recorded outcome is
  a failure about RetroFrontier's own configuration, because the provider was never actually asked.
  A build with no ScreenScraper credentials parks every identification with `credentials_unavailable`,
  and treating that as an answer would hide those games from every future run even after the
  configuration is fixed. The line is M5's own taxonomy: every `Permanent` failure class is a
  configuration failure, and `Permanent` already means "do not retry until configuration or content
  changes". A no-match, an ambiguous candidate set, an unsupported content shape and a genuinely
  exhausted retry budget *are* answers, so repeated runs do not re-ask them. A game with live work is
  never eligible either way. It is deliberately not "games without a cover".
- **Refresh Matched Games** — accepted matches that still name a provider game, the same condition
  the per-game refresh action requires before it refreshes rather than re-identifies. Ambiguous,
  no-match and unsupported entries are never refreshed as if they were trusted matches.

Invariants:

- **Fixed membership.** The target set is captured when the run starts and never appended to. Games
  a later scan discovers belong to a future run, so an active run cannot grow for as long as the
  library does.
- **One active run per provider**, enforced by a partial unique index rather than an application
  check two concurrent starts could both pass. Explicit per-game actions stay available throughout.
- **Bounded feeding.** A 20,000-game target lives in `metadata_scrape_run_items`; only a window of
  it is ever a `metadata_job`. The feeder writes both tables in one transaction, so a crash cannot
  leave a queued job no run is waiting for, or a queued item with no job behind it.
- **Restart.** Runs live in SQLite, so an active run resumes by identity after a restart and keeps
  feeding. No React component is the source of truth, and the Settings screen need not stay open.
- **Cooperative stop.** Stopping detaches the queued work the run still owns exclusively, leaves
  promoted interactive work alone, and lets a request already in flight finish so its result is
  still recorded. Metadata already written is kept, and games the run never reached return to being
  untouched, so a later Missing Metadata run picks them up.
- **Progress in games.** Run item states are read back from authoritative M5 state rather than
  pushed from job processing, which makes reconciliation idempotent across a crash. A game is
  processed only once it has one of five terminal results — matched, needs review, no match,
  unsupported, failed — and `processed` is the sum of those five. Provider backoff, a retryable
  error, a queued job and an in-flight request are all explicitly not terminal, so a deferred game
  is never counted as processed. Refresh needs more than one job per game, which is why the unit is
  games and not jobs.
- **Ambiguity stays manual.** A run reports what M5 decided and never resolves a candidate set by
  preference or ordering. Ambiguous games are surfaced as NEEDS REVIEW, and REVIEW MATCHES leads
  back into the existing Library and Game Detail candidate workflow rather than a second resolver.
- **No invented provider timing.** ScreenScraper supplies no reset instant, and RetroFrontier's own
  waits are locally scheduled probes, so the UI says work is waiting for provider capacity and never
  when capacity will return. There is no ETA and no completion percentage derived from job counts.

## Quota handling

Quota is consumed dynamically from provider responses; no maximum observed during research is
hard-coded.

- `max_threads` caps concurrency and is never exceeded. With no reported value the scheduler allows
  one in-flight request.
- The per-minute maximum drives a local rolling-window budget. Deciding to look for work only peeks
  at the budget; issuing a request consumes a slot. *Every* outbound request is charged against the
  same window — deterministic identification, the heuristic fallback search, and the cover download
  alike — so a single job cannot exceed the advertised maximum by issuing several calls. Peek and
  consume happen under one lock, so two operations can never both take the last slot.
- A request the window cannot accommodate postpones the job rather than dropping the work: the job
  returns to `pending` without spending a retry attempt, and nothing partial is persisted.
- Daily and negative-lookup budgets are evaluated independently.
- The last snapshot and any deferral are persisted, so both survive restart.
- Absent quota information keeps the scheduler at its most conservative setting.

## Error taxonomy

| Provider signal      | `ProviderFailureClass`             | Disposition          |
| -------------------- | ---------------------------------- | -------------------- |
| HTTP 400             | `invalid_request`                  | permanent            |
| HTTP 401             | `provider_restricted`              | provider deferral    |
| HTTP 403 (developer) | `developer_authentication_failed`  | permanent            |
| HTTP 403 (personal)  | `user_authentication_failed`       | permanent            |
| HTTP 404             | `no_match`                         | negative result      |
| HTTP 423             | `provider_unavailable`             | provider deferral    |
| HTTP 426             | `client_rejected`                  | permanent            |
| HTTP 429             | `capacity_deferred`                | provider deferral    |
| HTTP 430             | `daily_quota_exceeded`             | provider deferral    |
| HTTP 431             | `negative_quota_exceeded`          | provider deferral    |
| transport/TLS/timeout| `transport`                        | bounded retry        |
| HTTP 5xx             | `transient_server`                 | bounded retry        |
| unusable 2xx body    | `malformed_response`               | bounded retry        |
| no usable credential | `credentials_unavailable`          | permanent            |
| unusable media       | `media_unavailable`                | bounded retry        |

The 403 case inspects only field *names* in the safe error body; when no personal credentials were
sent it can only be a developer failure.

## Retry and deferral

- **Bounded retry** applies to genuinely transient failures: exponential backoff from 5 s, capped at
  30 min, with additive jitter, and at most five attempts before the job is parked. A parked job
  stays inspectable and is re-armed by an explicit user request.
- **Provider deferral** applies to quota and availability. It does not consume the retry budget, and
  it defers the whole provider so a second job does not immediately repeat the same request.
  Capacity starts at 1 min (cap 15 min), unavailability at 5 min (cap 1 h), and daily/negative quota
  at 30 min (cap 6 h). The provider returns no `Retry-After`, reset timestamp, or next-allowed
  timestamp, so these are conservative local probes rather than an invented reset instant. A
  deferral never becomes a permanent failure: work resumes when the provider allows it.
- **Permanent** failures (malformed request, authentication, rejected client, missing credentials)
  are never retried automatically.
- **Negative results** are answers: the job completes and the state records `no_match`.
- The clock and the jitter source are injectable, so every retry test is deterministic and no test
  depends on wall-clock timing.

## Matching pipeline

**Stage 1 — system gate.** Map `SystemId` to exactly one provider system. If there is none, persist
`deferred` with `system_not_mapped` and issue no request.

**Stage 2 — deterministic content lookup.** Send the provider system, ROM type, basename, byte size,
and every available hash (SHA-1, MD5, CRC32). Automatic attachment requires all of:

1. an unambiguous system mapping;
2. a concrete returned provider content record;
3. agreement between the returned record and the current M4 evidence, size included;
4. no conflicting provider evidence.

Evidence preference is SHA-1 + size, then MD5 + size, then CRC32 + size — the last only when no
stronger hash was comparable on both sides and no second provider record shares that CRC32. Any hash
present on both sides that disagrees rejects the match outright. A filename, a title, a result's
position in a list, and a successful HTTP response are never deterministic evidence.

**Stage 3 — heuristic search.** Title search produces `ProviderCandidate` rows and an `ambiguous` or
`deferred` state. No score or threshold is invented, and no candidate is ever silently attached. A
404 hash miss is *not* turned into a title search: it is recorded as a deterministic negative answer.

A user may pin a provider game explicitly. That is recorded in `user_provider_selections` as
user-owned state and produces `heuristic_user_confirmed`, which is never reported as deterministic.
A provider refresh replaces normalized metadata and media but never touches a user selection.

## Content capability matrix

| Content format            | Automatic deterministic matching | Heuristic candidates | State                            |
| ------------------------- | -------------------------------- | -------------------- | -------------------------------- |
| Single-file cartridge ROM | Yes                              | Yes                  | `matched` on agreeing evidence   |
| GameCube ISO              | Yes                              | Yes                  | `matched` on agreeing evidence   |
| CHD                       | No                               | Yes                  | `deferred`                       |
| CUE/BIN                   | No                               | Yes                  | `deferred`                       |
| GDI                       | No                               | Yes                  | `deferred`                       |
| M3U / multi-disc          | No                               | Yes                  | `deferred`                       |
| GameCube RVZ, GCM         | No                               | Yes                  | `deferred`                       |
| PlayStation/Saturn/Dreamcast single-file images (`.iso`, `.pbp`, `.cdi`, bare `.bin`) | No | Yes | `deferred` |

The eligible set is an allowlist, not a denylist: a container whose canonical provider
representation is not published is deferred rather than hashed and hoped for. The playlist file is
never provider identity, and its bytes are never submitted for identification.

## Stale evidence and revalidation

M4 deliberately keeps `GameId`, `ContentUnitId`, and `ContentFileId` stable across same-path byte
replacement while the hashes and unit fingerprint change. Identity alone therefore proves nothing,
so every accepted match is bound to a versioned evidence snapshot.

When the current evidence no longer agrees:

- the local `Game` and its availability are unchanged;
- the last-known-good normalized metadata and cover remain readable;
- the match is marked `stale`/needs revalidation and stops being reported as deterministic — a read
  reports `stale` immediately, before any sweep has run;
- re-identification is enqueued for the next time the provider is reachable.

Both refresh kinds behave the same way: a metadata refresh *and* a cover refresh on stale evidence
re-identify instead of re-fetching the previous provider identity, so a provider ID whose evidence
no longer holds is never silently re-trusted.

The same rule applies at the commit boundary. A provider request is issued with no transaction open,
so the content can be replaced while the answer is in flight. Before an accepted match is written,
the evidence the answer was judged against is compared with the live evidence again; if they no
longer agree the match is not committed, the relationship is marked stale, and identification runs
again against the new evidence without spending a retry attempt.

## Normalized metadata

Deliberately small — what M6 needs to render a library entry, and nothing more:

`title`, `sort_title`, `synopsis`, `release_date`, `developer`, `publisher`, `genre`, `players`,
`region`.

Provider-controlled strings are bounded before they are stored (512 characters for the short
fields, 8192 for the synopsis), far above any real value, so one malformed response cannot write
megabytes per game.

`sort_title` is a normalization of the provider's own title (a leading English article is moved to
the end), not invented metadata. Region preference for localized values is `wor`, `us`, `eu`, `ss`,
`jp`; language preference is `en`, then `de`.

Every record carries `MetadataProvenance`: provider identity, provider game ID, the provider's
available source credit, and the fetch timestamp. Provider-derived metadata is replaceable: a
successful refresh overwrites it atomically, and a failed refresh leaves the previous snapshot
untouched.

## Media

Exactly one primary front cover per game and provider. Selection accepts only front box art
(`box-2D`), rejects media attached to anything other than the game itself, and prefers the
configured region order. Screenshots, wheels/logos, fan art, back covers, physical media, videos,
manuals, and bezels are deliberately not eligible.

Publication is atomic:

1. download to memory with a hard 8 MiB cap enforced by the transport;
2. validate the content type against `image/png`, `image/jpeg`, `image/webp`;
3. validate the container signature so a mislabelled or truncated payload is rejected;
4. write to a temporary file in the target directory and `fsync` it;
5. rename over the target — the only way an existing cover is ever replaced;
6. commit the database row, then delete a superseded file.

Crash windows are bounded by design. A crash before the rename leaves a `.part` file, which startup
cleanup removes. A crash after the rename but before the row commits leaves the cached path holding
newer bytes than the row describes: the cover stays readable and the relationship is untouched, and
the next refresh rewrites the row — when the new bytes needed a different extension, the superseded
file is a tolerated unreferenced orphan rather than a correctness problem. A crash after the commit
but before the old file is deleted leaves the same kind of harmless orphan. The last-known-good
cover is never lost to a crash.

Any failure leaves the previous cover in place and records a media failure marker without blanking
the cached asset. A provider that simply holds no eligible front cover is recorded as such and is
*not* retried: it is an answer, not a transient fault. A refresh whose provider checksums match the cached asset skips the download
entirely. A cover file that has disappeared from the cache is reported as `missing` without changing
stored state or the match.

### Storage location

Covers live in the app-owned data directory only:

```text
<app data>/metadata/
└── media/covers/<provider>/<game id>.<ext>
```

An in-progress download is written as a hidden `.part` file *in the target directory*, not in a
separate temporary tree, so publication is always a same-directory rename and can never cross a
filesystem boundary.

The database stores the path *relative* to the media root, and `MetadataPaths::resolve_media`
refuses absolute paths and traversal, so a corrupted row cannot be turned into an arbitrary
filesystem read. Nothing is ever written beside user ROMs, inside managed ROM roots, inside BIOS
roots, or into source-controlled paths.

### WebView delivery boundary (M6.1)

The UI never receives the persisted `cache_relative_path`. A bounded library item or readable
metadata state exposes only an opaque target-specific reference: `rfmedia://localhost/cover/<game-id>`
on Linux/macOS desktop, and `http://rfmedia.localhost/cover/<game-id>` on Windows. The native
Tauri custom-protocol registration and reference generator own this platform distinction. The
strongest boundary is that the WebView supplies only an opaque local game identity; the persisted
cache path never crosses into React. Rust loads the current cached cover row, then reuses the cover
cache's lexical/canonical containment and PNG/JPEG/WebP validation before returning bytes. Those
containment checks are defence-in-depth against corrupted persisted cache paths, not the identity
boundary itself. Traversal, absolute paths, symlinks that leave the cache, missing files, unsupported
types, and invalid content produce safe protocol failures. The CSP already permits the two required
origins; no generic file-serving route exists. Provider URLs and credentials never cross this
boundary.

## Offline behaviour

Offline is a first-class state, observable as repeated transport failure.

- The local library stays fully usable; no local `Game`, `ContentUnit`, or `ContentFile` state
  changes.
- Cached normalized metadata and the cached cover remain readable.
- Pending provider work becomes and stays `deferred`, with the deferral persisted.
- After the first failure the provider itself is deferred, so no further request is issued and there
  is no busy retry loop. Consecutive transport failures lengthen the deferral.
- `get_metadata_provider_status` reports `offline` rather than pretending everything is fine.
- Restarting offline is consistent and still issues nothing.

## Failure isolation invariant

No provider operation may delete or hide a `Game`, change local availability, change `GameId`,
modify `ContentUnit` ownership, or modify `ContentFile` identity. The metadata repository writes to
no M4 table, and every failure class is covered by a regression test that asserts the local rows and
the user's files on disk are byte-for-byte unchanged.

## Database concurrency

M5 introduces background metadata writes alongside interactive library operations, which closes the
previously deferred SQLite write-concurrency question. See
[ADR-013](adr/ADR-013-sqlite-write-concurrency.md).

The database opens with `journal_mode = WAL`, `synchronous = NORMAL`, a 10 s busy timeout, and
foreign keys enforced. `synchronous`, `foreign_keys`, and `busy_timeout` are connection-local, so
they are supplied through the connect options and therefore applied to every connection the pool
opens; a regression test holds all of them open at once and checks each one. Writers stay short: every metadata write is a small transaction, job claiming
is one transaction, and no provider request is ever made while a transaction is open. Worker
concurrency is bounded by the provider's advertised thread count.

## IPC

Commands validate input, call one application service method, and return a typed DTO. They contain
no provider logic, SQL, filesystem access, or retry behaviour.

| Command                                | Responsibility                                              |
| -------------------------------------- | ----------------------------------------------------------- |
| `get_game_metadata`                    | Full metadata state for one game                            |
| `request_game_metadata`                | Enqueue identification                                      |
| `refresh_game_metadata`                | Enqueue refresh, or identification when no match is accepted |
| `get_metadata_provider_status`         | Quota, deferral, offline flag, job counts, account state     |
| `select_game_metadata_candidate`       | Record a user-owned pinned provider game                     |
| `clear_game_metadata_candidate`        | Remove that user-owned decision                              |
| `set_metadata_provider_credentials`    | Write-only personal credential submission                    |
| `clear_metadata_provider_credentials`  | Remove the personal account                                  |
| `get_metadata_provider_account`        | Configured yes/no, state, account name — never a password     |
| `preview_metadata_scrape`              | Games a mode would target if started now                     |
| `get_metadata_scrape_status`           | Active run, or the most recent finished one, with progress   |
| `start_metadata_scrape`                | Start a run in one mode                                      |
| `stop_metadata_scrape`                 | Begin a cooperative stop of the active run                   |

A scrape command's entire input is the mode. React never sends game identifiers for a batch
operation: eligibility, the fixed target set, feeding, attribution, stopping and completion are all
decided in Rust, where the authoritative state already is.

Whole-run progress is read through `get_metadata_scrape_status`, not assembled from
`metadata-state-changed`. A 20,000-game run emits tens of thousands of those events and the screen
shows eight numbers, so the status query is one bounded aggregate; per-game invalidation stays for
Game Detail and the visible Library page. Settings polls it about once a second while a run is
active and stops when the run finishes, because a finished run is a static summary.

The DTOs have no field for a developer credential, a password, a raw provider payload, an
authenticated URL, or a SQL/domain internal. The cover is exposed as an opaque native media
reference; React never resolves or owns filesystem paths.

Metadata changes that affect visible game state emit `metadata-state-changed` after the relevant
database write. Its payload is only `{ gameId, providerId }`; it is an invalidation signal, and M6.3
/M6.4 refetch the bounded library/detail or existing metadata state rather than treating the event
as a data source. It remains a per-game invalidation signal: bulk enrichment can emit many events.
M6.3 debounces/coalesces visible-page events, while M6.4 debounces only the currently open game;
neither refetches the entire library immediately for every event.

The M6 list and M5 detail use one coherent provider-match vocabulary. `pending` means that no
accepted provider match is currently available, including a game that has not been requested yet or
has queued work; there is no separate `notRequested` state in the UI contract. `stale` means that
the last-known-good match no longer agrees with current local evidence, while its cached normalized
metadata and cover remain usable.

## Testing

Normal tests require no internet access, no real credentials, and no OS keychain. The HTTP boundary,
the provider, the credential vault, the clock, and the jitter source are all injectable. Fixtures are
synthetic or sanitized, free of credentials, credential-bearing URLs, and provider artwork; cover
fixtures are a few bytes with a valid container signature.

There is no opt-in live provider test in M5. Adding one later would require separate gating,
`#[ignore]` by default, secret backing, read-only behaviour, and explicit documentation.

## Known limitations

- Automatic deterministic matching is not available for CHD, CUE/BIN, GDI, M3U/multi-disc, RVZ, or
  the disc-system single-file images listed above. Heuristic candidates exist for all of them, and
  none can silently attach.
- Broad media scraping is not implemented: one front cover only.
- Portable export or backup of the provider cache is not implemented.
- Exact visible attribution is an M6 responsibility. M5 preserves provider identity and the
  available source credit to support it, and hard-codes no legal attribution sentence.
- Library title search escapes `\`, `%`, and `_` as literal text, but its SQLite `lower()` matching
  remains ASCII-oriented in the current configuration. Full Unicode case folding is deferred; the
  M6.1 corrective pass adds no folded-search column or replacement search subsystem.
- ScreenScraper Web API v2 is documented as beta and may change without notice.
- Application developer credentials are recoverable from a distributed binary.
- The provider publishes no authoritative quota reset instant, so re-probing is conservative rather
  than precisely timed. Nothing in the UI predicts one.
- Scrape runs are whole-library only. Scrape-by-system, scrape-by-filter, scrape-by-favorites and
  scrape-by-selection are deliberate non-goals: they multiply the eligibility surface without
  changing what the pipeline below does.
- There is no user-visible pause and resume, and no scrape run history surface. Automatic recovery
  of a still-running run after a restart is implemented; deliberately resuming a stopped one is not.
- A run item whose provider work finishes without producing any classifiable relationship — the
  local game disappeared underneath the request, for instance — is recorded as `failed`. It is the
  terminal state that keeps the run finishable and the processed total honest, rather than leaving a
  game suspended in progress forever.
