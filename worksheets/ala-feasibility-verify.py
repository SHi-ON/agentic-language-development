#!/usr/bin/env python3
"""ALA-LV P6-feasibility independent verification (pinned worksheet env).

Recomputes the feasibility numbers from raw evidence without importing the
TypeScript scorer or trusting console summaries:

  V1  dyad receipt present, valid, 240/240 cases
  V2  receipt evidenceBytes == bytes on disk (exact)
  V3  all six cost fields present/sane; unit conversion pinned (primitives
      vs derived recorded separately; no independent CPU remeasurement)
  V4  240 slot dirs / 1680 branch dirs on disk
  V5  journal accounts 3/3 dev slots with terminal states
  V6  P4 receipt v2 cost fields match dyad receipt (exact) + derived
      quotients exact (v1 derived cpuHours/peakGiB from stale primitives)
  V7  caps parsed from the frozen amendment + P5 record (no hardcoded
      constants); reservation math recomputed; NO-GO reproduced at the
      historical dev assumption AND at dev=0 (consumed cost unknown)
  V8  benchmark rehearsal under its 0.1 CPU-h cap
  V9  no admission.json exists (collection disabled)
  V10 qualification worksheet reruns green (subprocess, exit 0)

Writes reports/research/ala-p6-feasibility-verification.v1.json and exits
nonzero on any failure. No study statistics exist, so there is no Brier /
interval reduction to reproduce; this verifies the cost/qualification basis
of the feasibility paper instead.

Run: .venv/bin/python worksheets/ala-feasibility-verify.py
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


# SQLite WAL-mode sidecars are ephemeral read state (ignored, untracked),
# not retained evidence: ala-byte-audit records them under
# partition["sqliteSidecars"] instead of counting them. Mirror that here
# so a 32768 B shm left by an earlier read cannot fail the exact V2 check.
SQLITE_SIDECARS = ("evidence.sqlite-shm", "evidence.sqlite-wal")


def du_bytes(path: Path) -> tuple[int, dict[str, int]]:
    total = 0
    sidecars: dict[str, int] = {}
    for dirpath, _, files in os.walk(path):
        for f in files:
            full = os.path.join(dirpath, f)
            try:
                size = os.path.getsize(full)
            except OSError:
                continue
            if f in SQLITE_SIDECARS:
                sidecars[full] = size
                continue
            total += size
    return total, sidecars


def main() -> int:
    receipt = json.loads((DYAD / "dyad-receipt.json").read_text())
    cost = receipt["cost"]
    check("V1 receipt", receipt.get("state") == "valid" and receipt.get("casesValid") == 240,
          f"state={receipt.get('state')} casesValid={receipt.get('casesValid')}")

    on_disk, sidecars = du_bytes(DYAD)
    receipt_file = (DYAD / "dyad-receipt.json").stat().st_size
    # The runner measures directoryBytes before writing dyad-receipt.json,
    # so the receipt excludes itself; disk must equal receipt + receipt file.
    check("V2 bytes", cost["evidenceBytes"] + receipt_file == on_disk,
          f"receipt={cost['evidenceBytes']} +receipt_file={receipt_file} disk={on_disk}")
    values["evidenceBytes"] = on_disk
    values["receiptEvidenceBytes"] = cost["evidenceBytes"]
    values["sqliteSidecarsExcluded"] = sidecars

    cpu_h = cost["cpuMicroseconds"] / 3.6e9
    wall_min = cost["wallMilliseconds"] / 60000.0
    expected_keys = {"cpuMicroseconds", "wallMilliseconds", "peakBytes",
                     "evidenceBytes", "verificationMilliseconds", "unresolved"}
    keys_ok = set(cost) == expected_keys
    sane_ok = all(cost[k] >= 0 for k in expected_keys - {"unresolved"}) and cost["unresolved"] == []
    conv_ok = rel_close(cpu_h * 3.6e9, cost["cpuMicroseconds"]) and rel_close(wall_min * 60000.0, cost["wallMilliseconds"])
    check("V3 cost-fields", keys_ok and sane_ok and conv_ok,
          f"keys={sorted(cost)} unresolved={cost['unresolved']} cpu_h={cpu_h:.6f} wall_min={wall_min:.4f}")
    values["primitives"] = {k: cost[k] for k in sorted(expected_keys)}
    values["derived"] = {"cpuHours": cpu_h, "wallMinutes": wall_min}
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

    p4 = json.loads((ROOT / "reports/research/ala-p4-qualification-receipt.v2.json").read_text())
    p4c = p4["calibrationDyad"]["cost"]
    # v1 derived cpuHours/peakGiB from stale primitives (round-3 M5); v2
    # recomputes them, and this check pins the quotient relation so the
    # bug class fails here instead of hiding in a rounded display value.
    derived_ok = (
        p4.get("receiptVersion") == "v2"
        and rel_close(p4c["cpuHours"], p4c["cpuMicroseconds"] / 3.6e9)
        and rel_close(p4c["peakGiB"], p4c["peakBytes"] / 1024**3)
    )
    check("V6 p4-match",
          p4c["evidenceBytesOnDisk"] == on_disk and p4c["evidenceBytes"] == cost["evidenceBytes"]
          and p4c["cpuMicroseconds"] == cost["cpuMicroseconds"] and derived_ok,
          "P4 receipt v2 cost fields match dyad receipt + disk exactly; derived quotients exact")

    # V7: reservation math from the frozen contract (no hardcoded constants).
    amend = json.loads((ROOT / "protocols/ala-lv-scope-and-resource-amendment.v1.json").read_text())
    p5 = json.loads((ROOT / "reports/research/ala-p5-admission-decision.v1.json").read_text())
    cap_h = amend["newAllowance"]["campaignCpuHoursMax"]
    per_h_cap = amend["newAllowance"]["perDyadCpuHoursMax"]
    per_w_cap = amend["newAllowance"]["perDyadWallMinutesMax"]
    analysis_h = amend["newCaps"]["analysisReproductionCpuHoursReserved"]
    cap_b = amend["newCaps"]["additionalRetainedWorkingBytesMax"]
    assert p5["campaignCapCpuHours"] == cap_h, "P5/amendment cap mismatch"
    assert p5["analysisReservedCpuHours"] == analysis_h, "P5/amendment analysis mismatch"
    assert p5["workingCapBytes"] == cap_b, "P5/amendment storage mismatch"
    assert "Twice the maximum" in p5["provisioningRule"], "provisioning rule changed"
    # dev=1.0 is the P5 historical assumption, NOT a measurement: the attempt
    # accounting correction withdrew the bound (crashed-run costs unknown).
    dev_h = p5["devConsumedCpuHoursUpperBound"]
    per_h, per_b = 2 * cpu_h, 2 * on_disk
    need_44 = 44 * per_h + analysis_h + dev_h
    need_24 = 24 * per_h + analysis_h + dev_h
    need_20_1x = 20 * cpu_h + analysis_h + dev_h
    stor_24 = 24 * per_b
    nogo_hist = need_44 > cap_h and need_24 > cap_h and need_20_1x > cap_h and stor_24 > cap_b
    nogo_0 = (44 * per_h + analysis_h) > cap_h and (24 * per_h + analysis_h) > cap_h \
        and (20 * cpu_h + analysis_h) > cap_h and stor_24 > cap_b
    guard_viol = cpu_h > per_h_cap and wall_min > per_w_cap
    check("V7 nogo", nogo_hist and nogo_0 and guard_viol,
          f"44slot={need_44:.1f}h 24slot={need_24:.1f}h 20slot@1x={need_20_1x:.1f}h (cap {cap_h}h); "
          f"24slot storage={stor_24 / 1024**3:.0f}GiB (cap {cap_b / 1024**3:.0f}GiB); "
          f"dev0-robust={nogo_0} guard-violated={guard_viol}")
    values["reservation"] = {"need44CpuH": need_44, "need24CpuH": need_24,
                             "need20at1xCpuH": need_20_1x, "need24StorageGiB": stor_24 / 1024**3,
                             "capsFrom": "protocols/ala-lv-scope-and-resource-amendment.v1.json",
                             "devAssumptionHistorical": dev_h, "nogoAtDev0": nogo_0}

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
