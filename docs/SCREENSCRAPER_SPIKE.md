# ScreenScraper research spike

Access date for all current web and API observations: **2026-08-27**.

Labels in this report have strict meanings:

- **VERIFIED** — supported by a first-party ScreenScraper page or a sanitized live response.
- **OBSERVED ES-DE PRECEDENT** — demonstrated by the inspected ES-DE revision; not proof of
  ScreenScraper permission or policy for RetroFrontier.
- **RETROFRONTIER V1 PROJECT DECISION** — a conservative, reversible project choice; not a
  provider rule or legal conclusion.
- **RECOMMENDATION** — non-binding follow-up guidance rather than a settled V1 decision.
- **UNRESOLVED** — the public provider material and safe live tests did not answer the question.

## 1. Executive conclusion

**M5 READY: YES**

ScreenScraper can technically return the game fields and media links M5 needs. Its Web API accepts
hash-and-size evidence, returns provider game and ROM identifiers, exposes quota counters,
represents multiple support/disc records, and provides media checksums. All eleven V1 systems have
verified ScreenScraper IDs.

Current ES-DE provides important public precedent for a direct distributed-client integration,
recoverable application credentials, optional user credentials, local metadata persistence, and
local media downloads. That precedent is not ScreenScraper authorization for RetroFrontier, and
the provider still does not publicly define every credential-distribution, caching, attribution,
or container-hash rule. RetroFrontier nevertheless can implement a conservative and reversible V1:
direct Rust communication, optional OS-vault user credentials, normalized metadata, one cover,
strict returned-evidence validation, heuristic candidates without silent attachment, and explicit
deferral of unsupported container matching. The remaining questions are residual policy risks or
deferred capabilities, not engineering blockers.

The pre-M5 identity cleanup is now on `main`. It preserves one unambiguous M3U predecessor
`GameId`, refuses ambiguous ownership and contested moves, and retains local IDs while replacing
hashes/fingerprint after same-path byte replacement. M5 must bind provider trust to an evidence
snapshot and make it stale when that evidence changes.

## 2. Research methodology

**VERIFIED:** The repository was refreshed against `origin/main` at
`e7c1917cba727ce88d340cae1d1c472a8e07ae77`. The M4 domain, repository, scanner, application
service, IPC, both migrations, documentation, review fixes, and pre-M5 identity-cleanup tests were
inspected. The current spike branch was rebased onto that commit.

First-party sources were preferred. Controlled API work used an ignored, untracked `.env`; requests
were GET-only and limited to infrastructure, system-list, search, one official documented hash
example, and one synthetic no-match. Returned media URLs can contain developer credentials, so raw
responses are secret-bearing and were not retained. No media was downloaded and no provider state
was changed.

No suitable legal ROM fixture exists in the repository. Therefore no local CHD/CUE/GDI/M3U content
was sent or uploaded. ES-DE behavior is recorded separately as precedent and never promoted to a
verified provider rule.

The local `.env` exists, is ignored, is untracked, and contains the expected non-empty development
credential fields. No values were displayed or compared, and no new authenticated request was
needed: the sanitized live evidence below was already collected on this report's access date.

## 3. Sources and access dates

| Source                                | URL                                                | Evidence used                                                                                                   | Authority / confidence                                                   |
| ------------------------------------- | -------------------------------------------------- | --------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------ |
| ScreenScraper Web API v2 Beta         | https://www.screenscraper.fr/webapi2.php           | API status and eligibility; authentication; schemas; quotas; errors; matching; ROM/disc fields; media checksums | First-party documentation; high for stated behavior, but explicitly beta |
| ScreenScraper registration page       | https://www.screenscraper.fr/membreinscription.php | Account-sharing prohibition; account/IP quota; contribution consent; supporter limits                           | First-party policy; high                                                 |
| ScreenScraper FAQ                     | https://www.screenscraper.fr/faq.php               | Provider directs quota/thread questions here; public rendering did not expose all answers                       | First-party but incomplete through public rendering; medium              |
| ScreenScraper site footer and credits | https://www.screenscraper.fr/                      | CC BY-NC-SA 4.0 label and category/source credits                                                               | First-party notice; insufficient for each returned item                  |
| Creative Commons BY-NC-SA 4.0         | https://creativecommons.org/licenses/by-nc-sa/4.0/ | Meaning of the license named by ScreenScraper                                                                   | Canonical license text; does not establish which API item is covered     |
| Live `ssinfraInfos`                   | Authenticated URL intentionally omitted            | API 2.0 header; infrastructure/status shape                                                                     | First-party live response; high for observation                          |
| Live `systemesListe`                  | Authenticated URL intentionally omitted            | V1 mappings, extensions, ROM types, guest quota fields                                                          | First-party live response; high for observation                          |
| Live `jeuInfos`                       | Authenticated URL intentionally omitted            | Official documented CRC/size lookup returned game ID, ROM ID and hashes                                         | First-party live response; high for observation                          |
| Live `jeuRecherche`                   | Authenticated URL intentionally omitted            | Ordered candidates and response keys; no score field in observed JSON                                           | First-party live response; not a stability guarantee                     |

All sources were accessed on 2026-08-27. Search results or third-party projects were not treated as
authority for credentials, permissions, quotas, licensing, or eligibility.

## 4. API eligibility and status

**VERIFIED:** The current documented API is Web API **v2**, and the live header reports `2.0`.
The documentation still labels v2 “Beta” and says it can change without notice. No stability,
deprecation window, schema compatibility, or version-lifetime guarantee is published.

**VERIFIED:** The API may be integrated into applications that are entirely free and distributed.
Other applications require prior permission and ScreenScraper-defined conditions. A developer must
present the software through the ScreenScraper forum to obtain the developer ID and password that
validate its right to use the API.

**VERIFIED:** HTTP 426 is used when a scraper/client or version is blacklisted as non-conforming or
obsolete.

**INTERPRETATION:** RetroFrontier's intended GPL-3.0-or-later, no-charge distribution appears to fit
the published “entirely free and distributed” category, but obtaining developer credentials still
requires presenting RetroFrontier. That process is software registration/approval in practice.

**UNRESOLVED:** The public rule does not explicitly address public source code, public binaries,
commercial app stores distributing a free application, donations, or the exact `softname` and
version lifecycle. Eligibility is not final until the provider confirms the distribution and
credential model.

## 5. Developer authentication

**VERIFIED:** The common client parameters are:

| Parameter     | Status                                        | Meaning                                      |
| ------------- | --------------------------------------------- | -------------------------------------------- |
| `devid`       | Mandatory                                     | Developer identifier issued by ScreenScraper |
| `devpassword` | Mandatory                                     | Developer password issued by ScreenScraper   |
| `softname`    | Mandatory                                     | Name of the calling software                 |
| `output`      | Optional                                      | `xml` by default; `json` is supported        |
| `ssid`        | Optional on normal read/list/search endpoints | End-user ScreenScraper username              |
| `sspassword`  | Optional on normal read/list/search endpoints | End-user ScreenScraper password              |

Endpoint-specific matching fields are additionally mandatory for `jeuInfos`; `recherche` is
mandatory for `jeuRecherche`. `ssuserInfos` itself requires user credentials.

