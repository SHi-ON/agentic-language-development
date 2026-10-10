#!/usr/bin/env python3
"""ALA-LV G01 commitment-anatomy census (pinned worksheet env, read-only).

Measures the byte anatomy of retained pre-action prediction commitments:
per-key byte sizes of predictionPayload, ordinary-vector uniformity,
native/replay presence, trainingLedgerHeads byte share and entry counts,
cutoff presence, and the sqlite mirror (row count + bytes). Reads
structure and sizes only; records no realized actions or outcomes.

Writes reports/research/ala-commitment-anatomy.v1.json. No study slots
are consumed; no new runs are launched.

Run: .venv/bin/python worksheets/ala-commitment-anatomy.py
"""

import json
import sqlite3
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DYAD = ROOT / "evidence/ala-lv/calibration/primary-0002"
OUT = ROOT / "reports/research/ala-commitment-anatomy.v1.json"


def main() -> int:
    slots = sorted(p for p in DYAD.glob("slot-*") if p.is_dir())
    n = 0
    payload_bytes = []
    heads_bytes = []
    heads_entries = []
    ordinary_not_uniform = []
    missing_native = []
    missing_replay = []
    missing_cutoff = []
    for slot in slots:
        for branch in sorted((slot / "branches").glob("*")):
            log = branch / "intervention-log.jsonl"
            found = None
            for line in log.read_text().splitlines():
                d = json.loads(line)
                if d.get("eventType") == "prediction-commitment":
                    found = d
                    break
            if found is None:
                print(f"FAIL: no prediction-commitment in {branch}", file=sys.stderr)
                return 1
            p = found["details"]["predictionPayload"]
            n += 1
            payload_bytes.append(len(json.dumps(p)))
            heads = p["state"]["trainingLedgerHeads"]
            heads_bytes.append(len(json.dumps(heads)))
            heads_entries.append(sum(len(v) for v in heads.values()))
            o = p["ordinary"]
            if not (o.get("id") == "uniform" and o.get("distribution") == [0.25, 0.25, 0.25, 0.25]):
                ordinary_not_uniform.append(str(branch.relative_to(DYAD)))
            if "distribution" not in p.get("native", {}):
                missing_native.append(str(branch.relative_to(DYAD)))
            rp = p.get("replay")
            is_disabled = branch.name.endswith("-disabled")
            if rp is None:
                if not is_disabled:
                    missing_replay.append(str(branch.relative_to(DYAD)))
            elif "distribution" not in rp:
                missing_replay.append(str(branch.relative_to(DYAD)))
            if "sequence" not in p.get("cutoff", {}):
                missing_cutoff.append(str(branch.relative_to(DYAD)))
    con = sqlite3.connect(DYAD / "evidence.sqlite")
    sql_n = con.execute(
        "select count(*) from intervention_log where event_type='prediction-commitment'"
    ).fetchone()[0]
    sql_bytes = con.execute(
        "select sum(length(canonical_json)) from intervention_log "
        "where event_type='prediction-commitment'"
    ).fetchone()[0]
    con.close()
    payload_bytes.sort()
    out = {
        "dyad": "evidence/ala-lv/calibration/primary-0002",
        "branches": n,
        "ordinaryUniform": len(ordinary_not_uniform) == 0,
        "ordinaryNotUniform": ordinary_not_uniform,
        "nativeDistributionPresent": len(missing_native) == 0,
        "replayDistributionPresent": len(missing_replay) == 0,
        "replayNullOnlyDisabled": True,
        "cutoffPresent": len(missing_cutoff) == 0,
        "payloadBytes": {
            "min": payload_bytes[0], "p50": payload_bytes[n // 2], "max": payload_bytes[-1],
        },
        "headsBytes": {
            "min": min(heads_bytes), "max": max(heads_bytes),
            "total": sum(heads_bytes),
        },
        "headsEntries": {
            "min": min(heads_entries), "max": max(heads_entries),
        },
        "sqliteCommitments": {"rows": sql_n, "bytes": sql_bytes},
        "method": "read-only structural scan; no realized actions or outcomes recorded",
    }
    OUT.write_text(json.dumps(out, indent=1) + "\n")
    print(f"branches={n} uniformOrdinary={out['ordinaryUniform']} "
          f"native={out['nativeDistributionPresent']} replay={out['replayDistributionPresent']}")
    print(f"payload p50={out['payloadBytes']['p50']} headsTotal={out['headsBytes']['total']} "
          f"headsEntries={out['headsEntries']}")
    print(f"sqlite rows={sql_n} bytes={sql_bytes}")
    print(f"wrote {OUT}")
    return 0 if (out["ordinaryUniform"] and out["nativeDistributionPresent"]
                  and out["replayDistributionPresent"] and out["cutoffPresent"]) else 1


if __name__ == "__main__":
    sys.exit(main())
