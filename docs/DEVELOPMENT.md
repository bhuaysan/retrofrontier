# RetroFrontier development

M4 establishes the local library scanner, durable content model, root management,
and system-readiness snapshot boundary. M5 adds provider-backed metadata
enrichment behind a provider-neutral boundary. M6.1 adds bounded backend/IPC
contracts, and M6.2 adds the library shell, empty/setup state, scan UX, and
root-management entry points. M6.3 consumes the bounded query for local library
browsing, debounced search, system/favorite filters, page controls, favorites,
cached covers, and coalesced metadata invalidation. Core selection UI and game
launching remain later milestones. M6.4 adds bounded game detail, normalized
metadata, content-unit presentation, and display of the existing Rust-authoritative
system readiness snapshot. M6.5 adds bounded provider/account status settings,
write-only optional account credentials, metadata request/refresh actions, and
ordered candidate selection. Local library and cached metadata remain usable when
the provider is offline. M6.6 completes the UI hardening pass: candidate/action projections remain
DTO-driven, provider/account copy reflects normalized guarantees, and focus, heading, live-region,
positive/available-state contrast, and async race behavior are covered without moving policy or
secrets across IPC. The focused corrective pass restores truthful copy for ambiguous candidate
states; light-theme error/negative-status contrast remains mandatory M6.7 input.

## Prerequisites

- Node.js 22 LTS
- pnpm 11
- Rust stable with `rustfmt` and `clippy`
- Linux WebKit/GTK development packages required by Tauri 2

On Fedora, install the Tauri/WebKit development packages provided by the
distribution. The pull-request workflow uses Ubuntu Linux and installs the
corresponding packages before compiling Rust.

## Run the application

```bash
pnpm install
pnpm tauri:dev
```

`pnpm dev` starts the Vite frontend by itself for browser-level work. Native
IPC is only available inside the Tauri window; a plain browser page cannot
complete native reads or actions and is therefore not a supported native
integration environment.

The foundation build intentionally has bundling disabled. This verifies the
desktop executable without claiming a Windows, macOS, or Linux release package.

## Checks

```bash
pnpm typecheck
pnpm lint
pnpm format:check
pnpm test
pnpm build
pnpm tauri:build

cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
cargo build --manifest-path src-tauri/Cargo.toml --release
```

Rust tests use temporary SQLite files. The application database is created in
the OS-specific Tauri application-data directory, under its `database/`
subdirectory; no source-tree database is used or committed.

## BIOS development checks

Production discovery uses the OS-resolved user-data path:

```text
Documents/RetroFrontier/BIOS
```

Discovery is intentionally non-recursive. Declared BIOS filenames must be
directly below this root; system-specific nested folders are not automatically
searched and may therefore still be reported as missing. This is the current
M3 folder-layout policy.

BIOS files are user-owned data. The service reads expected candidates and hashes
them without modifying, moving, renaming, deleting, downloading, or executing
them. Standard tests use synthetic files in temporary directories.

If a developer has intentionally supplied local files in the ignored repository
`BIOS/` directory, the opt-in integration check can be run with:

```bash
cargo test --manifest-path src-tauri/Cargo.toml inspect_local_real_bios_directory_read_only -- --ignored --nocapture
```

This test is ignored by default, is not part of CI, prints only filename/state/
size/SHA-256, and uses explicit absolute overrides. It does not copy or modify
the files. Do not add `BIOS/` files to Git with `git add -f`.

## Metadata development credentials

Metadata enrichment needs the ScreenScraper application credentials issued to
RetroFrontier. Copy `.env.example` to `.env` — which is git-ignored — and fill in
the values:

```bash
cp .env.example .env
```

The variables are also read from the process environment, so exporting them works
too. Release builds do not read the file: they receive the same variables through
protected build-time injection at compile time.

A distributed desktop binary makes an application credential recoverable, so this
is an application identity rather than a confidentiality boundary. Never commit
real values, and never reuse another project's credential.

The application starts and the local library works normally without credentials.
Metadata enrichment then stays idle and `get_metadata_provider_status` reports
that credentials are not configured.

Optional personal ScreenScraper accounts are separate. On Linux the `keyring`
crate talks to the Secret Service over D-Bus, so persisting an account needs a
running provider such as gnome-keyring or KWallet; distribution packaging should
recommend one. Its absence is not an error — RetroFrontier logs a warning and
falls back to a session-only store, and the local library is unaffected.

Rust stores personal accounts in the OS credential vault; SQLite holds only an opaque reference, and no read command
returns a password. The M6.5 Settings form submits credentials through the narrow
write-only command, clears its password field after the command settles, and never
renders or logs the password. Tests never touch a real keychain: they inject an in-memory
vault, and the same in-memory vault is the session-only fallback on a host with no
usable credential store.

Metadata tests require no network access, no credentials, and no keychain. The
HTTP boundary, the provider, the vault, the clock, and the jitter source are all
injectable, and all fixtures are synthetic or sanitized.

Cached covers live under the OS-specific Tauri application-data directory in
`metadata/media/`. Nothing is written beside user ROMs or into the source tree. The future WebView
receives an opaque target-specific reference: `rfmedia://localhost/cover/<game-id>` on Linux/macOS
desktop or `http://rfmedia.localhost/cover/<game-id>` on Windows. The native custom protocol resolves
the durable cover row and validates cache containment and image content; the relative cache path is
never serialized through IPC.

## Boundaries and conventions

The representative foundation path is:

```text
React `getAppInfo()`
  -> TypeScript IPC wrapper
  -> thin Tauri `get_app_info` command
  -> `AppInfoService`
  -> `SettingsRepository`
  -> SQLite adapter and migration
```

Rust structs use `serde(rename_all = "camelCase")` and the TypeScript wrapper
mirrors the small stable response shape in `src/platform/ipc.ts`. M1 does not
add code generation; the convention is intentionally visible and easy to
replace if the IPC surface becomes large.

`AppError` keeps internal Rust/SQLx details in logs and serializes only a safe
`code` plus user-facing `message` to Tauri callers. The tracing subscriber is
configured for development output and can gain an application-data file layer
later without changing command or service code.

M6.1 keeps the UI-facing library contracts in the same manually mirrored boundary: Rust DTOs use
`serde(rename_all = "camelCase")`, and `src/platform/ipc.ts` mirrors the bounded list, summary,
detail, favorite, scan-issue-page, and metadata-invalidation shapes. M6.2 adds frontend state and
query orchestration for the shell, roots, scan UX, and saved issues. M6.3 adds `useLibraryQuery`,
which owns bounded page identity, debounced search, filter resets, race-safe request/loading
ownership, authoritative favorite refreshes, scan-completion refreshes, and visible-page metadata
invalidation coalescing. Cards consume only the list DTO and opaque cached-cover reference. M6.4's
`useGameDetail` independently reads one bounded local detail and one authoritative metadata detail,
while readiness reuses the existing `get_systems` response; it does not fetch a full snapshot or
provider payload.

## Design tokens

`src/styles/index.css` imports `docs/design/tokens.css` directly. The handoff
file remains the single source of truth for colors, typography, shadows, and
focus conventions. The bundled `@fontsource` packages provide the handoff's
OFL-licensed Press Start 2P, VT323, and Space Grotesk fonts without a network
request at runtime.
