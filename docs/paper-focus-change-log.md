# Paper-focus change log (tracked, append-only)

Every P2 deletion/change group, newest last. Format per group: date;
source-before; paths; old hash; action; why; new authority; dependency
check + test; preservation locator; source-after; unresolved issues.

Preservation artifact for all groups below:
`/home/maria/Development/ala-pre-trim-archive/`
(bundle `ald-feat-verifiable-core.bundle`, worktree diff, file hashes,
plans mirror, manifest, SHA256SUMS; restoration tested 2026-10-09:
HEAD/tree match, patch applies, 4/4 hashes OK, 1314 tracked files).
Git history on `ala-workshop` retains every prior byte; no history
rewrite, no moved tags, no deleted negative evidence.

Dependency method (all groups): transitive import closure from the paper
runtime surface (16 reusable components + 18 focused tests +
`scripts/lv01.mjs`) = 252 files, computed with `.js`->`.ts`
resolution, `@ald/*` aliases, static + dynamic imports. A REMOVE-flagged
file was deleted only if outside that closure, outside the preserved
anchor-lane closure (45 files), not one of the 4 pre-existing worktree
edits, and with no basename string reference from kept files.

## 2026-10-09 — G1 book/ + twins/ removal (79 files)

- source-before: commit `a1fc539`, tree `5137fb1741d939c5828ac5161b5f6bd63d32ed8b`
- paths: `book/**` (61), `twins/**` (18); see TSV rows
  `REMOVE-AFTER-ARCHIVE-AND-DEPENDENCY-CHECK
- old hash: group-sha256 book `3fa59dd260c4a712`, twins `0818317459c06036`
  (sorted per-file HEAD blob hashes)
- action: removed (archived); ignored build output (`twins/**/dist`,
  `node_modules`) deleted as regenerable
- why: 51-page research-book presentation and twin product UI/service do
  not support the short empirical paper; they consumed docs/CI attention
- new authority: paper manuscript lives in sibling repo
  `agentic-language-development-paper`
- dependency check: 0 of 79 in paper closure; kept-file references are
  comments/default path strings only (`config` twin packs default,
  orchestrator/registry/scenario comments); `lint` script `twins` glob
  fixed in the facade group below
- test: focused `test:paper` gate after all groups (see P2 close-out)
- preservation: archive bundle + Git history; restore check passed
- source-after: P2 commit (this log's commit)
- unresolved: none

## 2026-10-09 — G2 deploy/ removal (44 files)

- source-before: commit `a1fc539` (same P2 working set)
- paths: `deploy/mode-r/**` (42), `deploy/a0/**` (2)
- old hash: group-sha256 `8d81facd174d9b96`
- action: removed (archived)
- why: research-grade 17-service topology, Fort deployment, and
  one-second scheduling are not required for trusted Prototype claims
- new authority: `docs/paper-study.md` (Prototype threat model);
  `docs/reproduction.md`
- dependency check: 0 of 44 in paper closure; kept references are doc
  comments only (isolation transports, 3 files); no disposable-worker
  helper still imported was removed; `pnpm-workspace.yaml`
  `twins/packs/*` + `deploy/mode-r` globs dropped in facade group
- test: focused `test:paper` gate after all groups
- preservation: archive bundle + Git history
- source-after: P2 commit
- unresolved: none

## 2026-10-09 — G3 redteam + crypto-research removal (64 files)

- source-before: commit `a1fc539` (same P2 working set)
- paths: `packages/redteam/**` (58), `packages/crypto-research/**` (6)
- old hash: group-sha256 redteam `4d42ccef2ecbb568`, crypto `a551195d6e7f9e6a`
- action: removed (archived); whole packages leave the workspace
- why: broad red-team batteries and neural-cryptography research are off
  the paper's critical path (one GRU family, fixed tokens, no
  hostile-process claims)
- new authority: P4 corruption/mutation fixtures (small, paper-scoped)
  replace campaign batteries for the paper's integrity checks
- dependency check: 0 of 64 in paper closure; sole external importer
  `scripts/audit-e02-ocr.mjs` removed in G4; no tsconfig cross-refs;
  root `tsconfig.json` refs and `@ald/redteam` root dep dropped in
  facade group; conformance-matrix section map keeps historical ALD IDs
  (matrix is a historical record, not a live import)
- test: focused `test:paper` gate after all groups
- preservation: archive bundle + Git history
- source-after: P2 commit
- unresolved: none

## 2026-10-09 — G4 campaign scripts removal (164 files)

- source-before: commit `a1fc539` (same P2 working set)
- paths: `scripts/**/*.mjs` (117 + libs) and `scripts/__tests__/**` (47
  minus preserved anchor-staging test); full list in TSV
- old hash: group-sha256 `c69e08c73ba33f89`
- action: removed (archived)
- why: old launch commands, per-amendment generators/checkers, obsolete
  operator tooling, and unrelated campaign gates
- new authority: `scripts/paper.mjs` facade (`paper:*` verbs) reusing
  kept helpers; historical `.mjs` verifiers remain directly invocable
  for old receipts but are off the advertised surface
- dependency check: outside paper closure; basename string-reference
  sweep of kept files found one runtime spawn
  (`anchor-faucet-watch.mjs` -> `build-conformance-matrix.mjs`) which
  blocked 2 files from removal (see blocked list); one comment-only
  mention (`contract-lint.ts` -> `lint-learner-contracts.mjs`) logged
  as a dangling doc reference, harmless
- test: focused `test:paper` gate after all groups; preserved
  `anchor-staging.test.ts` (117 tests) rerun after all groups
- preservation: archive bundle + Git history
- source-after: P2 commit
- unresolved: none

## 2026-10-09 — Blocked removals (6 files, kept with reason)

- `scripts/__tests__/anchor-staging.test.ts` + `scripts/anchor-faucet-watch.mjs`
  + `scripts/anchor-public-submit.mjs` + `scripts/anchor-staging-gate.mjs`:
  REMOVE-flagged but the test carries a pre-existing worktree edit under
  the P0 preservation rule; its 45-file import closure (anchor core +
  evidence/hashing/types spine) is exempt with it.
- `scripts/build-conformance-matrix.mjs` + `scripts/lib/acceptance-evidence.mjs`:
  runtime-spawned by the kept faucet watch; removal would break the
  preserved lane.
- `reports/manuscript/README.md` + `first-paper-draft.md` (MIGRATE flag):
  prose migrated to the paper repo in P1; ALD copies stay until the
  paper source builds (CI push blocked on `workflow` token scope).
- `BACKLOG.md` + `SPECIFICATION.md` + `LEDGER-INTEGRITY-DESIGN.md`:
  retired in place, bytes unchanged — the preserved lane's matrix
  `--check` consumes exact bytes/line numbers. Retirement is documented
  in `docs/paper-study.md`, not by rewriting.

## 2026-10-09 — Facade + config + docs refresh

- source-before: commit `a1fc539`
- paths: `package.json` (222 -> 22 scripts; dropped `@ald/redteam` and
  `read-as-book` deps), `pnpm-workspace.yaml` (globs + allowBuilds),
  `pnpm-lock.yaml` (regenerated), `tsconfig.json` (dropped 5 dead refs),
  `scripts/paper.mjs` (new dispatcher, fail-closed verbs),
  `.github/workflows/book-integrity.yml` (jobs `paper-check` +
  `paper-validate`), `README.md` (paper workflow + status),
  `docs/paper-study.md` + `docs/reproduction.md` (new active
  authorities), `docs/required-check-enforcement.md`,
  `docs/test-reliability-policy.md`, `docs/release-process.md`,
  `docs/hosted-ci-evidence.md`, `docs/external-prerequisite-handoff.md`,
  `docs/recurrent-baseline-qualification.md`, `CONFIGURATION.md`
  (CI/command coherence), root version `0.1.529` -> `0.1.531` (P0 bump
  folded in; see unresolved), `docs/requirement-conformance-matrix.*`
  (regenerated for the version bump)
- old hash: n/a (edits; prior bytes in Git history + archive)
- action: narrowed / refreshed
- why: one paper question needs one small honest command surface; every
  removed script referenced a deleted file (178/222 broken) or an
  archived campaign
- new authority: this log + `docs/paper-study.md`
- dependency check: every `scripts/*` token in kept commands exists;
  `pnpm run check` verified by running it (see P2 close-out)
- test: `pnpm run build` + `pnpm run test:paper` +
  `pnpm run paper:validate` + matrix `--check` + preserved anchor file
  (117 tests); see P2 close-out for exact results
- preservation: prior bytes in Git history + archive
- source-after: P2 commit
- unresolved: P0 commit `a1fc539` did not bump patch per
  `docs/release-process.md` (same-commit rule); the missed bump is
  folded into this commit's `0.1.531` and recorded here instead of
  rewriting history.

## P2 close-out — verification results (2026-10-09, pre-commit)

- `pnpm run build` (tsc --build, narrowed refs): exit 0.
- `pnpm run test:paper` (18 focused files): 18/18 files, 144/144 tests pass.
- `pnpm run paper:validate`: OK (secret scan 970 files; data/claim
  manifest valid: 140 bundles, 0 research-included).
- `pnpm run lint` (eslint packages): exit 0.
- Preserved `scripts/__tests__/anchor-staging.test.ts`: 117/117 pass
  (with the other agent's 1-line edit intact, still uncommitted).
- New `scripts/__tests__/conformance-matrix-scope.test.ts`: 5/5 pass.
- `node scripts/build-conformance-matrix.mjs --check`: current
  (261 criteria, 96 normative lines; 33 retired criteria, 1 retired
  normative line; version 0.1.531).
- Every `scripts/*` token in the 22 kept commands resolves to an
  existing file; `pnpm-lock.yaml` regenerated (-187 lines).
- Restoration of historical source/protocols: pre-trim archive
  restore-tested in P0 (HEAD/tree match, 1314 files); no history
  rewrite. The 4 pre-existing worktree edits and 1 stash entry are
  untouched by this commit.
- Dependency audit: pre-existing HIGH (`source-map-js` 1.2.1 via
  vitest/vite/postcss, identical at HEAD) fixed with a workspace
  `overrides` pin `>=1.2.2` (1.2.2 resolved; root `package.json`
  `pnpm.overrides` was ignored by pnpm 12.9.1 in this workspace, so
  the override lives in `pnpm-workspace.yaml`). `pnpm audit
  --audit-level=high` now reports no known vulnerabilities; build +
  `test:paper` re-verified after the bump.
