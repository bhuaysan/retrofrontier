# M6.5 Candidate Corrective Pass Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make persisted, backend-authoritative metadata candidates reachable from every valid non-matched state while removing misleading automatic retries when manual resolution is already available.

**Architecture:** Keep the existing typed `GameMetadataState` DTO and Rust metadata lifecycle unchanged. Add one frontend presentation predicate in `metadataActions.ts`, use it for the detail candidate panel and action mapping, preserve the existing `useGameDetail` mutation/event path, and document the verified lifecycle in `docs/M6_REPORT.md`.

**Tech Stack:** React 19, TypeScript, Vitest, Testing Library, Rust/Tauri metadata contracts, Markdown.

**Spec:** `M6_5_REVIEW.md` HIGH-1/MEDIUM-1 corrective requirements and the M5 contract in `docs/METADATA.md`.

## Global Constraints

- Work only on `feat/m6-library-ui`; do not merge, push, rewrite history, or begin M6.6.
- Preserve `M6_5_REVIEW.md` and all pre-existing untracked review/report artifacts byte-for-byte.
- Do not change Rust production code, migrations, DTOs, Tauri commands, provider policy, quota logic, or credential code unless the existing DTO is proven insufficient.
- Candidate order remains the backend-provided order; provider IDs, confidence, scores, and raw provider data stay out of visible UI.
- Candidate selection, clear-selection, route-change guards, metadata invalidation, and authoritative rereads remain on the existing `useGameDetail` path.
- Use synthetic/local fixtures only and run the focused red-green tests before full verification.

### Task 1: Encode and test the candidate/action presentation decision

**Files:**
- Modify: `src/features/library/metadataActions.ts`
- Test: `src/features/library/metadataActions.test.ts`

**Interfaces:**
- Consumes: `GameMetadataState.status`, `unsupportedReason`, `candidates`, `matchType`, `providerGameId`, and live job state.
- Produces: one exported `hasSelectableCandidates(state: GameMetadataState | null): boolean` predicate used by detail presentation and a state-aware `getMetadataAction`.

- [ ] **Step 1: Write failing tests**

Add cases proving:

```ts
expect(hasSelectableCandidates(metadataState('ambiguous', candidates))).toBe(true);
expect(hasSelectableCandidates(metadataState('deferred', candidates))).toBe(true);
expect(hasSelectableCandidates(metadataState('failed', candidates))).toBe(true);
expect(hasSelectableCandidates(metadataState('deferred'))).toBe(false);
expect(hasSelectableCandidates(metadataState('failed'))).toBe(false);
expect(hasSelectableCandidates({ ...metadataState('matched'), candidates })).toBe(false);
expect(
  getMetadataAction({ ...metadataState('deferred'), unsupportedReason: 'chdRepresentationUndefined' }),
).toBeNull();
expect(getMetadataAction({ ...metadataState('deferred'), candidates })).toBeNull();
expect(getMetadataAction({ ...metadataState('failed'), candidates })).toBeNull();
expect(getMetadataAction({ ...metadataState('deferred') })).toEqual(expect.objectContaining({ kind: 'request' }));
```

Keep the existing ambiguous, no-candidate, live-job, and all-status coverage. The matched assertion protects against rendering historical candidate rows after an accepted provider relationship.

- [ ] **Step 2: Run the focused test and verify the expected failure**

Run: `pnpm test -- src/features/library/metadataActions.test.ts`

Expected: FAIL because the predicate is not yet exported and deferred/failed candidate actions still map to provider requests.

- [ ] **Step 3: Implement the smallest projection**

Implement one predicate that treats non-empty candidate DTO data as selectable except when an accepted `matched` relationship makes persisted rows historical. Make `getMetadataAction` suppress the automatic request for capability-gated `deferred` state and for `deferred`/`failed` states with selectable candidates. Leave live-job suppression and all other mappings unchanged.

- [ ] **Step 4: Run the focused test and verify it passes**

Run: `pnpm test -- src/features/library/metadataActions.test.ts`

Expected: PASS with no unrelated test changes.

### Task 2: Make the Game Detail candidate panel reachable and truthful

**Files:**
- Modify: `src/features/library/GameDetailPage.tsx`
- Test: `src/features/library/GameDetailPage.test.tsx`
- Test: `src/hooks/useGameDetail.test.tsx`

**Interfaces:**
- Consumes: `hasSelectableCandidates`, `GameMetadataState`, and existing `GameDetailModel.selectMetadataCandidate`.
- Produces: candidate panel reachability for ambiguous/deferred/failed DTOs with candidates, no empty picker for deferred/failed without candidates, and state-appropriate manual-resolution copy.