**VERIFIED:** A developer-only live request succeeded with `devid`, `devpassword`, and `softname`,
without `ssid` or `sspassword`. This proves end-user login is technically optional for the tested
read endpoints.

**UNRESOLVED:** Public documentation defines `softname` only as the calling software's name. It does
not prescribe separators, application/version encoding, channels, or whether each release must be
registered.

**RETROFRONTIER V1 PROJECT DECISION:** Use a stable non-secret product/version identity, include a
platform/build identifier where useful, and treat HTTP 426 as a release-blocking client
compatibility signal. Do not encode user identity. Provider registration expectations remain a
residual policy question.

## 6. User authentication

**VERIFIED:** `ssid` and `sspassword` are optional on ordinary API list, lookup, search, and media
requests. Without them, responses identify a non-member/guest quota profile.

**VERIFIED:** Supplying invalid user credentials produced HTTP 403 with a user-login error even
though developer authentication was valid. A future adapter must distinguish developer and user
authentication failures from the safe error body, not status code alone.

**VERIFIED:** ScreenScraper forbids sharing a member account and warns of permanent account/IP bans.
Quota is applied to the same account and/or IP. RetroFrontier must never ship a shared member login.

**VERIFIED:** A user's contribution/support level changes concurrency and quota benefits. The
registration/support page currently advertises 50,000 daily requests for lower supporter tiers,
100,000 for higher tiers, and additional threads at specified contribution levels. These are current
offers, not values to hard-code.

**RETROFRONTIER V1 PROJECT DECISION:** M5 supports optional credentials belonging only to the
individual user. The application remains usable without an account, while login can improve
capacity and access when guest/leecher access is restricted. Credentials are sent directly from
Rust to ScreenScraper and are never shared between users.

## 7. Production developer credential distribution

**VERIFIED:** ScreenScraper issues a developer ID/password after the developer presents the
software. The values are placed in GET query parameters. Live JSON media URLs also repeated the
developer credentials. They must be treated as secrets in URLs, logs, caches, crash reports, and
diagnostics.

**SECURITY FACT:** A secret embedded in an open-source desktop application cannot be considered
confidential. Obfuscation, dynamic extraction, or compilation does not change that fact.

**UNRESOLVED:** No first-party public statement found answers whether a developer credential may be:

- committed to a public repository;
- compiled into or shipped with a public desktop binary;
- fetched dynamically by the client;
- supplied out-of-band to users;
- entered separately by every user;
- retained only by a private RetroFrontier service.

**RETROFRONTIER V1 PROJECT DECISION:** Never commit or distribute the local development values.
Release builds receive a RetroFrontier-owned credential through protected build-time injection.
The resulting binary is acknowledged to be extractable; no obfuscation is a security boundary.
This accepts a residual provider-policy risk and does not claim provider permission.

## 8. Credential storage recommendation

No credential storage is implemented by this spike.

**RECOMMENDATION:** The future boundary should be:

- Rust owns provider authentication and constructs requests.
- Development credentials may come from an ignored environment file or process environment only.
- User credentials, if supported, go to the OS keychain/credential vault; SQLite stores only a
  provider name, non-secret settings, and an opaque credential reference.
- Session-only credentials are an acceptable fallback when secure persistence is unavailable.
- React may submit a password through a narrow write-only command but never receives it back.
- Ordinary IPC reads expose only states such as `notConfigured`, `configured`, or `invalid`.
- URL query strings, response media URLs, headers, bodies, and errors are redacted before logging.
- Credential buffers are short-lived where practical; backups exclude the keychain secret.

Release application credentials use the separate build-time model in Section 33. Personal
credentials retain the stronger OS-vault boundary above.

## 9. Backend/proxy conclusion

**VERIFIED:** The provider documents direct GET access for frontends and utilities, so direct desktop
access is technically contemplated. It does not publish a requirement for a backend.

**UNRESOLVED:** No public first-party rule found explicitly permits or prohibits a project-owned
backend, credential-hiding proxy, or many end users sharing one developer credential behind a
service. Such a proxy changes attribution, IP/account quota, abuse, privacy, availability, operating
cost, and the project's data-controller responsibilities. It must not be assumed acceptable.

**RETROFRONTIER V1 PROJECT DECISION:** Use direct Rust-to-ScreenScraper access and no M5 proxy. Do
not build a proxy merely to create the appearance that a desktop-embedded application credential is
confidential. Revisit only if ScreenScraper later requires a different topology.

## 10. Quota and concurrency model

**VERIFIED:** Quota management by client software is mandatory. Limits vary with user level and
financial/database contribution. The scheduler must consume returned values rather than rely only
on static defaults.

The live guest profile observed on 2026-08-27 returned:

| Field                 |   Observed value | Meaning                                      |
| --------------------- | ---------------: | -------------------------------------------- |
| `maxthreads`          |                1 | Concurrent requests allowed for this profile |
| `maxdownloadspeed`    |              128 | Download speed in KiB/s per documentation    |
| `maxrequestspermin`   |             3072 | Maximum API requests/minute                  |
| `maxrequestsperday`   |            10000 | Maximum API requests/day                     |
| `maxrequestskoperday` |             1000 | Maximum negative lookup requests/day         |
| `requeststoday`       | 0 at observation | Current daily request counter                |
| `requestskotoday`     | 0 at observation | Current daily negative counter               |

The documentation has inconsistent spellings (`maxrequestspermin` and
`maxrequestsperdmin`); the observed JSON used `maxrequestspermin`. Parsers must deliberately
tolerate this beta schema, with fixtures for both known spellings and unknown fields.

**VERIFIED:** `ssinfraInfos` exposes CPU/load, recent API traffic, scraper counts, global member and
non-member thread pools, and flags closing the API to non-members or non-contributing members.

**VERIFIED:** Documentation describes the API day as French time and elsewhere as GMT+1. It returns
counters and maxima, not a quota reset timestamp or next-allowed-request timestamp.

**UNRESOLVED:** DST behavior and the precise reset instant are not defined. No public documentation
was found for media request accounting versus metadata request accounting.

**RECOMMENDATION:** Persist the last quota snapshot and deferred reason. Enforce the returned
`maxthreads`, a rolling-minute token bucket capped by the returned minute maximum, and daily and
negative budgets. Treat values as mutable. The scheduler must also honor infrastructure closure and
must not issue work merely because a local timer elapsed.

## 11. Error taxonomy

