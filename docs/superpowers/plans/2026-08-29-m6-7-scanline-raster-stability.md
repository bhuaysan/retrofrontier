# M6.7 Global Fidelity Delta — Scanline Raster Stability Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make RetroFrontier’s global scanline background a stable 1 CSS px line / 3 CSS px gap / 4 CSS px pitch across viewport sizes and themes, then document native-scale evidence for this delta only.

**Architecture:** Keep one CSS-owned raster on `.app-shell`, behind the existing product surfaces. First measure the real production app at fixed width/different heights and both themes; if the implicit repeat is the unstable mechanism, replace only that declaration with one explicit 4 CSS px background tile, fixed at `0 0`, preserving the approved `--scanline` tokens. No React, Rust, resize JavaScript, new dependency, or product surface changes are planned.

**Tech Stack:** React + TypeScript + Vite, Tauri 2 frontend boundary, CSS design tokens, locally installed browser/WebView-compatible native-scale screenshots, pnpm verification scripts.

**Spec:** `docs/design/README.md`, `docs/design/tokens.css`, `docs/design/screens/B1 Bibliothek.dc.html`, and the user-provided M6.7 Global Fidelity Delta requirements.

## Global Constraints

- The target raster is exactly `1 CSS px` scanline, `3 CSS px` transparent gap, `4 CSS px` total pitch.
- Dark uses `--scanline: rgba(255,255,255,.028)` and Light uses `--scanline: rgba(0,0,0,.03)`; do not retune opacity.
- There is one global raster layer, behind header/sidebar/cards/panels/controls; do not add an overlay or compound layers.
- Do not use viewport-relative, font-relative, responsive, animated, JavaScript/DPR-derived, canvas, SVG-filter, or large-texture scanlines.
- Preserve M6.7B card activation/favorite/focus/z-index behavior and do not begin M6.7C.
- Preserve all pre-existing untracked review artifacts; do not modify unrelated source or Rust.
- Stay on `feat/m6-library-ui`; no rebase, merge to main, force push, or history rewrite.

## File Map

- Modify: `src/styles/index.css` — the single production `.app-shell` raster declaration, only after baseline evidence identifies the needed CSS change.
- Create: `M6_7_SCANLINE_DELTA_REVIEW.md` — root review artifact following existing M6.7 review structure; leave untracked if that is the repository convention.
- Temporary only: a gitignored or `/tmp` real-App visual harness and measurement script if native Tauri rendering is unavailable; delete it after verification.
- Do not modify: `src/app/AppShell.tsx`, `src/main.tsx`, React feature files, Rust, tokens, or prior review artifacts unless diagnosis proves a root/background ownership defect.

### Task 1: Validate repository and source-of-truth state

**Files:**
- Read: `AGENTS.md`, `PROJECT_CONTEXT.md`, `PRODUCT.md`, `DOMAIN.md`, `ARCHITECTURE.md`, `BACKLOG.md`, `docs/design/README.md`, `docs/design/tokens.css`, `docs/design/screens/B1 Bibliothek.dc.html`.
- Inspect: `src/styles/index.css`, `src/app/AppShell.tsx`, `src/main.tsx`, `index.html`, relevant recent commits and review artifacts.

- [ ] **Step 1: Confirm branch/HEAD/status without changing the worktree.** Run `git fetch origin`, `git branch --show-current`, `git rev-parse HEAD`, `git rev-parse origin/feat/m6-library-ui`, and `git status --short`. Expected: branch `feat/m6-library-ui`, local and remote start at `17fa766b5b80e246810005fb0c1f4136a2e025c8`, and only pre-existing untracked artifacts are listed.
- [ ] **Step 2: Trace raster ownership and scaling inputs.** Inspect `body`, `#root`, `.app-shell`, `AppShell`, `main.tsx`, `index.html`, media queries, transforms, zoom, background sizing/positioning, and resize/DPR code. Record whether there is exactly one production raster and whether any ancestor can scale it.
- [ ] **Step 3: Check architecture impact.** Record that this is a routine CSS fidelity correction with no architectural boundary, IPC, persistence, or product behavior change; no ADR update is needed unless the trace finds a cross-layer design decision.

### Task 2: Reproduce and measure the baseline

**Files:**
- Temporary only: real-App harness/fixture aliases and native-scale measurement script under `/tmp` or an ignored local path.
- Read-only reference: production `src/App.tsx`, `src/app/AppShell.tsx`, `src/styles/index.css`, bundled `@fontsource` typography.

**Interfaces:**
- Consumes: the real production `App` and CSS with only throwaway Tauri IPC/event fixtures where native Tauri is impractical.
- Produces: a table of viewport, DPR, theme, CSS computed values, and measured consecutive line/gap/pitch runs from native-scale pixels.

