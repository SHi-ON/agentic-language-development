"""ALA-LV P4 software + learning qualification worksheet (Marimo).

Evaluates one dyad directory against the P4 qualification checks that need a
real executed dyad (the `test:paper` suite covers the fixture-level checks):

  Q0  receipt present and valid [structural: retained summary]
  Q1  turn chain integrity (hash links, counts, roles) [structural]
  Q2  learning curve recomputed from retained outcomes [observed behavior]
  Q3  update counters consistent (updateCount, optimizer, checkpoints)
      [structural: cached counters, not a live-update proof]
  Q4  ledger event counts recomputed [observed behavior]
  Q5  collection census + paired-scenario match [structural]
  Q6  freeze config flags set [configurational: flags only, no live
      weight/optimizer/hidden-state comparison]

Fail-closed: any FAIL verdict raises at the end. No test-effect labels are
read or reported here (P5 blinding): branch checks use scenario hashes and
config flags only, never outcomes. Every verdict carries the run
timestamp; cached outputs must never be presented as fresh observations.

Run headless:
  .venv-worksheet/bin/python worksheets/ala-qualification.py \
      --dyad evidence/ala-lv/calibration/primary-0002
"""

import marimo as mo

app = mo.App()


@app.cell
def _():
    import marimo as mo

    return (mo,)


@app.cell
def _(mo):
    import hashlib
    import json
    import os
    import sys
    from datetime import datetime, timezone
    from pathlib import Path

    argv = sys.argv[1:]
    if "--dyad" in argv:
        dyad_raw = argv[argv.index("--dyad") + 1]
    else:
        dyad_raw = os.environ.get("ALA_DYAD")
    if dyad_raw is None:
        raise SystemExit("missing required --dyad <dir> (or ALA_DYAD env)")
    DYAD = Path(dyad_raw)
    PARENT = DYAD / "parent-bundle"
    if not PARENT.is_dir():
        raise SystemExit(f"no parent-bundle under {DYAD}")

    verdicts: list[tuple[str, bool, str]] = []
    RUN_AT = datetime.now(timezone.utc).isoformat(timespec="seconds")
    print(f"worksheet run at {RUN_AT} on {DYAD}", flush=True)

    def check(name: str, ok: bool, detail: str) -> str:
        verdicts.append((name, ok, detail))
        line = f"{'PASS' if ok else 'FAIL'} {name}: {detail}"
        print(line, flush=True)
        return line

    mo.md(f"## ALA-LV P4 qualification: `{DYAD}`\n\nRun at {RUN_AT}.")
    return DYAD, PARENT, check, hashlib, json, os, sys, verdicts, RUN_AT


@app.cell
def _(DYAD, check, json, mo):
    q0_receipt_path = DYAD / "dyad-receipt.json"
    q0_receipt = json.loads(q0_receipt_path.read_text()) if q0_receipt_path.exists() else None
    q0_ok = q0_receipt is not None and q0_receipt.get("state") == "valid"
    q0_msg = check(
        "Q0 receipt [structural]",
        q0_ok,
        f"state={None if q0_receipt is None else q0_receipt.get('state')}, "
        f"casesValid={None if q0_receipt is None else q0_receipt.get('casesValid')}",
    )
    mo.md(f"**{q0_msg}**")
    return (q0_receipt,)


@app.cell
def _(PARENT, check, json, mo):
    q1_turns = [json.loads(ql) for ql in (PARENT / "turn-records.jsonl").read_text().splitlines()]
    q1_running = [qt for qt in q1_turns if qt.get("phase") == "running"]
    q1_by_seq = sorted(q1_turns, key=lambda qt: qt["sequence"])
    q1_chain_ok = all(
        cur["previousEntryHash"] == prev["entryHash"]
        for prev, cur in zip(q1_by_seq, q1_by_seq[1:])
    )
    q1_roles = sorted({str((qt.get("roles") or {}).get("receiver")) for qt in q1_running})
    q1_msg = check(
        "Q1 turn chain [structural]",
        q1_chain_ok and len(q1_running) == 3000,
        f"{len(q1_turns)} turns, {len(q1_running)} running, chain-linked={q1_chain_ok}, receivers={q1_roles}",
    )
    mo.md(f"**{q1_msg}**")
    return q1_running, q1_turns


