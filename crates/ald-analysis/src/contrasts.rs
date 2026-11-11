//! Control contrasts: V2 binary-success version plus the V3 paired-Brier
//! version, simultaneous one-sided t bounds, invalidity bounds, fidelity.
//!
//! Estimation: each case's normal-branch and control-branch full
//! distributions give a paired Brier difference on the same scale as the
//! primary, instead of a 0/1 turn-success difference.

use crate::ledger::brier;
use crate::{summarize, student_t_quantile, AnalysisError};
use serde::{Deserialize, Serialize};

pub const CONTROL_BRANCHES: [&str; 4] = ["disabled", "constant", "random", "shuffled"];
pub const CONTRAST_IDS: [&str; 4] = [
    "normal-minus-disabled",
    "normal-minus-constant",
    "normal-minus-random",
    "normal-minus-shuffled",
];
/// Simultaneous family: 0.05 over eight contrasts (four per cohort x A/B).
pub const FAMILY_ALPHA: f64 = 0.05;
pub const FAMILY_SIZE: usize = 8;
/// Admissible ranges for the primary and success contrasts.
pub const PRIMARY_ADMISSIBLE: (f64, f64) = (-2.0, 2.0);
pub const SUCCESS_CONTRAST_ADMISSIBLE: (f64, f64) = (-1.0, 1.0);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchCase {
    pub case_id: String,
    pub role: String,
    pub normal_distribution: Vec<f64>,
    pub control_distributions: std::collections::HashMap<String, Vec<f64>>,
    pub action: usize,
    /// V2-compat binary outcomes.
    pub normal_success: bool,
    pub control_success: std::collections::HashMap<String, bool>,
}

