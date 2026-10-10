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
| Seven paired branches from recorded pre-state | Actual (executed); live restoration unverified | 1680/1680 branches, paired scenario match, evaluationOnly + checkpoint binding; see next row |
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

## D6. Retention rule vs embedded heads (critique G09, 2026-10-10)

Promised: the study contract's `retention.blobRule` requires sharing
immutable blobs by digest with explicit references and never cloning a
full bundle per branch; the pre-action payload lists the ledger cutoff
plus policy/state digests. Actual: every branch's
`prediction-commitment` embeds the full `state.trainingLedgerHeads`
(13,519 entries, ~1MB per payload) alongside the small cutoff record,
repeated across all 1,680 branches and mirrored in the live
`evidence.sqlite` (1,680 commitment rows, ~1.69GB). Measured by
`worksheets/ala-commitment-anatomy.py` (read-only structural scan;
`reports/research/ala-commitment-anatomy.v1.json`). Consequence: the
retention format as executed violates blobRule; per-branch bytes are
structural, not a measurement artifact. The paper's bottleneck sentence
is licensed by the sealed anatomy census, not by the P5 prose alone.

## D7. Treatment controls ineffective as executed (critique N1, 2026-10-10)

Seven branch labels existed, but three control families were not
effective. Ledger shuffle: both cases copy one scheduled example, so
the derangement swaps identical selections (240/240). Random: one
dyad-level seed serves all slots and branches, so turn-0 deliveries
repeat exactly (S12 everywhere). Sender shuffle: shared pre-state plus
shared seed make turn-0 proposals identical, so the shuffle is a no-op
(240/240 normal==shuffled). Full trace and counts:
`reports/research/ala-treatment-validity-correction.v1.json`;
reproducing extract: `reports/research/ala-message-extract.v1.json`;
discriminating fixture:
`packages/orchestrator/__tests__/ala/ala-treatment-validity.test.ts`.
Original calibration is partial machinery with invalid control
construction; its compute/storage measurements describe that
historical workload, not a complete valid study.

## D8. Census follow-ups + audit-text corrections (round 2, 2026-10-10)

Seed census (`worksheets/ala-seed-census.py`;
`reports/research/ala-seed-census.v1.json`): all 1680 branch
run-configs share one `randomSeed` and one `seedBindings.gateway`,
agreeing with the N1 extract — gateway constancy now has a
reproducing artifact. Intention census
(`worksheets/ala-intention-census.py`;
`reports/research/ala-intention-census.v1.json`): turn-0 intention
multiplicity is exactly 1 in all 1680 branches (the extract's
last-match projection is unique), token/null agreement 1680/1680,
and turn-0 `babyProposalHash` is equal across all 7 siblings in
240/240 slots. Byte-audit corrections (same run): the corpus rule
text wrongly said "median-size" while the code picks smallest-size —
text fixed (picks unchanged); sqlite `-shm`/`-wal` sidecars
(32768 B + 0 B) now counted separately; coverage denominator derived
from the branch directories instead of hardcoded.
