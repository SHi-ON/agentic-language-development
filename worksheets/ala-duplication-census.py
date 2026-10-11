#!/usr/bin/env python3
"""ALA-LV R04 duplication census (pinned worksheet env, read-only).

Asks how often exact-duplicate messages occur in retained calibration
evidence, partitioned by sender role (training), message treatment
(test branches), and candidate/scenario overlap. Training messages come
from parent-ledger intention records joined to turn scenario hashes;
test messages come from branch-ledger turn-0 intention records
(treatment-batch.json supplies receiver role and candidate sets only).
Missing turn-0 intentions are recorded per slot/branch (disabled
branches are expected-null, never missing). N1.5: this docstring was
corrected 2026-10-10; the old text inverted the test-token sourcing.

Writes reports/research/ala-duplication-census.v1.json. No study slots
are consumed; no new runs are launched.

Run: .venv/bin/python worksheets/ala-duplication-census.py
"""

import json
import sys
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CAL = ROOT / "evidence/ala-lv/calibration"
OUT = ROOT / "reports/research/ala-duplication-census.v1.json"


def training_tokens(dyad: str):
    parent = CAL / dyad / "parent-bundle"
    turns = [json.loads(line) for line in (parent / "turn-records.jsonl").read_text().splitlines()]
    scen = {}
    sender = {}
    for t in turns:
        if t.get("phase") == "running":
            scen[t["turn"]] = t.get("scenarioStateHash")
            sender[t["turn"]] = (t.get("roles") or {}).get("sender")
    symbols = {}
    for baby in ("baby-a", "baby-b"):
        for line in (parent / f"{baby}-ledger.jsonl").read_text().splitlines():
            e = json.loads(line)
            if e.get("eventType") == "intention.recorded" and isinstance(e.get("turn"), int):
                syms = ((e.get("content") or {}).get("symbols") or [None])
                symbols[e["turn"]] = syms[0]
    skipped = sorted(n for n in scen if symbols.get(n) is None)
    if skipped:
        # Fail closed: silently dropping running turns would bias every
        # training rate below. Current data joins 3000/3000 on both dyads.
        raise SystemExit(f"FAIL: {len(skipped)} running turns lack recorded symbols: {skipped[:5]}")
    rows = [
        {"turn": n, "token": symbols.get(n), "scenario": scen.get(n), "sender": sender.get(n)}
        for n in sorted(scen)
    ]
    return rows


def census_training(rows: list[dict]) -> dict:
    toks = [r["token"] for r in rows]
    counts = Counter(toks)
    top = counts.most_common(5)
    adjacent = sum(1 for a, b in zip(toks, toks[1:]) if a == b)
    seen: dict[str, set] = {}
    diff_scen = 0
    for r in rows:
        prior = seen.setdefault(r["token"], set())
        if prior and r["scenario"] not in prior:
            diff_scen += 1
        prior.add(r["scenario"])
    by_sender = {}
    for who in ("baby-a", "baby-b"):
        sub = [r["token"] for r in rows if r["sender"] == who]
        by_sender[who] = {"n": len(sub), "distinct": len(set(sub)),
                          "top1": Counter(sub).most_common(1)[0] if sub else None}
    return {"n": len(rows), "distinctTokens": len(counts),
            "top5": [[t, c] for t, c in top],
            "top1Share": (top[0][1] / len(rows)) if rows else 0.0,
            "adjacentRepeatRate": (adjacent / (len(rows) - 1)) if len(rows) > 1 else 0.0,
            "sameTokenDifferentScenario": diff_scen,
            "sameTokenDifferentScenarioRate": (diff_scen / len(rows)) if rows else 0.0,
            "bySender": by_sender}


