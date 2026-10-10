#!/usr/bin/env python3
"""ALA-LV D-B seed census (pinned worksheet env, read-only).

Parses the 1680 retained branch run-configs and counts distinct
`randomSeed` and `seedBindings.gateway` values, asserting agreement
with the N1 message extract's per-slot seeds. Closes the gap where N1
cited gateway-seed constancy with no reproducing artifact.

Writes reports/research/ala-seed-census.v1.json (kilobytes). No study
slots are consumed; no new runs are launched; no database writes.

Run: .venv/bin/python worksheets/ala-seed-census.py
"""

import hashlib
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DYAD = ROOT / "evidence/ala-lv/calibration/primary-0002"
EXTRACT = ROOT / "reports/research/ala-message-extract.v1.json"
OUT = ROOT / "reports/research/ala-seed-census.v1.json"


def main() -> int:
    if not DYAD.is_dir():
        print(f"FAIL: dyad evidence missing: {DYAD}", file=sys.stderr)
        return 1
    random_seeds: dict[str, int] = {}
    gateways: dict[str, int] = {}
    checked = 0
    mismatches = []
    slots = sorted(p for p in DYAD.glob("slot-*") if p.is_dir())
    for slot in slots:
        for bdir in sorted((slot / "branches").glob("*")):
            if not bdir.is_dir():
                continue
            cfg = json.loads((bdir / "configuration" / "run-config.json").read_text())
            rs = cfg.get("randomSeed")
            gw = (cfg.get("seedBindings") or {}).get("gateway")
            if not isinstance(rs, str) or not isinstance(gw, str):
                mismatches.append(f"{slot.name}/{bdir.name}: missing seed fields")
                continue
            random_seeds[rs] = random_seeds.get(rs, 0) + 1
            gateways[gw] = gateways.get(gw, 0) + 1
            checked += 1
    extract = json.loads(EXTRACT.read_text())
    extract_seeds = set()
    for row in extract["testRows"]:
        extract_seeds.update(row.get("seeds", []))
    agreement = set(random_seeds) == extract_seeds
    gateway_digest = hashlib.sha256(
        "\n".join(sorted(gateways)).encode()
    ).hexdigest()
    out = {
        "dyad": "evidence/ala-lv/calibration/primary-0002",
        "branchesChecked": checked,
        "distinctRandomSeed": len(random_seeds),
        "distinctGateway": len(gateways),
        "randomSeedHistogram": random_seeds,
        "gatewayDigest": f"sha256:{gateway_digest}",
        "extractSeeds": sorted(extract_seeds),
        "extractAgreement": agreement,
        "mismatches": mismatches,
        "method": "read-only parse of configuration/run-config.json per branch",
    }
    OUT.write_text(json.dumps(out, indent=1) + "\n")
    print(f"branches={checked} distinctRandom={len(random_seeds)} "
          f"distinctGateway={len(gateways)} extractAgreement={agreement}")
    print(f"wrote {OUT}")
    if mismatches or not agreement:
        print("FAIL: seed census mismatches present", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