| Signal                    | Provider meaning                                                         | Future M5 classification                                                         |
| ------------------------- | ------------------------------------------------------------------------ | -------------------------------------------------------------------------------- |
| HTTP 400                  | Missing/invalid field, filename/path, or hash                            | Permanent configuration/request failure; do not retry unchanged                  |
| HTTP 401                  | API closed to non-members/inactive members under high load               | Transient provider restriction/unavailable; preserve job                         |
| HTTP 403                  | Developer login error; live behavior also used it for invalid user login | Authentication failure; distinguish developer vs user from sanitized body        |
| HTTP 404                  | No matching game/ROM/folder                                              | Deterministic no-match for the submitted evidence                                |
| HTTP 423                  | API completely closed due to severe server problems                      | Provider maintenance/unavailable                                                 |
| HTTP 426                  | Client blacklisted, non-conforming, or obsolete                          | Unsupported/outdated client; no retry until software/config changes              |
| HTTP 429                  | User concurrency/minute limit or global thread pool reached              | Concurrency backpressure or minute/global-capacity deferral; inspect safe reason |
| HTTP 430                  | Daily scrape quota exceeded                                              | Daily-quota deferral                                                             |
| HTTP 431                  | Daily negative/no-match quota exceeded                                   | Negative-lookup-quota deferral                                                   |
| Transport/DNS/TLS/timeout | No valid provider response                                               | Transient network/provider failure unless configuration is permanently invalid   |
| HTTP 5xx                  | Undocumented generic server failure                                      | Transient provider failure; bounded backoff                                      |
| Malformed successful body | Beta schema or provider defect                                           | Transient provider/protocol failure; retain last-known-good data                 |

**VERIFIED:** The controlled 404 response did not include `Retry-After`. No official text found
documents `Retry-After`, reset timestamps, or next-allowed timestamps for any status. Rate-limit
errors were not intentionally triggered.

## 12. Retry and defer implications

**RETROFRONTIER V1 PROJECT DECISION:** Do not use a generic fixed sleep.

- Never retry unchanged 400, unresolved 403, or 426 requests automatically.
- Cache 404 against the exact system and evidence version; do not turn a hash miss into a title
  match automatically.
- For network/5xx/423 failures, use bounded exponential backoff with jitter and persist the next
  attempt, attempt count, and last safe error category.
- For 429, lower active concurrency immediately and defer according to local rolling-minute state.
  If the provider supplies no time, use bounded jittered probing; never invent an authoritative
  reset time.
- For 430/431, persist a provider-day deferral and recheck conservatively with bounded jitter. The
  exact DST/reset rule is not required for correctness.
- A manual user retry may reprioritize a job but may not bypass provider budgets.
- Every refresh writes a new valid snapshot atomically; failure keeps the previous snapshot.

## 13. Metadata licensing

**VERIFIED:** ScreenScraper's site displays a CC BY-NC-SA 4.0 notice and lists multiple information
sources, including its community and third parties. Registration asks contributors to accept sharing
their texts, pictures, and videos under a Creative Commons license and makes them downloadable by
software through the API.

**VERIFIED:** The API returns titles, localized synopses, dates, developer, publisher, genres,
players, regions, ratings/classifications, and other fields.

**UNRESOLVED:** The public notice does not establish, field by field, whether every API value is
owned/licensed by ScreenScraper under CC BY-NC-SA 4.0, whether third-party source terms differ, or
what attribution must accompany normalized local records. It also does not explicitly address
persistent local storage, indefinite offline use, database backup/migration, refresh/deletion
obligations, or redistribution of a user's cache.

**RETROFRONTIER V1 PROJECT DECISION:** This is product/architecture research, not legal advice. M5
does not treat the site footer as a field-by-field grant. It stores a narrow normalized,
source-attributed, replaceable record for local/offline use, forbids redistribution, and keeps
user-owned overrides separate. Portable provider-cache export remains deferred.

## 14. Media licensing

**VERIFIED:** ScreenScraper provides API endpoints for game/system/group/company images, game/system
videos, and manuals. `jeuInfos` exposes screenshots, fanart, wheels/logos, box/support imagery,
flyers, manuals, bezels, and other categories. The site credits different sources by category.

**VERIFIED:** Media endpoints are download endpoints and accept local CRC32, MD5, and SHA-1 values.
If unchanged, they can return `CRCOK`, `MD5OK`, or `SHA1OK`; they can also return `NOMEDIA`.
Metadata responses expose media checksums. This demonstrates technical support for a local media
copy and refresh comparison.

**UNRESOLVED:** Technical download support is not a complete permission statement. Public material
does not clearly establish category-specific local-cache/offline/backup rights, attribution,
expiry, revalidation, or redistribution rights for box art, screenshots, logos/wheels, fanart,
videos, and manuals. Source-specific rights may differ.

**RETROFRONTIER V1 PROJECT DECISION:** Limit M5 to one selected primary front cover/box-art asset.
Do not include videos, manuals, or other media merely because endpoints exist. Never bundle or
redistribute ScreenScraper media. Portable backup/migration of provider media remains deferred.

## 15. Attribution requirements

**VERIFIED:** The provider site identifies CC BY-NC-SA 4.0 and publishes general source/category
credits. That license normally requires attribution, a license link, change indication, and
share-alike treatment when covered material is shared.

**UNRESOLVED:** ScreenScraper does not publicly specify exact in-application attribution text,
placement, per-record/per-media source credit, link requirements, or whether a credits screen is
sufficient for API consumers.

**RETROFRONTIER V1 PROJECT DECISION:** Normalized/provider/media records retain provider and
available source-credit data. M6 must present visible attribution; exact copy and placement remain
an M6 requirement rather than an M5 backend blocker.

## 16. Cache and offline requirements

**VERIFIED:** Media checksums and unchanged responses are a provider-supported change-detection
mechanism. No public TTL, expiry, mandatory refresh interval, or deletion notification was found.

**SECURITY FINDING:** Raw responses can contain authenticated media URLs with `devid` and
`devpassword`. An unredacted raw-response cache would become a credential store and could leak via
logs, backups, diagnostics, or SQLite inspection.

**UNRESOLVED:** Rights to cache normalized metadata, raw responses, lookup/no-match results, media
URLs, and downloaded media are not sufficiently stated. URLs may also be secret-bearing and should
not be durable identifiers.

**RETROFRONTIER V1 PROJECT DECISION:** Apply this cache model:

- `fresh`: valid provider snapshot inside a product-defined refresh interval;
- `stale`: valid last-known-good snapshot due for refresh, still displayable;
- `expired`: use only if provider terms define a hard expiry; no such rule is verified;
- `offline`: display permitted local last-known-good data and make no HTTP calls;
- `failedRefresh`: retain data and persist a safe error/next attempt;
- `noMatch`: bind to exact evidence and use a conservative product TTL to avoid wasting negative
  quota while allowing provider database improvements;
- media: store provider ID/type/region/checksums/local path and use checksum validation on refresh;
- raw payload: avoid by default; use only synthetic/redacted parser fixtures.

**Invariant:** A provider refresh failure must never delete the last valid metadata, cached media,
or any local library state.

## 17. V1 system mapping

The source is the live first-party `systemesListe` response on 2026-08-27.

| RetroFrontier `SystemId` | RetroFrontier system      | ScreenScraper ID | ScreenScraper name      |
| ------------------------ | ------------------------- | ---------------: | ----------------------- |
| `nes`                    | NES                       |                3 | NES                     |
| `snes`                   | SNES                      |                4 | Super Nintendo          |
| `nintendo_64`            | Nintendo 64               |               14 | Nintendo 64             |
| `game_boy`               | Game Boy                  |                9 | Game Boy                |
| `game_boy_color`         | Game Boy Color            |               10 | Game Boy Color          |
| `game_boy_advance`       | Game Boy Advance          |               12 | Game Boy Advance        |
| `mega_drive`             | Sega Mega Drive / Genesis |                1 | Megadrive (US: Genesis) |
| `playstation`            | PlayStation               |               57 | Playstation             |
| `sega_saturn`            | Sega Saturn               |               22 | Saturn                  |
| `sega_dreamcast`         | Sega Dreamcast            |               23 | Dreamcast               |
| `nintendo_gamecube`      | Nintendo GameCube         |               13 | Gamecube                |