- [ ] **Step 1: Build the throwaway harness without reimplementing UI.** Mount the real production app, load the real CSS and bundled fonts, and stub only the Tauri calls/events needed to let the shell render deterministic synthetic legal data. Do not add the harness to tracked files or production imports.
- [ ] **Step 2: Measure the requested baseline matrix.** At minimum capture Dark and Light at `1280×640`, `1280×800`, `1280×1000`, `1280×1200`, approximately `960×640`, and a large viewport such as `1920×1080`. Record DPR, computed `background-image`, `background-size`, `background-position`, element bounds, and native screenshot pixel runs at several consecutive scanlines in an exposed `.app-shell` region.
- [ ] **Step 3: Resize the same browser session.** For at least one dark and one light session, change only viewport height through `640 → 800 → 1000 → 1200 → 640` and repeat measurements. Distinguish geometry changes from screenshot presentation scaling and note any phase shift.
- [ ] **Step 4: Form one evidence-backed hypothesis before editing.** State whether drift is caused by implicit gradient rasterization, background sizing/positioning, duplicate ownership, an ancestor transform, browser zoom/DPR, responsive CSS, or screenshot scaling. If exact engine internals cannot be proven, document the observed mechanism and stable CSS invariant the fix must establish.

### Task 3: Red/green structural guard and minimal CSS correction

**Files:**
- Modify: `src/styles/index.css` at the existing `.app-shell` background declaration only if baseline evidence supports it.
- Temporary only: one diagnostic assertion or measurement script, deleted after verification.

**Interfaces:**
- Consumes: Task 2’s measured baseline and hypothesis.
- Produces: one fixed-origin, fixed-size CSS raster owned by `.app-shell`, preserving `var(--scanline)` and M6.7B stacking/pointer behavior.

- [ ] **Step 1: Establish the failing diagnostic.** Before production CSS edits, run the baseline measurement against the original declaration and retain the observed height-dependent geometry/phase result. If a durable automated test cannot assert rendered pixels in the repository test environment, explicitly use this native-scale diagnostic as the failing reproduction rather than adding a brittle CSS-source test.
- [ ] **Step 2: Apply the smallest supported CSS change.** Prefer one explicit tile with a 4 CSS px vertical extent, `background-repeat: repeat-y`, and `background-position: 0 0`; set `background-size` explicitly only as needed to prevent implicit sizing. Keep the 1px/4px stops and theme token unchanged. Do not alter opacity or add another layer.
- [ ] **Step 3: Re-run the diagnostic.** Verify the fixed-width/different-height matrix and same-session resize matrix now report approximately 1 CSS px line, 3 CSS px gap, and 4 CSS px pitch, with deterministic origin and identical geometry in both themes. Verify the global raster remains behind product surfaces.
- [ ] **Step 4: Inspect the focused diff.** Run `git diff -- src/styles/index.css` and check that no card, layout, z-index, pointer, theme token, or unrelated rule changed.

### Task 4: Protect the accepted M6.7B surface and document evidence

**Files:**
- Create: `M6_7_SCANLINE_DELTA_REVIEW.md`.
- Read-only: `M6_7A_REVIEW.md`, `M6_7B_REVIEW.md`, `M6_7B_DELTA_REVIEW.md` for convention.

- [ ] **Step 1: Re-check the accepted M6.7B invariants.** Run existing frontend tests and inspect the rendered populated library enough to confirm compact cards, full-card detail activation, independent favorite action, A6 focus, system badges, light card accents, missing-content presentation, and stretched-anchor z-index/pointer behavior remain unchanged.
- [ ] **Step 2: Run all requested verification commands.** Run and record `pnpm typecheck`, `pnpm lint`, `pnpm format:check`, `pnpm test`, `pnpm build`, `git diff --check`, `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`, `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings`, and `cargo test --manifest-path src-tauri/Cargo.toml`.
- [ ] **Step 3: Write the review artifact.** Include repository state (expected/actual/final HEAD, branch, commit/push), exact reproduction and before measurements, proven-versus-inferred diagnosis, exact CSS approach and B1 comparison, Dark/Light before/after tables across multiple heights, native/harness method and limitations, HIGH/MEDIUM/LOW/INFO findings, and verdict `READY FOR EXTERNAL REVIEW` or `FIXES REQUIRED` for this delta only. Explicitly say M6.7C was not started.
- [ ] **Step 4: Remove temporary harness files and check status.** Delete only throwaway files created for this task. Confirm all pre-existing untracked artifacts remain and no prohibited binaries, ROMs, BIOS files, generated builds, or secrets are present.
- [ ] **Step 5: Commit and push the focused checkpoint.** After verification, use `git add src/styles/index.css M6_7_SCANLINE_DELTA_REVIEW.md`, `git commit -m "fix(ui): stabilize scanline raster"`, and `git push -u origin feat/m6-library-ui`. Report final local/remote HEAD and push status; do not merge or begin M6.7C.

## Self-Review Checklist

- [ ] Diagnosis is supported by measured native-scale renders, not assumed from CSS appearance.
- [ ] Height-only resize does not alter measured CSS pitch or phase.
- [ ] Width/fullscreen-like changes do not alter measured pitch or phase.
- [ ] Dark and Light preserve approved token opacities and identical geometry.
- [ ] There is one global raster behind product surfaces.
- [ ] No M6.7B behavior or z-index/pointer behavior changed.
- [ ] All requested commands have fresh observed results.
- [ ] Review artifact covers this delta only and preserves historical artifacts.

