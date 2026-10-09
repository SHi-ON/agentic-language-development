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

Currently fails closed: the P6 sealed export does not exist yet.

## Fresh training replication (optional, bounded)

Reruns the study from the frozen protocol with the exact source commit:

```sh
pnpm run paper:run --manifest <execution-manifest> --cohort A|B --batch <i>
```

Bounded by the P5 admission reservation inside the additional 12 CPU-hour
allowance. Cohort B is the fresh-seed check; reproducing the same data is
not a fresh-seed replication.

Currently fails closed: protocols unfrozen, runner unbuilt (P3/P6).

## Environment

- Node v24.21.0, pnpm (CI pins 12.3.4), uv-managed Python 3.13 with
  `requirements.txt` (numpy/scipy; no R/Rscript; no Homebrew/system
  installers; no Docker or LaTeX on this host).
- Paper PDF builds in CI from the pinned TeX Live image; the PDF and log
  are downloaded for local inspection with `pdfinfo`/`pdftotext`/rendering.

## Provenance checks

- `pnpm run paper:validate` — secrets + frozen claim boundary.
- `pnpm run paper:audit` — claim boundary today; full study-evidence
  audit once the P6 export exists.
- Every manuscript number traces to the sealed export via
  `docs/claims.tsv` in the paper repo. No console summaries, placeholder
  macros, or hand-typed table cells.
