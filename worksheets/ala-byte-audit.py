#!/usr/bin/env python3
"""ALA-LV N3 byte audit (pinned worksheet env, read-only).

Partitions the retained primary-0002 dyad bytes without double-counting:
SQLite tables/blobs vs pages, parent checkpoints/ledgers, per-case
commitments, run exports, indexes and other metadata. Distinguishes
apparent file size, logical payload bytes, physical disk allocation,
unique byte strings (digest dedup of training-ledger heads) and
compressibility (in-memory gzip samples).

Also selects the two representative commitments + parent head for the
N3 representation-fixture corpus (selection rule recorded; extraction is
a separate sealing step).

Writes reports/research/ala-byte-audit.v1.json (kilobytes). No study
slots are consumed; no new runs are launched; no database writes.

Run: .venv/bin/python worksheets/ala-byte-audit.py
"""

import gzip
import hashlib
import json
import os
import sqlite3
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DYAD = ROOT / "evidence/ala-lv/calibration/primary-0002"
OUT = ROOT / "reports/research/ala-byte-audit.v1.json"


def tree_bytes(path: Path) -> dict:
    """Apparent + physical bytes for a tree or file (single stat walk, no copies)."""
    apparent = physical = files = 0
    if path.is_file():
        st = path.stat()
        return {"apparentBytes": st.st_size, "physicalBytes": st.st_blocks * 512, "files": 1}
    for dirpath, _dirnames, filenames in os.walk(path):
        for name in filenames:
            try:
                st = os.stat(os.path.join(dirpath, name))
            except FileNotFoundError:
                continue
            apparent += st.st_size
            physical += st.st_blocks * 512
            files += 1
    return {"apparentBytes": apparent, "physicalBytes": physical, "files": files}


def sqlite_anatomy(db: Path) -> dict:
    con = sqlite3.connect(f"file:{db}?mode=ro", uri=True)
    page_size = con.execute("pragma page_size").fetchone()[0]
    page_count = con.execute("pragma page_count").fetchone()[0]
    freelist = con.execute("pragma freelist_count").fetchone()[0]
    def ident(name: str) -> str:
        return '"' + name.replace('"', '""') + '"'

    tables = {}
    for (t,) in con.execute(
        "select name from sqlite_master where type='table' order by name"
    ):
        cols = [(r[1], r[2]) for r in con.execute(f"pragma table_info({ident(t)})")]
        text_cols = [c for c, typ in cols if typ.upper() in ("TEXT", "BLOB", "")]
        n = con.execute(f"select count(*) from {ident(t)}").fetchone()[0]
        payload = 0
        if n and text_cols:
            sums = con.execute(
                f"select {', '.join(f'sum(length({ident(c)}))' for c in text_cols)} "
                f"from {ident(t)}"
            ).fetchone()
            payload = sum(s or 0 for s in sums)
        tables[t] = {"rows": n, "textPayloadBytes": payload}
    indexes = [
        r[0]
        for r in con.execute(
            "select name from sqlite_master where type='index' and sql is not null"
        )
    ]
    con.close()
    return {
        "pageSize": page_size,
        "pageCount": page_count,
        "freelistPages": freelist,
        "fileBytes": page_size * page_count,
        "tables": tables,
        "indexCount": len(indexes),
    }


def find_commitment(branch: Path) -> dict:
    for line in (branch / "intervention-log.jsonl").read_text().splitlines():
        d = json.loads(line)
        if d.get("eventType") == "prediction-commitment":
            return d["details"]["predictionPayload"]
    raise ValueError(f"no prediction-commitment in {branch}")


