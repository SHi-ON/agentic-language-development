"""Exercise both extractors without the private corpus or any training."""
import contextlib
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest

WORKSHEETS = Path(__file__).resolve().parents[1]
ELIGIBLE = [1, 2, 3, 4, 6, 7, 8, 9, 11, 12, 13, 14]


class RetainedIdentityTests(unittest.TestCase):
    def check_extractors(self, mutation=None, accepted=False):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            extract_rows = []
            for i in range(240):
                slot = root / f"slot-{i + 1:04d}"
                slot.mkdir()
                role = "baby-a" if i < 120 else "baby-b"
                target = ELIGIBLE[i % 120 % 12]
                candidates = [target] + [t for t in ELIGIBLE if t != target][:3]
                schedule = {"executedCaseId": f"within-support-test:{role}:{i % 120:04d}",
                            "commitment": "sha256:fixture-schedule"}
                cases = [{"caseId": f"{slot.name}-{branch}:turn:0", "receiverRole": role,
                          "targetTypeCode": target, "candidateTypeCodes": candidates,
                          "selectedToken": "T05", "shuffledDeliveredToken": "T05",
                          "shuffledSourceCaseId": f"{slot.name}-{other}:turn:0",
                          "identicalTokenCoincidence": True}
                         for branch, other in (("ledger-consistent", "ledger-shuffled"),
                                               ("ledger-shuffled", "ledger-consistent"))]
                if mutation and i == 239:
                    mutation(schedule, cases)
                (slot / "schedule.json").write_text(json.dumps(schedule))
                (slot / "treatment-batch.json").write_text(json.dumps({
                    "cases": cases, "batchCommitment": "batch", "derangementSeed": "seed"}))
                extract_rows.append({"slot": slot.name, "branchTokens": {
                    "normal": "T05", "random": "T00", "shuffled": "T05"}, "seeds": ["seed"]})
            extract = root / "extract.json"
            extract.write_text(json.dumps({"testRows": extract_rows}))
            for name in ("ala-treatment-rows", "ala-schedule-coverage"):
                with self.subTest(extractor=name):
                    spec = importlib.util.spec_from_file_location(name, WORKSHEETS / f"{name}.py")
                    module = importlib.util.module_from_spec(spec)
                    spec.loader.exec_module(module)
                    module.DYAD = root
                    module.EXTRACT = extract
                    module.OUT = root / f"{name}.json"
                    with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
                        result = module.main()
                    self.assertEqual(result, 0 if accepted else 1)
                    self.assertEqual(module.OUT.exists(), accepted)

    def test_complete_fixture(self):
        self.check_extractors(accepted=True)

    def test_unknown_schedule_id(self):
        self.check_extractors(lambda s, c: s.update(executedCaseId="within-support-test:baby-b:9999"))

    def test_second_branch_role(self):
        self.check_extractors(lambda s, c: c[1].update(receiverRole="baby-a"))

    def test_wrong_scheduled_target(self):
        self.check_extractors(lambda s, c: [r.update(targetTypeCode=1) for r in c])

    def test_candidate_order_disagreement(self):
        self.check_extractors(lambda s, c: c[1].update(candidateTypeCodes=list(reversed(c[0]["candidateTypeCodes"]))))

    def test_repeated_identity_conflict(self):
        self.check_extractors(lambda s, c: s.update(executedCaseId="within-support-test:baby-b:0000"))

    def test_schedule_commitment_disagreement(self):
        self.check_extractors(lambda s, c: s.update(commitment="sha256:different-schedule"))


if __name__ == "__main__":
    unittest.main()
