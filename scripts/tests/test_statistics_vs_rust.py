#!/usr/bin/env python3
"""Cross-check validate-statistics.py reference rows against Rust ald-analysis.

Normative anchor: Rust values (locked by ald-analysis unit tests named
below) plus stated tolerances. Run with:
    uv run --locked python3 -m unittest discover -s scripts/tests -v

Bridge contract: each comparison pins the SAME reference value on both
sides. If Rust changes a value, its own unit test fails first — update
both sides together, never one alone.

Covered (script fn -> Rust fn -> tolerance):
  mc_interval -> stats::wilson_interval (1e-4, quoted endpoints)
  quantile_type7 -> stats::quantile_sorted (1e-15)
  holm_adjust verdicts -> stats::holm_bonferroni flags (exact)
  row mean/sd -> stats::summarize (1e-15 / 1e-12)
  norm.ppf/t.ppf/t.cdf rows -> special::{normal_quantile,
    student_t_quantile, student_t_cdf} (1e-8 / 1e-9 / 1e-9)
Not comparable (documented, no Rust counterpart or different RNG/output):
  one-sample-t/tost p-values (Rust returns intervals, not p-values),
  percentile bootstrap (SeededPrng vs PCG64: distribution-only),
  beta-binomial fit, entropy/MI, Welch, binomial tail (design-only rows).

Also checks the committed receipt reports/research/statistical-validation.tsv
exists with the expected reference rows (e03 registrationFacingReceipt).
"""

import importlib.util
import math
import os
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
SCRIPT = os.path.join(ROOT, "scripts", "validate-statistics.py")
RECEIPT = os.path.join(ROOT, "reports", "research", "statistical-validation.tsv")


def load_module():
    spec = importlib.util.spec_from_file_location("validate_statistics", SCRIPT)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


vs = load_module()


def read_receipt_rows():
    rows = {}
    with open(RECEIPT, encoding="utf-8") as handle:
        header = handle.readline().rstrip("\n").split("\t")
        assert header == [
            "category",
            "case",
            "metric",
            "value",
            "mc_n",
            "lower95",
            "upper95",
        ], header
        for line in handle:
            parts = line.rstrip("\n").split("\t")
            rows[(parts[0], parts[1], parts[2])] = parts[3]
    return rows


class TestStatisticsVsRust(unittest.TestCase):
    def test_wilson_matches_rust(self):
        # Rust side: stats::tests::wilson_reference (center 0.71674,
        # lower 0.49016, upper 0.94332 @1e-4).
        lo, hi = vs.mc_interval(8, 10)
        self.assertAlmostEqual(lo, 0.49016, delta=1e-4)
        self.assertAlmostEqual(hi, 0.94332, delta=1e-4)

    def test_quantile_type7_matches_rust(self):
        # Rust side: stats::tests::crosscheck_values_for_python_port.
        self.assertAlmostEqual(vs.quantile_type7([0, 1, 2, 3, 4], 0.025), 0.1, delta=1e-15)
        self.assertAlmostEqual(vs.quantile_type7([0, 1, 2, 3, 4], 0.975), 3.9, delta=1e-15)

    def test_holm_verdicts_match_rust(self):
        # Rust side: stats::tests::holm_basic ([0.01, 0.04, 0.5] ->
        # [True, False, False]); script returns adjusted p-values, so
        # compare verdicts at alpha = 0.05.
        adjusted = vs.holm_adjust([0.01, 0.04, 0.5])
        self.assertEqual([bool(v <= 0.05) for v in adjusted], [True, False, False])

    def test_mean_sd_match_rust(self):
        # Rust side: stats::tests::crosscheck_values_for_python_port.
        import numpy as np

        sample = [0.24, 0.26, 0.25, 0.23, 0.27]
        self.assertAlmostEqual(float(np.mean(sample)), 0.25, delta=1e-15)
        self.assertAlmostEqual(
            float(vs.row_sd([sample])[0]), 0.015811388300841896, delta=1e-12
        )

    def test_special_functions_match_rust(self):
        # Rust side: special::tests::crosscheck_values_for_python_port.
        from scipy import stats

        self.assertAlmostEqual(
            float(stats.norm.ppf(0.975)), 1.959963984540054, delta=1e-12
        )
        self.assertAlmostEqual(
            float(stats.t.ppf(0.975, 10)), 2.228138851986274, delta=1e-12
        )
        self.assertAlmostEqual(
            float(stats.t.cdf(2.228138851964, 10)), 0.9749999999990558, delta=1e-12
        )

    def test_receipt_exists_with_expected_reference_rows(self):
        self.assertTrue(os.path.isfile(RECEIPT), RECEIPT)
        rows = read_receipt_rows()
        expected = {
            ("reference", "normal", "qnorm-0.975"): (1.959963984540054, 1e-12),
            ("reference", "student-t", "qt-0.975-df10"): (2.228138851986274, 1e-12),
            ("reference", "student-t", "pt-2.228138851964-df10"): (
                0.9749999999990558,
                1e-12,
            ),
            ("reference", "wilson-8-of-10", "lower"): (0.49016, 1e-4),
            ("reference", "wilson-8-of-10", "upper"): (0.94332, 1e-4),
            ("reference", "quantile-type7", "q-0.025"): (0.1, 1e-12),
            ("reference", "quantile-type7", "q-0.975"): (3.9, 1e-12),
        }
        for key, (want, tol) in expected.items():
            self.assertIn(key, rows)
            self.assertAlmostEqual(float(rows[key]), want, delta=tol, msg=str(key))
        # MC rows are structural only (seed-sensitive by design).
        mc_keys = [k for k in rows if k[0] in ("coverage", "type-i", "clustering")]
        self.assertTrue(mc_keys)
        for key in mc_keys:
            self.assertTrue(math.isfinite(float(rows[key])), key)


if __name__ == "__main__":
    unittest.main()
