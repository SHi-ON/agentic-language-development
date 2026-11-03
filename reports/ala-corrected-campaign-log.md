# Corrected-campaign log (ALA-LV v2)

Fork-only, local-only basis: all engineering in this repository
(`agentic-language-development`, our fork; upstream is fetch-only and
never a work base), all runs on project hosts, no CI. The paper
repository stays sealed: no v1 claim edits, no paper-side changes for
the corrected campaign. Working plans live gitignored under `plans/`;
this log records committed gate verdicts and measured campaign data.

## Gate G1 (Phase A exit): PASS

Date: 2026-11-03 UTC. Tip at verdict: `16fc847`
(chore rebind), Phase A units `d847e7c` (A5) through `d7cd33d` (A7).

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

### Watch items for Phase B

- AVPS disk was 95% full (13 GiB free) at G1. The qualification dyad
  runs on TVPS per the plan; keep AVPS clear of evidence output.
- Fleet hosts need the paper repo as a read-only sibling checkout:
  the A7 driver imports the paper diagnostic's `verdict()` at runtime
  (`--paper-repo`, `$PAPER_REPO`, or the sibling path). B1/C0 setup
  must clone it alongside the fork and never modify it.
