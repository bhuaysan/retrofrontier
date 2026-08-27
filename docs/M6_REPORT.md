# M6 Library UI Implementation Report

## A. Repository State

- Starting main commit: `9a1a1e3d8c38633c1c82bc95293c6a6024e94e93`
- Branch: `feat/m6-library-ui`
- Current HEAD: pending final report commit
- Merged to main: No
- Pushed: No
- Pre-existing untracked files: `M3_REVIEW.md`, `M4_REVIEW.md`, `M4_REVIEW_2.md`,
  `M4_REVIEW_3.md`, `M5_REVIEW.md`, `docs/M5_IMPLEMENTATION_REPORT.md`

## B. Overall M6 Status

- [x] M6.1 Backend Enablement
- [ ] M6.2 Shell / Empty Library / Scan UX
- [ ] M6.3 Library Browsing
- [ ] M6.4 Game Detail / Readiness
- [ ] M6.5 Metadata UX / Settings
- [ ] M6.6 Hardening / Accessibility / Documentation

Current phase: M6.2 Shell / Empty Library / Scan UX (not started)
Overall status: M6.1 is complete and ready for review; M6.2 has not started.

## C. M6.1 — Backend Enablement

### Implemented

- Added bounded `query_library`, `get_library_summary`, and `get_library_game_detail` application,
  repository, Tauri, and TypeScript contracts.
- Added title search with normalized metadata/local-title fallback, system/favorite/genre/region/
  availability filters, deterministic title ordering, total counts, and a hard list limit of 60.
- Added durable user-owned `game_user_state` favorites with a SQLite boolean check, FK ownership,
  set/clear commands, list/detail composition, and explicit delete restriction.
- Added the narrow `rfmedia://localhost/cover/<game-id>` custom protocol. Rust resolves the durable
  media row, enforces cache containment, rejects traversal/absolute/symlink escapes, validates PNG,
  JPEG, and WebP content, and never exposes the persisted cache path to React.
- Added `metadata-state-changed` as a minimal durable-state invalidation event carrying only game and
  provider identity.
- Added bounded latest-persisted-scan issue paging with total count, offset, default limit 50, hard
  limit 100, and deterministic newest-first ordering.
- Added safe granular content-root IPC errors for invalid paths, unavailable roots, non-directories,
  overlap, and invalid operations.

### Architecture decisions

- The existing full M4 snapshot remains stable as a diagnostic/domain contract; M6 UI consumers use
  bounded query-oriented projections instead.
- The list and detail projections do not select or serialize CRC32, MD5, SHA-1, content fingerprints,
  physical-file memberships, provider payloads, credentials, or authenticated URLs.
- User-owned favorite state is separate from scanner-owned library identity and provider-derived M5
  state. A game cannot be deleted while its user-state row remains.
- Cached media is addressed by stable local game identity through an application-owned custom
  protocol, not by a caller-supplied path or provider URL. No new dependency was added.
- The metadata event is an invalidation signal emitted after durable persistence; the future UI must
  refetch bounded authoritative state.
- No ADR was added: this slice uses the existing repository/application/Tauri boundaries and the
  existing Tauri 2 protocol capability without introducing a new cross-cutting architecture.

### Files/layers changed

- Rust domain/application/repository: bounded library DTOs and queries, favorites, issue paging,
  root-error mapping, and metadata event sink.
- Rust persistence/services: forward-only favorites migration, secure cover delivery, and metadata
  media serialization boundary.
- Tauri commands/bootstrap/configuration: thin commands, `rfmedia` protocol registration, CSP, and
  event wiring.
- TypeScript IPC: mirrored request/response/event contracts and wrappers only; no screen/component
  state was added.
- Documentation: architecture, scanner, metadata, development, backlog, README, and this report.

### Tests added

- Empty, single/multiple-page, offset boundary, hard-limit, total-count, deterministic-order,
  tie-breaker, search, blank search, system, availability, genre, region, metadata-state, favorite,
  cover-reference, and large synthetic-library query coverage.
