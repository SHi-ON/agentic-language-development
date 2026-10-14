# Agentic Language Development — ALA Workshop Paper Source

**Engineering snapshot:** v0.1.539 · P0 backlog re-audit 258/261 (historical).

**Status: FEASIBILITY-ONLY. The original ALA-LV cohort study is NO-GO
(P5 admission refused on measured cost); no study estimates exist. The
active program is a feasibility/methods case study from retained
calibration evidence. See `docs/ala-implementation-discrepancy-record.md`
for promised-vs-actual implementation status.**

One question governed the original design: **does a learner's
contemporaneous token-association record predict its receiver actions
better than a validation-selected predictor trained on ordinary
observations and transcripts?** That question is unresolved: the
comparison was never executed with fitted baselines (discrepancy D2).

## Active authorities

- Study scope and resource authorization:
  `protocols/ala-lv-scope-and-resource-amendment.v1.json`
- Live claim/evidence index:
  `reports/research/ala-claim-evidence-inventory.v1.json`
- Paper study guide: `docs/paper-study.md`
- Reproduction: `docs/reproduction.md`
- Every trim/change group: `docs/paper-focus-change-log.md`
- Manuscript (design draft): sibling repo
  `agentic-language-development-paper` (private)

The broad program docs (`BACKLOG.md`, `SPECIFICATION.md`,
`LEDGER-INTEGRITY-DESIGN.md`, `CONCEPT-IDEA.md`, `RESEARCH.md`,
`EXPERIMENT-NOTEBOOK.md`) are preserved byte-for-byte as history and
retired from the active paper gates. The historical protocols, receipts,
failed attempts, and resource-deviation records are likewise preserved,
never edited to match the new study. Pre-trim source remains recoverable
from Git at `2e042b1de8e13d05eb6252a63b6f0815c7841c49`; the verified
restore procedure is in `docs/paper-focus-change-log.md`.

## Threat model (paper)

Trusted process and operator, cooperative learners, one synthetic
referential game, scratch recurrent policies, fixed 32-token inventory,
within-support test cases, Prototype disposable execution. No
adversarial-process, timing-channel, hardware-isolation, generalization,
acquisition, or human-language claim.

## Runnable workflow

```sh
pnpm install
pnpm run check            # lint + secrets + deps + build + paper tests (24 files / 178 tests)
pnpm run paper:validate   # secrets + frozen claim boundary + protocol binding
```

Study collection is disabled: `paper:run --manifest <path>
--cohort <name> --index <n>` refuses cohorts A/B without a P5 admission
record (none exists; NO-GO recorded), and the 3-slot development budget
is exhausted. Same-seed `--resume` reruns a crashed slot only; it never
overwrites a terminal receipt.

`paper:validate` and `paper:run` are implemented; `paper:audit` reports
PARTIAL (claim boundary only, no sealed study-evidence audit);
`paper:qualify/calibrate/freeze/analyze/export/reproduce` are deprecated
unimplemented facades that fail closed (exit 10) and are not part of the
supported interface. `pnpm test` still runs the broad remaining suite for
diagnosis; the gate is `pnpm run test:paper`.

## Evidence status and remaining gates

- Historical E03/LV01 originals: still **absent** on this host. Retained
  summaries are unreproduced summaries.
- New retained calibration: one complete dyad (`evidence/ala-lv/`,
  ~4.6 GiB) plus two failed/aborted slots with journal + logs. See the
  discrepancy record for what this evidence does and does not support.
- Eligible ALA-LV study results: **0**. P5 admission was refused; no
  cohort ran. The sealed paper export is FEASIBILITY-ONLY (methods
  measurements, no estimates).
- Historical deviation preserved: 10 sealed development-iteration candidates
  against the 5-iteration allowance; cumulative CPU and working peaks
  unknown. New work charges the additional allowance, never a reset.

## Reproduction without this checkout

Reviewers reproduce the paper's numbers from the sealed compact export in
the paper repo (`make reproduce`) without rerunning training. Optional
fresh training uses the public source archive at its frozen commit.
