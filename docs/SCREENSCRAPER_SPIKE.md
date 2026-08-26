# ScreenScraper research spike

Access date for all current web and API observations: **2026-08-27**.

Labels in this report have strict meanings:

- **VERIFIED** — supported by a first-party ScreenScraper page or a sanitized live response.
- **RECOMMENDATION** — a RetroFrontier design conclusion, not a provider rule.
- **UNRESOLVED** — the public provider material and safe live tests did not answer the question.

## 1. Executive conclusion

**M5 READY: NO**

ScreenScraper can technically return the game fields and media links M5 needs. Its Web API accepts
hash-and-size evidence, returns provider game and ROM identifiers, exposes quota counters,
represents multiple support/disc records, and provides media checksums. All eleven V1 systems have
verified ScreenScraper IDs.

The integration is not safe to implement yet. ScreenScraper requires developer credentials obtained
after presenting the software, but its public material does not say whether those credentials may be
published or embedded in an open-source desktop binary. A secret in such a binary is not
confidential. The provider also does not publicly define proxy use, raw/normalized response caching,
offline and backup rights, category-specific media rights/attribution, or canonical lookup evidence
for CHD, CUE/BIN, GDI, and M3U. These are implementation-blocking questions and require a direct,
written answer from ScreenScraper.

M4 also has one confirmed identity issue that should be corrected before durable metadata is
attached: adding an M3U after its discs were scanned creates a new game while leaving the previous
standalone game(s) historically present and unavailable. Contested move reporting should be handled
in the same focused pre-M5 correctness pass.

## 2. Research methodology

**VERIFIED:** The repository was refreshed against `origin/main` at `f451a87`, the M4 domain,
repository, scanner, application service, IPC, migrations, documentation, review fixes, and tests
were inspected, and 121 non-ignored Rust tests passed.

First-party sources were preferred. Controlled API work used an ignored, untracked `.env`; requests
were GET-only and limited to infrastructure, system-list, search, one official documented hash
example, and one synthetic no-match. Returned media URLs can contain developer credentials, so raw
responses are secret-bearing and were not retained. No media was downloaded and no provider state
was changed.

No suitable legal ROM fixture exists in the repository. Therefore no local CHD/CUE/GDI/M3U content
was sent or uploaded, and no claim about those formats is inferred from third-party scrapers.

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

**RECOMMENDATION:** Subject to provider confirmation, use a stable non-secret value containing both
product and version, for example `RetroFrontier/<semver>`, and treat HTTP 426 as a release-blocking
client compatibility signal. Do not encode user identity.

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

**RECOMMENDATION:** Later M5 may support optional credentials belonging to the individual user. The
application remains usable without an account, but user login can improve capacity and access when
guest/leecher access is restricted. RetroFrontier should state that credentials are sent directly
to ScreenScraper only under the provider-approved architecture.

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

**RECOMMENDATION:** Do not commit, embed, or distribute the current development credentials. Do not
assume the existence of development credentials authorizes production distribution. This blocker
must be resolved in writing by ScreenScraper.

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

The developer credential location depends on the provider's answer: direct-client permission may
still imply an intentionally public project credential, while a required confidential secret means
it cannot live in the desktop client.

## 9. Backend/proxy conclusion

**VERIFIED:** The provider documents direct GET access for frontends and utilities, so direct desktop
access is technically contemplated. It does not publish a requirement for a backend.

**UNRESOLVED:** No public first-party rule found explicitly permits or prohibits a project-owned
backend, credential-hiding proxy, or many end users sharing one developer credential behind a
service. Such a proxy changes attribution, IP/account quota, abuse, privacy, availability, operating
cost, and the project's data-controller responsibilities. It must not be assumed acceptable.

**RECOMMENDATION:** Prefer direct Rust-to-ScreenScraper access if ScreenScraper explicitly permits
the project credential to be distributed. Use a backend only if ScreenScraper requires or explicitly
approves it. Do not build a proxy merely to create the appearance that a desktop-embedded secret is
confidential.

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

**RECOMMENDATION:** Do not use a generic fixed sleep.

- Never retry unchanged 400, unresolved 403, or 426 requests automatically.
- Cache 404 against the exact system and evidence version; do not turn a hash miss into a title
  match automatically.
