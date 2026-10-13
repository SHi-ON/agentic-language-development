#!/usr/bin/env python3
"""Round-5 F2: executed-schedule coverage census over the retained dyad.

Compares the executed multiset of scheduled case IDs (from each slot's
retained schedule.json) with the expected within-support schedule for
this dyad. Reports duplicates, missing IDs, and the role-by-target
table. The retained execution's final coverage call checked membership
only; this census checks expected multiplicity and coverage.

Expected universe (documented derivation): the frozen within-support
partition holds 240 cases per dyad, 120 per receiver role, addressed by
the scheduleCaseId scheme in lv01-schedule.ts as
within-support-test:{baby-a,baby-b}:{0000..0119}; the expected target
of case index i is LV01_ELIGIBLE_TYPE_CODES[i % 12] (the round-robin in
buildLv01Schedule), ten per eligible target per role. Only the intended
test partition is compared, since validation never executed.

Reads (read-only; fails closed on any structural gap):
  evidence/ala-lv/calibration/primary-0002/slot-*/schedule.json
  evidence/ala-lv/calibration/primary-0002/slot-*/treatment-batch.json

Writes reports/research/ala-schedule-coverage.v1.json. Usage:
  python3 worksheets/ala-schedule-coverage.py
"""
import json
import sys
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DYAD = ROOT / "evidence/ala-lv/calibration/primary-0002"
OUT = ROOT / "reports/research/ala-schedule-coverage.v1.json"

ELIGIBLE = [1, 2, 3, 4, 6, 7, 8, 9, 11, 12, 13, 14]
ROLES = ("baby-a", "baby-b")
PER_ROLE = 120


def expected_ids() -> list[str]:
    return [f"within-support-test:{role}:{i:04d}"
            for role in ROLES for i in range(PER_ROLE)]


def expected_target(case_id: str) -> int:
    index = int(case_id.rsplit(":", 1)[1])
    return ELIGIBLE[index % len(ELIGIBLE)]


def main() -> int:
    if not DYAD.is_dir():
        print(f"FAIL: dyad evidence missing: {DYAD}", file=sys.stderr)
        return 1
    slots = sorted(p for p in DYAD.glob("slot-*") if p.is_dir())
    if len(slots) != 240:
        print(f"FAIL: {len(slots)} slots, want 240", file=sys.stderr)
        return 1
    executed = []  # (slot, executedCaseId, role, target)
    commitments = set()
    id_content: dict[str, tuple] = {}
    expected = expected_ids()
    universe = set(expected)
    for slot in slots:
        schedule_path = slot / "schedule.json"
        batch_path = slot / "treatment-batch.json"
        if not schedule_path.is_file() or not batch_path.is_file():
            print(f"FAIL: {slot.name} lacks schedule/treatment files",
                  file=sys.stderr)
            return 1
        schedule = json.loads(schedule_path.read_text())
        case_id = schedule.get("executedCaseId")
        commitment = schedule.get("commitment")
        if not isinstance(case_id, str) or not case_id \
                or not isinstance(commitment, str) or not commitment:
            print(f"FAIL: {slot.name} schedule.json malformed", file=sys.stderr)
            return 1
        if case_id not in universe:
            print(f"FAIL: {slot.name} executedCaseId outside expected universe: {case_id}", file=sys.stderr)
            return 1
        parts = case_id.split(":")
        if len(parts) != 3 or parts[0] != "within-support-test" \
                or parts[1] not in ROLES:
            print(f"FAIL: {slot.name} unexpected executedCaseId {case_id}",
                  file=sys.stderr)
            return 1
        batch = json.loads(batch_path.read_text())
        cases = batch["cases"]
        if len(cases) != 2 or any(c["receiverRole"] != parts[1]
                or c["targetTypeCode"] != expected_target(case_id) for c in cases):
            print(f"FAIL: {slot.name} batch disagrees with schedule identity",
                  file=sys.stderr)
            return 1
        target = cases[0]["targetTypeCode"]
        content = (parts[1], target, tuple(cases[0]["candidateTypeCodes"]))
        if (cases[0]["candidateTypeCodes"] != cases[1]["candidateTypeCodes"]
                or (case_id in id_content and id_content[case_id] != content)):
            print(f"FAIL: {case_id} has inconsistent conditioning content", file=sys.stderr)
            return 1
        id_content[case_id] = content
        commitments.add(commitment)
        executed.append((slot.name, case_id, parts[1], target))
    ids = [e[1] for e in executed]
    counts = Counter(ids)
    if len(commitments) != 1:
        print("FAIL: retained slots disagree on schedule commitment", file=sys.stderr)
        return 1
    missing = sorted(set(expected) - set(ids))
    duplicated = sorted(i for i, n in counts.items() if n > 1)
    histogram: dict[str, int] = {}
    for n in counts.values():
        histogram[str(n)] = histogram.get(str(n), 0) + 1
    table = {}
    for role in ROLES:
        role_rows = [e for e in executed if e[2] == role]
        per_target = {}
        for t in ELIGIBLE:
            t_rows = [e for e in role_rows if e[3] == t]
            per_target[str(t)] = {
                "expectedDistinctIds": PER_ROLE // len(ELIGIBLE),
                "executedDistinctIds": len({e[1] for e in t_rows}),
                "executedSlots": len(t_rows),
            }
        table[role] = {
            "executedSlots": len(role_rows),
            "distinctExecutedIds": len({e[1] for e in role_rows}),
            "targets": per_target,
        }
    report = {
        "censusVersion": "v1",
        "studyId": "ALA-LV",
        "dyad": "calibration/primary-0002",
        "method": ("executed multiset from slot schedule.json files joined "
                   "to treatment-batch.json targets, compared with the "
                   "expected within-support universe (240 IDs, 120 per "
                   "role, round-robin targets); duplicates, missing IDs, "
                   "and the role-by-target table reported"),
        "expectedUniverse": {
            "total": len(expected),
            "perRole": PER_ROLE,
            "idScheme": "within-support-test:{baby-a,baby-b}:{0000..0119}",
            "targetRule": "LV01_ELIGIBLE_TYPE_CODES[caseIndex % 12]",
        },
        "scheduleCommitments": sorted(commitments),
        "executedSlots": len(executed),
        "distinctExecutedIds": len(counts),
        "multiplicityHistogram": histogram,
        "duplicatedIds": duplicated,
        "missingIds": missing,
        "roleTargetTable": table,
    }
    OUT.write_text(json.dumps(report, indent=1))
    print(f"slots={len(executed)} distinct={len(counts)} "
          f"duplicated={len(duplicated)} missing={len(missing)}")
    print(f"wrote {OUT}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