def main() -> int:
    if not DYAD.is_dir():
        print(f"FAIL: dyad evidence missing: {DYAD}", file=sys.stderr)
        return 1
    partition = {}
    for name in ("parent-bundle", "runs", "scenario-registry"):
        partition[name] = tree_bytes(DYAD / name)
    slots = sorted(p for p in DYAD.glob("slot-*") if p.is_dir())
    slot_branches = slot_other = 0
    for slot in slots:
        slot_branches += tree_bytes(slot / "branches")["apparentBytes"]
        for extra in ("treatment-batch.json", "schedule.json"):
            p = slot / extra
            if p.exists():
                slot_other += p.stat().st_size
    partition["slotsBranches"] = {"slotCount": len(slots), "apparentBytes": slot_branches}
    partition["slotsManifests"] = {"apparentBytes": slot_other}
    partition["dyad-receipt.json"] = (DYAD / "dyad-receipt.json").stat().st_size
    partition["evidence.sqlite"] = tree_bytes(DYAD / "evidence.sqlite")
    runs = DYAD / "runs"
    run_dirs = sorted(p for p in runs.iterdir() if p.is_dir())
    parent_run = tree_bytes(runs / "ala-calibration-primary-0002-parent")
    branch_runs_n = len(run_dirs) - 1
    branch_runs_bytes = tree_bytes(runs)["apparentBytes"] - parent_run["apparentBytes"]

    db = sqlite_anatomy(DYAD / "evidence.sqlite")

    # Heads digest census: one streaming pass, digests + lengths only.
    head_digests: dict[str, dict] = {}
    envelope_bytes: list[int] = []
    scanned = 0
    first_payload = None
    for slot in slots:
        for branch in sorted((slot / "branches").glob("*")):
            p = find_commitment(branch)
            if first_payload is None:
                first_payload = p
            heads = p["state"]["trainingLedgerHeads"]
            canon = json.dumps(heads, sort_keys=True).encode()
            digest = hashlib.sha256(canon).hexdigest()
            entry = head_digests.setdefault(
                digest, {"count": 0, "bytes": len(canon), "exampleBranch": None}
            )
            entry["count"] += 1
            if entry["exampleBranch"] is None:
                entry["exampleBranch"] = str(branch.relative_to(DYAD))
            env = {k: v for k, v in p.items() if k != "state"}
            env_bytes = len(json.dumps(env).encode())
            env_bytes += len(
                json.dumps(
                    {k: v for k, v in p["state"].items() if k != "trainingLedgerHeads"}
                ).encode()
            )
            envelope_bytes.append(env_bytes)
            scanned += 1
    distinct = len(head_digests)
    total_head_bytes = sum(e["count"] * e["bytes"] for e in head_digests.values())
    unique_head_bytes = sum(e["bytes"] for e in head_digests.values())
    top = max(head_digests.items(), key=lambda kv: kv[1]["count"])

    # Compressibility: in-memory gzip of one heads object + one ledger file.
    sample_heads = json.dumps(
        first_payload["state"]["trainingLedgerHeads"], sort_keys=True
    ).encode()
    ledger_sample = (DYAD / "parent-bundle/baby-a-ledger.jsonl").read_bytes()
    gzip_heads = len(gzip.compress(sample_heads, compresslevel=6))
    gzip_ledger = len(gzip.compress(ledger_sample, compresslevel=6))

    # Corpus selection: median-heads normal branch + median-heads
    # ledger-consistent branch from two different slots (deterministic).
    sizes = []
    for slot in slots:
        for branch in sorted((slot / "branches").glob("*")):
            if branch.name.endswith("-normal") or branch.name.endswith("-ledger-consistent"):
                sizes.append(branch)
    sizes.sort(key=lambda b: (b / "intervention-log.jsonl").stat().st_size)
    pick_normal = next(b for b in sizes if b.name.endswith("-normal"))
    pick_consistent = next(
        b for b in sizes
        if b.name.endswith("-ledger-consistent") and b.parent != pick_normal.parent
    )
    corpus = []
    for pick in (pick_normal, pick_consistent):
        p = find_commitment(pick)
        canon = json.dumps(p, sort_keys=True).encode()
        corpus.append({
            "branch": str(pick.relative_to(DYAD)),
            "payloadBytes": len(canon),
            "payloadSha256": hashlib.sha256(canon).hexdigest(),
        })

    out = {
        "dyad": "evidence/ala-lv/calibration/primary-0002",
        "partition": partition,
        "runsSplit": {
            "parentRunApparentBytes": parent_run["apparentBytes"],
            "branchRunCount": branch_runs_n,
            "branchRunsApparentBytes": branch_runs_bytes,
        },
        "sqlite": db,
        "headsCensus": {
            "branchesScanned": scanned,
            "distinctHeadDigests": distinct,
            "totalHeadBytes": total_head_bytes,
            "uniqueHeadBytes": unique_head_bytes,
            "topDigestShare": top[1]["count"] / scanned,
            "topDigestExampleBranch": top[1]["exampleBranch"],
            "digests": [
                {"sha256": d, **e} for d, e in
                sorted(head_digests.items(), key=lambda kv: -kv[1]["count"])
            ],
        },
        "envelopeBytes": {
            "min": min(envelope_bytes), "max": max(envelope_bytes),
            "total": sum(envelope_bytes),
        },
        "gzipSamples": {
            "headsBytes": len(sample_heads), "headsGzipBytes": gzip_heads,
            "ledgerBytes": len(ledger_sample), "ledgerGzipBytes": gzip_ledger,
        },
        "corpusSelection": {
            "rule": "median-size -normal branch + median-size -ledger-consistent "
                    "branch from a different slot (sorted by intervention-log.jsonl "
                    "apparent bytes, deterministic)",
            "picks": corpus,
        },
        "coverage": {
            "branches": f"{scanned}/1680 commitments parsed",
            "sqlite": "all tables: row counts + TEXT/BLOB payload sums; page stats",
            "unmeasured": "per-index byte sizes (index DDL counted only); "
                          "parent-run export internals; slot manifest semantics",
        },
        "method": "read-only single-pass scan; digests + lengths only in memory",
    }
    OUT.write_text(json.dumps(out, indent=1) + "\n")
    print(f"branches={scanned} distinctHeads={distinct} "
          f"total={total_head_bytes} unique={unique_head_bytes}")
    print(f"topDigestShare={top[1]['count']}/{scanned}")
    print(f"corpus={[c['branch'] for c in corpus]}")
    print(f"wrote {OUT}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
