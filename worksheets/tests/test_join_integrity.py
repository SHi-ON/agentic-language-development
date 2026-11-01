"""Join-integrity regressions for the worksheet extractors/censuses.

Covers the silent-mislabel failure modes fixed 2026-11-01: batch-order
treatment labels, sender-blind training-token joins, duplicate keys,
and last-match turn-0 projection. No private corpus is read.
"""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

WORKSHEETS = Path(__file__).resolve().parents[1]


def load(name):
    spec = importlib.util.spec_from_file_location(name, WORKSHEETS / f"{name}.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def intention(turn, symbol):
    return json.dumps({"eventType": "intention.recorded", "turn": turn,
                       "content": {"symbols": [symbol]}})


class JoinIntegrityTests(unittest.TestCase):
    def test_label_cases_ignores_batch_order(self):
        mod = load("ala-message-extract")
        shuffled = {"caseId": "s0001-ledger-shuffled:turn:0", "selectedToken": "TS"}
        consistent = {"caseId": "s0001-ledger-consistent:turn:0", "selectedToken": "TC"}
        a, b = mod.label_cases([shuffled, consistent])
        self.assertIs(a, consistent)
        self.assertIs(b, shuffled)

    def test_label_cases_rejects_unknown_branch(self):
        mod = load("ala-message-extract")
        with self.assertRaises(SystemExit):
            mod.label_cases([{"caseId": "s0001-normal:turn:0"},
                             {"caseId": "s0001-ledger-shuffled:turn:0"}])

    def test_turn0_symbol_multiplicity_fails_closed(self):
        mod = load("ala-message-extract")
        self.assertEqual(mod.turn0_symbol([intention(0, "T01")]), "T01")
        self.assertIsNone(mod.turn0_symbol([intention(1, "T01")]))
        with self.assertRaises(SystemExit):
            mod.turn0_symbol([intention(0, "T01"), intention(0, "T02")])

    def test_training_symbols_keyed_by_sender(self):
        mod = load("ala-message-extract")
        with tempfile.TemporaryDirectory() as tmp:
            parent = Path(tmp)
            (parent / "baby-a-ledger.jsonl").write_text(intention(3, "TA") + "\n")
            (parent / "baby-b-ledger.jsonl").write_text(intention(3, "TB") + "\n")
            symbols = mod.training_symbols(parent)
        self.assertEqual(symbols[("baby-a", 3)], "TA")
        self.assertEqual(symbols[("baby-b", 3)], "TB")

    def test_training_symbols_duplicate_fails_closed(self):
        mod = load("ala-message-extract")
        with tempfile.TemporaryDirectory() as tmp:
            parent = Path(tmp)
            (parent / "baby-a-ledger.jsonl").write_text(
                intention(3, "TA") + "\n" + intention(3, "TA") + "\n")
            (parent / "baby-b-ledger.jsonl").write_text("")
            with self.assertRaises(SystemExit):
                mod.training_symbols(parent)

    def test_duplication_training_join_uses_sender(self):
        mod = load("ala-duplication-census")
        with tempfile.TemporaryDirectory() as tmp:
            cal = Path(tmp) / "primary-0002" / "parent-bundle"
            cal.mkdir(parents=True)
            (cal / "turn-records.jsonl").write_text(
                json.dumps({"phase": "running", "turn": 0, "scenarioStateHash": "h0",
                            "roles": {"sender": "baby-b"}}) + "\n" +
                json.dumps({"phase": "running", "turn": 1, "scenarioStateHash": "h1",
                            "roles": {"sender": "baby-a"}}) + "\n")
            (cal / "baby-a-ledger.jsonl").write_text(
                intention(0, "WRONG") + "\n" + intention(1, "TA") + "\n")
            (cal / "baby-b-ledger.jsonl").write_text(intention(0, "TB") + "\n")
            mod.CAL = Path(tmp)
            rows = mod.training_tokens("primary-0002")
        self.assertEqual([r["token"] for r in rows], ["TB", "TA"])

    def test_index_extract_duplicate_fails(self):
        mod = load("ala-intention-census")
        with self.assertRaises(SystemExit):
            mod.index_extract([{"slot": "slot-0001"}, {"slot": "slot-0001"}])


if __name__ == "__main__":
    unittest.main()
