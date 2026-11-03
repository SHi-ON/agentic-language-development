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

### B2 qualification dyad: VALID (720/720)

Slot `development/primary/0`, TVPS, resumed run 00:09:32Z to
01:28:44Z. Receipt `valid`, all paired cases executed and audited,
protocol 2. Cost (all inside guard with margin):

- CPU 1.296 h (cap 4.0; 32%), wall 78.9 min (cap 240; 33%),
  retained 3.42 GiB receipt / 4.1 GiB root du (cap 8.0; 43%).
- Peak RSS: 725 MiB process; 906 MiB sampled tree peak (79
  one-minute samples; stated as sampled).
- Verification (in-slot audits) 5.9 min of wall; no guard within
  20% of any cap, so G2.3 needs no second qualification dyad.
- Extract 0.9s (6 batches, 1440 records); census 7.8s;
  analysis 25.4s.

Construction (hard Phase B exit: SATISFIED): A7 driver 6/6
batches ACCEPT with 106 genuine collisions counted and 0
repeated-content pairs.

Realized controls: shuffled delivery join 720/720 with 0
mismatches (planned block tokens equal live shuffled-branch
deliveries); random-branch deliveries 32/720 distinct (full
inventory variation) with 720/720 distinct per-case draw seeds;
coverage exactly 120 distinct cases per block with balanced
targets (10 per eligible type).

Learning sanity (reported, not gated): flat at chance. Accuracy
blocks hover 0.23-0.30 across all 3000 training turns; last full
block 0.248 vs 0.25 chance. The census first misreported 1.000
from the single-turn ragged tail (post-training evaluation
turn); fixed to read the last full block and re-frozen before
G2 per B4 (`d273931`). A chance-level parent is inside the
design's contemplated outcomes (bad-task-agent explicitly
allowed); estimates below predict its actions regardless.

### B4 analysis dry run: COMPLETE

Pipeline runs end to end on real v2 output in 25s/dyad (A input
for G2). n=1 development cohort: primary +0.1425 (positive
favors native), interval degenerate by rule as expected.
Selection discriminates by role (baby-a: ordinary-record-softmax,
baby-b: task-history; validation Briers 0.31-0.75); test ordinary
0.45/0.35 vs native 0.33/0.19; replay matches-or-beats native
(excess 0.0001/0.0745); native missing-token fallbacks 0/240
(100% test coverage). Analysis code frozen at `d273931`; no
changes after G2 except the G2 tuple fill and rebind.

## Gate G2: DECIDED (10,10,2)

Date: 2026-11-03 UTC. Decided on measured numbers (no model
uncertainty in the decision inputs).

### G2.1 reservation

R = n*f*C + A + D with n=24, f=2, C=1.29617 CPU-h/dyad (B2
receipt), A=24x25.4s=0.17 CPU-h (B4 dry run), D=1.29619 CPU-h
(summed measured dev: B2 + 70ms false start):

R = 63.68 CPU-h, frozen as campaignCpuHoursMax=63.7 in the
amendment (rounded up; a cap must not sit below the need).

### G2.2 size decision

(20,20,2) is disk-impossible: 44 dyads need 150 GiB extracted
against 125 GiB fleet capacity, and no host can hold the 57 GiB
second tarball set. (10,10,2) fits everywhere with >=20%
headroom (see C1 topology). Wall: ~5h at <=50% load, fits one
week. Precision note: n=20/cohort gives a 97.5% Student-t
half-width of 0.544xSD (t(0.9875,19)=2.4334 from the repo's own
quantile); SD is unknown until data (B4 n=1 gives no SD), so no
power claim attaches: exploratory estimation as registered.

### G2.3 second qualification: NOT TRIGGERED

B2 peaked at 43% of the tightest cap (retained GiB); nothing
within 20% of any cap, 720/720 clean.

### G2.4 tuple fill and admission

`48e3cb0`: chosen=[10,10,2], 2 batches per cohort, reservation
cap filled; `8e7bad6`: manifests rebound to the G2 protocol
commit (a mid-G2 miss: rebinding before committing the protocol
edit broke protocolCommit verification on all hosts; fixed by
rebinding after, check green everywhere). Manifest bytes
identical on AVPS/TVPS/LVPS/OVPS
(edad59a3...); admission records bind manifestSha256
83c7b1bc... identically on TVPS-A, TVPS-B, OVPS-A, OVPS-B
(full (10,2) cohort tuples; index ranges are procedural in the
C1 registry).

### G2.5/C0 smokes: PASS

One-case harness smoke under the verified manifest (see the
amendment-corrected design: non-study, logged not journalled,
excluded from D). TVPS twice identical (self-parity),
LVPS and OVPS identical to TVPS byte-for-byte, including ARM:
case bef69732..., bundle 26c2e781.... No host dropped. Smoke
cost is seconds; fixtures logged here, smoke roots deleted after
parity (explicit rm on all hosts).

### Watch items for Phase C

- AVPS disk was 95% full (13 GiB free) at G1. The qualification dyad
  runs on TVPS per the plan; keep AVPS clear of evidence output.
- Fleet hosts need the paper repo as a read-only sibling checkout:
  the A7 driver imports the paper diagnostic's `verdict()` at runtime
  (`--paper-repo`, `$PAPER_REPO`, or the sibling path). B1/C0 setup
  must clone it alongside the fork and never modify it.

## Phase C: campaign

### C1 registry: FROZEN

