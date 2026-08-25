# RetroFrontier development

M1 establishes the desktop application foundation only. Runtime installation,
ROM discovery, metadata, cores, BIOS validation, and game launching are later
milestones and are intentionally not part of the local workflow yet.

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
IPC is only available inside the Tauri window, so the shell reports an IPC
availability state when opened as a plain browser page.

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
```

Rust tests use temporary SQLite files. The application database is created in
the OS-specific Tauri application-data directory, under its `database/`
subdirectory; no source-tree database is used or committed.

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

## Design tokens

`src/styles/index.css` imports `docs/design/tokens.css` directly. The handoff
file remains the single source of truth for colors, typography, shadows, and
focus conventions. The bundled `@fontsource` packages provide the handoff's
OFL-licensed Press Start 2P, VT323, and Space Grotesk fonts without a network
request at runtime.
