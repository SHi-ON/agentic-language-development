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

### C3 archive progress (wave-1 completions)

TVPS A/primary/0-5: six deterministic tarballs (1.3-1.4 GiB
each) written to the TVPS archive with sha256 sidecars, all
self-verified at creation; root journal + admission copied
alongside (snapshots, refreshed at campaign end). OVPS pulled
all six via rsync --inplace and verified sha256 -c 6/6: two
verified copies exist, so the TVPS extracted trees were deleted
(24 GiB freed, 57 GiB free now). OVPS A/primary/7-8: local
tarballs verified (1.4 + 1.3 GiB); the OVPS-to-TVPS rsync leg
waits on the headroom rule (2 dyads in flight, needs <=1), then
LVPS mirrors the OVPS tarballs from the TVPS archive, then OVPS
extracted trees go. (Operator slip: first mirror rsync omitted
-r and copied nothing; retried correctly, no harm.)

### C2/C3 wave-2 completion and wave-3 launch

TVPS A/primary/6 and B/primary/0-4 all valid; OVPS A/primary/9
and B/primary/7 valid. Every completed dyad census-clean
(720/720 delivery join, 0 mismatches, 120/120 per-block
coverage), including the repaired B/primary/2 and all ARM
dyads. No invalid verdicts anywhere: zero reserves consumed.

Wave 3 launched staggered with journal verification between
launches (race rule holds, no losses): TVPS B/5 + B/6 at
06:05/06:06Z, OVPS B/8 + B/9 at 06:39/06:40Z. TVPS-A cohort
complete (7/7 valid).

C3: OVPS tarballs (A/7-9, B/7) rsynced to the TVPS archive at
zero in flight and verified 4/4; LVPS mirrored them 4/4 (LVPS
disk now 93% full, 5.2 GiB free: the final B/8-9 pull of ~2.7
GiB fits, nothing else goes there). OVPS extracted trees plus
local tarballs deleted after both copies verified (22 GiB
freed, 28 GiB free). TVPS A/6 + B/2 tarballs written; their
OVPS mirror leg in progress, extracted deletion pending
verification.

### C4 completion census: PASS (20/20 over the archive)

Per-dyad recount from the verified tarballs on TVPS, projected
extraction excluding only `runs/` + `evidence.sqlite*` (3.2
GiB/dyad of parent-run checkpoints + sqlite store that no C4/D
code path reads; retained in the hashed tarballs): extraction
refused nothing, all 20 dyads complete against the A7 retained
manifest (6 batches, 1440 records each). Driver verdicts
(diagnoseLedgerBatch over each dyad's 6 block files, mapping
replicating build-valid-ledger-batch.ts): 120/120 ACCEPT, 0
repeated-content pairs everywhere; genuine collisions A
110,168,162,150,168,86,158,228,98,132 (1460 total) and B
202,244,90,190,214,330,116,212,210,212 (2020 total).
Archive censuses repeat the live censuses byte-for-byte in
every headline (720/720 slots, 720/720 delivery join with 0
mismatches, 32/720 random distinct, identical per-dyad
accuracies). (Operator note: first extraction attempt targeted
/tmp and died on tmpfs inode exhaustion; reran on the evidence
filesystem. No evidence touched.)

### Phase D: estimates frozen

Frozen pipeline (unchanged since `d273931`, verified) over the
archive, one invocation per cohort (10 dyads each, ~3m50s):

- Cohort A: n=10, mean +0.1250, SD 0.1303, Student-t 97.5%
  interval [+0.0143, +0.2356] (excludes zero; positive favors
  the native ledger predictor over the validation-selected
  ordinary predictor).
- Cohort B: n=10, mean +0.1230, SD 0.1835, Student-t 97.5%
  interval [-0.0328, +0.2788] (includes zero; reported as-is
  per the pre-agreed null rule).

Both cohorts: 8/10 dyad primaries positive; selector
discriminates (task-history 12 roles, ordinary-record-softmax
8 roles, identically in A and B); native missing tokens 0/480
(100% test coverage); replay matches-or-beats native in all 40
roles (excess Brier positive throughout, max 1.07 in B).

