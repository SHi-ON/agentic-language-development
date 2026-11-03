# Corrected-campaign log (ALA-LV v2)

Fork-only, local-only basis: all engineering in this repository
(`agentic-language-development`, our fork; upstream is fetch-only and
never a work base), all runs on project hosts, no CI. The paper
repository stays sealed: no v1 claim edits, no paper-side changes for
the corrected campaign. Working plans live gitignored under `plans/`;
this log records committed gate verdicts and measured campaign data.

## Gate G1 (Phase A exit): PASS

Date: 2026-11-03 UTC. Suites ran on `16fc847` plus the G1
evidence files; evidence committed at `f0c08da`, manifest stamp
rebound at `432a4e1` (closure hash unchanged throughout).
Phase A units: `d847e7c` (A5) through `d7cd33d` (A7).

- `pnpm run check` green on AVPS: 26 files, 257 tests.
- Broad fork suite green: 213 files, 2323 tests.
- Worksheet suite green: 20/20 (`python3 -m unittest discover
  -s worksheets/tests`), including the 6 A7 driver agreement cases.
- V1 paper-lane cases byte-identical: the baseline-to-tip diff over
  all test files removes exactly 2 lines, both single-line imports
  expanded into multi-line import lists that retain the original
  symbols (`ala-runner-gates.test.ts`, `lv01-collector-planning.test.ts`).
  Every other test change is purely additive. No v1 case body changed.
- A7 agreement: the driver ACCEPTs the sealed genuine fixtures
  (`valid_indomain`, `collision`, `repeated_content`) with the
  manifest's expected counters and REJECTs all 240 retained sham
  pairs with sham reasons, through the paper diagnostic's own
  imported `verdict()`. Sealed-input hashes pinned in the tests.
- Conformance matrix current (261 criteria, 96 normative lines); the
  v1 execution manifest rebound to the 371-file closure
  (`sha256:54fdb285...`).
- Freeze: the v2 protocol triplet
  (`protocols/ala-lv-study.v2.json`,
  `protocols/ala-lv-analysis.v2.json`,
  `protocols/ala-lv-scope-and-resource-amendment.v2.json`), the v2
  schemas, and the allowance are frozen in git at this tip. Only the
  G2 campaign-size tuple may change them afterwards.

Issues found and closed during G1 (all green at verdict): the
execution closure went stale under the new G1 script (rebound); the
conformance matrix went stale under Phase A assertions (regenerated
with the sanctioned writer; the diff is exactly the two new A3
gateway assertion mappings); the first broad-suite run failed 14
gate tests plus 3 CLI tests on the stale closure (all pass rebound,
12/12 CLI in isolation and in the full run).

### A4 size datum (head-share measurement)

Method: `scripts/ala-g1-head-measure.ts` builds both prediction-state
shapes exactly as the nursery call site does, with real-length
71-char entry hashes. Fan-out per v2 dyad is exactly 720 slots x 7
branches x 1 turn = 5040 payload instances (each branch serves one
evaluating turn; verified in the executor).

Measured (ledger length L per role):

- Embedded state: 74.0 bytes per hash (L=6600: 976,821 B).
- Digest state: 126-128 bytes regardless of L (test bound: <300 B).
- Sidecar: embedded bytes + ~190 B envelope (L=6600: 977,009 B).

Cross-check: L=6600 reproduces the paper-era cited v1 scale (~1.57
GiB literal heads over 240 slots x 7 branches = 1680 instances) to
within ~3%, so L~6600-6800 is the calibrated ledger length.

Projection for a full v2 dyad at L=6600: v1-rate head share 4.59 GiB
(5040 x 976,821 B) becomes ~1.55 MiB (0.93 MiB sidecar once + 5040 x
128 B digests). The repeated-head share is gone (three orders of
magnitude). Total retained dyad size is NOT projected here (per-slot
non-head bytes need a live dyad); B2 measures real retained GiB
against the A7 manifest and confirms or corrects this datum.

## Phase B: qualification (TVPS)

### B1 host setup: DONE

Date: 2026-11-03 UTC. Method: `scripts/fleet-setup-host.sh`
(idempotent; verified hash-identical over scp before first run).

- Alias TVPS (user `maria`, 12 CPUs, 94 GiB RAM, 81 GiB free at setup).
- Egress to nodejs.org and the npm registry: OK.
- node v24.22.0, pnpm 12.9.1 (`~/.local/bin`), python 3.14.4.
- Fork `git@github.com:SHi-ON/agentic-language-development.git` at
  `601a92c6d7529e005e6f35ba4c326adc0c7937d5` (`/home/maria/ald/fork`).
- Paper (read-only sibling for the A7 driver) at
  `03c6e8159c1ba4301624f6981cdd976bbe60e45e` (`/home/maria/ald/paper`).
- `pnpm install --frozen-lockfile` + `pnpm run check`: PASSED on the
  host (26 files, 258 tests, includes the v2-manifest binding case).
- No host has write access to another host's evidence root; the
  paper checkout is never modified (driver imports only).
- Setup-script fixes during B1 (both committed): short SHAs now
  resolve before the ref comparison; first run used a wrong home
  directory for the TVPS user (resumed idempotently, no harm).

### B2 false start and resume (index 0)

The first launch ran the v2 path against the v1 execution
manifest: Phase A never built `ala-lv-execution-manifest.v2.json`,
so the runner crashed past the gates on an unbound protocol path
(70ms, zero compute). Fixed at `bd1ae3b` (manifest built with the
sanctioned writer + regression case), TVPS moved to `601a92c`,
slot `development/primary/0` resumed with `--resume` on the v2
manifest at 2026-11-03T00:09:32Z. The resume wiped partial output
and reruns deterministically with identical seeds; no additional
budget consumed (one distinct dev slot ID). G1 lesson: no Phase A
test ever executed the v2 path end to end, so the missing binding
survived the gate; the regression case now pins v2-manifest
verification in the suite. Evidence root:
`/home/maria/ald-evidence/ala-lv` (outside the checkout). RSS
sampled every 60s to `/tmp/ala-b2-rss.log` (stated as sampled).

### Early C0 prep: LVPS/OVPS setup DONE (during B2)

Same setup script (arch fix at `ecceb1e`, hash-verified over scp),
both pinned to fork `601a92c` + paper `03c6e81`, check PASSED:

- LVPS (x86_64, user `ubuntu`, 4 CPUs, 7 GiB RAM): node v24.22.0,
  pnpm 12.9.1, python 3.14.4. Disk 85% full (12 GiB free) at setup:
  may not fit a full v2 dyad; G2 must scope or exclude LVPS.
- OVPS (aarch64, user `ubuntu`, 4 CPUs, 19 GiB RAM): same toolchain
  (ARM dist). First attempt failed on the x64 binary (exec format
  error); script now selects the dist arch from `uname -m`. Disk
  21% used (38 GiB free).

All fleet checkouts stay at `601a92c` until G2 re-pins every host
together. (Incidental: the arch-fix commit subject contains a
non-English word from a typing slip; left as-is rather than
rewriting pushed history.)

### Watch items for Phase B

- AVPS disk was 95% full (13 GiB free) at G1. The qualification dyad
  runs on TVPS per the plan; keep AVPS clear of evidence output.
- Fleet hosts need the paper repo as a read-only sibling checkout:
  the A7 driver imports the paper diagnostic's `verdict()` at runtime
  (`--paper-repo`, `$PAPER_REPO`, or the sibling path). B1/C0 setup
  must clone it alongside the fork and never modify it.