@app.cell
def _(check, mo, q1_running):
    q2_rows = []
    for q2_start in range(0, len(q1_running), 100):
        q2_block = q1_running[q2_start : q2_start + 100]
        q2_ok = sum(1 for qt in q2_block if (qt.get("outcome") or {}).get("success") is True)
        q2_recv_a = [qt for qt in q2_block if (qt.get("roles") or {}).get("receiver") == "baby-a"]
        q2_ok_a = sum(1 for qt in q2_recv_a if (qt.get("outcome") or {}).get("success") is True)
        q2_rows.append((q2_start, len(q2_block), q2_ok / len(q2_block), (q2_ok_a / len(q2_recv_a)) if q2_recv_a else float("nan")))
    q2_msg = check(
        "Q2 learning curve [observed]",
        len(q2_rows) == 30,
        f"30x100-turn blocks, first={q2_rows[0][2]:.2f}, last={q2_rows[-1][2]:.2f}, chance=0.25 (curve captured; no threshold)",
    )
    q2_table = ["| block | n | success | baby-a-receiver success |", "| --- | --- | --- | --- |"]
    for q2_s, q2_n, q2_r, q2_a in q2_rows:
        q2_cell = f"{q2_a:.3f}" if isinstance(q2_a, float) and q2_a == q2_a else "n/a"
        q2_table.append(f"| {q2_s}-{q2_s + q2_n - 1} | {q2_n} | {q2_r:.3f} | {q2_cell} |")
    mo.md(f"**{q2_msg}**\n\n" + "\n".join(q2_table))
    return (q2_rows,)


@app.cell
def _(PARENT, check, json, mo):
    q3_upd = {}
    for q3_baby in ("baby-a", "baby-b"):
        q3_pol = json.loads((PARENT / "policies" / f"{q3_baby}-latest.json").read_text())
        q3_model = q3_pol["model"]
        q3_step = q3_model["optimizer"].get("step", 0)
        q3_nonzero = any(c not in "0,. []-+e" for c in json.dumps(q3_model["optimizer"].get("firstMoment", [])))
        q3_upd[q3_baby] = (q3_model["updateCount"], q3_model["parameterCount"], q3_step, q3_nonzero)
    q3_ckpts = sorted((PARENT / "checkpoints").glob("*.json"))
    q3_chain = [json.loads(qp.read_text()) for qp in q3_ckpts]
    q3_links_ok = all(qb["previousCheckpointHash"] == qa["checkpointHash"] for qa, qb in zip(q3_chain, q3_chain[1:]))
    q3_roots = {qc["babyA"]["merkleRoot"] for qc in q3_chain} | {qc["babyB"]["merkleRoot"] for qc in q3_chain}
    q3_ok = all(qu[0] == 3000 and qu[2] > 0 and qu[3] for qu in q3_upd.values()) and q3_links_ok and len(q3_roots) > 10
    q3_msg = check(
        "Q3 update counters [structural]",
        q3_ok,
        f"updateCount={ {qb: qu[0] for qb, qu in q3_upd.items()} }, params={ {qb: qu[1] for qb, qu in q3_upd.items()} }, "
        f"optSteps={ {qb: qu[2] for qb, qu in q3_upd.items()} }, {len(q3_ckpts)} checkpoints linked={q3_links_ok}, "
        f"{len(q3_roots)} distinct ledger roots",
    )
    mo.md(f"**{q3_msg}**")
    return (q3_upd,)


@app.cell
def _(PARENT, check, json, mo):
    from collections import Counter

    q4_counts = {}
    for q4_baby in ("baby-a", "baby-b"):
        q4_events = [json.loads(ql) for ql in (PARENT / f"{q4_baby}-ledger.jsonl").read_text().splitlines()]
        q4_counts[q4_baby] = Counter(qe.get("eventType") for qe in q4_events)
    q4_total = sum(sum(qc.values()) for qc in q4_counts.values())
    q4_has_hyp = any(any("hypothesis" in str(qk) or "term" in str(qk) for qk in qc) for qc in q4_counts.values())
    q4_msg = check(
        "Q4 ledger activity [observed]",
        q4_total > 1000 and q4_has_hyp,
        f"{q4_total} ledger events; baby-a={dict(q4_counts['baby-a'])}, baby-b={dict(q4_counts['baby-b'])}",
    )
    mo.md(f"**{q4_msg}**")
    return (q4_counts,)


