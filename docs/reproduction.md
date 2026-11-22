# Reproduction guide

The dyad run/analyze/verify path runs locally with a Rust toolchain.
No network, external service, or second language runtime is involved
in that path; operator tooling under `scripts/` has its own
requirements (see each script's header).

## Build and test

All campaign binaries were built with one documented target directory:
the checkout's `env/cargo-target` (never the default `target/`).
Export it for every build so local artifacts land where the campaign
records say they do:

```sh
export CARGO_TARGET_DIR="$PWD/env/cargo-target"
cargo build --workspace
cargo test --workspace
```

Release reproduction (checked 2026-11-22, rustc 1.99.1): at commit
`ab2c2a7`, `cargo build --release -p ald-cli` reproduces the executed
ablation binary digest
`4963c506c9ce9cb2122f00e0b6b286685f84aa21bc106f1ebff56747731a03b0`
exactly. The k8.idx215 exception pin
`f29377f18be0f25c9653078c42c9e405ccdc1150d1088b7e455d23ef013e3737`
(@`9e07fcd`) is digest-only: it rebuilds as `262bf4bc…` under the
current toolchain (compiler-version dependence; see the paper
repository `SEEDS.md` section 3 for the full note).

The emergence claim is pinned by
`agent_policies_co_train_unsupervised_like_probe_v4` in
`crates/ald-learners`: 3,000 alternating training episodes (1,500 per
direction) from exact-uniform initial policies, asserting Brier
advantage over uniform above 0.4 and receiver sharpness above 0.8
(measured 0.5353 and 0.8414 under the test's fixed seed, verified
2026-11-22 on `main`; regenerate with `cargo test -p ald-learners
--test agent_policy agent_policies_co_train_unsupervised_like_probe_v4
-- --nocapture`).

## Run one dyad

The canonical manifest is `protocols/ala-lv-execution-manifest.v2.json`
(`ala manifest` with no flags resolves to it; v1 is retained frozen —
see `protocols/README.md`).

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

## Run one development ablation

Non-default development flags (full reference in
`../CONFIGURATION.md`) require `--cohort development`:

```sh
cargo run -p ald-cli -- run \
  --manifest protocols/ala-lv-execution-manifest.v2.json \
  --cohort development --kind primary --index 0 \
  --evidence-root ./evidence-dev \
  --training-interactions-per-direction 500 \
  --learning-mode joint \
  --drop-probability 1/4 \
  --active-sender-tokens 16 \
  --held-out-types 12,13,14,15
```

The run root is suffixed with the configuration id
(`./evidence-dev/development/primary-0000-dev-<8 hex>/`) and carries a
`development-config.json` sidecar with the requested and resolved
options, digests, per-role per-split seeds, the `2N`/`N` counts, the
exposure census, and the protocol versions; the same config hash is
bound into the receipt and both journal lines. Rerunning the identical
configuration into the same root is refused; use a fresh evidence
root per ablation (or per config, since distinct configs already land
in distinct roots). Rejected invocations — a non-development cohort, a
malformed value, a repeated flag — exit nonzero before creating any
directory, journal, or database.

Development runs are deterministic like default runs: rerun the same
flags into a second root and every JSON artifact (plus the journal)
is byte-identical. `analyze select|score` read an ablation's run root
like any dyad directory.

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
