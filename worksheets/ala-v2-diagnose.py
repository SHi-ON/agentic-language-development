#!/usr/bin/env python3
"""A7 driver: batch-level verdicts over extracted v2 dyad batches.

Reads the extractor's diagnostic-rows.json (one whole-dyad batch per
partition x role, six batches) and calls the paper diagnostic's own
verdict() once per batch. verdict() is IMPORTED from the paper repo,
never reimplemented: any semantic drift fails at import or on the
pinned sealed-input hashes in the agreement tests, not silently.

Fail-closed: rows schema drift, a missing/extra batch, an uncovered
grid cell, an empty batch, or an unresolvable paper repo exits 1 with
no verdicts. Verdict outcomes themselves are reported (exit 0) unless
--require pins an expectation for gate use (exit 2 on violation).

Writes a verdicts JSON report (--out) with per-batch ok/reasons plus
genuine-collision and repeated-content counters.
"""
import argparse
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ROWS_SCHEMA = "ala-v2-diagnostic-rows/v1"
PARTITIONS = ("validation-fit", "validation-selection", "within-support-test")
ROLES = ("baby-a", "baby-b")


def fail(message):
    print(f"DIAGNOSE FAIL: {message}")
    raise SystemExit(1)


def resolve_paper_repo(explicit):
    """Paper checkout: explicit flag, $PAPER_REPO, then the sibling."""
    import os

    candidates = [explicit, os.environ.get("PAPER_REPO"),
                  str(ROOT.parent / "agentic-language-development-paper")]
    for candidate in candidates:
        if not candidate:
            continue
        repo = Path(candidate)
        module = repo / "scripts" / "treatment_diagnostic.py"
        if module.is_file():
            return repo
    fail("paper repo with scripts/treatment_diagnostic.py not found "
         "(--paper-repo, $PAPER_REPO, or the sibling checkout)")


def load_verdict(paper_repo):
    sys.path.insert(0, str(Path(paper_repo) / "scripts"))
    try:
        import treatment_diagnostic
    except ImportError as exc:
        fail(f"cannot import the paper verdict(): {exc}")
    verdict = getattr(treatment_diagnostic, "verdict", None)
    if not callable(verdict):
        fail("the paper module has no callable verdict()")
    return verdict


def diagnose_rows(doc, verdict):
    """Run one verdict call per batch. Returns (results, problems)."""
    if not isinstance(doc, dict) or doc.get("schema") != ROWS_SCHEMA:
        return ([], [f"rows schema drifted: {doc.get('schema')!r} "
                     f"(want {ROWS_SCHEMA!r})"])
    blocks = doc.get("blocks")
    if not isinstance(blocks, list) or len(blocks) != 6:
        count = len(blocks) if isinstance(blocks, list) else type(blocks).__name__
        return ([], [f"rows carry {count} batches, want 6"])
    want_cells = {(p, r) for p in PARTITIONS for r in ROLES}
    got_cells = set()
    results = []
    problems = []
    for position, block in enumerate(blocks):
        where = f"batch {position}"
        if not isinstance(block, dict):
            problems.append(f"{where} is not an object")
            continue
        cell = (block.get("partition"), block.get("role"))
        if cell in got_cells:
            problems.append(f"{where} repeats grid cell {cell}")
        got_cells.add(cell)
        cases = block.get("cases")
        if not isinstance(cases, list) or not cases:
            problems.append(f"{where} {cell} carries no diagnostic cases")
            continue
        try:
            ok, reasons, collisions, repeated = verdict(cases)
        except (KeyError, TypeError, AttributeError) as exc:
            problems.append(f"{where} {cell} shape error: {exc}")
            continue
        results.append({
            "partition": cell[0],
            "role": cell[1],
            "scheduledCases": block.get("scheduledCases"),
            "records": block.get("records"),
            "ok": bool(ok),
            "reasons": list(reasons),
            "genuineCollisions": int(collisions),
            "repeatedContent": int(repeated),
        })
    missing = sorted(want_cells - got_cells)
    if missing:
        problems.append(f"grid cells uncovered: {missing}")
    if problems:
        return ([], problems)
    return (results, [])


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rows", required=True)
    parser.add_argument("--paper-repo", default=None)
    parser.add_argument("--out", required=True)
    parser.add_argument("--require", choices=("accept", "reject"), default=None)
    args = parser.parse_args(argv)
    rows_path = Path(args.rows)
    if not rows_path.is_file():
        fail(f"rows file not found: {rows_path}")
    try:
        doc = json.loads(rows_path.read_text())
    except json.JSONDecodeError as exc:
        fail(f"rows file is not valid JSON: {exc}")
    verdict = load_verdict(resolve_paper_repo(args.paper_repo))
    results, problems = diagnose_rows(doc, verdict)
    if problems:
        for problem in problems:
            print(f"DIAGNOSE FAIL: {problem}")
        return 1
    accepts = sum(1 for r in results if r["ok"])
    collisions = sum(r["genuineCollisions"] for r in results)
    repeated = sum(r["repeatedContent"] for r in results)
    report = {
        "schema": "ala-v2-diagnostic-verdicts/v1",
        "dyad": doc.get("dyad"),
        "batches": len(results),
        "accepts": accepts,
        "genuineCollisions": collisions,
        "repeatedContent": repeated,
        "results": results,
    }
    out_path = Path(args.out)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(report, indent=2) + "\n")
    for item in results:
        status = "ACCEPT" if item["ok"] else "REJECT"
        print(f"{item['partition']}:{item['role']}: {status} "
              f"({item['genuineCollisions']} genuine collisions, "
              f"{item['repeatedContent']} repeated-content pairs)")
    print(f"DIAGNOSED: {accepts}/{len(results)} batches accepted, "
          f"{collisions} genuine collisions, {repeated} repeated-content "
          f"pairs -> {out_path}")
    if args.require == "accept" and accepts != len(results):
        print("DIAGNOSE FAIL: --require accept violated")
        return 2
    if args.require == "reject" and accepts != 0:
        print("DIAGNOSE FAIL: --require reject violated")
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
