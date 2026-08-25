# RetroFrontier Agent Instructions

## Read First
Before non-trivial changes, read:
1. `PROJECT_CONTEXT.md`
2. `PRODUCT.md`
3. `DOMAIN.md`
4. `ARCHITECTURE.md`
5. `BACKLOG.md`

For architecture work, also read relevant `docs/adr/` files.

Do not assume older RetroFrontier implementations or RF stories represent current state.

## Current Phase
RetroFrontier is greenfield. During planning/foundation, do not begin broad feature implementation unless explicitly asked. Do not scaffold unrelated systems "for later".

## Model Strategy

### Default
Use **GPT Luna Max** for:
- planning
- documentation
- React/TypeScript
- Rust
- SQLite/repositories
- normal migrations
- tests
- refactoring
- UI
- routine debugging
- ScreenScraper
- scanning
- basic RetroArch launch integration

### Escalate to Sol Max
Use **GPT Sol Max** only when:
- architecture is difficult to reverse
- a mistake could damage user data
- runtime update security/signing is involved
- rollback/recovery design is involved
- a migration could endanger user data
- a difficult cross-platform issue remains unresolved
- a difficult concurrency/race issue exists
- Luna repeatedly fails without identifying root cause
- release-readiness architecture/security review is requested

Prefer Sol as focused reviewer where Luna can implement safely.

## Git Rules
After initial bootstrap:
- never work directly on `main`
- create a focused branch
- one focused task per branch
- use a PR
- squash merge
- do not mix unrelated refactors
- do not rewrite shared history

Prefixes:
- `feat/`
- `fix/`
- `refactor/`
- `test/`
- `docs/`
- `chore/`
- `spike/`

## Commit Style
Prefer:
- `feat:`
- `fix:`
- `refactor:`
- `test:`
- `docs:`
- `build:`
- `ci:`
- `chore:`

## Scope Discipline
Before coding:
1. State what the task changes.
2. Identify affected subsystems.
3. Check whether an ADR/architecture update is needed.
4. Avoid unrelated cleanup.

Do not silently change product scope.

## Dependency Rules
Do not add a dependency without concrete need. Consider maintenance, licensing, and cross-platform behavior. Avoid large UI frameworks that replace the existing design system.

## Architecture Boundaries

### React must not directly:
- access SQLite
- scan filesystem
- launch RetroArch
- install runtime
- contain ScreenScraper secrets
- implement OS-specific filesystem behavior

### Rust owns:
- filesystem
- scanning/hashing
- SQLite/sqlx
- metadata adapters
- runtime management
- RetroArch processes
- BIOS validation
- OS integration

Keep Tauri commands thin.

## Runtime Rules
RetroArch is not bundled in the installer. RetroFrontier manages an isolated runtime after install.

Never:
- use system `retroarch` from `PATH`
- depend on existing RetroArch config
- write runtime updates over user data
- activate update while a game runs
- download arbitrary unapproved runtime versions

## Library Rules
A Game is not a ROM file. Do not design persistence as one row with one `file_path`.

Distinguish:
- Game
- Content Unit
- Content File
- Content Root

V1 must not automatically rename, move, convert, or delete ROM files.

## Tests
Use synthetic/legal fixtures. Never commit commercial ROMs or copyrighted BIOS files.

For bug fixes, add regression tests when practical.

Run relevant checks that actually exist:
- frontend typecheck/tests
- Rust tests
- `cargo fmt --check`
- `cargo clippy`

## Security and Secrets
Never commit:
- ScreenScraper secrets
- signing keys
- tokens/passwords
- copyrighted BIOS files
- commercial ROMs
- downloaded RetroArch/core binaries
- generated build artifacts

Do not log secrets.

Archive extraction, executable downloads, manifest verification, and updater logic are security-sensitive.

## Documentation
Update docs when behavior changes materially. Add/update an ADR for significant architecture decisions, not routine implementation choices.

## Completion Standard
Before declaring a task complete:
- requested scope is implemented
- relevant checks pass
- no unrelated changes without justification
- docs/architecture remain consistent
- no secrets/prohibited binaries are present
- final summary lists important changes and unresolved risks
