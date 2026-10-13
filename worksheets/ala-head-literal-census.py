#!/usr/bin/env python3
"""Round-4 R6: literal-byte census of retained ledger-head values.

For each of the 1680 retained branch commitment logs, extracts the
EXACT UTF-8 substring of the "trainingLedgerHeads" JSON value from the
prediction-commitment event line (balanced-brace scan with JSON string
awareness, no reserialization). Reports distinct literal byte strings,
their lengths, and digests, plus decoded-value equality.

This complements ala-byte-audit, which measures Python-reserialized
logical payload bytes (json.dumps with default spaces). The two byte
domains differ by serialization overhead and must never be mixed.

Fail-closed: 1680 branches, exactly one head event line each, else
exit 1 with no report.
"""
import hashlib
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DYAD = ROOT / "evidence/ala-lv/calibration/primary-0002"
OUT = ROOT / "reports/research/ala-head-literal-census.v1.json"


def literal_value(line: bytes, key: bytes) -> bytes:
    """Exact raw bytes of a top-level JSON object's value for key.

    Operates on bytes throughout (JSON structure characters are ASCII),
    so no decode/re-encode round trip stands between the retained file
    bytes and the measured substring.
    """
    anchor = b'"' + key + b'"'
    at = line.find(anchor)
    if at < 0:
        raise ValueError(f"key {key!r} absent")
    i = at + len(anchor)
    while line[i] in b" \t\r\n":
        i += 1
    if line[i] != 0x3A:  # :
        raise ValueError(f"key {key!r} not followed by colon")
    i += 1
    while line[i] in b" \t\r\n":
        i += 1
    if line[i] != 0x7B:  # {
        raise ValueError(f"key {key!r} value is not an object")
    start = i
    depth = 0
    instr = False
    esc = False
    while True:
        ch = line[i]
        if instr:
            if esc:
                esc = False
            elif ch == 0x5C:  # backslash
                esc = True
            elif ch == 0x22:  # quote
                instr = False
        elif ch == 0x22:
            instr = True
        elif ch == 0x7B:
            depth += 1
        elif ch == 0x7D:  # }
            depth -= 1
            if depth == 0:
                return line[start:i + 1]
        i += 1


def main() -> int:
    if not DYAD.is_dir():
        print(f"FAIL: dyad evidence missing: {DYAD}", file=sys.stderr)
        return 1
    branches = sorted(DYAD.glob("slot-*/branches/*"))
    if len(branches) != 1680:
        print(f"FAIL: {len(branches)} branches, want 1680", file=sys.stderr)
        return 1
    substrings: dict[bytes, int] = {}
    decoded: dict[str, int] = {}
    total = 0
    for bdir in branches:
        hits = []
        with open(bdir / "intervention-log.jsonl", "rb") as f:
            for line in f:
                if b'"trainingLedgerHeads"' in line:
                    hits.append(line.rstrip(b"\n"))
        if len(hits) != 1:
            print(f"FAIL: {bdir} holds {len(hits)} head lines", file=sys.stderr)
            return 1
        try:
            raw = literal_value(hits[0], b"trainingLedgerHeads")
        except (ValueError, IndexError) as exc:
            print(f"FAIL: {bdir} extraction failed: {exc}", file=sys.stderr)
            return 1
        substrings[raw] = substrings.get(raw, 0) + 1
        total += len(raw)
        try:
            canon = json.dumps(json.loads(raw.decode("utf-8")), sort_keys=True)
        except (UnicodeDecodeError, ValueError) as exc:
            print(f"FAIL: {bdir} head bytes are not valid UTF-8 JSON: {exc}",
                  file=sys.stderr)
            return 1
        decoded[canon] = decoded.get(canon, 0) + 1
    # A census reports what it finds: distinct counts are findings, not
    # assertions. The paper seal pins the observed values for this corpus.
    (literal, _count), = sorted(substrings.items(), key=lambda kv: -kv[1])[:1]
    entries = sum(len(v) for v in json.loads(literal.decode("utf-8")).values())
    OUT.write_text(json.dumps({
        "censusVersion": "v1",
        "studyId": "ALA-LV",
        "dyad": "calibration/primary-0002",
        "method": ("exact UTF-8 bytes of the trainingLedgerHeads JSON value "
                   "in each branch prediction-commitment event line "
                   "(byte-level balanced-brace scan, JSON string aware, no "
                   "decode/re-encode round trip); decoded with json.loads "
                   "for value equality only"),
        "branches": len(branches),
        "literalDistinct": len(substrings),
        "literalUniqueBytes": sum(len(b) for b in substrings),
        "literalTotalBytes": total,
        "literalHeadDigest": hashlib.sha256(literal).hexdigest(),
        "decodedDistinct": len(decoded),
        "headEntryCount": entries,
    }, indent=1))
    print(f"branches={len(branches)} literalDistinct={len(substrings)} "
          f"unique={sum(len(b) for b in substrings)} total={total}")
    print(f"digest={hashlib.sha256(literal).hexdigest()}")
    print(f"wrote {OUT}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