`be0cc76` advances the registry to the hygiene-fix fork tip and
manifest binding 484fd16e... (assignments unchanged from the G2
record: TVPS A/B 0-6 at concurrency 6; OVPS A/B 7-9 at
concurrency 2; LVPS mirror-only; reserves A/B 10-11 run on the
same host as the invalid primary).

### C2 wave 1: systematic cohort-A failure, root-caused and fixed

TVPS A/primary/0-5 all failed in ~220ms each at the first
observation: `HygieneViolationError: prose-string at runId`
(runId `ala-A-primary-0000-parent`). Systematic, cohort-A-only:
the prose heuristic read the cohort letter `A` as the article
`a`. Six `failed` journal entries stand as evidence (resumable,
charged to D).

Fix at `049e603` (hygiene layer, not a runId rename: runIds are
frozen in receipts, admissions, registry, and seeds): narrow the
prose gate to ignore single-character words in run-id-shaped
strings only (`RUN_ID_PATTERN`-gated). Multi-letter function
words still reject; free-text fields keep full sensitivity
(pinned in the suite). 43/43 hygiene tests green (2 failed
pre-fix as intended); `pnpm run check` green 26 files/258 tests.

Consequences of the fix commit, all executed: manifests rebound
to the post-fix closure (377 files, identity 484fd16e...);
stale 83c7b1bc... admission records confirmed on all four
host-cohort roots, removed, and regenerated (10 primaries, 2
reserves each) with identical 484fd16e... on TVPS-A/B and
OVPS-A/B; fleet re-pinned to `be0cc76` with check PASSED on
TVPS/OVPS/LVPS (26/258 each); G2.5/C0 smokes re-run under the
new manifest: TVPS self-parity plus LVPS/OVPS byte-identical to
TVPS including ARM, fixture unchanged (bef69732..., 26c2e781...),
so the fix is byte-preserving outside the gate. No host dropped.
Smoke roots deleted after parity (explicit rm on all hosts).

### C2 wave-1 second failure: child-prefix case, root-caused and fixed

The relaunched TVPS A/primary/0-5 plus OVPS A/primary/7-8 all
failed ~10 min in at the first paired case: `LV01 collector:
child run identifier prefix is invalid`. The runner built
childRunIdPrefix as `ala-A-primary-...`, but the LV01 collector
grammar (predating the ALA runner) accepts lowercase slugs
only. B2 passed only because cohort `development` is already
lowercase, and no test ever executed the runner deep with
cohort A/B: the same G1 lesson as the missing v2 binding, one
layer deeper. Eight more `failed` journal entries stand as
evidence (resumable, charged to D).

Fix at `de48a99`: the runner emits the collector-facing prefix
lowercased (`ala-a-...`, extracted as `alaChildRunIdPrefix`);
journal, receipt, seeds, and directory identity keep the cohort
as-is, and B2 (`development`) bytes are unchanged. A full audit
of every `input.cohort` sink found no other charset-validated
consumer (derangement seeds take any non-empty non-NUL string;
receipt prefix is write-only; draw-scope values are B2-proven).
Pinned: every cohort's prefix must match `/^[a-z0-9-]+$/`
(failed pre-fix, 9/9 seeds tests green post-fix); gate green
26 files/259 tests.

Cascade repeated: manifests rebound (closure c3742b70...,
identity 9996b9bd...); stale 484fd16e... records replaced on
all four roots with identical 9996b9bd...; fleet re-pinned to
`1d78a06` (check PASSED everywhere, 26/259); smokes re-run with
full TVPS/LVPS/OVPS parity and an unchanged fixture
(bef69732..., 26c2e781...), as the smoke bypasses the runner
prefix. No host dropped; smoke roots deleted. (Operator slip:
the first re-pin attempt used a mistyped ref SHA and failed
identically on all hosts before any checkout moved; no harm.)

### C2 journal-append race: B/primary/2 attempted entry lost, repaired

Wave-2 TVPS launch (A/6 + B/0-4 in one loop) exposed that
`appendAlaJournal` is read-modify-write, not O_APPEND: five
same-millisecond appends to the B journal landed four entries
and lost B/primary/2's `attempted` (wave-1's 20+ same-loop
appends all landed by luck). A missing attempted entry is a
live hazard (resume refused, duplicate fresh launch allowed),
and the supervisor writer deliberately refuses attempt records,
so no sanctioned repair-by-rewrite exists. Repair, all
sanctioned: killed the B/2 runner tree (4 PIDs verified
pre-kill), removed its partial dyad dir, appended a supervisor
`note` documenting race + kill + relaunch (plus a second note
correcting an imprecise timestamp in the first), and relaunched
B/2 fresh and solo at 04:25:58Z; the runner journalled
`attempted` normally. B journal now complete (attempted 0-4).

Rule for the rest of the campaign: stagger every multi-dyad
launch (solo journal access per root) and verify journal counts
after each launch. No mid-campaign code change: the fleet holds
one uniform closure, and staggering closes the ms-scale race
window procedurally. Post-campaign hardening (filed): make
`appendAlaJournal` O_APPEND with a concurrent-append regression
test.

Pause-rule check at wave-1 completion: TVPS first-five census
all clean (120/120 coverage per block, 720/720 delivery join
with 0 mismatches, balanced target histograms, matching B2's
realized controls); sixth dyad and OVPS A/7 (ARM) identical.
No host paused.
