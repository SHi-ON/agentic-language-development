# Historical records (pre-Rust design era)

Nothing under this directory is read by any build, run, test, or
script in the workspace. These files are retained for provenance and
are quarantined here (P2-CLEAN-FORK-03) so the top level shows only
the live Rust tree.

Path basis: file paths cited inside these records are relative to the
repository root at quarantine time. `packages/…`, `SPECIFICATION.md`,
and other TypeScript-era paths are intentionally stale — no `packages/`
directory exists in the Rust tree. The one pointer that still resolves
is `e03-design-simulation.json`'s `registrationFacingReceipt`,
`reports/research/statistical-validation.tsv`, which remains tracked
and test-checked (see `scripts/tests/test_statistics_vs_rust.py`).

## Learner contracts (moved from `contracts/`)

Alternative learner tracks from the pre-Rust design era. The workspace
implements exactly one learner, `AgentPolicy` in
`crates/ald-learners`; no code, manifest, or run path reads these
contracts. Each file carries its own HISTORICAL banner.

- `learner-contract.frozen-llm.v1.md`
- `learner-contract.hybrid.v1.md`
- `learner-contract.no-learning.v1.md`
- `learner-contract.scratch-rl.v1.md`
- `learner-contract.self-supervised.v1.md`

## Design and gate artifacts (moved from `docs/`)

- `e03-design-simulation.json` — E03 outcome-blind power simulation;
  receipt pointer above is the live artifact.
- `e03-seed-manifest.json` — prospective seed-allocation record
  (blinded-pilot stage).
- `experiment-readiness-gates.json` — TypeScript-era G1/G2/G3 software
  readiness gates citing `packages/…` vitest paths. Superseded by the
  Rust gates: `.github/workflows/cargo.yml` (build, test, fmt, clippy,
  MSRV, marker grep) plus `cargo test --workspace`.
- `requirement-conformance-matrix.json` — provisional TS-era
  requirement-to-test mapping. Superseded by the Rust gates above.
- `research-console-ux-checklist.json` — research-console UX review
  record against a specification that no longer ships.