- For network/5xx/423 failures, use bounded exponential backoff with jitter and persist the next
  attempt, attempt count, and last safe error category.
- For 429, lower active concurrency immediately and defer according to local rolling-minute state.
  If the provider supplies no time, use bounded jittered probing; never invent an authoritative
  reset time.
- For 430/431, persist a provider-day deferral and recheck conservatively after the next provider
  day. The exact DST/reset rule still requires confirmation.
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

**RECOMMENDATION:** This is product/architecture research, not legal advice. Until confirmed, M5
must not assume that a generic site footer grants all persistence and migration rights. Store source
provenance per provider record and keep provider data replaceable. User-owned overrides remain
separate.

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

**RECOMMENDATION:** Limit initial M5 product scope to the smallest approved cover/box-art category
after confirmation. Do not include videos or manuals merely because endpoints exist. Never bundle
ScreenScraper media with RetroFrontier releases. Do not copy cached media into backup/migration
flows until ScreenScraper confirms that right.

## 15. Attribution requirements

**VERIFIED:** The provider site identifies CC BY-NC-SA 4.0 and publishes general source/category
credits. That license normally requires attribution, a license link, change indication, and
share-alike treatment when covered material is shared.

**UNRESOLVED:** ScreenScraper does not publicly specify exact in-application attribution text,
placement, per-record/per-media source credit, link requirements, or whether a credits screen is
sufficient for API consumers.

**RECOMMENDATION:** Design normalized/provider/media records to retain provider and source-credit
data, but do not finalize UI copy until ScreenScraper confirms the required attribution.

## 16. Cache and offline requirements

**VERIFIED:** Media checksums and unchanged responses are a provider-supported change-detection
mechanism. No public TTL, expiry, mandatory refresh interval, or deletion notification was found.

**SECURITY FINDING:** Raw responses can contain authenticated media URLs with `devid` and
`devpassword`. An unredacted raw-response cache would become a credential store and could leak via
logs, backups, diagnostics, or SQLite inspection.

**UNRESOLVED:** Rights to cache normalized metadata, raw responses, lookup/no-match results, media
URLs, and downloaded media are not sufficiently stated. URLs may also be secret-bearing and should
not be durable identifiers.

**RECOMMENDATION:** If provider permission is obtained:

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

**RECOMMENDATION:** A future match is deterministic only when `jeuInfos` returns a concrete matched
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

**RECOMMENDATION:** Search results remain candidates/ambiguous state for later user selection. Do
not invent a score or threshold, and do not silently turn the first result into a durable
high-confidence match. A user-selected provider game records `heuristicUserConfirmed`, not
hash-exact evidence.

## 21. Single-file findings

**VERIFIED:** Standard cartridge/single-file systems use `romtype=rom`; disc mappings use
`romtype=iso`. `jeuInfos` expects basename, size, and at least one well-formed hash, with all three
recommended.

**RECOMMENDATION:** For an M4 `single_file`, send provider system ID, correct provider ROM type,
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

**RECOMMENDATION:** Do not send M4's whole-CHD hashes as deterministic evidence until the provider
confirms them. If whole-file CHD identity is correct, M4 is sufficient. If internal/decompressed
evidence is required, M5 needs a bounded read-only Rust derivation service, not an M4 redesign.

## 23. CUE/BIN findings

**VERIFIED:** ScreenScraper lists `cue` and `bin` for PlayStation, Saturn, and Dreamcast. The general
API documents one file/ISO or a folder, and says folder hashes correspond to the largest file. It
does not describe CUE track selection.

**UNRESOLVED:** Public material does not say whether identity is the CUE descriptor, first BIN, data
track, largest track, all tracks, directory aggregate, serial, or another representation. It also
does not define total-size semantics.

**RECOMMENDATION:** M4's ordered descriptor/track memberships contain enough raw evidence to
implement a confirmed rule without changing M4. Until confirmation, no member should be guessed as
canonical. Serial lookup may be separate deterministic evidence if extraction rules are confirmed.

## 24. GDI findings

**VERIFIED:** Dreamcast's system list includes `gdi`, track-like extensions, and `romtype=iso`.

