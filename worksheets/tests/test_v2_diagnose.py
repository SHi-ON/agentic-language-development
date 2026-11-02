"""A7 driver agreement: the imported verdict() matches the sealed
diagnostic on the genuine fixtures and the 240 retained sham pairs.

Reads the sealed paper inputs from the resolved paper checkout (the
driver cannot run without it, so a missing checkout fails instead of
skipping). Sealed-input hashes are pinned: a silent swap of the
agreement corpus fails before any verdict runs.
"""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import unittest

WORKSHEETS = Path(__file__).resolve().parents[1]
FORK_ROOT = WORKSHEETS.parent


def load(name):
    spec = importlib.util.spec_from_file_location(name, WORKSHEETS / f"{name}.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


driver = load("ala-v2-diagnose")

PINNED_ROWS_SHA256 = "68781c2bad2ba4f93bb4a08f8b8e2add8323de745502ee539709c9af2217f0b4"
PINNED_MANIFEST_SHA256 = "77d0ba927ab24ee125b720c87d17dfba345016904bb0e4b8b3a17c7698b9809c"
GENUINE_FIXTURES = ("valid_indomain", "collision", "repeated_content")


def sha256_of(path):
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(65536), b""):
            digest.update(chunk)
    return digest.hexdigest()


class DriverAgreementTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        try:
            cls.paper = driver.resolve_paper_repo(os.environ.get("PAPER_REPO"))
        except SystemExit as exc:
            raise AssertionError(f"paper checkout missing, agreement unverifiable: {exc}")
        cls.rows_path = cls.paper / "results" / "inputs" / "treatment-rows.json"
        cls.manifest_path = cls.paper / "results" / "inputs" / "diagnostic-manifest.json"
        if not cls.rows_path.is_file() or not cls.manifest_path.is_file():
            raise AssertionError("sealed diagnostic inputs are not in the paper checkout")
        # staticmethod: a bare function stored on the class would bind
        # self as a phantom first verdict() argument.
        cls.verdict = staticmethod(driver.load_verdict(cls.paper))
        cls.rows_doc = json.loads(cls.rows_path.read_text())
        cls.manifest = json.loads(cls.manifest_path.read_text())

    def test_sealed_inputs_match_pins(self):
        self.assertEqual(sha256_of(self.rows_path), PINNED_ROWS_SHA256)
        self.assertEqual(sha256_of(self.manifest_path), PINNED_MANIFEST_SHA256)
        self.assertEqual(self.rows_doc.get("schema"), "ala-treatment-rows.v2")
        self.assertEqual(self.manifest.get("schema"), "ala-diagnostic-manifest.v1")

    def test_genuine_fixtures_accept_with_expected_counters(self):
        for name in GENUINE_FIXTURES:
            with self.subTest(fixture=name):
                ok, _reasons, collisions, repeated = self.verdict(
                    self.manifest["fixtures"][name]["cases"])
                want = self.manifest["expected"][name]
                self.assertTrue(want["ok"])
                self.assertTrue(ok)
                self.assertEqual(collisions, want["genuineCollisions"])
                self.assertEqual(repeated, want["repeatedContent"])

    def test_retained_sham_pairs_all_reject(self):
        rows = self.rows_doc["rows"]
        self.assertEqual(len(rows), 240)
        for position, row in enumerate(rows):
            with self.subTest(slot=position):
                ok, reasons, _collisions, _repeated = self.verdict(row["cases"])
                self.assertFalse(ok)
                joined = "\n".join(reasons)
                self.assertTrue(
                    "sham shuffle" in joined or "one distinct scheduled case" in joined,
                    f"slot {position} rejected without a sham reason: {reasons[:1]}")

    def test_six_batch_envelope_diagnoses_each_cell(self):
        genuine = self.manifest["fixtures"]["valid_indomain"]["cases"]
        blocks = [
            {"partition": partition, "role": role, "scheduledCases": 2,
             "records": len(genuine), "cases": genuine}
            for partition in driver.PARTITIONS for role in driver.ROLES
        ]
        doc = {"schema": "ala-v2-diagnostic-rows/v1", "dyad": "fixture",
               "extractedAt": "t", "blocks": blocks}
        results, problems = driver.diagnose_rows(doc, self.verdict)
        self.assertEqual(problems, [])
        self.assertEqual(len(results), 6)
        for item in results:
            self.assertTrue(item["ok"])
            self.assertEqual(item["genuineCollisions"], 0)
            self.assertEqual(item["repeatedContent"], 0)

    def test_envelope_fails_closed(self):
        genuine = self.manifest["fixtures"]["valid_indomain"]["cases"]

        def cell(partition, role, cases):
            return {"partition": partition, "role": role, "scheduledCases": 2,
                    "records": len(cases), "cases": cases}

        full = [cell(p, r, genuine) for p in driver.PARTITIONS for r in driver.ROLES]
        _results, problems = driver.diagnose_rows(
            {"schema": "wrong/v1", "blocks": full}, self.verdict)
        self.assertTrue(any("schema drifted" in p for p in problems))
        _results, problems = driver.diagnose_rows(
            {"schema": driver.ROWS_SCHEMA, "blocks": full[:5]}, self.verdict)
        self.assertTrue(any("want 6" in p for p in problems))
        short = [cell(p, r, []) if (p, r) == ("validation-fit", "baby-a")
                 else cell(p, r, genuine)
                 for p in driver.PARTITIONS for r in driver.ROLES]
        _results, problems = driver.diagnose_rows(
            {"schema": driver.ROWS_SCHEMA, "blocks": short}, self.verdict)
        self.assertTrue(any("no diagnostic cases" in p for p in problems))
        dupe = [cell("validation-fit", "baby-a", genuine) for _ in range(6)]
        _results, problems = driver.diagnose_rows(
            {"schema": driver.ROWS_SCHEMA, "blocks": dupe}, self.verdict)
        self.assertTrue(any("repeats grid cell" in p for p in problems))

    def test_missing_paper_repo_refuses(self):
        saved_env = os.environ.get("PAPER_REPO")
        saved_root = driver.ROOT
        try:
            if "PAPER_REPO" in os.environ:
                del os.environ["PAPER_REPO"]
            driver.ROOT = Path("/nonexistent-fork-root-ald-a7")
            with self.assertRaises(SystemExit):
                driver.resolve_paper_repo("/nonexistent-paper-repo-ald-a7")
        finally:
            if saved_env is not None:
                os.environ["PAPER_REPO"] = saved_env
            driver.ROOT = saved_root


if __name__ == "__main__":
    unittest.main()
