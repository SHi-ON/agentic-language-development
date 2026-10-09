# ALA-LV paper study guide (active)

Study identity: `ALA-LV`. Question: does a learner's contemporaneous
token-association record predict its receiver actions better than a
validation-selected predictor trained on ordinary observations and
transcripts, and when does that prediction track causally effective
communication?

## Authorities (read in this order)

1. `protocols/ala-lv-scope-and-resource-amendment.v1.json` — prospective
   scope and the additional 12 CPU-hour allowance. Collection is disabled
   until its gate closes (P3 protocol freeze + P4 qualification + P5
   admission).
2. `reports/research/ala-claim-evidence-inventory.v1.json` — the live
   claim/evidence index. Zero live originals on this host; zero eligible
   new results until the P6 sealed export.
3. `protocols/ala-lv-study.v1.json` + `protocols/ala-lv-analysis.v1.json`
   (P3 freeze) — the exact frozen study/analysis contracts. Nothing runs
   before they exist.
4. `docs/reproduction.md` — how to reproduce every number.

## What is retired (preserved, not governing)

- The broad program docs (`BACKLOG.md`, `SPECIFICATION.md`,
  `LEDGER-INTEGRITY-DESIGN.md`, `CONCEPT-IDEA.md`, `RESEARCH.md`,
  `EXPERIMENT-NOTEBOOK.md`) stay byte-identical as history. They do not
  authorize ALA-LV runs, and their old gates (LV01 confirmatory family,
  Mode R campaign, five-iteration allowance for new work) do not govern
  this study. `BACKLOG.md` is retired in place rather than rewritten
  because the preserved anchor-lane checks consume its exact bytes (see
  the change log).
- Historical protocols, receipts, failed/aborted attempts, and the
  iteration-accounting deviation are never edited, re-pointed, or
  deleted. LV01 sample size, power requirement, margins, stage labels,
  and historical classifications are never silently changed.
- Removed active surface (book, twins, Mode R deployment, red-team and
  crypto-research packages, 166 campaign scripts) is preserved in the
  tested pre-trim archive beside this checkout and in Git history.
  Every removal group is logged in `docs/paper-focus-change-log.md`
  with its dependency proof.

## Reusable components (qualify, don't assume)

- `packages/learners/src/{recurrent-model,recurrent-scratch-policy,ledger-value-prediction,game}.ts`
- `packages/analysis/src/ledger-value.ts` (predictor/scoring primitives;
  `analyzeLv01Family` must not classify ALA cohorts)
- `packages/orchestrator/src/experiments/lv01-{collector,auditor,paired-predictions,ordinary-wiring,ordinary-loader,audit-loader,action-draw,ledger-treatments,schedule,snapshots,order-check}.ts`
- Integrity spine: `packages/{hashing,merkle,evidence,verifier,checkpoint,types,config,scenario,lifecycle}` + `tools/integrity-auditor`

Populated files and `passed:true` fields are not proof of integrated
execution. P4 qualifies actual vectors, fits, treatment chronology, and
restoration through a real worker before any study dyad runs.

## Boundaries that stay fixed

- One game, scratch GRU policies, fixed 32 tokens, alternating roles,
  within-support tests, trusted Prototype runtime. No second task or
  learner family to rescue a result.
- Native readout's information advantage is stated, not hidden.
  Complete-policy replay is a reference, not a baseline.
- Signed/unsigned equal content predicts identically; compact sufficient
  summaries predict identically to full history.
- A valid chance-level or negative dyad stays included. Never
  stop/continue on observed effects. Missing evidence never becomes
  eligible silently.