**UNRESOLVED:** No first-party public rule identifies whether ScreenScraper hashes the `.gdi`
descriptor, one data track, the largest track, all tracks/folder contents, or another image. Size and
serial semantics are also unstated.

**RECOMMENDATION:** M4's ordered GDI descriptor/track memberships are sufficient input after a rule
is confirmed. Do not infer that CUE and GDI share the same canonical member.

## 25. M3U and multi-disc findings

**VERIFIED:** ScreenScraper lists `m3u` for PlayStation, Saturn, and Dreamcast. Game responses can
contain multiple ROM records under one game. Each ROM exposes `romnumsupport` and
`romtotalsupport`; media supports can also be numbered. This is first-class disc/support data at the
provider game level.

**UNRESOLVED:** The documentation does not say that the M3U file itself is a lookup identity or hash
target, nor how clients should submit a playlist. It does not specify whether every disc should be
looked up independently, how partial matches are judged, or how duplicate/regional multi-disc
records are grouped.

**RECOMMENDATION:** Never hash the M3U as game identity. After provider confirmation of per-disc
canonical evidence:

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

Direct reconciliation analysis confirms the ghost-game scenario:

1. A standalone disc initially creates a CHD/CUE/GDI unit and provisional game.
2. Adding an M3U makes the scanner emit a playlist-owned M3U unit instead of standalone discs.
3. Existing-unit matching requires equal `ContentUnitKind`, so the old disc unit cannot become M3U.
4. The M3U fingerprint differs because it includes kind, playlist and ordered memberships.
5. A new game/unit is created; the old unit becomes missing/incomplete and its old game becomes
   unavailable, while both logical game rows remain historically present.

With multiple pre-existing disc games, the new playlist can leave multiple ghosts. This will split
favorites and later metadata across durable `GameId` values.

**RECOMMENDATION — pre-M5 correctness cleanup:** Preserve/transfer an existing `GameId` only when
ownership transfer is exact and unambiguous. Never merge by title/filename. If multiple previous
games could own the new playlist, choose none and emit an issue. Do not fix it in this spike.

## 28. Other deferred M4 concerns

| Concern                                   | Classification for metadata work                        | Reason                                                                                |
| ----------------------------------------- | ------------------------------------------------------- | ------------------------------------------------------------------------------------- |
| M3U ownership/ghost games                 | Must fix before durable M5 attachment                   | Can attach state to the wrong historical `GameId`                                     |
| Contested move identity reporting         | Must fix in focused pre-M5 cleanup                      | First lexical claimant is not fully reported; expose every ambiguity                  |
| SQLite WAL/busy timeout/write concurrency | Should address during M5 infrastructure                 | Queue/refresh writers increase contention; not an identity blocker itself             |
| Schema invariants                         | Should address with M5 migrations                       | Add constraints/indexes for provider/evidence tables and review cross-row assumptions |
| Full rehash performance                   | Safe to defer to M6/later                               | Expensive, but successful rehash gives strong staleness evidence                      |
| Scan-run/scan-issue retention             | Safe to defer to M6/later                               | Storage/diagnostics issue, not provider identity                                      |
| Full library snapshot scalability         | Safe to defer to M6 UI                                  | IPC/UI scaling issue, not adapter correctness                                         |
| Windows/case-insensitive paths            | Safe to defer to platform qualification, before release | Hash/fingerprint evidence limits metadata risk; still needs platform tests            |

## 29. Future M5 architecture recommendation

Do not implement this architecture until provider blockers are resolved and the focused M4 identity
cleanup lands.

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

## 31. Questions requiring ScreenScraper maintainer confirmation

Send these with RetroFrontier's repository URL, GPL-3.0-or-later/no-charge distribution model,
platforms, intended direct desktop architecture, and proposed attribution.

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

## 32. Final M5 readiness decision

ScreenScraper is technically promising and the provider-neutral architecture in ADR-007 remains
sound. Production credential distribution, provider-approved topology, cache/media rights,
attribution, and supported disc/container lookup evidence are unresolved. M0.2 remains open. No M5
production code should begin until written provider confirmation is recorded and any resulting
architecture decision is reviewed.

**M5 READY: NO**
