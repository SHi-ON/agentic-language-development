#!/usr/bin/env python3
"""ALA-LV N1.5 message extract (pinned worksheet env, read-only).

Preserves a compact case/role/token/seed-identity extract sufficient to
reproduce the reported treatment-equality and census-count results:
ledger batch equalities, branch turn-0 intentions, per-case seeds, and
primary-0002 training token rows. Reads retained records only; records
no realized actions or outcomes.

Selection/projection: primary-0002 only (the dyad behind every reported
count; primary-0000 training is unreported and excluded). Test rows
project treatment-batch.json cases plus branch-ledger turn-0 intentions
plus run-config randomSeed. Training rows project parent-ledger
intention.recorded symbols joined to running-turn scenario hashes and
senders (same join as the duplication census).

Writes reports/research/ala-message-extract.v1.json. No study slots
are consumed; no new runs are launched.

Run: .venv/bin/python worksheets/ala-message-extract.py
"""

import hashlib
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DYAD = ROOT / "evidence/ala-lv/calibration/primary-0002"
OUT = ROOT / "reports/research/ala-message-extract.v1.json"


def turn0_symbol(lines: list[str]):
    """The branch's sole turn-0 intention token (None if none recorded).

    Fails closed on multiplicity > 1: a last-match projection would
    silently keep one of several distinct intentions. The retained
    corpus records exactly one per branch (intention census).
    """
    found = []
    for line in lines:
        e = json.loads(line)
        if e.get("eventType") == "intention.recorded" and e.get("turn") == 0:
            found.append(((e.get("content") or {}).get("symbols") or [None])[0])
    if len(found) > 1:
        raise SystemExit(f"FAIL: {len(found)} turn-0 intentions in one branch ledger")
    return found[0] if found else None


def label_cases(cases: list[dict]) -> tuple[dict, dict]:
    """(consistent, shuffled) identified by caseId branch, never by order."""
    consistent = [c for c in cases if "-ledger-consistent:" in c["caseId"]]
    shuffled = [c for c in cases if "-ledger-shuffled:" in c["caseId"]]
    if len(consistent) != 1 or len(shuffled) != 1 or len(cases) != 2:
        raise SystemExit(
            f"FAIL: expected one ledger-consistent and one ledger-shuffled "
            f"case, got {[c.get('caseId') for c in cases]}")
    return consistent[0], shuffled[0]


def training_symbols(parent: Path) -> dict:
    """(baby, turn) -> first intention symbol; duplicate key fails closed."""
    symbols = {}
    for baby in ("baby-a", "baby-b"):
        for line in (parent / f"{baby}-ledger.jsonl").read_text().splitlines():
            e = json.loads(line)
            if e.get("eventType") == "intention.recorded" and isinstance(e.get("turn"), int):
                key = (baby, e["turn"])
                if key in symbols:
                    raise SystemExit(f"FAIL: duplicate training intention for {key}")
                symbols[key] = ((e.get("content") or {}).get("symbols") or [None])[0]
    return symbols


def main() -> int:
    test_rows = []
    for slot in sorted(p for p in DYAD.glob("slot-*") if p.is_dir()):
        batch = json.loads((slot / "treatment-batch.json").read_text())
        branches = {}
        seeds = set()
        for bdir in sorted((slot / "branches").glob("*")):
            branch = bdir.name.split(f"-s{slot.name.split('-')[1]}-", 1)[-1]
            cfg = json.loads((bdir / "configuration" / "run-config.json").read_text())
            seeds.add(cfg["randomSeed"])
            baby = batch["cases"][0]["receiverRole"]
            branches[branch] = turn0_symbol(
                (bdir / f"{baby}-ledger.jsonl").read_text().splitlines())
        a, b = label_cases(batch["cases"])
        test_rows.append({
            "slot": slot.name,
            "role": a["receiverRole"],
            "target": a["targetTypeCode"],
            "candidates": a["candidateTypeCodes"],
            "sameTarget": a["targetTypeCode"] == b["targetTypeCode"],
            "sameCandidates": a["candidateTypeCodes"] == b["candidateTypeCodes"],
            "selConsistent": a["selectedToken"],
            "selShuffled": b["selectedToken"],
            "delivConsistent": a["shuffledDeliveredToken"],
            "delivShuffled": b["shuffledDeliveredToken"],
            "branchTokens": branches,
            "seeds": sorted(seeds),
        })
    parent = DYAD / "parent-bundle"
    turns = [json.loads(line) for line in (parent / "turn-records.jsonl").read_text().splitlines()]
    scen, sender = {}, {}
    for t in turns:
        if t.get("phase") == "running":
            scen[t["turn"]] = t.get("scenarioStateHash")
            sender[t["turn"]] = (t.get("roles") or {}).get("sender")
    symbols = training_symbols(parent)
    train_rows = [{"turn": n, "sender": sender.get(n),
                   "token": symbols.get((sender.get(n), n)),
                   "scenario": scen.get(n)} for n in sorted(scen)]
    out = {
        "extractVersion": "v1",
        "studyId": "ALA-LV",
        "dyad": "evidence/ala-lv/calibration/primary-0002",
        "method": ("N1.5 compact extract. Test rows project treatment-batch.json "
                   "(role/target/candidates/selections/deliveries) plus branch-ledger "
                   "turn-0 intentions plus run-config randomSeed per slot. Training rows "
                   "project parent-ledger intention symbols joined to running-turn "
                   "scenario hashes and senders. No realized actions or outcomes."),
        "testRows": test_rows,
        "trainingRows": train_rows,
    }
    raw = json.dumps(out, indent=1) + "\n"
    OUT.write_text(raw)
    digest = hashlib.sha256(raw.encode()).hexdigest()
    print(f"test={len(test_rows)} training={len(train_rows)} "
          f"bytes={len(raw)} sha256={digest[:16]}...")
    print(f"wrote {OUT}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