- Bounded game-detail content summaries and serialized no-hash contract coverage.
- Favorite default/set/clear/persistence/filter/detail/invariant/delete behavior and scanner
  reconciliation preservation.
- Synthetic PNG/JPEG/WebP delivery, missing/invalid/unsupported media, traversal/absolute paths,
  symlink containment, MIME/content validation, and route parsing coverage.
- Metadata invalidation payload and durable processing coverage.
- Empty/bounded/paginated/deterministic large scan-issue coverage.
- Granular root path/overlap/invalid-operation and safe IPC error coverage.

### Verification

Focused and full Rust verification passes: 298 tests passed, 1 ignored, 0 failed. The complete
required frontend/Rust/release verification is recorded in section M.

### Known issues / deferred items

- The bounded issue page intentionally covers persisted issues from the latest scan run; transient
  watcher diagnostics remain on the legacy aggregate issue command.
- M6.1 exposes only the product-required title-ascending sort.
- A durable cover row may become unavailable between the list query and protocol request; the native
  protocol returns a safe 404 and the future UI should treat `coverRef` as an availability hint.
- M6.2 and later UI work is intentionally not started.

### Review status

Implementation complete and ready for review. Not merged or pushed.

## D. M6.2 — Shell / Empty Library / Scan UX

Not started. No implementation details are inferred here.

## E. M6.3 — Library Browsing

Not started. No implementation details are inferred here.

## F. M6.4 — Game Detail / Readiness

Not started. No implementation details are inferred here.

## G. M6.5 — Metadata UX / Settings

Not started. No implementation details are inferred here.

## H. M6.6 — Hardening / Accessibility / Documentation

Not started. No implementation details are inferred here.

## I. Architecture and Security Audit

- React has no SQLite, filesystem, scanner, provider, credential, or runtime access.
- SQL remains in repositories and Tauri commands remain thin.
- UI projections are bounded and omit physical hashes/fingerprints and provider internals.
- Cached-cover delivery accepts only stable game identity routes, enforces app-owned containment, and
  serves only validated supported image types.
- Provider URLs, credentials, raw responses, candidate lists, and queue internals do not cross the
  new list/detail/event/media boundaries.
- No dependency, copyrighted fixture, commercial ROM, BIOS, runtime binary, secret, or generated
  build artifact was added.

## J. Design Coverage

M6.1 establishes backend contracts for the existing M6 design handoff. It does not implement or
reinterpret the shell, empty state, library grid, cards, detail screen, settings, or responsive UI.

## K. Test Coverage

The new Rust coverage is synthetic/offline and includes large bounded query datasets, serialization
leakage checks, media security checks, durable user-state checks, event contract checks, scan-issue
pagination, and root-error taxonomy checks. Existing M4/M5 tests remain passing.

## L. Deferred Beyond M6

- M7 launch/process/play-session and per-game core behavior.
- M8 controller abstraction, focus graph, and on-screen keyboard.
- M9 save-game and save-state management.
- M10 packaging, signing, and release work.
- Existing documented non-blocking M5/M4 observations that are not correctness dependencies of M6.1.

## M. Final Verification

- `pnpm typecheck` — PASS (`tsc -b --pretty false`).
- `pnpm lint` — PASS (`eslint .`).
- `pnpm format:check` — PASS (all checked files use Prettier code style).
- `pnpm test` — PASS (2 test files, 4 tests).
- `pnpm build` — PASS (`vite build`, 29 modules transformed).
- `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` — PASS.
- `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings` — PASS.
- `cargo test --manifest-path src-tauri/Cargo.toml` — PASS (298 passed, 1 ignored, 0 failed).
- `cargo build --manifest-path src-tauri/Cargo.toml --release` — PASS (release application built).
- `pnpm tauri:build` — PASS (Tauri release application built at
  `src-tauri/target/release/retrofrontier`).
- `git diff --check` — PASS.

## N. Final M6 Verdict

`M6.1 COMPLETE — ready for review before M6.2`