@app.cell
def _(DYAD, check, json, mo):
    q5_slots = sorted(qp for qp in DYAD.glob("slot-*") if qp.is_dir())
    q5_branches_ok = True
    q5_paired_ok = True
    q5_branch_total = 0
    q5_first_bad = ""
    for q5_slot in q5_slots:
        q5_bdirs = sorted(qp for qp in (q5_slot / "branches").glob("*") if qp.is_dir()) if (q5_slot / "branches").is_dir() else []
        q5_branch_total += len(q5_bdirs)
        if len(q5_bdirs) != 7 or not (q5_slot / "treatment-batch.json").exists() or not (q5_slot / "schedule.json").exists():
            q5_branches_ok = False
            q5_first_bad = q5_first_bad or q5_slot.name
            continue
        q5_hashes = set()
        for q5_b in q5_bdirs:
            q5_tp = q5_b / "turn-records.jsonl"
            if not q5_tp.exists():
                q5_paired_ok = False
                q5_first_bad = q5_first_bad or f"{q5_slot.name}/{q5_b.name}"
                break
            q5_recs = [json.loads(ql) for ql in q5_tp.read_text().splitlines()]
            if len(q5_recs) != 1:
                q5_paired_ok = False
                q5_first_bad = q5_first_bad or f"{q5_slot.name}/{q5_b.name}"
                break
            q5_hashes.add(q5_recs[0].get("scenarioStateHash"))
        if len(q5_hashes) != 1:
            q5_paired_ok = False
            q5_first_bad = q5_first_bad or q5_slot.name
    q5_ok = len(q5_slots) == 240 and q5_branches_ok and q5_paired_ok
    q5_msg = check(
        "Q5 collection [structural]",
        q5_ok,
        f"{len(q5_slots)}/240 cases, {q5_branch_total}/1680 branches, paired-scenario-match={q5_paired_ok}"
        + ("" if q5_ok else f" (first bad: {q5_first_bad})"),
    )
    mo.md(f"**{q5_msg}**")
    return (q5_slots,)


@app.cell
def _(PARENT, check, json, mo, q5_slots):
    # Freeze = every test branch restores the exact final training state and
    # runs with updates disabled: derivedFromCheckpointHash must equal the
    # parent's final checkpoint hash and evaluationOnly must be true.
    q6_ckpts = sorted((PARENT / "checkpoints").glob("*.json"))
    q6_final = json.loads(q6_ckpts[-1].read_text())["checkpointHash"] if q6_ckpts else None
    q6_checked = 0
    q6_bad: list[str] = []
    for q6_slot in q5_slots:
        for q6_bdir in sorted((q6_slot / "branches").glob("*")):
            q6_cfg_path = q6_bdir / "configuration" / "run-config.json"
            if not q6_cfg_path.exists():
                q6_bad.append(f"{q6_slot.name}/{q6_bdir.name}:no-config")
                continue
            q6_cfg = json.loads(q6_cfg_path.read_text())
            q6_checked += 1
            if q6_cfg.get("evaluationOnly") is not True:
                q6_bad.append(f"{q6_slot.name}/{q6_bdir.name}:updates-enabled")
            elif q6_cfg.get("derivedFromCheckpointHash") != q6_final:
                q6_bad.append(f"{q6_slot.name}/{q6_bdir.name}:checkpoint-mismatch")
    if q6_checked == 0:
        q6_msg = check("Q6 freeze config [flags-only]", False, "no collected cases to verify" if not q5_slots else "no branch configs found")
    else:
        q6_msg = check(
            "Q6 freeze config [flags-only]",
            not q6_bad,
            f"{q6_checked} branches evaluationOnly + bound to final checkpoint {None if q6_final is None else q6_final[:24]}…, bad={q6_bad[:3]}",
        )
    mo.md(f"**{q6_msg}**")
    return (q6_final,)


@app.cell
def _(mo, verdicts, RUN_AT):
    qv_rows = ["| check | result | detail |", "| --- | --- | --- |"]
    qv_rows += [f"| {qn} | {'PASS' if qo else 'FAIL'} | {qd} |" for qn, qo, qd in verdicts]
    qv_failed = [qn for qn, qo, _qd in verdicts if not qo]
    mo.md(f"### Verdicts (run at {RUN_AT})\n\n" + "\n".join(qv_rows))
    print(f"verdicts timestamp: {RUN_AT}", flush=True)
    if qv_failed:
        raise SystemExit(f"QUALIFICATION FAILED at {RUN_AT}: {', '.join(qv_failed)}")
    print(f"ALL QUALIFICATION CHECKS PASSED at {RUN_AT}", flush=True)
    mo.md("**ALL QUALIFICATION CHECKS PASSED**")
    return qv_failed, qv_rows


if __name__ == "__main__":
    app.run()