**RECOMMENDATION:** This mapping belongs in the ScreenScraper adapter. The provider-neutral
`SystemCatalog` must not acquire ScreenScraper IDs.

## 18. Matching API

**VERIFIED:** `jeuInfos` accepts:

- ScreenScraper `systemeid`;
- CRC32, MD5, and SHA-1;
- `romtaille` in bytes;
- basename/folder name in `romnom` (paths are rejected);
- `romtype` for ROM, ISO, or folder semantics;
- serial number;
- ScreenScraper game ID.

The documentation requires at least one hash plus size absent a provider exemption and says all
three hashes are best. A game-ID request omits ROM information. The returned game includes game ID,
top-level ROM ID, system, normalized metadata, media, a list of known ROMs, and a matched `rom`
record with its own ROM ID, filename, size, CRC32, MD5, SHA-1, support number and support count.

**VERIFIED:** The official documented CRC32/size example returned game ID `3`, a matched ROM record,
and all three stored ROM hashes. This proves the response can expose evidence for an exact provider
ROM record; it does not prove undocumented collision resolution rules.

**VERIFIED:** `jeuRecherche` takes a name and optional system ID. Documentation limits results to 30
and says they are ordered by probability. A live search returned eight ordered candidates, but its
candidate objects had no score/probability field.

## 19. Deterministic matching evidence

**RETROFRONTIER V1 PROJECT DECISION:** A future match is deterministic only when `jeuInfos` returns a concrete matched
ROM record whose system, size, and submitted strong hash evidence agree with current M4 content.
Prefer SHA-1 + MD5 + CRC32 + size + basename because the provider recommends all hashes. SHA-1 or
MD5 plus size may establish an exact provider-record match. CRC32 plus size is weaker due to
collision risk and should be recorded distinctly.

Store provider game ID and matched ROM ID separately. The live response had different top-level and
nested ROM-ID values, so the adapter must preserve schema location and fixtures rather than assume
they are aliases.

Filename, local title, region text, and search ordering are not deterministic. A game-ID-only fetch
is identity retrieval after a match, not new matching evidence. ScreenScraper IDs never become
RetroFrontier `GameId`.

**UNRESOLVED:** The provider does not document hash precedence, conflicting-hash behavior, collision
handling, or a formal exact-match flag. The adapter must validate returned evidence instead of
trusting HTTP success alone.

## 20. Heuristic matching behavior

**VERIFIED:** `jeuRecherche` is a name search, returns at most 30 candidates, and orders them by an
undocumented probability. Neither the documentation nor the observed response defines a numeric
score, threshold, tie rule, or automatic-match guarantee.

**RETROFRONTIER V1 PROJECT DECISION:** Search results remain candidates/ambiguous state for later user selection. Do
not invent a score or threshold, and do not silently turn the first result into a durable
high-confidence match. A user-selected provider game records `heuristicUserConfirmed`, not
hash-exact evidence.

## 21. Single-file findings

**VERIFIED:** Standard cartridge/single-file systems use `romtype=rom`; disc mappings use
`romtype=iso`. `jeuInfos` expects basename, size, and at least one well-formed hash, with all three
recommended.

**RETROFRONTIER V1 PROJECT DECISION:** For an M4 `single_file`, send provider system ID, correct provider ROM type,
basename, byte size, SHA-1, MD5, and CRC32. Validate the returned matched-ROM hashes and size. The M4
fields are sufficient for ordinary provider-known files.

**CAVEAT:** The live GameCube system lists `iso,gcz`, while M4 also accepts RVZ. Direct RVZ file
hashing is not documented by ScreenScraper and must not be presumed equivalent to an ISO/provider
ROM record.

## 22. CHD findings

**VERIFIED:** ScreenScraper's current system list includes `chd` for PlayStation, Saturn, and
Dreamcast, and categorizes those systems as `romtype=iso`/CD. It does not list CHD for GameCube.

**UNRESOLVED:** No first-party public text states whether `jeuInfos` expects the compressed `.chd`
hash/size, decompressed source image, a specific track, the largest file, or another canonical
representation. Generic file/ISO wording is insufficient to choose safely.

**RETROFRONTIER V1 PROJECT DECISION:** Defer automatic CHD matching. Whole-file CHD hashes may not
be treated as canonical merely because ES-DE sends a selected file's MD5. Name search may produce
heuristic candidates. A later confirmed representation can use M4 evidence or a bounded read-only
Rust derivation service without redesigning M4.

## 23. CUE/BIN findings

**VERIFIED:** ScreenScraper lists `cue` and `bin` for PlayStation, Saturn, and Dreamcast. The general
API documents one file/ISO or a folder, and says folder hashes correspond to the largest file. It
does not describe CUE track selection.

**UNRESOLVED:** Public material does not say whether identity is the CUE descriptor, first BIN, data
track, largest track, all tracks, directory aggregate, serial, or another representation. It also
does not define total-size semantics.

**RETROFRONTIER V1 PROJECT DECISION:** Defer automatic CUE/BIN matching. M4's ordered
descriptor/track memberships can implement a later rule without changing M4, but V1 guesses no
canonical member. Name search may produce heuristic candidates. Confirmed serial extraction could
become separate evidence later.

## 24. GDI findings

**VERIFIED:** Dreamcast's system list includes `gdi`, track-like extensions, and `romtype=iso`.

**UNRESOLVED:** No first-party public rule identifies whether ScreenScraper hashes the `.gdi`
descriptor, one data track, the largest track, all tracks/folder contents, or another image. Size and
serial semantics are also unstated.

**RETROFRONTIER V1 PROJECT DECISION:** Defer automatic GDI matching. M4's ordered GDI
descriptor/track memberships preserve future inputs, but V1 guesses no canonical member and does
not infer that CUE and GDI share one. Name search may produce heuristic candidates.

## 25. M3U and multi-disc findings

**VERIFIED:** ScreenScraper lists `m3u` for PlayStation, Saturn, and Dreamcast. Game responses can
contain multiple ROM records under one game. Each ROM exposes `romnumsupport` and
`romtotalsupport`; media supports can also be numbered. This is first-class disc/support data at the
provider game level.

**UNRESOLVED:** The documentation does not say that the M3U file itself is a lookup identity or hash
target, nor how clients should submit a playlist. It does not specify whether every disc should be
looked up independently, how partial matches are judged, or how duplicate/regional multi-disc
records are grouped.

**RETROFRONTIER V1 PROJECT DECISION:** Never hash the M3U as game identity. Automatic M3U matching
is deferred until its owned discs use a supported provider representation. The eventual rule is:

- look up every disc independently;
- accept one provider game only when all resolved discs agree and support numbers/counts are
  consistent;
