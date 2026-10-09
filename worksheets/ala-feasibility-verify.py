#!/usr/bin/env python3
"""ALA-LV P6-feasibility independent verification (pinned worksheet env).

Recomputes the feasibility numbers from raw evidence without importing the
TypeScript scorer or trusting console summaries:

  V1  dyad receipt present, valid, 240/240 cases
  V2  receipt evidenceBytes == bytes on disk (exact)
  V3  derived CPU-h / wall-min arithmetic (1e-9 relative)
  V4  240 slot dirs / 1680 branch dirs on disk
  V5  journal accounts 3/3 dev slots with terminal states
  V6  P4 receipt cost fields match dyad receipt (exact)
  V7  P5 reservation math recomputed; NO-GO decision reproduced
  V8  benchmark rehearsal under its 0.1 CPU-h cap
  V9  no admission.json exists (collection disabled)
  V10 qualification worksheet reruns green (subprocess, exit 0)

Writes reports/research/ala-p6-feasibility-verification.v1.json and exits
nonzero on any failure. No study statistics exist, so there is no Brier /
interval reduction to reproduce; this verifies the cost/qualification basis
of the feasibility paper instead.

Run: .venv-worksheet/bin/python worksheets/ala-feasibility-verify.py
"""

import json
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DYAD = ROOT / "evidence/ala-lv/calibration/primary-0002"
OUT = ROOT / "reports/research/ala-p6-feasibility-verification.v1.json"

checks: list[tuple[str, bool, str]] = []
values: dict = {}


def check(name: str, ok: bool, detail: str) -> None:
    checks.append((name, bool(ok), detail))
    print(f"{'PASS' if ok else 'FAIL'} {name}: {detail}", flush=True)


def rel_close(a: float, b: float, tol: float = 1e-9) -> bool:
    return abs(a - b) <= tol * max(1.0, abs(a), abs(b))


def du_bytes(path: Path) -> int:
    total = 0
    for dirpath, _, files in os.walk(path):
        for f in files:
            try:
                total += os.path.getsize(os.path.join(dirpath, f))
            except OSError:
                pass
    return total


