//! Deterministic semantic-leakage qualification (SPEC §6.5).
//!
//! `SEMANTIC_LEAKAGE_*` constants and the linear-probe decision rule.
//! The probe itself is a pure function over caller-supplied accuracies
//! so offline analysis stays
//! dependency-free.

pub const SEMANTIC_LEAKAGE_ANALYSIS_VERSION: &str = "semantic-leakage-v2";
pub const SEMANTIC_LEAKAGE_CONFIDENCE: f64 = 0.95;
pub const SEMANTIC_LEAKAGE_MAXIMUM_ADVANTAGE: f64 = 0.1;
pub const SEMANTIC_LEAKAGE_MINIMUM_TEST_ROWS: usize = 200;
pub const SEMANTIC_LEAKAGE_POSITIVE_CONTROL_ADVANTAGE: f64 = 0.2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NegativeBoundDecision {
    BelowBound,
    NotBelowBound,
    InsufficientTestRows,
}

#[derive(Debug, Clone)]
pub struct LeakageVerdict {
    pub accuracy_advantage: f64,
    pub advantage_upper_bound: f64,
    pub decision: NegativeBoundDecision,
    pub positive_control_detected: bool,
}

/// Pure decision rule: `advantage = observed - baseline`.
#[must_use]
pub fn evaluate_leakage(
    observed_accuracy: f64,
    baseline_accuracy: f64,
    advantage_upper_bound: f64,
    positive_control_advantage: f64,
    test_rows: usize,
) -> LeakageVerdict {
    let accuracy_advantage = observed_accuracy - baseline_accuracy;
    let decision = if test_rows < SEMANTIC_LEAKAGE_MINIMUM_TEST_ROWS {
        NegativeBoundDecision::InsufficientTestRows
    } else if advantage_upper_bound < SEMANTIC_LEAKAGE_MAXIMUM_ADVANTAGE {
        NegativeBoundDecision::BelowBound
    } else {
        NegativeBoundDecision::NotBelowBound
    };
    LeakageVerdict {
        accuracy_advantage,
        advantage_upper_bound,
        decision,
        positive_control_detected: positive_control_advantage
            >= SEMANTIC_LEAKAGE_POSITIVE_CONTROL_ADVANTAGE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn below_bound_when_tight() {
        let v = evaluate_leakage(0.55, 0.5, 0.09, 0.25, 400);
        assert_eq!(v.decision, NegativeBoundDecision::BelowBound);
        assert!(v.positive_control_detected);
    }

    #[test]
    fn insufficient_rows() {
        let v = evaluate_leakage(0.9, 0.5, 0.5, 0.0, 10);
        assert_eq!(v.decision, NegativeBoundDecision::InsufficientTestRows);
    }
}
