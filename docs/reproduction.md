# Reproduction guide (active)

Two distinct operations; do not conflate them.

## Fast analysis reproduction (reviewers)

Regenerates every table, figure, macro, and abstract number from the
sealed compact export, without rerunning training. Runs in the paper
repository (sibling `agentic-language-development-paper`):

```sh
make reproduce
```

Inputs: `results/summary.json` (sealed export), `results/manifest.json`
(provenance), `results/per-dyad.tsv` (sufficient analysis inputs), pinned
`requirements.txt` (self-contained uv environment). No private token,
sibling repo, network fetch, or host path is required.

Current state: the sealed export is FEASIBILITY-ONLY (methods
measurements, no study estimates; `per-dyad.tsv` has no rows by design).
`make reproduce` regenerates methods macros only.

## Fresh training replication (not available)

The original study would rerun from the frozen protocol as:

```sh
pnpm run paper:run --manifest <execution-manifest> --cohort <name> --index <n>
```

but P5 admission was refused (NO-GO) and the development budget is
spent, so no cohort is runnable. Same-seed `--resume` reruns a crashed
slot only. Any future study needs a new versioned protocol revision and
a fresh prospective allowance; it would be a new study, never completion
of the v1 campaign.

## Environment

- Node v24.21.0, pnpm (CI pins 12.3.4), uv-managed Python 3.13 with
  `requirements.txt` (numpy/scipy; no R/Rscript; no Homebrew/system
  installers; no Docker or LaTeX on this host).
- Paper PDF builds in CI from the pinned TeX Live image; the PDF and log
  are downloaded for local inspection with `pdfinfo`/`pdftotext`/rendering.

## Provenance checks

- `pnpm run paper:validate` — secrets + frozen claim boundary + protocol
  binding.
- `pnpm run paper:audit` — claim boundary only; reports PARTIAL and exits
  2. There is no sealed study-evidence audit (no study ran).
- Every manuscript measurement traces to the sealed export via
  `docs/claims.tsv` in the paper repo; protocol design constants cite
  the frozen contracts digested in the export manifest. No console
  summaries, placeholder macros, or hand-typed table cells.
