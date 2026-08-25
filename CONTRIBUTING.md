# Contributing to RetroFrontier

RetroFrontier is in an early greenfield stage. Contributions should preserve current product/architecture decisions rather than recreate abandoned prior implementations.

## Before You Start
Read:
- `PROJECT_CONTEXT.md`
- `PRODUCT.md`
- `DOMAIN.md`
- `ARCHITECTURE.md`
- `BACKLOG.md`

For architecture work, read relevant ADRs.

## Workflow
After repository bootstrap:
1. Update `main`.
2. Create a focused branch.
3. Implement one focused task.
4. Add/update tests.
5. Run relevant checks.
6. Open a PR.
7. Squash merge after review.

## Commit Messages
Prefer Conventional Commit style, for example:
- `feat(scanner): add managed scan roots`
- `fix(runtime): validate staged archive`
- `docs(adr): define metadata provider boundary`

## Pull Requests
Explain:
- what changed
- why
- how tested
- architecture impact
- screenshots for meaningful UI changes

Do not combine unrelated cleanup.

## Architecture
React is presentation. Rust owns filesystem, SQLite, runtime management, RetroArch processes, metadata providers, and OS integration.

## Dependencies
New dependencies require justification. Avoid a general-purpose UI framework that replaces the existing design system.

## Tests and Fixtures
Do not commit commercial ROMs or copyrighted BIOS files. Use synthetic or redistributable fixtures.

## User Content Safety
V1 is non-destructive. Do not add automatic ROM rename, move, conversion, or deletion without an explicit decision.

## Runtime Safety
Runtime is replaceable app-managed data. ROMs, BIOS, saves, states, metadata, and DB must survive runtime update/repair/rollback.

## Security
Never commit credentials, tokens, signing keys, or secrets. See `SECURITY.md`.

## Licensing
Contributions must be compatible with `GPL-3.0-or-later` and relevant third-party licenses.
