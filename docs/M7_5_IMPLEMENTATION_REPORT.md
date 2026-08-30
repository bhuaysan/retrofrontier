# M7.5 Implementation Report — Real Managed Runtime and Linux Qualification

Companion to [`M7_5_RUNTIME_QUALIFICATION.md`](M7_5_RUNTIME_QUALIFICATION.md), which holds the
release identities, licences, digests, trust model, and full qualification matrix. This document
records what changed, why, and how it was verified.

## Scope

M7.5 makes the existing M2 and M7 architecture work with real software. It adds no new
architecture and weakens none: no ADR was changed, no security control was relaxed, and no
production claim is made.

## What changed

### Fixed: real AppImage extraction

`adapters/runtime_archive.rs`. `find_squashfs_offset` accepted the first `hsqs` byte sequence in an
artefact. The official RetroArch AppImage runtime embeds the literal signature table
`hsqs\0sqsh\0shsq\0qshs` at offset 194 183, while the real SquashFS superblock begins at 944 632.
Every real AppImage would therefore have failed extraction with *"AppImage SquashFS is invalid"*.

Now each candidate offset is checked against the SquashFS 4.0 superblock contract — version 4.0,
block size equal to `1 << block_log` within a bounded log range, a known compressor id, a non-zero
inode count, and a declared `bytes_used` that fits in the remaining artefact — and the first
structurally valid candidate wins. Only fixed header fields are read, so a hostile artefact cannot
steer extraction to an arbitrary offset by planting magic bytes.

Two regression tests reproduce the decoy signature table and the overrunning-superblock case.

### Added: declarative Runtime Release definition

`release/linux-x86_64/runtime-release.json`, parsed by `release/definition.rs`. Every identity that
can affect download, extraction, validation, or launch is pinned: upstream URL, upstream length and
SHA-256, the derived artefact's own length and SHA-256, install paths, executable paths, approved
system mappings, and licences. Unknown fields are rejected, target names must be flat and unique,
and a component cannot derive from an undeclared input.

Three derivations exist, each deterministic and each pinned by digest:

