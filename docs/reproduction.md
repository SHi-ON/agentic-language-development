# Reproduction guide

Everything runs locally with a Rust toolchain. No network, external
service, or second language runtime is involved.

## Build and test

```sh
cargo build --workspace
cargo test --workspace
```

The emergence claim is pinned by
`agent_policies_co_train_unsupervised_like_probe_v4` in
`crates/ald-learners`: 3,000 alternating training episodes (1,500 per
direction) from exact-uniform initial policies, asserting Brier
advantage over uniform above 0.4 and receiver sharpness above 0.8
(measured 0.5353 and 0.8414 under the test's fixed seed).

## Run one dyad

```sh
cargo run -p ald-cli -- run \
  --manifest protocols/ala-lv-execution-manifest.v2.json \
  --cohort development --kind primary --index 0 \
  --evidence-root ./evidence
```

The manifest fixes the workload: 3,000 training cases (1,500 per
role, i.e. 1,500 training episodes per direction), 240
validation-fit, 240 validation-selection, and 240 within-support
test cases. Cohorts `A` and `B` refuse to run without a GO admission
record at `<evidence-root>/admission.json`; `development` needs none.

The run prints the dyad receipt as JSON and writes
`./evidence/development/primary-0000/`:

- `agent-policy-agent-0.json`, `agent-policy-agent-1.json` — the
  frozen sender+receiver policies;
- `role-folds-agent-0.json`, `role-folds-agent-1.json` — per-role
  folds with native and replay test distributions;
- `branch-probabilities-agent-0.json`,
  `branch-probabilities-agent-1.json` — per-case native, replay, and
  control-branch distributions;
- `primary.json`, `dyad-result.json`, `dyad-receipt.json`;
- `evidence.sqlite` — hash-chained event streams for the run;

plus attempted/terminal lines appended to `./evidence/journal.jsonl`.

## Analyze

Selection and scoring are separate stages. `analyze` refuses to
overwrite an existing output path, and unknown arguments are
rejected rather than dropped.

```sh
cargo run -p ald-cli -- analyze select \
  --dyad ./evidence/development/primary-0000 \
  --cohort development --out ./evidence/selection.json
cargo run -p ald-cli -- analyze score \
  --dyad ./evidence/development/primary-0000 \
  --cohort development --selection ./evidence/selection.json \
  --bootstrap-seed <seed> --out ./evidence/score.json
```

`select` locks the lowest-validation-Brier ordinary predictor per
role and prints each record's hash. `score` recomputes the selection
and aborts if it differs from the locked record, then writes
per-dyad primaries, control contrasts, the cohort interval, and a
bootstrap interval. Pass one `--dyad` per dyad directory for a
cohort; `--coverage`, `--bootstrap-resamples`, and `--attempted`
override the defaults 0.975, 10,000, and the dyad count.

## Verify evidence

```sh
cargo run -p ald-cli -- verify ./evidence/development/primary-0000/evidence.sqlite
```

Prints a verification report (`ok: true` on success, with stream and
event counts) and exits nonzero on any chain or checkpoint failure.

## Determinism check

Rerun the same dyad into a second evidence root and compare:

```sh
cargo run -p ald-cli -- run \
  --manifest protocols/ala-lv-execution-manifest.v2.json \
  --cohort development --kind primary --index 0 \
  --evidence-root ./evidence-2
diff ./evidence/development/primary-0000/agent-policy-agent-0.json \
     ./evidence-2/development/primary-0000/agent-policy-agent-0.json
diff ./evidence/development/primary-0000/primary.json \
     ./evidence-2/development/primary-0000/primary.json
```

Policy, fold, and probability artifacts are byte-identical across
runs; only the receipt's measured wall-clock seconds differ. The
`determinism` tests in `crates/ald-orchestrator` assert this in the
suite.
