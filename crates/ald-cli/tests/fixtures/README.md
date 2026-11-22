# Fixtures: locked-mean-anchored dyads

One minimal dyad per study cohort (`cohort-A/primary-0000`,
`cohort-B/primary-0000`), each with per-role folds
(`role-folds-{agent-0,agent-1}.json`) and per-case branch probabilities
(`branch-probabilities-{agent-0,agent-1}.json`) in the `run_dyad` layout.

Each dyad is engineered so `analyze_dyad` reproduces the locked v2 cohort
mean as its primary: the uniform ordinary predictor is selected (ordinary
Brier exactly 0.75) and the native distributions are solved so native
Brier = 0.75 - mean. Locked means (pinned in
`scripts/regen_score_fixtures.py` — the original
`reports/ala-campaign-results/analysis-{A,B}.json` source does not exist
in this repo):

- A: 0.12497749132421103
- B: 0.12302667184362459

Regenerate with `./scripts/regen-score-fixtures.sh`, which also runs the
`score_fixture` suite as a self-check; on a fresh clone the regen leaves
`git diff` empty.

The `score_fixture` regression test scores these dyads and asserts the
cohort interval means match the locked means within 1e-9.