- keep a deferred/partial state when only some discs resolve;
- reject conflicts when discs resolve to different games;
- keep ambiguous individual discs as candidates.

RetroFrontier must refuse to guess when multi-disc evidence conflicts.

## 26. M4 content-replacement implication

Direct code inspection confirms same-path replacement behavior:

1. `ContentFile` is selected first by `(root, relative_path)` and updated in place, so its ID remains.
2. The unit is selected by same system/kind and primary path/member, so `ContentUnitId` remains.
3. The unit retains its existing `GameId`.
4. A successful rehash replaces CRC32, MD5, and SHA-1.
5. The fingerprint is recomputed from system, kind, ordered roles, and member hashes, so it changes.

This path-preserving behavior is intentional local identity, not proof that an old provider match
is still valid.

**RECOMMENDATION:** A provider match must bind to content unit, system, hashes/size, M4 fingerprint,
and an evidence-schema version. When current evidence differs, mark the match `stale` or
`needsRevalidation`; retain last-known-good metadata but stop presenting the match as trusted.

## 27. M4 M3U ownership implication

**VERIFIED:** The pre-M5 cleanup on `main` closed the former identity prerequisite. A new M3U uses
persisted member ownership and exact fingerprint evidence to retain one predecessor `GameId` only
when all applicable evidence identifies that same game. Multiple owners or conflicting evidence
remain separate, create an `ambiguousReconciliation` issue, and are never resolved by ordering.
The decision is stable across repeat scans and restart.

Provider identity remains a separate M5 concern. The playlist bytes are not provider identity;
only reliable evidence from owned disc content may establish the provider game.

## 28. Other deferred M4 concerns

| Concern                                   | Classification for metadata work                        | Reason                                                                                |
| ----------------------------------------- | ------------------------------------------------------- | ------------------------------------------------------------------------------------- |
| M3U ownership/ghost games                 | Closed by pre-M5 identity cleanup                       | One predecessor is retained only with unambiguous persisted ownership evidence        |
| Contested move identity reporting         | Closed by pre-M5 identity cleanup                       | One-to-one evidence is required before identity reuse                                 |
| SQLite WAL/busy timeout/write concurrency | Should address during M5 infrastructure                 | Queue/refresh writers increase contention; not an identity blocker itself             |
| Schema invariants                         | Should address with M5 migrations                       | Add constraints/indexes for provider/evidence tables and review cross-row assumptions |
| Full rehash performance                   | Safe to defer to M6/later                               | Expensive, but successful rehash gives strong staleness evidence                      |
| Scan-run/scan-issue retention             | Safe to defer to M6/later                               | Storage/diagnostics issue, not provider identity                                      |
| Full library snapshot scalability         | Safe to defer to M6 UI                                  | IPC/UI scaling issue, not adapter correctness                                         |
| Windows/case-insensitive paths            | Safe to defer to platform qualification, before release | Hash/fingerprint evidence limits metadata risk; still needs platform tests            |

## 29. Future M5 architecture recommendation

This is the constrained architecture approved for M5 implementation. This spike does not implement
it.

```text
thin Tauri commands
        |
MetadataApplicationService (Rust)
        |
match/evidence policy --- persistent job scheduler --- cache/refresh policy
        |                         |
MetadataProvider trait       Metadata repositories
        |                         |
ScreenScraper adapter        SQLite + owned media cache
        |
redacting HTTP client + credential source + provider quota state
```

### Domain and persistence boundaries

- Local identity remains M4 `Game` → `ContentUnit` → ordered `ContentFile`.
- `ProviderIdentity`: provider, provider game ID, provider ROM ID and schema version.
- `MatchEvidence`: content unit, system, hashes, size, content fingerprint, evidence version,
  deterministic/heuristic/user-confirmed classification, status, and timestamps.
- `NormalizedMetadata`: provider-independent fields required by M6.
- `ProviderSnapshot`: source/provenance and only permitted semi-raw fields; avoid raw bodies.
- `MediaAsset`: provider/type/region/support/source credit/checksums/local path/state.
- `MetadataJob`: persistent intent, priority, state, attempts, safe error category, deferred-until,
  evidence version and quota reason.
- `UserMetadataOverride`: separate records with field-level precedence; refresh never overwrites them.

### Service responsibilities

- Provider abstraction exposes typed lookup/search/fetch/media operations and typed failures, not
  HTTP statuses or ScreenScraper fields to the UI/domain.
- The ScreenScraper adapter owns V1 mapping, beta-compatible parsing, evidence validation, safe
  error parsing, URL redaction, and media checksum handling.
- The application service creates/revalidates matches and atomically applies only valid snapshots.
- One persistent scheduler owns concurrency, rolling-minute, daily and negative budgets. Jobs are
  event/deadline-driven, not polled with generic sleep.
- Offline mode performs no HTTP, retains deferred jobs, and reads permitted cached data.
- Media lives in an app-owned cache outside runtime and user ROM/save paths. SQLite stores metadata
  about files, not binary blobs.
- React receives normalized state only. Credential submission is write-only; secrets never appear
  in read IPC.

Provider IDs remain replaceable relationships and never become `GameId`. Failed refresh never
deletes a game, content, user override, or last-known-good provider snapshot.

## 30. Required M5 testing plan

Normal CI uses synthetic/redacted fixtures and a fake provider. Live tests are separately opt-in,
secret-backed, read-only, and never required to pass CI.

### Provider adapter

- parse representative redacted JSON/XML fixtures;
- reject malformed bodies and wrong types safely;
- tolerate missing optional fields and unknown additional fields;
- test all eleven system mappings and reject unmapped systems;
- prove media URLs and errors are redacted before logging/persistence.

### Authentication

- valid/invalid developer authentication;
- developer-only requests with absent user credentials;
- valid/invalid optional user authentication;
- distinguish developer and user 403 bodies;
- prove credentials are absent from logs, IPC reads, SQLite, fixtures, URLs in errors, and crash
  diagnostics.

### Queue and rate limiting

- enforce dynamic `maxthreads`;
- enforce rolling minute quota including known field spelling variants;
- enforce daily and negative quotas independently;
- classify 429 reason and reduce concurrency;
- persist deferral, attempt, quota snapshot and restart state;
- reconfigure safely when returned limits change.

### Retry

- network/TLS/timeout path;
- transient 5xx/423/401 provider restriction;
- permanent 400/403/426 path;
- 429 minute/concurrency defer, 430 daily defer, 431 negative defer;
- deterministic 404 no-match;
- bounded backoff, jitter, cap, and manual retry that cannot bypass quota.

### Offline

- assert zero HTTP calls;
- cached metadata and permitted cached media remain available;
- jobs remain persisted/deferred;
- local library state and last-known-good provider state are unchanged.

### Matching

- exact SHA-1 and exact MD5 provider-ROM matches;
- CRC32/size fallback is classified weaker;
- conflicting submitted/returned hashes are rejected;
- no match and negative-cache revalidation;
- ambiguous/name search never auto-attaches;
- provider-confirmed CHD, CUE/BIN, GDI and M3U strategies;
- consistent, partial, conflicting and ambiguous multi-disc results;
- changed content hashes/fingerprint mark evidence stale;
- game-ID fetch cannot masquerade as content matching.

