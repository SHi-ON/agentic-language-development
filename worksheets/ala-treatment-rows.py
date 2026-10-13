#!/usr/bin/env python3
"""Round-4 R5 + round-5 F2: portable treatment-row extract.

Reads the 240 retained per-slot treatment-batch.json files, each slot's
schedule.json (real scheduledCaseId + schedule commitment), and the
message extract (branch tokens + stream seeds); writes one outcome-free
row per slot. Identities are distinct fields: slot stem (slot identity,
derived from branch record ids), real scheduledCaseId (from
schedule.json), branch record ids, donor ids. Plus receiver role,
target + ORDERED candidates, selections, deliveries, and stream/context
identities. No scores or behavioral outcomes: the projection audits
treatment CONSTRUCTION and schedule coverage inputs only.

Fail-closed: 240 slots, 2 ledger cases per slot with the expected
branch suffixes, complete schedule + extract joins, else exit 1 with
no report.

Writes reports/research/ala-treatment-rows.v2.json (v1 preserved).
"""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DYAD = ROOT / "evidence/ala-lv/calibration/primary-0002"
EXTRACT = ROOT / "reports/research/ala-message-extract.v1.json"
OUT = ROOT / "reports/research/ala-treatment-rows.v2.json"


def branch_of(case_id: str) -> str:
    for suffix in ("-ledger-consistent:turn:0", "-ledger-shuffled:turn:0"):
        if case_id.endswith(suffix):
            return suffix[1:-len(":turn:0")]
    raise ValueError(f"unexpected ledger case id: {case_id}")


def slot_stem(case_id: str) -> str:
    """Slot identity derived from a branch record id (round-5 F2).

    Both pair records embed the same stem (e.g. ...-s0001-...) with
    different branch suffixes. This is the SLOT's identity, not the
    original scheduled-case ID; the real scheduledCaseId comes from the
    slot's schedule.json and is carried as its own field.
    """
    for suffix in ("-ledger-consistent:turn:0", "-ledger-shuffled:turn:0"):
        if case_id.endswith(suffix):
            return case_id[:-len(suffix)]
    raise ValueError(f"unexpected ledger case id: {case_id}")


def main() -> int:
    if not DYAD.is_dir():
        print(f"FAIL: dyad evidence missing: {DYAD}", file=sys.stderr)
        return 1
    extract = {row["slot"]: row for row in
               json.loads(EXTRACT.read_text())["testRows"]}
    slots = sorted(p for p in DYAD.glob("slot-*") if p.is_dir())
    if len(slots) != 240:
        print(f"FAIL: {len(slots)} slots, want 240", file=sys.stderr)
        return 1
    rows = []
    identities = {}
    commitments = set()
    eligible = [1, 2, 3, 4, 6, 7, 8, 9, 11, 12, 13, 14]
    universe = {f"within-support-test:{role}:{i:04d}"
                for role in ("baby-a", "baby-b") for i in range(120)}
    for slot in slots:
        batch = json.loads((slot / "treatment-batch.json").read_text())
        schedule_path = slot / "schedule.json"
        if not schedule_path.is_file():
            print(f"FAIL: {slot.name} lacks schedule.json", file=sys.stderr)
            return 1
        schedule = json.loads(schedule_path.read_text())
        scheduled_id = schedule.get("executedCaseId")
        commitment = schedule.get("commitment")
        if not isinstance(scheduled_id, str) or not scheduled_id \
                or not isinstance(commitment, str) or not commitment:
            print(f"FAIL: {slot.name} schedule.json lacks executedCaseId/commitment",
                  file=sys.stderr)
            return 1
        cases = batch["cases"]
        if len(cases) != 2:
            print(f"FAIL: {slot.name} holds {len(cases)} cases, want 2",
                  file=sys.stderr)
            return 1
        if scheduled_id not in universe:
            print(f"FAIL: {slot.name} schedule ID outside expected universe", file=sys.stderr)
            return 1
        _, role, index = scheduled_id.split(":")
        content = (role, eligible[int(index) % 12], tuple(cases[0]["candidateTypeCodes"]))
        if any((c["receiverRole"], c["targetTypeCode"], tuple(c["candidateTypeCodes"])) != content
               for c in cases) or (scheduled_id in identities and identities[scheduled_id] != content):
            print(f"FAIL: {slot.name} schedule identity/content mismatch", file=sys.stderr)
            return 1
        identities[scheduled_id] = content
        commitments.add(commitment)
        branches = sorted(branch_of(c["caseId"]) for c in cases)
        if branches != ["ledger-consistent", "ledger-shuffled"]:
            print(f"FAIL: {slot.name} branches {branches}", file=sys.stderr)
            return 1
        if slot.name not in extract:
            print(f"FAIL: {slot.name} missing from message extract",
                  file=sys.stderr)
            return 1
        row = extract[slot.name]
        tokens = row.get("branchTokens", {})
        seeds = row.get("seeds", [])
        if "random" not in tokens or "normal" not in tokens \
                or "shuffled" not in tokens or not seeds:
            print(f"FAIL: {slot.name} extract row lacks branch tokens/seed",
                  file=sys.stderr)
            return 1
        rows.append({
            "slot": slot.name,
            "receiverRole": cases[0]["receiverRole"],
            "scheduledCaseId": scheduled_id,
            "scheduleCommitment": commitment,
            "batchCommitment": batch["batchCommitment"],
            "derangementSeed": batch["derangementSeed"],
            "cases": [{
                "caseId": c["caseId"],
                "branch": branch_of(c["caseId"]),
                "slotStem": slot_stem(c["caseId"]),
                "scheduledCaseId": scheduled_id,
                "receiverRole": c["receiverRole"],
                "target": c["targetTypeCode"],
                "candidates": list(c["candidateTypeCodes"]),
                "selection": c["selectedToken"],
                "shuffledSource": c["shuffledSourceCaseId"],
                "shuffledDelivered": c["shuffledDeliveredToken"],
                "coincidence": c["identicalTokenCoincidence"],
            } for c in cases],
            "randomToken": tokens["random"],
            "normalToken": tokens["normal"],
            "shuffledToken": tokens["shuffled"],
            "seed": seeds[0],
        })
    if len(commitments) != 1:
        print("FAIL: retained slots disagree on schedule commitment", file=sys.stderr)
        return 1
    OUT.write_text(json.dumps({
        "extractVersion": "v2",
        "studyId": "ALA-LV",
        "dyad": "calibration/primary-0002",
        "method": ("one row per retained slot: treatment-batch.json (branch "
                   "record ids, slot stems derived from record ids, roles, "
                   "targets, ORDERED candidates, selections, donors, "
                   "deliveries, batch commitment, derangement seed) joined to "
                   "the slot's schedule.json (real scheduledCaseId + "
                   "schedule commitment) and ala-message-extract.v1.json "
                   "testRows (random-branch token, stream seed); "
                   "outcome-free"),
        "rows": rows,
    }, indent=1))
    sham = sum(1 for r in rows
               if (r["cases"][0]["target"], r["cases"][0]["candidates"])
               == (r["cases"][1]["target"], r["cases"][1]["candidates"]))
    print(f"rows=240 identical-input pairs={sham}")
    print(f"wrote {OUT}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
