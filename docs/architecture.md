# Architecture overview

The workspace implements one method, the **Emergent Signaling Dyad**
(see `../README.md`): two symmetric agents (`agent-0`, `agent-1`),
each with a sender policy and a receiver policy, jointly trained per
dyad by REINFORCE on a shared success reward and frozen at test.
Everything below exists to run that dyad deterministically and to
score and verify its evidence.

## Crate map

| Crate | Responsibility |
|---|---|
| `ald-learners` | `AgentPolicy`: sender logits `[16 types][32 tokens]` plus receiver logits `[32 tokens][16 types]`, softmax sampling, REINFORCE updates with running-mean baselines, deterministic serialization. The only learner. |
| `ald-orchestrator` | `run_dyad`: manifest and admission gates, seed derivation, fold generation, joint training, frozen-policy test, artifact and evidence writes, journal, receipt. |
| `ald-cli` | The `ala` binary: `run`, `analyze select|score`, `verify`, `manifest`, `journal`. |
| `ald-scenario` | Deterministic case generation: 4 distinct candidate type codes and a target index per case; the five branch labels (`normal`, `disabled`, `constant`, `random`, `shuffled`). |
| `ald-interventions` | Control-branch substitutions applied to a native distribution. |
| `ald-gateway` | Delivery routing of a token to the receiver role; names the substitution point. |
| `ald-analysis` | Ordinary-record predictors (uniform, validation-majority, transcript-only, task-history, ordinary-record-softmax), validation selection, dyad primaries, paired control contrasts, simultaneous bounds, invalidity bounds, cohort t-interval and bootstrap. |
| `ald-evidence` | SQLite evidence store: hash-chained event streams (`entryHash = SHA-256(domain || 0x00 || canonical event)`), append and chain verification. |
| `ald-verifier` | Independent verification of an evidence database: stream chains, checkpoint hashes, Merkle inclusion proofs. |
| `ald-hashing` | RFC 8785 canonical JSON, SHA-256 domain hashing, seed derivation, and `SeededPrng` (SHA-256 counter mode) used for every random draw. |
| `ald-merkle`, `ald-checkpoint`, `ald-anchor` | RFC 6962 trees and proofs; checkpoint manifests binding every stream of a run; simulated-chain anchor receipts. |
| `ald-types` | Shared hash domains, stream contracts, and study types. |
| `ald-config` | Runtime environment loading (`load_runtime_environment`); see `../CONFIGURATION.md`. |
| `ald-isolation`, `ald-leakage`, `ald-lifecycle`, `ald-ops` | Isolation-mode contracts, the semantic-leakage probe rule, the run lifecycle state machine, operator failure digests and retention. |

## The four phases of one dyad run

`run_dyad` (`crates/ald-orchestrator`) executes, in order:

1. **Fold generation.** From the manifest workload, per role:
   training, validation-fit, validation-selection, and
   within-support test folds, each generated from its own derived
   seed. Training records carry the candidate type codes and target
   index that episodes are built from.
2. **Joint training.** Two fresh `AgentPolicy` instances alternate
   sender/receiver roles, one episode per direction per step over the
   receiver role's training fold. The sender samples a token for the
   target type; the channel drops it with probability 1/8; the
   receiver samples a candidate position from its posterior (exact
   uniform when the token is dropped) without ever seeing the true
   type; both share reward 1/0 for correct/incorrect choice. The
   sender REINFORCEs its sampled token (per-type baseline) even when
   dropped; the receiver REINFORCEs toward the chosen candidate's
   type only (per-token baseline), and a dropped token is a receiver
   no-op.
3. **Freeze and persist.** The two full policies are written as
   `agent-policy-agent-0.json` and `agent-policy-agent-1.json`.
   Serialization is deterministic (fixed field order, stabilized
   floats), so the bytes are identical across runs.
4. **Frozen test.** Per role, test tokens come from the opposite
   agent as sender through the same 1/8 channel; native predictions
   are the role agent's frozen receiver position posteriors. Each
   case also gets an exact paired-draw replay distribution and all
   five branch distributions. Outputs: `role-folds-<role>.json`,
   `branch-probabilities-<role>.json`, `primary.json`,
   `dyad-result.json`, `dyad-receipt.json`, `evidence.sqlite`, and
   attempted/terminal lines in the evidence-root `journal.jsonl`.

## Boundaries

- Agents share nothing except the delivered token (or its absence)
  and the scalar episode reward. Neither policy table, baseline, nor
  seed stream is shared; the receiver is never given the true type,
  in training or at test.
- Learning happens only in phase 2. Phases 3–4 and all of
  `ala analyze` read frozen artifacts; scoring re-verifies the locked
  predictor selection and refuses to proceed on mismatch.
- Determinism is end to end: identical manifest, cohort, kind, and
  index reproduce byte-identical policy, fold, and probability
  artifacts (the receipt's wall-clock field excepted).
- Study cohorts `A`/`B` are gated on an admission record with
  decision GO at `<evidence-root>/admission.json`; other cohorts run
  without one.