### Persistence and migration

- upgrade a populated M4 database to M5 without changing M4 IDs/state;
- restart persistence for matches, metadata, queue and media indexes;
- atomic refresh and last-known-good retention;
- stale evidence after same-path content replacement;
- user overrides survive provider refresh/removal;
- migration rollback/forward behavior appropriate to project policy.

### Failure isolation

For every provider error, assert that `Game`, `ContentUnit`, and `ContentFile` remain, local
availability is unchanged, and a failed refresh cannot destroy last-known-good metadata/media.

## 31. Questions for optional ScreenScraper maintainer clarification

These questions remain worthwhile, but none is a prerequisite for the constrained V1 described
below. Send them with RetroFrontier's repository URL, GPL-3.0-or-later/no-charge distribution
model, platforms, intended direct desktop architecture, and proposed attribution. A provider answer
can narrow or expand later behavior; it must not be retroactively implied by this decision.

1. May RetroFrontier publish or embed its `devid`/`devpassword` in open-source code and public
   desktop binaries, acknowledging that they cannot remain secret? If not, what supported model
   should it use?
2. Do you permit or require direct desktop clients? Is a project-owned credential-hiding proxy
   permitted, including many installations sharing the developer credential behind it?
3. What exact `softname` format and version lifecycle should RetroFrontier use? Must releases be
   registered or approved before distribution?
4. May RetroFrontier persist normalized metadata locally for indefinite offline use and include it
   in user backup/migration? What attribution and refresh/deletion duties apply?
5. May RetroFrontier retain redacted raw or semi-raw responses? If so, for how long and under what
   attribution/license terms?
6. For each media category (box/cover, screenshot, logo/wheel, fanart, video, manual), may the app
   cache it locally, use it offline, and copy it in user backup/migration? What source-specific
   attribution, expiry/revalidation, and redistribution restrictions apply?
7. Does the CC BY-NC-SA 4.0 notice cover every metadata field and media returned by the API? If not,
   how can a client determine each item's source/license and required attribution?
8. For CHD on PlayStation, Saturn, and Dreamcast, which bytes and size must be submitted: the `.chd`
   file, decompressed image, a track, or another canonical representation?
9. For CUE/BIN, which descriptor/track(s), hashes, size, filename/romtype, or serial constitute the
   canonical `jeuInfos` request?
10. For Dreamcast GDI, which descriptor/track(s), hashes, size, filename/romtype, or serial constitute
    the canonical request?
11. For M3U/multi-disc, should each disc be queried independently? Is an M3U ever a lookup identity?
    How should clients validate support numbers/counts, partial matches, and conflicts?
12. For GameCube GCZ/RVZ/ISO, which representation is canonical and is direct RVZ lookup supported?
13. What is the exact quota reset timezone/instant (including DST), and are `Retry-After` or other
    next-request/reset timestamps ever returned for 429/430/431?

## 32. ES-DE reference implementation review

### Revision and sources

**OBSERVED ES-DE PRECEDENT:** The authoritative upstream project is
<https://gitlab.com/es-de/emulationstation-de>. Revision
`84679f733dc8c093c5f6fa3bcd3bb59913979985` on `master` was inspected on 2026-08-27. Relevant
files were:

- `es-app/src/scrapers/ScreenScraper.h` and `ScreenScraper.cpp` — application credentials,
  request construction, parsing, quota observations, media selection, and platform mapping;
- `es-core/src/utils/StringUtil.cpp` — the reversible credential transformation;
- `es-core/src/HttpReq.h` — one-character build-platform identifiers;
- `es-core/src/Settings.cpp`, `es-app/src/guis/GuiScraperMenu.cpp`, and `USERGUIDE-DEV.md` — user
  account settings and clear-text settings persistence;
- `es-app/src/guis/GuiScraperSearch.cpp`, `es-app/src/scrapers/Scraper.cpp`, and `Scraper.h` — MD5
  search, result selection, retries, and local media writes;
- `CREDITS.md` and `LICENSE` — upstream licensing context. No ES-DE code, credential value,
  obfuscation constant, or platform table is copied into RetroFrontier.

The inspected ES-DE source is distributed under the MIT license in its root `LICENSE`. That permits
study of the implementation but does not license ScreenScraper metadata/media or convert ES-DE's
provider relationship into permission for RetroFrontier. This spike records independently stated
architecture observations only.

### Developer credential and topology

**OBSERVED ES-DE PRECEDENT:** ES-DE embeds its ScreenScraper developer identifier and password as
byte material and reconstructs them at runtime with a repeating-key XOR-style reversible
transformation. The key and transformed values are shipped together. The original credential is
therefore recoverable from source or a distributed binary; the mechanism is obfuscation against
accidental reading, not a confidentiality boundary.

ES-DE builds `softname` from the product name, `PROGRAM_VERSION_STRING`, and a one-character build
platform identifier, then URL-encodes it. Requests are constructed and sent directly from the
distributed client to ScreenScraper's API. There is no ES-DE credential-hiding proxy in this path.
Application and optional user credentials are query parameters. Returned media URLs are consumed
by the client and local files are downloaded directly.

This establishes a mature public-client precedent. It does **not** establish a verified
ScreenScraper rule allowing RetroFrontier to distribute credentials in the same manner.

### Optional user credentials

**OBSERVED ES-DE PRECEDENT:** ES-DE scraping works without a user account. Its UI exposes optional
ScreenScraper username/password fields plus a switch controlling whether to use them. When enabled
and non-empty, the values are appended to direct ScreenScraper requests. ES-DE masks password entry
in the UI but documents that the value is stored in clear text in `es_settings.xml`. The response's
user/quota information is used to report account validity and allowance.

**RETROFRONTIER V1 PROJECT DECISION:** RetroFrontier will improve this boundary. A personal
ScreenScraper account is optional. Rust owns credential operations; persistent personal credentials
use the OS credential vault/keychain. SQLite may contain non-secret configuration or an opaque
reference, never the password. React may later submit credentials through a narrow write command,
but normal read IPC returns status only and never returns a secret. This is architecturally sound
and consistent with ADR-002's Rust persistence boundary.

### Metadata, media, and attribution behavior

**OBSERVED ES-DE PRECEDENT:** ES-DE normalizes and stores game name, rating, description, release
date, developer, publisher, genre, and player count in its gamelist metadata. It downloads local
3D boxes, back covers, front covers, fan art, marquees/wheels, physical-media images, screenshots,
title screens, videos, and PDF manuals, and can generate miximages from several assets. Its default
media tree is `downloaded_media/<system>/<media type>`, with a user-configurable root. Metadata and
media survive ordinary upgrades and support offline display.

The inspected ES-DE repository and guide identify and link ScreenScraper as a scraper service. No
precise in-application per-record attribution formula or category-specific legal rule was found.
This local caching is established implementation practice, not legal proof of RetroFrontier's
rights.

### Matching behavior and differences

