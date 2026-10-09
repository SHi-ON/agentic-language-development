# CI and test reliability policy

Every pull request and every push to `main` runs the paper-checks
workflow. The required checks are the `paper-check` job and the
`paper-validate` job; repository branch protection must require both job
names. The first job runs the linter, the secret scanner, a blocking
high/critical dependency audit, the TypeScript build, and the focused
paper Vitest gate (`test:paper`, 18 files). Its retained artifacts include
the complete pnpm audit JSON alongside the runtime reports. The project
explicitly approves only the pinned `better-sqlite3` and `esbuild`
lifecycle builds; new or upgraded lifecycle scripts require review before
installation. The `paper-validate` job re-verifies secrets and the frozen
claim boundary. Broad campaign suites (Mode R containers, red-team,
crash-safety) are archived history, not proposed-change gates.

The check job uploads `/usr/bin/time -v` output for each run. Artifacts
are retained for 30 days so runtime regressions and recurring failures can
be compared between runs.

Tests are never retried or silently ignored. A flaky test blocks merge until
it is fixed or explicitly quarantined. Quarantine requires all of the
following in the same change:

1. a tracked issue linked from the test;
2. a named owner and removal condition;
3. movement into a visibly named, non-required quarantine job rather than an
   inline skip in the consolidated suite; and
4. an update to the acceptance-coverage check so no `Done` criterion loses
   its required blocking coverage.

Seven evidence-absent checks are quarantined under the governed deviation in `docs/quarantine-record.md`; there is no separate quarantine job. Any future job
must preserve failure history and must not be accepted as evidence for an
experiment-readiness gate.

Active deviation: 7 evidence-absent (not flaky) checks are quarantined via
loud inline skip under R5-A governance; see `docs/quarantine-record.md`,
which records the owner, per-check removal conditions, and why a separate
job would carry no added signal. The deviation ends when that record shows
zero active quarantines.