- `upstream_file` — the upstream bytes become the target unchanged (all four cores).
- `seven_zip_member` — one named member is lifted out of a 7z container (the RetroArch AppImage).
- `zip_subtree_tar` — one zip subtree is repackaged as a deterministic tar rooted at that subtree
  (Dolphin's `Sys`, which extraction cannot re-root because it never rewrites archive paths).

### Added: release construction and TUF publication

`release/construct.rs`, `release/inventory.rs`, `release/canonical.rs`, `release/tuf.rs`, and the
`rf-runtime-release` binary. All behind the non-default `release-tools` cargo feature, so signing
and publication code never ships in the application binary.

Two properties matter most:

1. **The inventory is derived, then proven.** Construction reads each artefact with the same
   archive readers the client extractor uses and emits every path, type, size, digest, executable
   bit, and symlink target. It then extracts every component through the real
   `LinuxRuntimeArchiveExtractor` against that inventory and runs the client's own `verify_tree`
   and `validate_app_run`. A definition that would produce a tree the client refuses fails on the
   maintainer's machine, not on a user's.
2. **Publication uses the real ADR-012 profile.** Ed25519 only, SHA-256 targets, consistent
   snapshots, `root` and `targets` at 2-of-3 with separately scoped 1-of-1 snapshot and timestamp
   keys, and the ADR's metadata lifetimes. The generated root is verified self-authenticating
   before it is written. Keys live outside the repository at `0600`, and `*.pk8` is gitignored.

The manifest is serialized with RFC 8785 JCS so its SHA-256 is reproducible.

### Added: a configured trusted release source

`adapters/runtime_release_source.rs`. `RuntimeManager::for_app` now accepts an optional
`TrustedReleaseSource`; with none configured it keeps `UnavailableTrustedReleaseSource`, so "no
approved source" stays a trust refusal inside RuntimeManager rather than an absent capability a
caller could route around.

`production_release_source()` returns `None` and says so in a comment: ADR-012 requires the signed
application to ship the initial root, and that root does not exist until the M10 key ceremony. The
qualification origin requires the exact opt-in `RETROFRONTIER_RUNTIME_SOURCE=qualification` plus a
complete configuration; a partial configuration is a startup error, never a silent fallback. The
origin travels to the UI so a qualification build is never displayed as a public release channel.

### Added: installation service, IPC, and Settings UX

`application/runtime.rs`, `commands/runtime.rs`, `src/features/settings/RuntimePanel.tsx`,
`src/features/settings/runtimeStatus.ts`, `src/hooks/useManagedRuntime.ts`.

Following M7's launch-error contract exactly: anticipated problems are typed codes in the response,
never IPC errors, and no message carries a path, `errno`, or OS text. The runtime's real status
accompanies every response, so a failed install cannot make the UI believe an installed runtime
disappeared. Installation is single-flight in-process on top of the cross-process mutation lock, so
a second click reports `installationInProgress` rather than blocking an IPC worker on a kernel lock.

The panel disables its action with a stated reason whenever it cannot succeed, and offers a retry
only for failures a retry could fix.

## Verification

### Automated

| Check | Result |
| --- | --- |
| `pnpm typecheck` | pass |
| `pnpm lint` | pass |
| `pnpm format:check` | pass |
| `pnpm test` | 24 files, **366 tests** pass (was 362) |
| `pnpm build` | pass |
| `cargo fmt -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo clippy --all-targets --all-features -- -D warnings` | pass |
| `cargo test` (default features) | **411 passed**, 1 ignored |
| `cargo test --all-features` | **426 passed**, 7 ignored (the ignored set is the manual qualification harness) |
| `cargo build --release` | pass |

Focused suites, all green: `runtime_manager` 21, `runtime_source` 3, `runtime_archive` 6,
`runtime_installed` 1, `runtime_lock` 3, `runtime_process` 13, `runtime_pointer` 4,
`runtime_release_source` 2, `application::runtime` 24, `application::launch` 24, `release::` 15.

The full `--all-features` suite was run five times consecutively with identical results, so the
real-runtime additions reintroduced no concurrency or flock instability.

CI stays deterministic: nothing added here depends on an external server, real hardware, a user
ROM, a user BIOS, or a graphical desktop.

### The deterministic end-to-end test

`release/roundtrip_tests.rs` is the automated counterpart to the manual qualification. It builds a
structurally real miniature release — a genuine SquashFS AppDir behind an AppImage-shaped prefix
that *includes the decoy signature table*, a zipped core, and a tarred support asset — publishes it
into a signed TUF repository, and installs it with the exact `ToughTrustedReleaseSource` and
`RuntimeManager` the application composes. It then asserts the AppRun stays the authenticated
symlink, the core resolves with its release-declared systems, the support asset is correctly
re-rooted, and the installed tree matches its inventory exactly.

A second test tampers with one published target's bytes while leaving trusted metadata untouched —
the compromised-mirror case, where HTTPS still succeeds — and asserts the install is refused and no
runtime is activated.

Both run offline in under a tenth of a second.

### Release construction

Reconstructed from a clean temporary directory with `--offline`: all **8 targets byte-identical**
to the first build, same manifest digest `583260e1…`, 2932 inventory entries. Reproducibility is
therefore demonstrated, not asserted.

### Real qualification

Fedora 44, KDE Plasma 6 Wayland, x86_64. The full matrix, evidence, and the observed focus
behaviour are in [`M7_5_RUNTIME_QUALIFICATION.md`](M7_5_RUNTIME_QUALIFICATION.md). Summary:

- Real installation through `RuntimeApplicationService::install_runtime` → `Ready`, release
  `rf-runtime-1.22.2-linux-x86_64-001`, installation `i-18d0a4fda8be7c01-1-293535`. Installed tree:
  2759 files, 4 symlinks, 173 directories, accepted exactly by `verify_tree`.
- NES and SNES launched through the real M7 path and executed; *Super Mario World* rendering was
  captured during the run.
- GameCube resolved its core and managed `Sys` and ran, but no rendered frame was observed, so
  content execution is not claimed.
- PlayStation is blocked on an approved BIOS dump and legal content; readiness correctly reports
  `MissingRequiredBios` and does not accept the unapproved `scph1001.bin`.
- Real crash recovery: the harness was `SIGKILL`ed with the emulator alive, the orphan survived, and
  a fresh composition proved it alive, kept the session running, refused mutation with `GameActive`,
  and refused a new launch — then released everything only once death was proven.

## Findings not acted on

- `logs/retroarch/` is never populated: the generated config sets `log_dir` and `log_to_file` but
  no `log_verbosity`. `docs/RETROARCH_LAUNCH.md` now records that its claim is currently untrue.
  Changing log verbosity is a product decision with a performance cost, so it is left to M8/M9.
- `startup_reconcile` reports `Broken` while a managed game is alive, because its
  `ensure_no_active_game` failure is caught by the generic "startup must remain usable" handler.
  User-visible impact is one startup log line, since `verified_snapshot` — which every read
  boundary including the new Settings panel uses — performs no process check.
- The manifest is 626 988 bytes against a 1 MiB limit at four cores. Adding the remaining seven V1
  systems will exceed it; ADR-012 already permits a separate inventory target referenced by digest.

## Repository hygiene

No ROM, BIOS, RetroArch binary, core `.so`, AppImage, extracted runtime tree, database, TUF or
signing private key, credential, build output, or qualification log is tracked. `.gitignore` gained
release-construction output, `*.pk8`, and `qualification-keys/`. All pre-existing untracked review
and report files are unchanged.

Qualification artefacts live outside the repository: the published repository and input cache in a
scratch directory, and the signing keys in a maintainer-named directory outside the tree.

## Machine-state note for the operator

The qualification run left three things on this machine, all outside the repository and all easy to
undo:

- a real managed runtime installed under the application data directory — this is the deliverable;
- `Nintendo GameCube/Legend of Zelda, The - Twilight Princess.iso` in the managed ROMs folder, a
  **hardlink** to the operator's own copy in `~/Downloads` (the original is untouched; `rm` the link
  to remove it);
- library rows and three closed play sessions from the qualification launches.
