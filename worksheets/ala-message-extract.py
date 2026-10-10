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
            found = None
            for line in (bdir / f"{baby}-ledger.jsonl").read_text().splitlines():
                e = json.loads(line)
                if e.get("eventType") == "intention.recorded" and e.get("turn") == 0:
                    found = ((e.get("content") or {}).get("symbols") or [None])[0]
            branches[branch] = found
        a, b = batch["cases"]
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
    symbols = {}
    for baby in ("baby-a", "baby-b"):
        for line in (parent / f"{baby}-ledger.jsonl").read_text().splitlines():
            e = json.loads(line)
            if e.get("eventType") == "intention.recorded" and isinstance(e.get("turn"), int):
                symbols[e["turn"]] = ((e.get("content") or {}).get("symbols") or [None])[0]
    train_rows = [{"turn": n, "sender": sender.get(n), "token": symbols.get(n),
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
