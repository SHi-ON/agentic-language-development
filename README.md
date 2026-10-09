# Agentic Language Development — ALA Workshop Paper Source

**Engineering snapshot:** v0.1.535 · 258/261 backlog acceptance criteria verified.

One question governs this branch: **does a learner's contemporaneous
token-association record predict its receiver actions better than a
validation-selected predictor trained on ordinary observations and
transcripts, and when does that prediction track causally effective
communication?** The study identity is `ALA-LV`: a small prospective
exploratory estimation study for one ALA workshop paper.

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
never edited to match the new study. Full pre-trim history is also in the
tested archive next to this checkout (`ala-pre-trim-archive/`).

## Threat model (paper)

Trusted process and operator, cooperative learners, one synthetic
referential game, scratch recurrent policies, fixed 32-token inventory,
within-support test cases, Prototype disposable execution. No
adversarial-process, timing-channel, hardware-isolation, generalization,
acquisition, or human-language claim.

## Runnable workflow

```sh
pnpm install
pnpm run check            # lint + secrets + deps + build + paper tests
pnpm run paper:validate   # secrets + frozen claim boundary
pnpm run paper:run --manifest <path> --cohort A|B --batch <i>  # study (P6)
```

`paper:qualify/calibrate/freeze/run/audit/analyze/export/reproduce` are the
paper facade; verbs not yet implemented fail closed with their owning phase
instead of reporting success. `pnpm test` still runs the broad remaining
suite for diagnosis; the gate is `pnpm run test:paper` (18 focused files).

## Evidence status and remaining gates

- Live verified originals on this host: **0** (`evidence/` empty; E03/LV01
  originals absent). Retained summaries are unreproduced summaries.
- Eligible ALA-LV results: **0**. Collection stays disabled until the P3
  study/analysis protocols freeze, P4 qualification passes on the exact
  execution source, and P5 admission reserves the whole campaign inside the
  additional 12 CPU-hour allowance.
- Historical deviation preserved: 10 sealed development-iteration candidates
  against the 5-iteration allowance; cumulative CPU and working peaks
  unknown. New work charges the additional allowance, never a reset.

## Reproduction without this checkout

Reviewers reproduce the paper's numbers from the sealed compact export in
the paper repo (`make reproduce`) without rerunning training. Optional
fresh training uses the public source archive at its frozen commit.
