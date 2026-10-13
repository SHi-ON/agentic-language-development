#!/usr/bin/env python3
"""Round-4 R5: portable treatment-row extract over the retained dyad.

Reads the 240 retained per-slot treatment-batch.json files plus the
message extract (branch tokens + stream seeds) and writes one
outcome-free row per slot: scheduled/branch identities, donor
identities, receiver role, target + ORDERED candidates, selections,
deliveries, and stream/context identities. No scores or behavioral
outcomes: the projection audits treatment CONSTRUCTION only.

Fail-closed: 240 slots, 2 ledger cases per slot with the expected
branch suffixes, complete extract join, else exit 1 with no report.
"""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DYAD = ROOT / "evidence/ala-lv/calibration/primary-0002"
EXTRACT = ROOT / "reports/research/ala-message-extract.v1.json"
OUT = ROOT / "reports/research/ala-treatment-rows.v1.json"


def branch_of(case_id: str) -> str:
    for suffix in ("-ledger-consistent:turn:0", "-ledger-shuffled:turn:0"):
        if case_id.endswith(suffix):
            return suffix[1:-len(":turn:0")]
    raise ValueError(f"unexpected ledger case id: {case_id}")


def scheduled_stem(case_id: str) -> str:
    """Shared stem identifying the scheduled example a branch record came from.

    The retained batch carries no separate scheduled-case field; both pair
    records embed the same stem (e.g. ...-s0001-...) with different branch
    suffixes. Derived deterministically from the retained caseId, so the
    one-example/two-records shape is machine-checkable downstream.
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
    for slot in slots:
        batch = json.loads((slot / "treatment-batch.json").read_text())
        cases = batch["cases"]
        if len(cases) != 2:
            print(f"FAIL: {slot.name} holds {len(cases)} cases, want 2",
                  file=sys.stderr)
            return 1
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
            "batchCommitment": batch["batchCommitment"],
            "derangementSeed": batch["derangementSeed"],
            "cases": [{
                "caseId": c["caseId"],
                "branch": branch_of(c["caseId"]),
                "scheduledStem": scheduled_stem(c["caseId"]),
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
    OUT.write_text(json.dumps({
        "extractVersion": "v1",
        "studyId": "ALA-LV",
        "dyad": "calibration/primary-0002",
        "method": ("one row per retained slot treatment-batch.json "
                   "(case ids, scheduled stems derived from case ids, roles, "
                   "targets, ORDERED candidates, selections, donors, "
                   "deliveries, batch commitment, derangement seed) joined to "
                   "ala-message-extract.v1.json testRows for the random-branch "
                   "token and stream seed; outcome-free"),
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