def census_test() -> dict:
    # Delivered tokens come from each branch run's RECEIVER-ledger
    # turn-0 intention record (treatment-batch.json cases cover only the
    # two ledger treatments, so the batch cannot source all 7 branches).
    dyad = CAL / "primary-0002"
    slots = sorted(p for p in dyad.glob("slot-*") if p.is_dir())
    per_treatment: dict[str, Counter] = {}
    nulls: dict[str, int] = Counter()
    within_dup = 0
    pair_counter: Counter = Counter()
    missing_intention = []
    cand_sets: list[tuple[str, tuple]] = []
    for slot in slots:
        batch = json.loads((slot / "treatment-batch.json").read_text())
        receivers = {c["receiverRole"] for c in batch["cases"]}
        assert len(receivers) == 1, f"{slot.name} mixed receivers"
        baby = next(iter(receivers))
        cand_sets.append((slot.name, tuple(sorted(batch["cases"][0]["candidateTypeCodes"]))))
        toks: dict[str, str | None] = {}
        for bdir in sorted((slot / "branches").glob("*")):
            # Run dir: ala-calibration-primary-0002-sNNNN-<branch>; the
            # branch is everything after the slot label (ledger branches
            # contain a hyphen, so rsplit would truncate them).
            branch = bdir.name.split(f"-s{slot.name.split('-')[1]}-", 1)[-1]
            found = None
            for line in (bdir / f"{baby}-ledger.jsonl").read_text().splitlines():
                e = json.loads(line)
                if e.get("eventType") == "intention.recorded" and e.get("turn") == 0:
                    found = ((e.get("content") or {}).get("symbols") or [None])[0]
            if found is None and branch != "disabled":
                missing_intention.append(f"{slot.name}/{branch}")
            toks[branch] = found
            per_treatment.setdefault(branch, Counter())[found if found else "NULL"] += 1
            if found is None:
                nulls[branch] += 1
        delivered = [t for t in toks.values() if t]
        if len(set(delivered)) < len(delivered):
            within_dup += 1
        names = sorted(toks)
        for i in range(len(names)):
            for j in range(i + 1, len(names)):
                a, b = toks[names[i]], toks[names[j]]
                if a and b and a == b:
                    pair_counter[f"{names[i]}=={names[j]}"] += 1
    # Candidate-set reuse across cases.
    set_counts = Counter(s for _, s in cand_sets)
    return {"cases": len(slots),
            "perTreatment": {k: {"distinct": len(v), "n": sum(v.values()),
                                 "top3": [[t, c] for t, c in v.most_common(3)],
                                 "nulls": nulls.get(k, 0)} for k, v in per_treatment.items()},
            "casesWithWithinCaseDuplicate": within_dup,
            "distinctCandidateSets": len(set_counts),
            "maxCandidateSetReuse": max(set_counts.values()),
            "topCoincidentPairs": [[p, c] for p, c in pair_counter.most_common(8)],
            "missingIntention": missing_intention}


def main() -> int:
    rows2 = training_tokens("primary-0002")
    rows0 = training_tokens("primary-0000")
    test = census_test()
    out = {
        "censusVersion": "v1",
        "studyId": "ALA-LV",
        "method": ("Read-only joins over retained bundles; training tokens from "
                   "parent-ledger intention.recorded symbols joined to turn scenario "
                   "hashes; test tokens from branch-ledger turn-0 intention records "
                   "(treatment-batch.json supplies receiver role and candidate sets "
                   "only). Missing turn-0 intentions are listed per slot/branch; "
                   "disabled branches are expected-null, never missing. N1.5 "
                   "correction 2026-10-10: prior text inverted the test sourcing."),
        "limits": ["Turn records and transcripts carry hashes only; plaintext comes from ledger intention events.",
                   "Candidate-type overlap for training turns is proxied by scenario-hash (dis)equality.",
                   "primary-0000 is an aborted training-only run; pooled numbers would mix policies, so dyads stay separate.",
                   "Within-case branch coincidences partly reflect N1 control defects (shared seeds, same-example ledger construction), not independent message duplication."],
        "training": {"primary-0002": census_training(rows2), "primary-0000": census_training(rows0)},
        "test": test,
        "interpretation": ("Alphabet reuse over 3000 turns of a fixed 32-symbol inventory is "
                           "expected and is not evidence of duplicated examples. Test-branch "
                           "coincidences are dominated by construction defects (see the "
                           "treatment-validity correction), not by independent message "
                           "duplication. No fitted ordinary baseline exists, so repetition "
                           "effects under a fitted baseline are unmeasured."),
        "selectedOption": ("D2 (ratified): keep uniform-null ordinary, no repetition-sensitive "
                           "modeling; duplication is reported as this census. No fitted ordinary "
                           "exists to poison; whether repetition would matter under a fitted "
                           "baseline is unmeasured and stays an open future question."),
    }
    OUT.write_text(json.dumps(out, indent=1) + "\n")
    t2 = out["training"]["primary-0002"]
    print(f"training-0002: n={t2['n']} distinct={t2['distinctTokens']} "
          f"top1Share={t2['top1Share']:.3f} adjRepeat={t2['adjacentRepeatRate']:.3f} "
          f"diffScenRate={t2['sameTokenDifferentScenarioRate']:.3f}")
    print(f"test: cases={test['cases']} withinDup={test['casesWithWithinCaseDuplicate']} "
          f"distinctCandSets={test['distinctCandidateSets']} "
          f"missingIntention={len(test['missingIntention'])}")
    print(f"wrote {OUT}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
