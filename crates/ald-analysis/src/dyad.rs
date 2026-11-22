//! ALA-LV dyad estimation: per-role scoring, dyad primary, cohort t-interval.
//!
//! V3 test rows retain per-branch native distributions so control
//! contrasts are paired
//! Brier differences on the same scale as the primary.
#![allow(clippy::cast_precision_loss)] // small counts/indices; exact for realistic inputs
#![allow(clippy::cast_possible_truncation)] // small counts/indices; bounded by construction
#![allow(clippy::cast_sign_loss)] // non-negative by construction (validated ranges, draws in [0, 1))

use crate::ledger::{
    brier, score_ordinary_fit, select_ordinary_predictor, OrdinaryRecord, PredictorId,
};
use crate::{student_t_quantile, summarize, AnalysisError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestRow {
    pub record: OrdinaryRecord,
    pub native_distribution: Vec<f64>,
    pub replay_distribution: Vec<f64>,
    pub target_index: usize,
    pub native_missing: bool,
    /// V3: per-branch full distributions for Brier control contrasts.
    #[serde(default)]
    pub branch_distributions: std::collections::HashMap<String, Vec<f64>>,
    /// V2-compat: per-branch binary turn success.
    #[serde(default)]
    pub branch_success: std::collections::HashMap<String, bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleFolds {
    pub role: String,
    pub training: Vec<OrdinaryRecord>,
    pub validation_fit: Vec<OrdinaryRecord>,
    pub validation_selection: Vec<OrdinaryRecord>,
    pub test: Vec<TestRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleResult {
    pub role: String,
    pub selected_predictor_id: PredictorId,
    pub validation_brier_by_predictor: Vec<(PredictorId, f64)>,
    pub test_cases: usize,
    pub ordinary_brier: f64,
    pub native_brier: f64,
    pub replay_brier: f64,
    pub mean_excess_replay_brier: f64,
    pub difference: f64,
    pub native_missing_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DyadResult {
    pub dyad: String,
    pub roles: Vec<RoleResult>,
    pub primary: f64,
}

/// # Errors
///
/// Returns an error when the test set is empty, selection fails, a test row carries the wrong role, or a distribution is non-finite or misshapen.
pub fn analyze_role(folds: &RoleFolds, inventory: &[String]) -> Result<RoleResult, AnalysisError> {
    if folds.test.is_empty() {
        return Err(AnalysisError::Domain(format!(
            "role {} has no test rows",
            folds.role
        )));
    }
    let sel = select_ordinary_predictor(
        inventory,
        &folds.training,
        &folds.validation_fit,
        &folds.validation_selection,
    )?;
    let mut ordinary = 0.0;
    let mut native = 0.0;
    let mut replay = 0.0;
    let mut missing = 0u64;
    for row in &folds.test {
        if row.record.receiver_role != folds.role {
            return Err(AnalysisError::Domain(format!(
                "role {} test set carries a {} row",
                folds.role, row.record.receiver_role
            )));
        }
        let dist = score_ordinary_fit(
            sel.selected,
            &sel.refit,
            &row.record.candidate_type_codes,
            &row.record.delivered_token,
            inventory,
        )?;
        let action = row.record.actual_selected_candidate_index;
        for (name, d) in [
            ("ordinary", &dist),
            ("native", &row.native_distribution),
            ("replay", &row.replay_distribution),
        ] {
            if d.len() != ald_types::signaling::CANDIDATES || d.iter().any(|v| !v.is_finite()) {
                return Err(AnalysisError::Domain(format!(
                    "role {} test case {} has a non-finite or misshapen {name} distribution",
                    folds.role, row.record.case_id
                )));
            }
        }
        ordinary += brier(&dist, action);
        native += brier(&row.native_distribution, action);
        replay += brier(&row.replay_distribution, action);
        if row.native_missing {
            missing += 1;
        }
    }
    let n = folds.test.len() as f64;
    Ok(RoleResult {
        role: folds.role.clone(),
        selected_predictor_id: sel.selected,
        validation_brier_by_predictor: sel.validation_brier_by_predictor.to_vec(),
        test_cases: folds.test.len(),
        ordinary_brier: ordinary / n,
        native_brier: native / n,
        replay_brier: replay / n,
        mean_excess_replay_brier: (native - replay) / n,
        difference: (ordinary - native) / n,
        native_missing_tokens: missing,
    })
}

/// # Errors
///
/// Returns an error unless exactly the two receiver-role folds are present, or when a role analysis fails.
#[allow(clippy::manual_midpoint)] // (a+b)/2 is byte-identical by design; see below.
pub fn analyze_dyad(
    dyad: &str,
    roles: &[RoleFolds],
    inventory: &[String],
) -> Result<DyadResult, AnalysisError> {
    if roles.len() != 2 {
        return Err(AnalysisError::Domain(format!(
            "dyad {dyad} needs exactly two role folds"
        )));
    }
    let mut ids: Vec<&str> = roles.iter().map(|r| r.role.as_str()).collect();
    ids.sort_unstable();
    if ids != ["agent-0", "agent-1"] {
        return Err(AnalysisError::Domain(format!(
            "dyad {dyad} folds do not cover both receiver roles"
        )));
    }
    let results: Vec<RoleResult> = roles
        .iter()
        .map(|f| analyze_role(f, inventory))
        .collect::<Result<_, _>>()?;
    Ok(DyadResult {
        dyad: dyad.to_string(),
        roles: results.clone(),
        // Byte-identical arithmetic: keep (a+b)/2, not f64::midpoint (which
        // can differ by 1 ulp and would change persisted primaries).
        primary: (results[0].difference + results[1].difference) / 2.0,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CohortInterval {
    pub cohort: String,
    pub n: usize,
    pub mean: f64,
    pub sd: f64,
    pub se: f64,
    pub coverage: f64,
    pub lower: Option<f64>,
    pub upper: Option<f64>,
    pub degenerate: Option<String>,
}

/// Two-sided Student-t interval across dyad primaries.
///
/// # Errors
///
/// Returns an error when coverage is outside (0, 1), or the primaries are empty or non-finite.
pub fn cohort_interval(
    cohort: &str,
    primaries: &[f64],
    coverage: f64,
) -> Result<CohortInterval, AnalysisError> {
    if !(coverage > 0.0 && coverage < 1.0) {
        return Err(AnalysisError::Domain(
            "coverage must be strictly between 0 and 1".to_string(),
        ));
    }
    let s = summarize(primaries)?;
    // INVARIANT: summarize errors on empty input, so s.n > 0 always holds;
    // the else branch is unreachable dead code kept for structural symmetry.
    let base_se = if s.n > 0 {
        s.sd / (s.n as f64).sqrt()
    } else {
        f64::NAN
    };
    if s.n < 2 {
        return Ok(CohortInterval {
            cohort: cohort.to_string(),
            n: s.n,
            mean: s.mean,
            sd: s.sd,
            se: base_se,
            coverage,
            lower: None,
            upper: None,
            degenerate: Some(format!("n={}: no variance estimate", s.n)),
        });
    }
    // NaN-safe degenerate guard (never `<=`: NaN sd must stay on the
    // degenerate path). No unwrap: panic-free by construction.
    if s.sd.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
        return Ok(CohortInterval {
            cohort: cohort.to_string(),
            n: s.n,
            mean: s.mean,
            sd: s.sd,
            se: base_se,
            coverage,
            lower: None,
            upper: None,
            degenerate: Some("zero variance across dyads".to_string()),
        });
    }
    let crit = student_t_quantile(1.0 - (1.0 - coverage) / 2.0, (s.n - 1) as u32)?;
    let half = crit * s.sd / (s.n as f64).sqrt();
    Ok(CohortInterval {
        cohort: cohort.to_string(),
        n: s.n,
        mean: s.mean,
        sd: s.sd,
        se: base_se,
        coverage,
        lower: Some(s.mean - half),
        upper: Some(s.mean + half),
        degenerate: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inv() -> Vec<String> {
        (0..32).map(|i| format!("tok{i:02}")).collect()
    }

    #[test]
    fn degenerate_single_dyad() {
        let iv = cohort_interval("A", &[0.1], 0.975).unwrap();
        assert!(iv.lower.is_none());
        assert!(iv.degenerate.is_some());
    }

    fn record(case: &str) -> OrdinaryRecord {
        OrdinaryRecord {
            case_id: case.to_string(),
            receiver_role: "agent-0".to_string(),
            delivered_token: None,
            candidate_type_codes: vec![0, 1, 2, 3],
            actual_selected_candidate_index: 0,
        }
    }

    fn folds_with_test_distributions(native: &[f64], replay: &[f64]) -> RoleFolds {
        let mk = |case: &str| TestRow {
            record: record(case),
            native_distribution: native.to_vec(),
            replay_distribution: replay.to_vec(),
            target_index: 0,
            native_missing: false,
            branch_distributions: std::collections::HashMap::new(),
            branch_success: std::collections::HashMap::new(),
        };
        RoleFolds {
            role: "agent-0".to_string(),
            training: vec![record("t0")],
            validation_fit: vec![record("f0")],
            validation_selection: vec![record("s0")],
            test: vec![mk("x0"), mk("x1")],
        }
    }

    #[test]
    fn analyze_role_rejects_non_finite_native_distribution() {
        let folds = folds_with_test_distributions(&[0.25, 0.25, f64::NAN, 0.25], &[0.25; 4]);
        let err = analyze_role(&folds, &inv()).unwrap_err().to_string();
        assert!(err.contains("non-finite"), "unexpected error: {err}");
    }

    #[test]
    fn analyze_role_accepts_finite_distributions() {
        let folds = folds_with_test_distributions(&[0.25; 4], &[1.0, 0.0, 0.0, 0.0]);
        assert!(analyze_role(&folds, &inv()).is_ok());
    }
}