**OBSERVED ES-DE PRECEDENT:** In automatic mode, ES-DE optionally computes MD5 over the selected
game file and sends MD5 plus that file's size to `jeuInfos`. It does not hash directories, skips
hashing above a configurable maximum, and otherwise falls back to provider name search. It compares
the returned ROM MD5 to the local MD5. Interactive/refined searches use `jeuRecherche` and present
candidates. The guide states hash searching does not work for directories, scripts, shortcuts, or
M3U files.

ES-DE keeps a compile-time map from its platform identifiers to ScreenScraper numeric system IDs.
It can issue a less constrained search when a platform is absent from that map. RetroFrontier will
instead keep provider IDs inside the adapter and refuse automatic attachment unless exactly one
provider system mapping exists.

There is no format-specific canonicalization in this path: a selected CHD, CUE, GDI, or RVZ file is
treated like another file when eligible for MD5 hashing; an M3U can only fall back to name search.
ES-DE's library model and automatic first-result behavior do not implement RetroFrontier's ordered
multi-file evidence or conservative conflict rules.

RetroFrontier differs intentionally: M4 already has SHA-1, MD5, CRC32, size, a content-unit
fingerprint, and ordered membership. M5 will submit all supported evidence where practical,
validate a concrete returned ROM record, never auto-attach a name-search first result, and defer
formats whose lookup representation is not established.

ES-DE consumes returned daily counters but the inspected ScreenScraper path does not assume an
exact quota reset timezone or DST rule. Its generic HTTP layer has coarser retry/status handling
than the provider-aware persistent scheduler required by RetroFrontier.

## 33. RetroFrontier V1 decisions

Every item in this section is a **RETROFRONTIER V1 PROJECT DECISION**, not a verified provider rule.

### Credentials and topology

- M5 uses direct Rust-backend to ScreenScraper communication. It does not introduce a
  RetroFrontier cloud/backend proxy.
- RetroFrontier uses only its own issued application developer credentials and never ES-DE's.
- The application developer credential is an extractable application credential, not a
  cryptographic secret boundary. Optional obfuscation may reduce accidental disclosure but must
  never be described as security.
- Development values come from the ignored local `.env` or process environment.
- Release values come from a protected CI/release secret and are injected at compile/build time.
  No real value or generated credential-bearing source is committed. The released binary remains
  inherently recoverable.
- Personal user credentials are optional and separate. Persistent values use OS vault/keychain
  storage under Rust ownership; normal IPC, SQLite, logs, URLs in errors, fixtures, diagnostics,
  and crash reports contain no secret.
- Use a stable product/version `softname` convention, include platform/build identity where useful,
  and treat HTTP 426 as a non-retryable client lifecycle signal. Exact provider registration
  expectations remain a residual policy question.

### Cache, media, and attribution

- Persist normalized metadata and the provider/source identifiers required to refresh it.
- Cache exactly one selected primary front cover/box-art asset for M5. Do not add screenshots,
  title screens, back covers, fan art, logos/wheels, physical media, videos, manuals, bezels, or
  miximages to M5.
- Allow ordinary offline application use of the local normalized snapshot and selected cover.
- Do not bundle provider data/media with releases, redistribute a user's cache, commit provider
  media, or use provider downloads as source-controlled fixtures.
- Do not persist raw authenticated API responses by default. Use synthetic/redacted parser
  fixtures. Do not persist credential-bearing media URLs; persist provider identity, media type,
  source/provenance, checksums, and owned local path.
- Provider refresh is replaceable and atomic. A failed refresh retains the last-known-good
  metadata and cover.
- General backup/export or migration of provider media is deferred until policy is clearer. Normal
  database schema migration in place may preserve the last-known-good normalized record; it must
  not create a distributable provider archive.
- M5 preserves `ScreenScraper` as provider identity plus available source/category provenance so
  M6 can present visible attribution. Exact UI wording, placement, link, license notice, and
  per-category credit are an M6 requirement if the provider has not prescribed a format. Unknown
  wording does not block the backend source boundary.

### Matching

- An automatic provider match is permitted only when the RetroFrontier system maps unambiguously,
  ScreenScraper returns a concrete ROM/content record, returned content evidence agrees with the
  current M4 evidence snapshot, and no conflicting provider result exists.
- Evidence preference is SHA-1 plus size, then MD5 plus size, then CRC32 plus size only when no
  stronger evidence is available and the provider record is unambiguous. Send all supported hashes
  where practical. A successful response without agreeing returned evidence is not deterministic.
- `jeuRecherche`, filename, title, and result ordering are heuristic. Results remain candidates;
  no arbitrary score or threshold is invented and no first result silently attaches.
- The playlist file is never provider identity. M3U discs may be looked up only when their own
  provider representation is supported. All reliable discs must resolve consistently to one
  logical provider game; inconsistent results are `ambiguous`, and unsupported/partial evidence is
  `deferred`. Result order is never a tie-breaker.
- Provider-specific state is one of `pending`, `matched`, `no_match`, `ambiguous`, `deferred`,
  `failed`, or `stale`/`needs_revalidation`. A provider operation never changes local library
  ownership or availability.

### Quota and stale evidence

- Consume returned quota maxima/counters dynamically, including known beta spelling variants, and
  persist quota snapshots and provider deferrals.
- Treat 429 concurrency/minute/global capacity, 430 daily quota, and 431 daily negative quota as
  distinct states. Avoid tight loops and use bounded, jittered conservative re-probing when the
  provider supplies no authoritative retry/reset time. Resume when the provider permits requests.
- Exact French-time/GMT+1 reset timezone and DST behavior are non-blocking operational detail; no
  undocumented timestamp is invented.
- A match stores a versioned evidence snapshot including system, content unit, relevant file IDs,
  hashes/sizes, ordered membership as applicable, and content-unit fingerprint. If current evidence
  differs, retain the local game and last-known-good provider data, mark the match
  `stale`/`needs_revalidation`, stop treating it as deterministic, and schedule online
  re-identification.

## 34. Content capability matrix

“Automatic” means the constrained rule above may attach without user confirmation. ES-DE behavior
in this table is precedent only.