- [ ] **Step 1: Write failing detail regressions**

Add parameterized detail tests for `ambiguous`, `deferred`, `failed`, and `stale` with the same ordered candidate fixtures. Assert the list is present, titles retain backend order, provider IDs/confidence/score are absent, and clicking a candidate calls `selectMetadataCandidate` with the corresponding opaque ID. Add deferred/failed empty-candidate cases asserting no candidate list. Add a matched fixture with historical candidate rows asserting no picker. Add an unsupported deferred fixture asserting no automatic retry button while manual candidates are visible. Extend the existing hook selection test across ambiguous, deferred, and failed states to prove the same command and authoritative reread path.

- [ ] **Step 2: Run the focused detail tests and verify they fail**

Run: `pnpm test -- src/features/library/GameDetailPage.test.tsx`

Expected: FAIL because `MetadataCandidates` currently requires `status === 'ambiguous'`, and the action row still exposes retries for deferred/failed candidate states.

- [ ] **Step 3: Implement the minimal rendering/copy change**

Replace the status-only guard with `hasSelectableCandidates(metadataState)`. Keep the existing empty ambiguous explanation only for ambiguous with no candidates. Add a concise manual-resolution intro that remains truthful for capability-deferred and failure-demoted states. Keep the same candidate map, order, selection callback, pending behavior, and no-confidence rendering.

- [ ] **Step 4: Run focused detail/action tests**

Run: `pnpm test -- src/features/library/metadataActions.test.ts src/features/library/GameDetailPage.test.tsx`

Expected: PASS, including existing selection, clear, focus, cached-data, and race-facing presentation tests.

### Task 3: Record the verified lifecycle and final UX matrix

**Files:**
- Modify: `docs/M6_REPORT.md`

**Interfaces:**
- Consumes: verified M5 Rust lifecycle, focused frontend regressions, and exact command output.
- Produces: a corrective-pass section documenting HIGH-1, MEDIUM-1, candidate lifecycle, capability formats, action hierarchy, verification, and M6.6 deferrals.

- [ ] **Step 1: Document backend evidence and invariant**

Record that `persist_unsupported` writes `deferred` and replaces candidates; provider failure handling upserts a stable match row without replacing candidates, so existing candidates survive `deferred`/`failed`; `mark_match_stale` preserves rows; `replace_candidates` orders by backend ordinal; user selection queues the existing `identify` command; clear deletes only user-owned selection; provider retries consume the shared backend request budget.

- [ ] **Step 2: Document final presentation rules**

State the predicate, ambiguous/deferred/failed/no-candidate/matched behavior, unsupported CHD/CUE-BIN/GDI/M3U/RVZ/GCM and other allowlist-deferred behavior, retry/action hierarchy, and the unchanged mutation/event safety contract.

- [ ] **Step 3: Add exact focused/full/manual verification results**

Append the command exit codes and observed test counts after running the requested frontend, Rust, build, `tauri:dev`, and `git diff --check` commands. Mark M6.6 work as not started.

### Task 4: Complete verification and focused commit

**Files:**
- Modify: only the files above plus the saved plan itself.

**Interfaces:**
- Consumes: focused green tests and the documented verification matrix.
- Produces: one focused commit, no merge/push, and a final repository-state report.

- [ ] **Step 1: Run all requested checks**

Run the exact commands from the task request: `pnpm typecheck`, `pnpm lint`, `pnpm format:check`, `pnpm test`, `pnpm build`, Rust fmt/clippy/test/build, `pnpm tauri:build`, and `git diff --check`.

- [ ] **Step 2: Run manual startup sanity**

Run `pnpm tauri:dev` with no real provider credentials/quota and inspect startup/idle behavior and the focused DOM tests; stop the process without altering existing user processes or generated repository files.

- [ ] **Step 3: Recheck protected artifacts and worktree**

Compare the final `M6_5_REVIEW.md` hash to the recorded starting hash, inspect `git status --short`, and verify no prohibited binaries, secrets, or historical untracked files changed.

- [ ] **Step 4: Commit the focused corrective pass**

```bash
git add docs/M6_REPORT.md docs/superpowers/plans/2026-08-28-m6-5-candidate-corrective-pass.md src/features/library/GameDetailPage.test.tsx src/features/library/GameDetailPage.tsx src/features/library/metadataActions.test.ts src/features/library/metadataActions.ts src/hooks/useGameDetail.test.tsx
git commit -m "fix(metadata-ui): expose persisted candidates across deferred states"
```

Do not merge or push.
