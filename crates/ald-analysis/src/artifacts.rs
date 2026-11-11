//! Dyad artifact readers for `ala analyze select|score`.
//!
//! A scored dyad directory carries per-role folds plus the per-case branch
//! probabilities persisted by `run_dyad`:
//! `role-folds-{agent-0,agent-1}.json` ([`RoleFolds`]) and
//! `branch-probabilities-{agent-0,agent-1}.json` ([`CaseProbabilities`]).

use crate::contrasts::{BranchCase, CONTROL_BRANCHES};
use crate::AnalysisError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const ROLES: [&str; 2] = ["agent-0", "agent-1"];

/// One persisted per-case branch-probability record (mirrors the `run_dyad`
/// writer; camelCase on disk).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchOutcomeArtifact {
    pub probabilities: Vec<f64>,
    pub success: bool,
}

/// Per-case native/replay/branch probabilities for one role.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseProbabilities {
    pub case_id: String,
    pub target_index: usize,
    pub delivered_token: Option<String>,
    pub native_missing: bool,
    pub native: Vec<f64>,
    pub replay: Vec<f64>,
    pub shared_draw: f64,
    pub draw_bits_hex: String,
    pub branches: BTreeMap<String, BranchOutcomeArtifact>,
}

/// Convert persisted per-case probabilities into contrast cases: the
/// `normal` branch against every control branch.
pub fn branch_cases(
    dyad: &str,
    role: &str,
    probs: &[CaseProbabilities],
) -> Result<Vec<BranchCase>, AnalysisError> {
    probs
        .iter()
        .map(|p| {
            let normal = p.branches.get("normal").ok_or_else(|| {
                AnalysisError::Domain(format!(
                    "dyad {dyad} role {role} case {} lacks a normal branch",
                    p.case_id
                ))
            })?;
            let mut control_distributions = std::collections::HashMap::new();
            let mut control_success = std::collections::HashMap::new();
            for b in CONTROL_BRANCHES {
                let o = p.branches.get(b).ok_or_else(|| {
                    AnalysisError::Domain(format!(
                        "dyad {dyad} role {role} case {} lacks a {b} branch",
                        p.case_id
                    ))
                })?;
                control_distributions.insert(b.to_string(), o.probabilities.clone());
                control_success.insert(b.to_string(), o.success);
            }
            Ok(BranchCase {
                case_id: format!("{role}/{}", p.case_id),
                role: role.to_string(),
                normal_distribution: normal.probabilities.clone(),
                control_distributions,
                action: p.target_index,
                normal_success: normal.success,
                control_success,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probs() -> Vec<CaseProbabilities> {
        let mut branches = BTreeMap::new();
        for b in ["normal", "disabled", "constant", "random", "shuffled"] {
            branches.insert(
                b.to_string(),
                BranchOutcomeArtifact {
                    probabilities: vec![0.0, 1.0, 0.0, 0.0],
                    success: true,
                },
            );
        }
        vec![CaseProbabilities {
            case_id: "c1".to_string(),
            target_index: 1,
            delivered_token: None,
            native_missing: true,
            native: vec![0.25; 4],
            replay: vec![0.0, 1.0, 0.0, 0.0],
            shared_draw: 0.3,
            draw_bits_hex: String::new(),
            branches,
        }]
    }

    #[test]
    fn converts_to_contrast_cases() {
        let cases = branch_cases("d", "agent-0", &probs()).unwrap();
        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0].case_id, "agent-0/c1");
        assert_eq!(cases[0].control_distributions.len(), 4);
    }

    #[test]
    fn missing_branch_fails_closed() {
        let mut p = probs();
        p[0].branches.remove("random");
        assert!(branch_cases("d", "agent-0", &p).is_err());
    }
}