/// V3 paired-Brier contrast: mean over cases of
/// Brier(control) - Brier(normal), per control, equal-averaged over roles.
/// Positive favors normal. Same scale as the dyad primary.
pub fn brier_control_contrasts(
    dyad: &str,
    cases: &[BranchCase],
) -> Result<std::collections::HashMap<String, f64>, AnalysisError> {
    if cases.is_empty() {
        return Err(AnalysisError::Domain(format!("dyad {dyad} has no branch-outcome cases")));
    }
    let mut seen = std::collections::HashSet::new();
    for c in cases {
        if c.case_id.is_empty() || !seen.insert(c.case_id.clone()) {
            return Err(AnalysisError::Domain(format!(
                "dyad {dyad} branch-outcome case ids must be non-empty and unique"
            )));
        }
    }
    let mut out = std::collections::HashMap::new();
    for role in ["agent-0", "agent-1"] {
        let role_cases: Vec<&BranchCase> = cases.iter().filter(|c| c.role == role).collect();
        if role_cases.is_empty() {
            return Err(AnalysisError::Domain(format!(
                "dyad {dyad} has no branch-outcome cases for role {role}"
            )));
        }
        for control in CONTROL_BRANCHES {
            let dist = role_cases
                .iter()
                .map(|c| {
                    let ctrl = c.control_distributions.get(control).ok_or_else(|| {
                        AnalysisError::Domain(format!("case {} lacks {control} distribution", c.case_id))
                    })?;
                    Ok::<f64, AnalysisError>(
                        brier(ctrl, c.action) - brier(&c.normal_distribution, c.action),
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            let m = dist.iter().sum::<f64>() / dist.len() as f64;
            *out.entry(contrast_id(control)).or_insert(0.0) += m / 2.0;
        }
    }
    Ok(out)
}

fn contrast_id(control: &str) -> String {
    format!("normal-minus-{control}")
}

/// V2 binary-success contrast (kept for comparability with sealed v2 reports).
pub fn success_control_contrasts(
    dyad: &str,
    cases: &[BranchCase],
) -> Result<std::collections::HashMap<String, f64>, AnalysisError> {
    if cases.is_empty() {
        return Err(AnalysisError::Domain(format!("dyad {dyad} has no branch-outcome cases")));
    }
    let mut out = std::collections::HashMap::new();
    for role in ["agent-0", "agent-1"] {
        let role_cases: Vec<&BranchCase> = cases.iter().filter(|c| c.role == role).collect();
        if role_cases.is_empty() {
            return Err(AnalysisError::Domain(format!(
                "dyad {dyad} has no branch-outcome cases for role {role}"
            )));
        }
        for control in CONTROL_BRANCHES {
            let total: f64 = role_cases
                .iter()
                .map(|c| {
                    f64::from(c.normal_success)
                        - f64::from(*c.control_success.get(control).unwrap_or(&false))
                })
                .sum();
            *out.entry(contrast_id(control)).or_insert(0.0) += total / role_cases.len() as f64 / 2.0;
        }
    }
    Ok(out)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlBound {
    pub contrast: String,
    pub n: usize,
    pub mean: f64,
    pub sd: f64,
    pub se: f64,
    pub critical_value: f64,
    pub lower_bound: f64,
    pub degenerate: Option<String>,
}

/// Simultaneous one-sided t lower bounds at alpha = FAMILY_ALPHA/FAMILY_SIZE.
pub fn simultaneous_bounds(
    contrast: &str,
    per_dyad: &[f64],
) -> Result<ControlBound, AnalysisError> {
    let s = summarize(per_dyad)?;
    let alpha = FAMILY_ALPHA / FAMILY_SIZE as f64;
    let base = ControlBound {
        contrast: contrast.to_string(),
        n: s.n,
        mean: s.mean,
        sd: s.sd,
        se: if s.n > 0 { s.sd / (s.n as f64).sqrt() } else { f64::NAN },
        critical_value: f64::NAN,
        lower_bound: f64::NAN,
        degenerate: None,
    };
    if s.n < 2 || !(s.sd > 0.0) {
        return Ok(ControlBound {
            degenerate: Some("degenerate: no variance estimate".to_string()),
            ..base
        });
    }
    let crit = student_t_quantile(1.0 - alpha, (s.n - 1) as u32)?;
    let se = s.sd / (s.n as f64).sqrt();
    Ok(ControlBound {
        critical_value: crit,
        lower_bound: s.mean - crit * se,
        ..base
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fidelity {
    pub n: usize,
    pub mean_squared_distance: f64,
}

/// Native/replay fidelity: mean over cases of the squared Euclidean distance
/// between the native distribution and the exact replay distribution.
/// (Protocol `fidelity.measure`: mean squared distance of native to exact
/// replay probabilities.)
pub fn fidelity(native: &[Vec<f64>], replay: &[Vec<f64>]) -> Result<Fidelity, AnalysisError> {
    if native.len() != replay.len() {
        return Err(AnalysisError::LengthMismatch(format!(
            "fidelity needs paired native/replay distributions ({} vs {})",
            native.len(),
            replay.len()
        )));
    }
    if native.is_empty() {
        return Err(AnalysisError::EmptySample(
            "fidelity needs at least one case".to_string(),
        ));
    }
    let mut total = 0.0;
    for (n, r) in native.iter().zip(replay.iter()) {
        if n.len() != r.len() {
            return Err(AnalysisError::LengthMismatch(
                "fidelity case distributions have different lengths".to_string(),
            ));
        }
        total += n
            .iter()
            .zip(r.iter())
            .map(|(a, b)| (a - b).powi(2))
            .sum::<f64>();
    }
    Ok(Fidelity {
        n: native.len(),
        mean_squared_distance: total / native.len() as f64,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvalidityBounds {
    pub valid_n: usize,
    pub attempted_n: usize,
    pub missing_or_invalid_n: usize,
    pub valid_mean: f64,
    pub admissible_min: f64,
    pub admissible_max: f64,
    pub lower: f64,
    pub upper: f64,
}

/// Worst-admissible bounds: missing/invalid dyads take the extremes.
pub fn invalidity_bounds(
    valid_values: &[f64],
    attempted_n: usize,
    admissible: (f64, f64),
) -> InvalidityBounds {
    let valid_n = valid_values.len();
    let missing = attempted_n.saturating_sub(valid_n);
    let total: f64 = valid_values.iter().sum();
    let valid_mean = if valid_n > 0 { total / valid_n as f64 } else { 0.0 };
    InvalidityBounds {
        valid_n,
        attempted_n,
        missing_or_invalid_n: missing,
        valid_mean,
        admissible_min: admissible.0,
        admissible_max: admissible.1,
        lower: (total + missing as f64 * admissible.0) / attempted_n as f64,
        upper: (total + missing as f64 * admissible.1) / attempted_n as f64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case(id: &str, role: &str, normal_ok: bool, ctrl_ok: bool) -> BranchCase {
        let mut cd = std::collections::HashMap::new();
        let mut cs = std::collections::HashMap::new();
        for b in CONTROL_BRANCHES {
            cd.insert(
                b.to_string(),
                if ctrl_ok { vec![0.0, 1.0, 0.0, 0.0] } else { vec![0.25; 4] },
            );
            cs.insert(b.to_string(), ctrl_ok);
        }
        BranchCase {
            case_id: id.to_string(),
            role: role.to_string(),
            normal_distribution: if normal_ok {
                vec![0.0, 1.0, 0.0, 0.0]
            } else {
                vec![0.25; 4]
            },
            control_distributions: cd,
            action: 1,
            normal_success: normal_ok,
            control_success: cs,
        }
    }

    #[test]
    fn brier_contrast_zero_when_equal() {
        let cases = vec![
            case("a1", "agent-0", true, true),
            case("b1", "agent-1", true, true),
        ];
        let c = brier_control_contrasts("d", &cases).unwrap();
        for id in CONTRAST_IDS {
            assert_eq!(c[id], 0.0);
        }
    }

    #[test]
    fn success_contrast_positive_when_normal_wins() {
        let cases = vec![
            case("a1", "agent-0", true, false),
            case("b1", "agent-1", true, false),
        ];
        let c = success_control_contrasts("d", &cases).unwrap();
        assert_eq!(c["normal-minus-disabled"], 1.0);
    }
}