| Content format            | M4 evidence available                                                      | ScreenScraper evidence confirmed                                                                                     | Observed ES-DE reference behavior                                        | Automatic deterministic matching allowed | Heuristic search allowed | M5 handling                                                                                                                |
| ------------------------- | -------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------ | ---------------------------------------- | ------------------------ | -------------------------------------------------------------------------------------------------------------------------- |
| Single-file cartridge ROM | Whole-file SHA-1, MD5, CRC32, size; unit fingerprint                       | `jeuInfos` accepts all three hashes plus size and returns a concrete ROM record                                      | Optional whole-file MD5 + size; name fallback                            | Yes                                      | Yes                      | Automatic only after returned strong evidence agrees; otherwise candidate/no-match                                         |
| CHD                       | Whole-file hashes/size; one-file CHD unit                                  | Provider system lists include CHD for PS1/Saturn/Dreamcast, but canonical compressed-vs-derived bytes are not stated | Whole selected CHD can be MD5-hashed; name fallback                      | No                                       | Yes                      | `deferred` for automatic V1; name candidates only                                                                          |
| CUE/BIN                   | Descriptor and ordered track hashes/sizes; unit fingerprint                | Provider lists CUE/BIN but does not define descriptor/track/aggregate identity                                       | Whole selected CUE file can be MD5-hashed; no relationship model         | No                                       | Yes                      | `deferred` for automatic V1; name candidates only                                                                          |
| GDI                       | Descriptor and ordered track hashes/sizes; unit fingerprint                | Dreamcast lists GDI/track formats but no canonical member/aggregate rule                                             | Whole selected GDI file can be MD5-hashed; no relationship model         | No                                       | Yes                      | `deferred` for automatic V1; name candidates only                                                                          |
| M3U / multi-disc          | Playlist plus ordered owned disc membership and each member's hashes/sizes | M3U is listed and provider has support/disc records, but playlist/per-disc matching semantics are unstated           | M3U hash search is documented as ineffective; name fallback              | No                                       | Yes                      | Playlist never identity; automatic capability deferred until disc representations are supported; conflicts are `ambiguous` |
| GameCube RVZ              | Whole-file hashes/size; single-file unit fingerprint                       | GameCube list observed ISO/GCZ, not RVZ; no RVZ canonical lookup rule                                                | Selected RVZ can be MD5-hashed generically if under limit; name fallback | No                                       | Yes                      | `deferred` for automatic V1; name candidates only                                                                          |

Partial support is intentional. Single-file deterministic enrichment can ship without guessing disc
or container identity. A later provider-confirmed rule can enable a row without changing M4 IDs or
the provider-neutral matching contract.

## 35. Provider failure isolation invariant

No ScreenScraper operation may delete or hide a `Game`, alter local availability, change `GameId`,
modify `ContentUnit` ownership, or modify `ContentFile` identity. Metadata enrichment is downstream
of M4. Provider errors and stale evidence affect only provider-specific state and last-known-good
provider snapshots.

## 36. Previous blocker reclassification

| Previous item                                | Classification                             | Reason                                                                                                                                                                                   |
| -------------------------------------------- | ------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Production developer credential distribution | CLOSED — RETROFRONTIER V1 PROJECT DECISION | Build-time injection avoids source disclosure while acknowledging binary recoverability; ES-DE is precedent, not permission                                                              |
| Direct client vs backend/proxy               | CLOSED — RETROFRONTIER V1 PROJECT DECISION | Direct Rust client; no M5 cloud service or false secret boundary                                                                                                                         |
| Software eligibility                         | CLOSED — VERIFIED PROVIDER RULE            | ScreenScraper explicitly allows entirely free distributed applications and requires software presentation for issued developer credentials; RetroFrontier remains no-charge GPL software |
| `softname` lifecycle                         | CLOSED — RETROFRONTIER V1 PROJECT DECISION | Stable product/version/platform identity; 426 blocks unchanged retry; provider release-registration detail remains residual risk                                                         |
| Metadata persistence rights                  | CLOSED — RETROFRONTIER V1 PROJECT DECISION | Store normalized, replaceable, attributed V1 data only; no redistribution claim                                                                                                          |
| Offline metadata use                         | CLOSED — RETROFRONTIER V1 PROJECT DECISION | Last-known-good normalized data remains available locally offline                                                                                                                        |
| Backup/migration rights                      | NON-BLOCKING — CAPABILITY DEFERRED         | No portable export/redistribution of provider cache in V1; ordinary in-place schema migration may preserve state                                                                         |
| Raw-response caching                         | CLOSED — RETROFRONTIER V1 PROJECT DECISION | Disabled by default because responses/URLs may carry credentials                                                                                                                         |
| Media caching                                | CLOSED — RETROFRONTIER V1 PROJECT DECISION | One local front cover only, replaceable and non-redistributed                                                                                                                            |
| Attribution                                  | CLOSED — RETROFRONTIER V1 PROJECT DECISION | Persist provider/source provenance in M5; exact visible formula is an M6 requirement                                                                                                     |
| Redistribution                               | CLOSED — RETROFRONTIER V1 PROJECT DECISION | No bundling, cache redistribution, committed media, or provider fixture corpus                                                                                                           |
| CHD matching                                 | NON-BLOCKING — CAPABILITY DEFERRED         | Heuristic candidates allowed; automatic matching waits for a canonical provider representation                                                                                           |
| CUE/BIN matching                             | NON-BLOCKING — CAPABILITY DEFERRED         | Ordered M4 evidence exists, but no member/aggregate representation is assumed                                                                                                            |
| GDI matching                                 | NON-BLOCKING — CAPABILITY DEFERRED         | Ordered M4 evidence exists, but no member/aggregate representation is assumed                                                                                                            |
| M3U matching                                 | NON-BLOCKING — CAPABILITY DEFERRED         | Playlist is never identity; automatic matching waits for supported disc representations                                                                                                  |
| RVZ matching                                 | NON-BLOCKING — CAPABILITY DEFERRED         | Provider support/canonical representation is not established; heuristic candidates remain possible                                                                                       |
| Quota-reset timezone/DST                     | CLOSED — RETROFRONTIER V1 PROJECT DECISION | Dynamic counters, persisted deferral, separate status handling, and conservative probing remove dependence on an assumed reset instant                                                   |

None of the remaining provider-policy questions forces M5 to expose user secrets, guess provider
identity, mutate M4 ownership, redistribute assets, or depend on an undocumented reset clock.

## 37. Residual risks and deferred capability

### Provider-policy and legal residual risks

- ScreenScraper does not publicly say that every open-source application may distribute its issued
  developer credential in an extractable desktop client. RetroFrontier consciously accepts the
  direct-client model demonstrated by ES-DE, subject to using its own credential and provider
  response to any future clarification.
- Public material does not establish field-by-field metadata/media licensing, indefinite cache or
  backup rights, or an exact in-app attribution formula. V1 minimizes exposure, preserves source,
  and forbids redistribution. This is an architecture risk decision, not legal advice.
- The v2 API is labelled beta and may change without notice. Typed parsing, fixtures, safe unknown
  fields, atomic refresh, and last-known-good retention are required.

### Deferred capabilities

- Automatic CHD, CUE/BIN, GDI, M3U/multi-disc, and RVZ matching;
- portable backup/export of provider metadata or media;
- broad artwork, screenshots, logos, fan art, videos, manuals, and other media;
- exact M6 attribution presentation pending provider guidance;
- a proxy/cloud topology, which would require a separate product, privacy, abuse, quota, and
  provider-policy decision.

### Engineering blockers

None for the constrained V1. M5 implementation still requires normal migrations, repositories,
credential-vault adapters, HTTP redaction, a persistent scheduler, and tests; those are milestone
work, not unresolved prerequisites.

## 38. Final M5 readiness decision

ADR-007's provider-neutral architecture remains sound. M0.2 is complete because the remaining
uncertainties are explicitly accepted residual risks or bounded/deferred capabilities. M5 may begin
with one provider, direct Rust integration, RetroFrontier-owned application credentials, optional
OS-vault user credentials, provider-aware scheduling, deterministic single-file matching,
candidate-only heuristics, normalized metadata, one primary cover, offline cache, refresh, stale
evidence revalidation, and strict isolation from M4 library state.

**M5 READY: YES**
