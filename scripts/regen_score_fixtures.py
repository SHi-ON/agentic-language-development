#!/usr/bin/env python3
"""Regenerate the locked-mean-anchored score fixtures byte-identically.

Named regen command (P0-FORK-12): run via scripts/regen-score-fixtures.sh,
which also runs the score_fixture suite as a self-check.

Locked means (pinned here):
  A: 0.12497749132421103
  B: 0.12302667184362459
Provenance: fixtures/README.md previously derived these from
reports/ala-campaign-results/analysis-{A,B}.json, which does not exist in
this repo. The original reports/ source is missing, so the values are now
pinned here; the native distributions below are SOLVED from the means (not
transcribed), and the script asserts the solved Brier reproduces each mean.

Derivation: each dyad selects the uniform ordinary predictor (ordinary
Brier exactly 0.75) and every native distribution has the form [a,b,b,b]
with action 0, so native Brier = (a-1)^2 + 3b^2 = (4/3)(1-a)^2 with
a + 3b = 1. Setting native Brier = 0.75 - mean gives
a = 1 - sqrt((0.75 - mean) * 0.75), b = (1 - a) / 3.

Output: crates/ald-cli/tests/fixtures/cohort-{A,B}/primary-0000/
(role-folds + branch-probabilities per role) in serde_json pretty format
(1-space indent). The committed files must be byte-identical to a fresh
regen: `git diff` is empty after running.
"""

import struct
import sys
from pathlib import Path

LOCKED_MEANS = {
    "A": 0.12497749132421103,
    "B": 0.12302667184362459,
}

ROLES = ["agent-0", "agent-1"]
BRANCH_ORDER = ["normal", "disabled", "constant", "random", "shuffled"]
UNIFORM4 = [0.25, 0.25, 0.25, 0.25]
SHARED_DRAW = 0.1


def solve_native(mean):
    """Solve [a, b, b, b] with action 0 so Brier == 0.75 - mean."""
    target = 0.75 - mean
    a = 1.0 - ((target * 0.75) ** 0.5)
    b = (1.0 - a) / 3.0
    native = [a, b, b, b]
    brier = sum((v - (1.0 if i == 0 else 0.0)) ** 2 for i, v in enumerate(native))
    assert abs(brier - target) < 1e-15, (brier, target)
    assert abs(sum(native) - 1.0) < 1e-15
    return native


def draw_bits_hex(draw):
    return format(struct.unpack("<Q", struct.pack("<d", draw))[0], "016x")


def emit(value, indent):
    """serde_json::to_string_pretty subset: 1-space indent, shortest floats."""
    pad = " " * indent
    if value is None:
        return "null"
    if value is True:
        return "true"
    if value is False:
        return "false"
    if isinstance(value, int):
        return str(value)
    if isinstance(value, float):
        return repr(value)
    if isinstance(value, str):
        out = ['"']
        for ch in value:
            if ch == '"':
                out.append('\\"')
            elif ch == "\\":
                out.append("\\\\")
            elif ch == "\n":
                out.append("\\n")
            elif ord(ch) < 0x20:
                out.append("\\u%04x" % ord(ch))
            else:
                out.append(ch)
        out.append('"')
        return "".join(out)
    if isinstance(value, list):
        if not value:
            return "[]"
        inner = (",\n").join(pad + " " + emit(v, indent + 1) for v in value)
        return "[\n" + inner + "\n" + pad + "]"
    if isinstance(value, dict):
        if not value:
            return "{}"
        inner = (",\n").join(
            pad + " " + emit(k, 0) + ": " + emit(v, indent + 1)
            for k, v in value.items()
        )
        return "{\n" + inner + "\n" + pad + "}"
    raise TypeError(type(value))


def record(cohort, role, case_id, token, action):
    return {
        "case_id": case_id,
        "receiver_role": role,
        "delivered_token": token,
        "candidate_type_codes": [0, 1, 2, 3],
        "actual_selected_candidate_index": action,
    }


def role_folds(cohort, role, native):
    fits = [
        record(cohort, role, "%s-%s-fit-%d" % (cohort, role, i), None, 0)
        for i in range(4)
    ]
    sels = [
        record(cohort, role, "%s-%s-sel-%d" % (cohort, role, i), "tok31", 1)
        for i in range(2)
    ]
    tests = []
    for i in range(2):
        case_id = "%s-%s-test-%d" % (cohort, role, i)
        tests.append(
            {
                "record": record(cohort, role, case_id, "tok05", 0),
                "native_distribution": list(native),
                "replay_distribution": [1.0, 0.0, 0.0, 0.0],
                "target_index": 0,
                "native_missing": False,
                "branch_distributions": {
                    b: (list(native) if b == "normal" else list(UNIFORM4))
                    for b in BRANCH_ORDER
                },
                "branch_success": {b: (b == "normal") for b in BRANCH_ORDER},
            }
        )
    return {
        "role": role,
        "training": [],
        "validation_fit": fits,
        "validation_selection": sels,
        "test": tests,
    }


def branch_probabilities(cohort, role, native):
    cases = []
    for i in range(2):
        case_id = "%s-%s-test-%d" % (cohort, role, i)
        cases.append(
            {
                "caseId": case_id,
                "targetIndex": 0,
                "deliveredToken": "tok05",
                "nativeMissing": False,
                "native": list(native),
                "replay": [1.0, 0.0, 0.0, 0.0],
                "sharedDraw": SHARED_DRAW,
                "drawBitsHex": draw_bits_hex(SHARED_DRAW),
                "branches": {
                    b: {
                        "probabilities": (
                            list(native) if b == "normal" else list(UNIFORM4)
                        ),
                        "success": (b == "normal"),
                    }
                    for b in BRANCH_ORDER
                },
            }
        )
    return cases


def main():
    root = Path(__file__).resolve().parent.parent
    fixtures = root / "crates/ald-cli/tests/fixtures"
    for cohort, mean in LOCKED_MEANS.items():
        native = solve_native(mean)
        for role in ROLES:
            dyad = fixtures / ("cohort-%s" % cohort) / "primary-0000"
            dyad.mkdir(parents=True, exist_ok=True)
            (dyad / ("role-folds-%s.json" % role)).write_text(
                emit(role_folds(cohort, role, native), 0) + "\n"
            )
            (dyad / ("branch-probabilities-%s.json" % role)).write_text(
                emit(branch_probabilities(cohort, role, native), 0) + "\n"
            )
    print("fixtures regenerated for cohorts: %s" % ", ".join(LOCKED_MEANS))


if __name__ == "__main__":
    sys.exit(main())