D2 supplements (operator-computed, frozen outputs as inputs,
pipeline untouched; the frozen script emits neither): percentile
bootstrap (repo bootstrapMeanCi, seed
ala-campaign-d2-bootstrap-v1, 10000 iterations, 0.975): A
[+0.0361, +0.2099], B [-0.0113, +0.2299), agreeing qualitatively
with the primaries. Cutoff age per dyad from the heads sidecars:
all 20 dyads froze ledgers at parent-training turn 2996-3000
(sequences 6378-6884); every test prediction uses pre-cutoff
associations, so staleness is structural and near-identical, not
a dyad-level confound. Inherited limitations travel with the
numbers: privileged possibly-stale projections, no
matched-information control, ledger advantage is not
added-information evidence over replay fidelity.

D3 freeze: analysis-A.json (sha256 54efbb34...) +
analysis-B.json (sha256 3171c189...) committed in
reports/ala-campaign-results/ from TVPS; inputs hashed
(manifest 9996b9bd..., script freeze d273931, 20 tarball sha256
sidecars in the archive). Reproducibility verified, not
asserted: cohort-A analysis rerun from fresh extraction is
bit-for-bit identical (excluding producedAt/inputs paths).
Scratch deleted; evidence retained until G3 per C3.

## Extension E1: author-directed n=80/cohort round

Author decision (Gate G3 redirected): no new paper; run to
n=80 primaries per cohort and fold the results into the
existing paper. This lifts the paper's sealed-audit status by
author directive (the D5 rationale against it stands
recorded); paper edits happen after the numbers land, and the
sealed claims are not touched until then.

Composition: 80 total per cohort = 10 frozen round-1 slots
(0-9, valid, identical code and manifest 9996b9bd..., reruns
forbidden by append-only gates) + 70 new slots (10-79). No
code change, no rebind, no re-smoke (source identical; prior
smokes still prove the closure). Fleet stays at `1d78a06`.

Partition (3:1 matching 6:2 concurrency, ~18 waves each):
TVPS A/B 10-62, OVPS A/B 63-79; LVPS mirror-only (97% full).
Reserves stay at indices 10,11 per cohort (kind-distinct from
the new primaries; engine-slot overlap with primaries 30,31 is
contingent on a reserve running, and 0 invalids in 20
round-1 dyads make that unlikely; documented, not patched).
Admissions regenerate at primaries=80/reserves=2 under the
unchanged manifest hash (the runner gates status/cohort/hash,
not index bounds; regeneration keeps the record truthful).

