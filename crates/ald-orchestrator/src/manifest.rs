//! Dyad requests, receipts, manifests, and admission.

use crate::{dev_options::DevelopmentOptions, workload::StudyCounts, OrchestratorError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DyadRequest {
    pub manifest_path: String,
    pub cohort: String,
    pub kind: String,
    pub index: u32,
    pub evidence_root: String,
    pub treatment_batch_version: u32,
    pub protocol_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub development: Option<DevelopmentOptions>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DyadReceipt {
    pub cohort: String,
    pub kind: String,
    pub index: u32,
    pub state: String,
    pub cases_attempted: u64,
    pub cases_valid: u64,
    pub manifest_sha256: String,
    #[serde(default)]
    pub protocol_version: u32,
    #[serde(default)]
    pub treatment_batch_version: u32,
    /// Dyad primary: mean per-role ordinary-minus-native Brier gap.
    #[serde(default)]
    pub primary: f64,
    /// Measured wall-clock cost of the run in seconds.
    #[serde(default)]
    pub wall_seconds: f64,
    /// Hash-chained root-journal entry hashes bounding this run.
    #[serde(default)]
    pub attempted_hash: String,
    #[serde(default)]
    pub terminal_hash: String,
    /// Development-config binding hash ([`development_config_hash`]) for
    /// non-default runs. Absent on legacy runs, so default receipt bytes are
    /// unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub development_config_sha256: Option<String>,
}

/// Parse workload counts from an ALA execution manifest.
///
/// `manifest_path` points at the execution manifest (which pins the study
/// protocol by hash); the frozen per-partition case totals live in its
/// `workload` block. Refuses anything that is not valid JSON, does not
/// carry the frozen manifest version / study id, or lacks positive counts.
///
/// # Errors
///
/// Returns an error for invalid JSON, a wrong manifest version/study id, or non-positive counts.
pub fn execution_counts_from_manifest(text: &str) -> Result<StudyCounts, OrchestratorError> {
    let v: serde_json::Value = serde_json::from_str(text)
        .map_err(|e| OrchestratorError::Domain(format!("ALA manifest: invalid JSON: {e}")))?;
    let version = v
        .get("manifestVersion")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    if version != "ala-lv-execution-manifest/v1" {
        return Err(OrchestratorError::Domain(format!(
            "ALA manifest: manifestVersion must be ala-lv-execution-manifest/v1, got {version:?}"
        )));
    }
    let study = v
        .get("studyId")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    if study != "ALA-LV" {
        return Err(OrchestratorError::Domain(format!(
            "ALA manifest: studyId must be ALA-LV, got {study:?}"
        )));
    }
    let at = |name: &str| -> Result<u64, OrchestratorError> {
        v.pointer(&format!("/workload/{name}"))
            .and_then(serde_json::Value::as_u64)
            .filter(|&n| n >= 1)
            .ok_or_else(|| {
                OrchestratorError::Domain(format!("ALA manifest: workload has no positive {name}"))
            })
    };
    Ok(StudyCounts {
        training_cases: at("trainingCases")?,
        validation_fit_cases: at("validationFitCases")?,
        validation_selection_cases: at("validationSelectionCases")?,
        within_support_test_cases: at("withinSupportTestCases")?,
    })
}

pub(crate) fn manifest_sha256_hex(raw: &[u8]) -> String {
    use sha2::Digest as _;
    let mut h = sha2::Sha256::new();
    h.update(raw);
    format!("sha256:{}", hex::encode(h.finalize()))
}

/// P5 admission gate: study cohorts A/B refuse to run without an admission
/// record at `<evidence_root>/admission.json` whose decision is GO.
/// Development (and any non-study cohort) runs without admission.
pub(crate) fn check_admission(evidence_root: &str, cohort: &str) -> Result<(), OrchestratorError> {
    if cohort != "A" && cohort != "B" {
        return Ok(());
    }
    let path = format!("{evidence_root}/admission.json");
    let text = std::fs::read_to_string(&path).map_err(|_| {
        OrchestratorError::Domain(format!(
            "ALA admission: cohort {cohort} requires {path} with a GO decision; refusing study cohort without admission"
        ))
    })?;
    let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| {
        OrchestratorError::Domain(format!("ALA admission: {path} is invalid JSON: {e}"))
    })?;
    let decision_go = v
        .get("decision")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|d| d.eq_ignore_ascii_case("GO"));
    let admitted_flag = v
        .get("admitted")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    if !(decision_go || admitted_flag) {
        return Err(OrchestratorError::Domain(format!(
            "ALA admission: {path} does not admit cohort {cohort} (need {{\"decision\": \"GO\"}})"
        )));
    }
    Ok(())
}