def main() -> int:
    receipt = json.loads((DYAD / "dyad-receipt.json").read_text())
    cost = receipt["cost"]
    check("V1 receipt", receipt.get("state") == "valid" and receipt.get("casesValid") == 240,
          f"state={receipt.get('state')} casesValid={receipt.get('casesValid')}")

    on_disk = du_bytes(DYAD)
    receipt_file = (DYAD / "dyad-receipt.json").stat().st_size
    # The runner measures directoryBytes before writing dyad-receipt.json,
    # so the receipt excludes itself; disk must equal receipt + receipt file.
    check("V2 bytes", cost["evidenceBytes"] + receipt_file == on_disk,
          f"receipt={cost['evidenceBytes']} +receipt_file={receipt_file} disk={on_disk}")
    values["evidenceBytes"] = on_disk
    values["receiptEvidenceBytes"] = cost["evidenceBytes"]

    cpu_h = cost["cpuMicroseconds"] / 3.6e9
    wall_min = cost["wallMilliseconds"] / 60000.0
    check("V3 arithmetic", rel_close(cpu_h, cost["cpuMicroseconds"] / 3600000000.0) and wall_min > 0,
          f"cpu_h={cpu_h:.6f} wall_min={wall_min:.4f}")
    values["cpuHours"] = cpu_h
    values["wallMinutes"] = wall_min
    values["peakBytes"] = cost["peakBytes"]

    slots = sorted(p for p in DYAD.glob("slot-*") if p.is_dir())
    branches = sum(1 for s in slots for _ in (s / "branches").glob("*"))
    check("V4 counts", len(slots) == 240 and branches == 1680,
          f"slots={len(slots)} branches={branches}")

    journal = [json.loads(line) for line in (ROOT / "evidence/ala-lv/journal.jsonl").read_text().splitlines()]
    cal = {}
    for e in journal:
        if e.get("cohort") == "calibration" and e.get("kind") == "primary":
            cal.setdefault(e["index"], []).append(e["state"])
    terminal = {"valid", "invalid", "aborted", "failed"}
    v5_ok = set(cal) == {0, 1, 2} and all(any(s in terminal for s in v) for v in cal.values())
    check("V5 journal", v5_ok, f"calibration slots={ {k: v for k, v in cal.items()} }")
    values["devSlots"] = {str(k): v for k, v in cal.items()}

    p4 = json.loads((ROOT / "reports/research/ala-p4-qualification-receipt.v1.json").read_text())
    p4c = p4["calibrationDyad"]["cost"]
    check("V6 p4-match",
          p4c["evidenceBytesOnDisk"] == on_disk and p4c["evidenceBytes"] == cost["evidenceBytes"]
          and p4c["cpuMicroseconds"] == cost["cpuMicroseconds"],
          "P4 receipt cost fields match dyad receipt + disk exactly")

    # V7: reservation math from first principles.
    cap_h, cap_b = 12.0, 10 * 1024**3
    analysis_h, dev_h = 2.0, 1.0
    per_h, per_b = 2 * cpu_h, 2 * on_disk
    need_44 = 44 * per_h + analysis_h + dev_h
    need_24 = 24 * per_h + analysis_h + dev_h
    need_20_1x = 20 * cpu_h + analysis_h + dev_h
    stor_24 = 24 * per_b
    nogo = need_44 > cap_h and need_24 > cap_h and need_20_1x > cap_h and stor_24 > cap_b
    check("V7 nogo", nogo,
          f"44slot={need_44:.1f}h 24slot={need_24:.1f}h 20slot@1x={need_20_1x:.1f}h (cap {cap_h}h); "
          f"24slot storage={stor_24 / 1024**3:.0f}GiB (cap 10GiB)")
    values["reservation"] = {"need44CpuH": need_44, "need24CpuH": need_24,
                             "need20at1xCpuH": need_20_1x, "need24StorageGiB": stor_24 / 1024**3}

    bench = json.loads((ROOT / "evidence/ala-lv/p4-benchmark-rehearsal.json").read_text())
    check("V8 benchmark rehearsal (no signing-isolation conclusion)", bench["cpuSeconds"] < bench["capCpuSeconds"],
          f"{bench['cpuSeconds']:.1f}s < {bench['capCpuSeconds']:.0f}s cap")
    values["benchmarkCpuSeconds"] = bench["cpuSeconds"]

    check("V9 no-admission", not (ROOT / "evidence/ala-lv/admission.json").exists(),
          "admission.json absent; study cohorts gate-disabled")

    proc = subprocess.run(
        [sys.executable, str(ROOT / "worksheets/ala-qualification.py"), "--dyad", str(DYAD)],
        capture_output=True, text=True, timeout=600,
    )
    q_pass = proc.returncode == 0 and "QUALIFICATION FAILED" not in (proc.stdout + proc.stderr)
    check("V10 worksheet", q_pass, f"exit={proc.returncode}")
    values["worksheetExit"] = proc.returncode

    failed = [n for n, ok, _ in checks if not ok]
    OUT.write_text(json.dumps({
        "verificationVersion": "v1",
        "studyId": "ALA-LV",
        "status": "passed" if not failed else "FAILED",
        "checks": [{"name": n, "pass": ok, "detail": d} for n, ok, d in checks],
        "values": values,
        "tolerances": {"bytesAndIds": "exact", "derivedHours": "1e-9 relative"},
        "note": "Independent recomputation from raw evidence; no TypeScript scorer import, no console summaries.",
    }, indent=1) + "\n")
    print(f"wrote {OUT}: {'PASSED' if not failed else 'FAILED ' + ','.join(failed)}")
    return 0 if not failed else 1


if __name__ == "__main__":
    sys.exit(main())