Reservation: R2 = 140 new dyads x 1.7 CPU-h (observed
concurrency pace, above B2's solo 1.296) + 5 analysis/margin =
245 CPU-h, author-directed; the frozen amendment file keeps
63.7 as the G2 record (untouched), this entry is the extension
authorization. The campaign cap is not mechanically enforced
(watchdog gates null-only, pre-G2); per-dyad caps still bind
every dyad.

Archive at n=80 volume (~190 GiB tarballs): the two-copy rule
cannot ride on OVPS/LVPS disks, so SVPS (author host, 829 GiB
free at E1) becomes the second-copy target for all 140 new
tarballs (rsync pull from the TVPS archive, sha256 -c verify).
OVPS still pushes its own tarballs to TVPS under the <=1
in-flight headroom rule. Extracted trees delete only after
archive + SVPS copies verify. Journals/admissions snapshot per
wave as before.

Timing protocol (author asked how long it takes): T0 at the
first wave-1 launch; each wave completion timestamped;
T-end at the 140th valid; |wall/dyad and CPU-h/dyad reported
at E3 from receipts.

### Extension wave 1: launched (T0 2026-11-03T20:22:25Z)

TVPS A/primary/10-15 + OVPS A/primary/63-64, staggered 20s
with journal verification (8/8 attempted). ~18 waves follow
at the measured ~100-min pace.

### Extension wave 1: complete 2026-11-03T22:06Z (T0+104min)

TVPS A/10-15 + OVPS A/63-64: 8/8 valid, all census-clean
(720/720 joins, 0 mismatches). A/14 parent accuracy 0.452 vs
0.25 chance noted (learning sanity is reported, not gated).
Wave 2 launched staggered with journal verification: TVPS
A/16-21, OVPS A/65-66 (OVPS tarballed + rsynced its wave-1
pair to TVPS first at zero in flight, verified 2/2).

### C3-at-scale amendment (extension wave 1)

TVPS cannot hold the n=80 archive (140 new tarballs near
190 GiB against 26 GiB free with 160 GiB of other agents
data untouchable), so SVPS is now the durable archive, not
just the second copy: all 28 tarballs so far (round-1 20 +
wave-1 8) pulled to SVPS and shasum-verified 28/28, then the
TVPS copies deleted (TVPS 23G to 56G free; journals and
admissions stay). Durability per dyad: one verified SVPS copy
plus proven bit-for-bit regenerability from the frozen slot,
except round-1 which keeps real two-copy redundancy (SVPS
plus the OVPS and LVPS mirrors). TVPS and OVPS go transient:
each wave pushes its tarballs to SVPS, verifies, then deletes
locally. Steady state near 35 GiB on TVPS. C4 and D will pull
tarballs back from SVPS in cohort-sized transients.

### Extension wave 2 (TVPS): complete 2026-11-03T23:56Z

TVPS A/16-21: 6/6 valid, all census-clean (720/720 joins, 0
mismatches). Wave 3 (A/22-27) launched staggered 23:57:22Z
with journal verification. OVPS A/65-66 still in flight.

### Extension wave 2 (OVPS): complete 2026-11-04T00:06Z

OVPS A/65-66: 2/2 valid, census-clean (720/720 joins, 0
mismatches). Tarballed + rsynced to TVPS at zero in flight
(verified 2/2); wave 3 (A/67-68) launched staggered with
journal verification. TVPS wave-2 6-pack archived; SVPS
second copies for all 8 in progress.

### Extension wave 3 (TVPS): complete 2026-11-04T01:36Z

TVPS A/22-27: 6/6 valid, all census-clean (720/720 joins, 0
mismatches). Wave 4 (A/28-33) launched staggered 01:37:51Z
with journal verification. OVPS A/67-68 still in flight.

### Extension wave 3 (OVPS): complete 2026-11-04T02:16Z

OVPS A/67-68: 2/2 valid (67 at ~02:06Z, 68 at ~02:16Z),
census-clean (720/720 joins, 0 mismatches). Tarballed +
pushed to TVPS in the <=1 window (verified 2/2); SVPS
second copies verified 2/2; locals deleted on both hosts.
Backfilled solo with journal verification: A/69 at 02:16Z,
A/70 at 02:20Z. TVPS wave-3 6-pack likewise SVPS-verified
and cleaned (TVPS 61G free).

### SVPS host-key change: mirror legs PAUSED 2026-11-04T03:20Z

The scheduled SVPS pull for dyad 28 failed on every client
(AVPS, TVPS): the endpoint vps-shawn.aureuma.ai:7129 now
presents an ED25519 host key where the stored key is ECDSA,
a key-type change consistent with a rebuilt endpoint or a
moved port forward, not with a routine reboot. Strict host
checking stays on; no bypass attempted. All SVPS legs pause
until the author confirms the endpoint out of band.

Exposure: round-1 keeps real two-copy redundancy (OVPS and
LVPS mirrors, untouched). Extension waves 1-3 (22 tarballs,
near 30 GiB) are SVPS-single-copy plus proven deterministic
regenerability from the frozen journalled slots: expensive to
rebuild, not lost. TVPS holds the unmirrored dyad-28 tarball
plus wave-4 extracted with 61 GiB free, about 4 waves of
runway without SVPS offload. Compute on TVPS and OVPS is
unaffected and continues.

### Extension wave 4 (TVPS): complete 2026-11-04T03:27Z

TVPS A/28-33: 6/6 valid, all census-clean (720/720 joins, 0
mismatches). Backfilled staggered with journal verification:
A/34 at 03:17Z, A/35-39 at 03:37Z. Tarballs held on TVPS
(SVPS still paused); extracted deleted after tarball verify
to protect headroom (TVPS 46G free with wave-5 growing).
Note: at SVPS-dark accumulation (near 8 GiB kept per 6-pack),
TVPS fits the rest of cohort A but not cohort B: the SVPS
answer is needed before A-end (near 12h out), else older
tarballs must drop to regenerability-only or B waits.

### Extension wave 4 (OVPS): complete 2026-11-04T04:07Z

OVPS A/69-70: 2/2 valid, census-clean (720/720 joins, 0
mismatches). Tarballed + pushed to TVPS at zero in flight
(verified 2/2); A/71-72 launched staggered with journal
verification. Extracted deleted; local tarballs stay as the
only second copies while SVPS is paused (OVPS 15G free).
Same SVPS-dark arithmetic as TVPS: cohort A tail fits,
cohort B needs the SVPS answer.

### Extension wave 5 (TVPS): complete 2026-11-04T05:17Z

TVPS A/34-39: 6/6 valid, all census-clean (720/720 joins, 0
mismatches). Backfilled staggered with journal verification:
A/40 at 04:58Z, A/41-45 at 05:18Z. Tarballs held on TVPS
(SVPS still paused); extracted deleted after tarball verify.

### Extension wave 5 (OVPS): complete 2026-11-04T06:07Z

OVPS A/71-72: 2/2 valid, census-clean (720/720 joins, 0
mismatches). Tarballed + pushed to TVPS at zero in flight
(verified 2/2); A/73-74 launched staggered with journal
verification. Extracted deleted; local tarballs stay as the
only second copies (OVPS 13G free).

### Extension wave 6 (TVPS): complete 2026-11-04T06:57Z

TVPS A/40-45: 6/6 valid (40 at ~06:37Z, 41-45 at ~06:57Z),
all census-clean (720/720 joins, 0 mismatches). Backfilled
staggered with journal verification: A/46 at 06:38Z, A/47-51
at 06:58Z. Tarballs held on TVPS (SVPS still paused).

### Extension wave 6 (OVPS): complete 2026-11-04T08:17Z

OVPS A/73-74: 2/2 valid, census-clean (720/720 joins, 0
mismatches). Tarballed + pushed to TVPS at zero in flight
(verified 2/2); A/75-76 launched staggered with journal
verification. TVPS A/46 likewise valid, clean, archived,
backfilled by A/52.

SVPS-dark degradation, second step: OVPS local tarballs
(69-74) dropped after their TVPS copies verified (OVPS 9.8G
to 18G free; ENOSPC was one wave out). Durability now:
waves 1-3 keep TVPS + SVPS copies; wave 4+ is TVPS-single
plus proven regenerability until SVPS returns. Journals and
admissions (the irreplaceable provenance) stay everywhere.

### Extension wave 7 (TVPS): complete 2026-11-04T08:37Z

TVPS A/46-51: 6/6 valid (46 at ~08:17Z, 47-51 at ~08:37Z),
all census-clean (720/720 joins, 0 mismatches). Backfilled
staggered with journal verification: A/52 at 08:23Z, A/53-57
at 08:48Z. Tarballs held on TVPS (SVPS still paused).

### Extension wave 7 (OVPS): complete 2026-11-04T10:18Z

OVPS A/75-76: 2/2 valid, census-clean (720/720 joins, 0
mismatches). Tarballed + pushed to TVPS at zero in flight
(verified 2/2); A/77-78 launched staggered with journal
verification. Extracted plus local tarballs deleted after
TVPS verify (single-copy plus regenerability; OVPS 18G
free). TVPS A/52 likewise valid, clean, archived, backfilled
by A/58.

### Extension wave 8 (TVPS): complete 2026-11-04T10:28Z

TVPS A/53-57: 5/5 valid (52 backfilled the first slot at
09:59Z), all census-clean (720/720 joins, 0 mismatches).
Backfilled staggered with journal verification: A/59-62
(the last 4 TVPS-A slots) plus B/10 (first TVPS-B extension
slot) at 10:29Z. Tarballs held on TVPS.
