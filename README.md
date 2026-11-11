# Agentic Language Development

A pure-Rust workspace that implements exactly one method: the
**Emergent Signaling Dyad**. Two symmetric agents with no built-in
shared vocabulary learn to communicate in a synthetic referential
game, trained only by whether they succeed together. There is no
other learner, trainer, or runtime in this repository, and no other
language toolchain is required to build, test, or run it.

## The method

One dyad is two agents, `agent-0` and `agent-1`. The agents are
symmetric: each owns the same kind of policy (`AgentPolicy` in
`crates/ald-learners`), holding both halves of the signaling problem:

- a **sender policy**: logits over a fixed inventory of 32 tokens
  (`tok00`–`tok31`) for each of 16 abstract types; and
- a **receiver policy**: logits over the 16 types for each delivered
  token, restricted in play to the 4 candidate types of the current
  case.

Both tables start at zero, so every initial sender and receiver
distribution is exactly uniform. No token has any assigned meaning;
any usable token-to-type association must emerge inside the dyad.

Training is joint and per dyad. The two agents alternate roles, one
episode per direction per step:

1. The sender agent observes the target type (one of the case's 4
   distinct candidate types) and samples a token from its sender
   policy.
2. A fixed channel drops the token with probability 1/8; otherwise
   the token is delivered to the receiver agent.
3. The receiver agent samples one of the 4 candidate positions from
   its receiver policy. It never sees the true type. A dropped token
   yields an exact uniform posterior over positions.
4. Both agents receive the same shared reward: 1 if the receiver
   chose the target's position, else 0. This shared success signal
   is the only learning signal in the system.
5. The sender updates by REINFORCE (learning rate 1.0, per-type
   running-mean baseline) on its sampled token — including when the
   token was dropped. The receiver updates by REINFORCE (learning
   rate 1.0, per-token running-mean baseline) toward the type of the
   candidate it actually chose — never toward the true type. A
   dropped token is a receiver no-op.

At test, both policies are frozen. For each test case of a role, the
opposite agent acts as sender through the same 1/8 channel, and the
role agent's frozen receiver posterior scores the case. The dyad
*primary* is the mean per-role Brier gap between a validation-selected
ordinary-record predictor (`crates/ald-analysis`) and the frozen
receiver's native predictions; control branches (disabled, constant,
random, shuffled) are scored as paired contrasts against the normal
branch.

Everything is deterministic per dyad: case generation, training, and
test draws come from SHA-256-derived per-role seed streams
(`crates/ald-hashing`), and persisted floats are stabilized so
artifacts are byte-identical across runs.

Emergence is covered by an integration test,
`agent_policies_co_train_unsupervised_like_probe_v4` in
`crates/ald-learners`: after 3,000 alternating episodes (1,500 per
direction) under its fixed seed it measures Brier advantage 0.5353
over uniform and receiver sharpness 0.8414, from an exactly uniform
start.

## Build and test

```sh
cargo build --workspace
cargo test --workspace
```

## Run

The single CLI is `ala` (`crates/ald-cli`). A dyad run needs an
execution manifest, which pins the frozen workload (3,000 training
cases split evenly across the two roles; 240 validation-fit, 240
validation-selection, and 240 within-support test cases):

```sh
cargo run -p ald-cli -- run \
  --manifest protocols/ala-lv-execution-manifest.v2.json \
  --cohort development --kind primary --index 0 \
  --evidence-root ./evidence
```

Study cohorts `A` and `B` additionally require an admission record at
`<evidence-root>/admission.json` whose decision is GO; any other
cohort (such as `development`) runs without one.

One run writes `<evidence-root>/<cohort>/<kind>-<index>/` containing
the frozen policies (`agent-policy-agent-0.json`,
`agent-policy-agent-1.json`), per-role folds and branch
probabilities (`role-folds-*.json`, `branch-probabilities-*.json`),
`primary.json`, `dyad-result.json`, `dyad-receipt.json`, and a
hash-chained `evidence.sqlite`; `<evidence-root>/journal.jsonl`
records the attempt and terminal lines. The receipt (also printed to
stdout) carries the dyad primary and measured case counts.

Analyze and verify:

```sh
cargo run -p ald-cli -- analyze select \
  --dyad ./evidence/development/primary-0000 \
  --cohort development --out ./evidence/selection.json
cargo run -p ald-cli -- analyze score \
  --dyad ./evidence/development/primary-0000 \
  --cohort development --selection ./evidence/selection.json \
  --bootstrap-seed <seed> --out ./evidence/score.json
cargo run -p ald-cli -- verify ./evidence/development/primary-0000/evidence.sqlite
cargo run -p ald-cli -- manifest --protocol-version 2
```

`analyze select` locks, per role, the ordinary predictor with the
lowest validation Brier; `analyze score` refuses to proceed unless
the recomputed selection matches the locked record, then writes
per-dyad primaries, control contrasts, and cohort intervals.

## Layout

- `crates/ald-learners` — `AgentPolicy`, the one learner.
- `crates/ald-orchestrator` — `run_dyad`: seeds, workload, joint
  training, frozen test, evidence, journal, receipt.
- `crates/ald-cli` — the `ala` command-line interface.
- `crates/ald-scenario`, `ald-interventions`, `ald-gateway` — case
  generation, control branches, delivery routing.
- `crates/ald-analysis` — ordinary predictors, dyad primaries,
  control contrasts, cohort intervals and bootstrap.
- `crates/ald-evidence`, `ald-verifier`, `ald-hashing`, `ald-merkle`,
  `ald-checkpoint`, `ald-anchor`, `ald-types` — the evidence spine:
  canonical hashing, seeded PRNG, hash-chained SQLite event streams,
  checkpoints, simulated anchors, independent verification.
- `crates/ald-config`, `ald-isolation`, `ald-leakage`,
  `ald-lifecycle`, `ald-ops` — supporting contracts (runtime
  environment, isolation modes, leakage probe rule, run lifecycle,
  operator digests).
- `protocols/` — frozen machine-readable study and manifest files.
- `docs/architecture.md` — crate map and the four phases of a run.
- `docs/reproduction.md` — exact reproduction procedure.
- `CONFIGURATION.md` — CLI and environment configuration surface.

## Scope

One synthetic referential game, 16 types, 4 candidates per case, a
fixed 32-token inventory, tabular policies, in-process execution.
No claim is made about human language, language acquisition,
pretrained models, or generalization beyond this game. Estimation
only: the analysis layer reports intervals, not supported/not-supported
verdicts.

## License

MIT — see `LICENSE`.
