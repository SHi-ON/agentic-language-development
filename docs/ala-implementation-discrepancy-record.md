# ALA-LV implementation discrepancy record (tracked, versioned)

Date: 2026-10-09. Execution source: ALD `66712b5` (branch `ala-workshop`).
This record distinguishes the frozen v1 protocol promise, the actual
implementation, directly observed evidence, inferred behavior, and missing
evidence. It corrects current description only; historical receipts stay
inspectable and are not rewritten.

## D1. Ledger content (critique F01)

- Promised (manuscript draft): token–type co-occurrence counts/weights.
- Actual (`packages/learners/src/tabular-reinforce.ts`, recurrent path):
  `associationRow` queries the learner's receiver distribution over all
  16 type codes for the delivered symbol
  (`recurrentModel.predictCandidates`), takes natural logs (floor
  1e-12), applies softmax at the configured temperature, rounds to 6
  decimals, and records `hypothesis.created` (new symbol) or
  `hypothesis.revised` (argmax type-code change) events; a wrong
  high-confidence outcome appends `hypothesis.contradicted` (> 0.5).
- Observed: retained ledger events carry `associationOverTypeCodes[16]`,
  `argmaxTypeCode`, `confidence`, `policyContextHash`, event/sequence/turn
  bindings. The readout is a privileged policy-derived projection, not a
  frequency table.
- Inferred: "contemporaneous" means recorded during the outcome step of
  the turn whose policy produced it; latest retained event per token is
  not proven equal to a final-model readout (staleness undisclosed).
- Missing: quantified staleness per token; a matched-information control.

## D2. Baseline/validation procedure (critique F02)

- Promised (frozen study contract): execute both validation folds, fit all
  five ordinary candidates, select by validation Brier with tie rule,
  refit on allowed pre-test records, commit selected-ordinary vectors
  pre-action.
- Actual (`packages/orchestrator/src/ala/ala-runner.ts`): validation
  partitions are scheduled (counts only) but never executed as episodes;
  paired collection binds `ordinaryId: uniform` with a uniform fit, and
  test branches commit four 0.25 probabilities. No fitted ordinary ran
  pre-action on this path. A code comment cites "BIC selection"; the
  frozen contract specifies validation Brier (comment inaccurate).
- Observed: sampled branch commitments show uniform ordinary id/fit.
- Missing: integrated fitted-baseline execution; any historical
  pre-action fitted-ordinary commitment on the ALA path.

## D3. Resource guards and attempt accounting (critique F03, F11)

- Promised (amendment + plan): enforce 0.25 CPU-hour / 20-minute per-dyad
  guards, retain failures, charge reruns.
- Actual: the runner samples `process.cpuUsage()` (start/end) and
  `process.memoryUsage().rss()` (start, per-case, end) and records them;
  it contains no termination guard, and the launcher sets no timeout.
  The one complete calibration exceeded both limits (0.540 CPU-h,
  33.7 min). `--resume` deletes the partial dyad directory (`rm`,
  ala-runner.ts:234) and retrains; cost is not charged twice.
- Observed: journal shows 3 seed slots (0 aborted, 1 failed, 2 valid via
  resume) but ≥4 physical executions (index-2 ran twice). Index-1's
  parent bundle was removed; only its journal + stdout log remain.
- Inferred: crashed runs cost less than a full dyad (bounded above, not
  measured). The recorded 1 CPU-hour development upper bound is an
  assertion, not a measurement.
- Missing: per-invocation CPU/RSS/byte charges for crashed runs; true RSS
  peak (recorded peak is a 3-point sample maximum).

## D5. Manuscript method → status map (R01 done-when table)

| Manuscript method | Status | Evidence |
|---|---|---|
| Referential game, roles, fixed 32-token channel | Actual (executed) | Parent + branch turn records; schedule |
| Scratch GRU training, 3000 turns, both roles | Actual (executed) | updateCount 3000/agent, optimizer steps, linked checkpoints |
| Policy-derived ledger rows (log-softmax-round, 6dp) | Actual (executed) | Retained hypothesis events with 16-way rows |
| Native predictor formula | Designed + component-tested; not executed on test cases as a scored comparison | `ledger-value-prediction` tests; native vectors persisted in branch commitments but never scored vs ordinary |
| Five ordinary candidates + Brier selection + refit | Designed + fitter component tests; selection/refit not executed | No validation-fold execution; branches commit uniform |
| Pre-action commitment payloads | Actual (executed, uniform ordinary) | Branch `prediction-commitment` intervention events |
| Seven paired branches from restored state | Actual (executed) | 1680/1680 branches, paired scenario match, evaluationOnly + checkpoint binding |
| Live weight/optimizer/hidden/PRNG restoration proof | Unverified | Config flags only; no live-state comparison captured |
| Cohort estimation (t-intervals, bootstrap, 0.02 rule) | Designed, unexecuted | Frozen analysis contract; no cohorts admitted |
| Overhead benchmark comparison | Partially executed (rehearsal); signing-isolation claim unsupported | Rehearsal repeats; see critique F08 |

## D4. Status consequences

- The study stays NO-GO (P5) on measured cost; the refusal is supported
  even with incomplete consumed-cost accounting.
- Calibration claims are partial-machinery observations, not full-study
  validation. Manuscript methods must use the D1 definition and the D2
  chronology; cost text must separate measured execution, safety-factor
  projection, and missing stages (no "order of magnitude on both").
- "Prospectively specified in versioned local contracts" replaces
  "pre-registered" everywhere: no external timestamped registration exists.
