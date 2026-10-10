#!/usr/bin/env python3
"""ALA-LV D-A turn-0 intention + proposal-hash census (read-only).

Per retained branch: counts `intention.recorded` turn-0 records in the
receiver baby ledger and compares the token/null projection to the N1
message-extract row (decides whether the extract's last-match
projection is unique). Per slot: compares turn-0 `babyProposalHash`
across the 7 sibling branches (closes the N1 `babyProposalHash` half
that had no reproducing artifact).

Writes reports/research/ala-intention-census.v1.json (kilobytes). No
study slots are consumed; no new runs are launched; no database writes.

Run: .venv/bin/python worksheets/ala-intention-census.py
"""

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DYAD = ROOT / "evidence/ala-lv/calibration/primary-0002"
EXTRACT = ROOT / "reports/research/ala-message-extract.v1.json"
OUT = ROOT / "reports/research/ala-intention-census.v1.json"


def branch_key(slot_name: str, bdir_name: str) -> str:
    return bdir_name.split(f"-s{slot_name.split('-')[1]}-", 1)[-1]


def main() -> int:
    if not DYAD.is_dir():
        print(f"FAIL: dyad evidence missing: {DYAD}", file=sys.stderr)
        return 1
    extract = {row["slot"]: row for row in
               json.loads(EXTRACT.read_text())["testRows"]}
    multiplicity: dict[int, int] = {}
    null_agree = null_total = 0
    branches_checked = 0
    equal_slots = 0
    mismatches = []
    slots = sorted(p for p in DYAD.glob("slot-*") if p.is_dir())
    for slot in slots:
        batch = json.loads((slot / "treatment-batch.json").read_text())
        baby = batch["cases"][0]["receiverRole"]
        row = extract.get(slot.name, {})
        expected_tokens = row.get("branchTokens", {})
        slot_hashes = set()
        for bdir in sorted((slot / "branches").glob("*")):
            if not bdir.is_dir():
                continue
            key = branch_key(slot.name, bdir.name)
            found = None
            count = 0
            for line in (bdir / f"{baby}-ledger.jsonl").read_text().splitlines():
                e = json.loads(line)
                if e.get("eventType") == "intention.recorded" and e.get("turn") == 0:
                    count += 1
                    found = ((e.get("content") or {}).get("symbols") or [None])[0]
            multiplicity[count] = multiplicity.get(count, 0) + 1
            branches_checked += 1
            if key in expected_tokens:
                null_total += 1
                if (found is None) == (expected_tokens[key] is None) and \
                        found == expected_tokens[key]:
                    null_agree += 1
                else:
                    mismatches.append(f"{slot.name}/{key}: token {found!r} "
                                      f"vs extract {expected_tokens[key]!r}")
            hashes = set()
            for line in (bdir / "turn-records.jsonl").read_text().splitlines():
                e = json.loads(line)
                if e.get("turn") == 0 and e.get("babyProposalHash"):
                    hashes.add(e["babyProposalHash"])
            if len(hashes) == 1:
                slot_hashes.add(next(iter(hashes)))
            else:
                mismatches.append(f"{slot.name}/{key}: {len(hashes)} distinct "
                                  f"turn-0 proposal hashes")
        if len(slot_hashes) == 1:
            equal_slots += 1
    out = {
        "dyad": "evidence/ala-lv/calibration/primary-0002",
        "branchesChecked": branches_checked,
        "multiplicityHistogram": {str(k): v for k, v in sorted(multiplicity.items())},
        "nullAgreement": {"agree": null_agree, "total": null_total},
        "proposalHashEqualSlots": equal_slots,
        "slotCount": len(slots),
        "mismatches": mismatches,
        "method": ("read-only scan of receiver baby-ledger turn-0 intentions "
                   "vs extract rows, plus turn-records turn-0 babyProposalHash "
                   "across 7 siblings per slot"),
    }
    OUT.write_text(json.dumps(out, indent=1) + "\n")
    print(f"branches={branches_checked} multiplicity={out['multiplicityHistogram']} "
          f"nullAgree={null_agree}/{null_total} equalSlots={equal_slots}/{len(slots)}")
    print(f"wrote {OUT}")
    if mismatches:
        print("FAIL: intention census mismatches present", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
