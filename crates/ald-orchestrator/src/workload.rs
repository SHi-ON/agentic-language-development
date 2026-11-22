//! Study workload accounting.

use crate::OrchestratorError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudyCounts {
    pub training_cases: u64,
    pub validation_fit_cases: u64,
    pub validation_selection_cases: u64,
    pub within_support_test_cases: u64,
}

/// # Errors
///
/// Returns an error for invalid JSON, or missing/non-positive partition counts.
pub fn study_counts_from_protocol(text: &str) -> Result<StudyCounts, OrchestratorError> {
    let v: serde_json::Value =
        serde_json::from_str(text).map_err(|e| OrchestratorError::Domain(e.to_string()))?;
    let at = |partition: &str| -> Result<u64, OrchestratorError> {
        v.pointer(&format!("/task/partitions/{partition}/cases"))
            .and_then(serde_json::Value::as_u64)
            .filter(|&n| n >= 1)
            .ok_or_else(|| {
                OrchestratorError::Domain(format!(
                    "ALA workload: study protocol has no positive {partition} cases"
                ))
            })
    };
    Ok(StudyCounts {
        training_cases: at("training")?,
        validation_fit_cases: at("validationFit")?,
        validation_selection_cases: at("validationSelection")?,
        within_support_test_cases: at("withinSupportTest")?,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionWorkload {
    pub profile: String,
    pub training_turns: u64,
    pub validation_fit_cases: u64,
    pub validation_selection_cases: u64,
    pub within_support_test_cases: u64,
}

/// Prototype workload: study totals split evenly across the two roles.
///
/// # Errors
///
/// Returns an error when a total is odd (roles split evenly).
pub fn prototype_workload(counts: &StudyCounts) -> Result<CollectionWorkload, OrchestratorError> {
    for (name, value) in [
        ("trainingCases", counts.training_cases),
        ("validationFitCases", counts.validation_fit_cases),
        (
            "validationSelectionCases",
            counts.validation_selection_cases,
        ),
        ("withinSupportTestCases", counts.within_support_test_cases),
    ] {
        if value % 2 != 0 {
            return Err(OrchestratorError::Domain(format!(
                "ALA workload: study {name} is not evenly split across roles"
            )));
        }
    }
    Ok(CollectionWorkload {
        profile: "prototype".to_string(),
        training_turns: counts.training_cases,
        validation_fit_cases: counts.validation_fit_cases / 2,
        validation_selection_cases: counts.validation_selection_cases / 2,
        within_support_test_cases: counts.within_support_test_cases / 2,
    })
}
