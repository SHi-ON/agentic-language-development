//! Outcome-safe ordinary-record predictors (LV01).
//!
//! Ports `packages/analysis/src/ledger-value.ts`: five predictors
//! (uniform, validation-majority, transcript-only, task-history,
//! ordinary-record-softmax), Laplace-smoothed count fits, 2000-step
//! gradient softmax, Brier selection with EPS tie-break, refit on
//! combined folds.

use crate::AnalysisError;
use serde::{Deserialize, Serialize};

pub const CANDIDATES: usize = 4;
pub const TYPES: usize = 16;
pub const TOKENS: usize = 32;
pub const COEFFICIENTS: usize = 565;
const EPS: f64 = 1e-12;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PredictorId {
    Uniform,
    ValidationMajority,
    TranscriptOnly,
    TaskHistory,
    OrdinaryRecordSoftmax,
}

pub const PREDICTOR_IDS: [PredictorId; 5] = [
    PredictorId::Uniform,
    PredictorId::ValidationMajority,
    PredictorId::TranscriptOnly,
    PredictorId::TaskHistory,
    PredictorId::OrdinaryRecordSoftmax,
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrdinaryRecord {
    pub case_id: String,
    pub receiver_role: String,
    pub delivered_token: Option<String>,
    pub candidate_type_codes: Vec<u32>,
    pub actual_selected_candidate_index: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum OrdinaryFit {
    #[serde(rename = "uniform")]
    Uniform,
    #[serde(rename = "count")]
    Count {
        marginal: Vec<f64>,
        by_token: std::collections::HashMap<String, Vec<f64>>,
        by_token_type: std::collections::HashMap<String, Vec<f64>>,
    },
    #[serde(rename = "softmax")]
    Softmax { weights: Vec<f64> },
}

fn softmax(logits: &[f64]) -> Vec<f64> {
    let max = logits.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let ex: Vec<f64> = logits.iter().map(|v| (v - max).exp()).collect();
    let total: f64 = ex.iter().sum();
    ex.iter().map(|v| v / total).collect()
}

pub fn brier(probabilities: &[f64], action: usize) -> f64 {
    probabilities
        .iter()
        .enumerate()
        .map(|(i, v)| (v - if i == action { 1.0 } else { 0.0 }).powi(2))
        .sum()
}

fn token_index(token: &Option<String>, inventory: &[String]) -> Result<Option<usize>, AnalysisError> {
    match token {
        None => Ok(None),
        Some(t) => inventory
            .iter()
            .position(|x| x == t)
            .map(Some)
            .ok_or_else(|| {
                AnalysisError::Domain("ordinary record uses a token outside the frozen inventory".to_string())
            }),
    }
}

fn assert_inventory(inventory: &[String]) -> Result<(), AnalysisError> {
    let distinct = {
        let mut s = std::collections::HashSet::new();
        inventory.iter().all(|t| s.insert(t.clone()))
    };
    if inventory.len() != TOKENS || !distinct {
        return Err(AnalysisError::Domain("LV01 requires 32 distinct inventory tokens".to_string()));
    }
    Ok(())
}

fn assert_record(r: &OrdinaryRecord, inventory: &[String]) -> Result<(), AnalysisError> {
    if r.case_id.is_empty() || (r.receiver_role != "baby-a" && r.receiver_role != "baby-b") {
        return Err(AnalysisError::Domain("ordinary record identity is invalid".to_string()));
    }
    let types_distinct = {
        let mut s = std::collections::HashSet::new();
        r.candidate_type_codes.iter().all(|t| s.insert(t))
    };
    if r.candidate_type_codes.len() != CANDIDATES
        || !types_distinct
        || r.candidate_type_codes.iter().any(|t| *t as usize >= TYPES)
    {
        return Err(AnalysisError::Domain("ordinary record candidate types are invalid".to_string()));
    }
    if r.actual_selected_candidate_index >= CANDIDATES {
        return Err(AnalysisError::Domain("ordinary record action position is invalid".to_string()));
    }
    let _ = token_index(&r.delivered_token, inventory)?;
    Ok(())
}

fn feature_indices(
    type_codes: &[u32],
    token: &Option<String>,
    position: usize,
    inventory: &[String],
) -> Result<Vec<usize>, AnalysisError> {
    let ty = type_codes[position] as usize;
    let tok = token_index(token, inventory)?;
    let mut out = vec![0, 33 + ty, 49 + position];
    if let Some(t) = tok {
        out.push(1 + t);
        out.push(53 + t * TYPES + ty);
    }
    Ok(out)
}

/// Pure scorer shared by fitted predictors and the commitment auditor.
pub fn score_ordinary_fit(
    id: PredictorId,
    fit: &OrdinaryFit,
    type_codes: &[u32],
    token: &Option<String>,
    inventory: &[String],
) -> Result<Vec<f64>, AnalysisError> {
    let normalize = |counts: &[f64]| -> Vec<f64> {
        let total: f64 = counts.iter().sum();
        counts.iter().map(|x| x / total).collect()
    };
    match fit {
        OrdinaryFit::Uniform => {
            if id != PredictorId::Uniform {
                return Err(AnalysisError::Domain("uniform ordinary fit is inconsistent".to_string()));
            }
            Ok(vec![1.0 / CANDIDATES as f64; CANDIDATES])
        }
        OrdinaryFit::Softmax { weights } => {
            if weights.len() != COEFFICIENTS {
                return Err(AnalysisError::Domain("softmax ordinary fit has invalid weights".to_string()));
            }
            let logits: Vec<f64> = (0..CANDIDATES)
                .map(|pos| {
                    feature_indices(type_codes, token, pos, inventory)
                        .map(|idx| idx.iter().map(|&i| weights[i]).sum())
                        .unwrap_or(0.0)
                })
                .collect();
            Ok(softmax(&logits))
        }
        OrdinaryFit::Count { marginal, by_token, by_token_type } => {
            if marginal.len() != CANDIDATES {
                return Err(AnalysisError::Domain("count ordinary fit is inconsistent".to_string()));
            }
            let key = token.clone().unwrap_or_else(|| "<disabled>".to_string());
            match id {
                PredictorId::ValidationMajority => Ok(normalize(marginal)),
                PredictorId::TranscriptOnly => Ok(normalize(
                    by_token.get(&key).map(Vec::as_slice).unwrap_or(&[1.0; CANDIDATES]),
                )),
                _ => {
                    let counts = by_token_type.get(&key).map(Vec::as_slice).unwrap_or(&[1.0; TYPES]);
                    Ok(normalize(
                        &type_codes.iter().map(|t| counts[*t as usize]).collect::<Vec<_>>(),
                    ))
                }
            }
        }
    }
}

fn count_model(
    id: PredictorId,
    rows: &[OrdinaryRecord],
    inventory: &[String],
) -> Result<OrdinaryFit, AnalysisError> {
    let mut marginal = vec![1.0; CANDIDATES];
    let mut by_token: std::collections::HashMap<String, Vec<f64>> = Default::default();
    let mut by_token_type: std::collections::HashMap<String, Vec<f64>> = Default::default();
    for row in rows {
        marginal[row.actual_selected_candidate_index] += 1.0;
        let key = row.delivered_token.clone().unwrap_or_else(|| "<disabled>".to_string());
        let tc = by_token.entry(key.clone()).or_insert_with(|| vec![1.0; CANDIDATES]);
        tc[row.actual_selected_candidate_index] += 1.0;
        let tt = by_token_type.entry(key).or_insert_with(|| vec![1.0; TYPES]);
        tt[row.candidate_type_codes[row.actual_selected_candidate_index] as usize] += 1.0;
    }
    if id == PredictorId::Uniform {
        return Ok(OrdinaryFit::Uniform);
    }
    Ok(OrdinaryFit::Count { marginal, by_token, by_token_type })
}

#[derive(Debug, Clone)]
pub struct Selection {
    pub selected: PredictorId,
    pub validation_brier_by_predictor: [(PredictorId, f64); 5],
    pub refit: OrdinaryFit,
}

/// Fit/choose ordinary predictors on allowed windows; refit winner on
/// combined folds. Folds must be disjoint and single-role.
pub fn select_ordinary_predictor(
    inventory: &[String],
    training: &[OrdinaryRecord],
    validation_fit: &[OrdinaryRecord],
    validation_selection: &[OrdinaryRecord],
) -> Result<Selection, AnalysisError> {
    assert_inventory(inventory)?;
    if training.is_empty() && validation_fit.is_empty() {
        return Err(AnalysisError::EmptySample("ordinary record rows must not be empty".to_string()));
    }
    let mut ids = std::collections::HashSet::new();
    for r in training.iter().chain(validation_fit).chain(validation_selection) {
        assert_record(r, inventory)?;
        if !ids.insert(r.case_id.clone()) {
            return Err(AnalysisError::Domain("ordinary predictor folds must be disjoint".to_string()));
        }
    }
    let role = training
        .first()
        .or(validation_fit.first())
        .map(|r| r.receiver_role.clone())
        .unwrap_or_else(|| "baby-a".to_string());
    for r in training.iter().chain(validation_fit).chain(validation_selection) {
        if r.receiver_role != role {
            return Err(AnalysisError::Domain("ordinary predictors must be fit per receiver role".to_string()));
        }
    }
    let combined: Vec<OrdinaryRecord> =
        training.iter().chain(validation_fit).cloned().collect();

    // Candidate scores on the selection window.
    let mut scores: [(PredictorId, f64); 5] = PREDICTOR_IDS.map(|id| (id, 0.0));
    for (id, score) in scores.iter_mut().map(|s| (s.0, &mut s.1)) {
        let fit_rows: &[OrdinaryRecord] = match id {
            PredictorId::Uniform => &[],
            PredictorId::ValidationMajority => validation_fit,
            _ => &combined,
        };
        let fit = build_fit(id, fit_rows, inventory)?;
        *score = score_fit(id, &fit, validation_selection, inventory)?;
    }
    let mut best = PREDICTOR_IDS[0];
    let mut best_score = f64::INFINITY;
    for (id, s) in &scores {
        if *s < best_score - EPS {
            best = *id;
            best_score = *s;
        }
    }
    let refit_rows: Vec<OrdinaryRecord> =
        combined.iter().chain(validation_selection).cloned().collect();
    let refit = refit_pair(best, &role, &refit_rows, inventory)?;
    Ok(Selection { selected: best, validation_brier_by_predictor: scores, refit })
}

fn build_fit(
    id: PredictorId,
    rows: &[OrdinaryRecord],
    inventory: &[String],
) -> Result<OrdinaryFit, AnalysisError> {
    count_model(id, rows, inventory)
}

fn refit_pair(
    id: PredictorId,
    _role: &str,
    rows: &[OrdinaryRecord],
    inventory: &[String],
) -> Result<OrdinaryFit, AnalysisError> {
    if id == PredictorId::OrdinaryRecordSoftmax {
        return softmax_fit(_role, rows, inventory);
    }
    count_model(id, rows, inventory)
}

fn score_fit(
    id: PredictorId,
    fit: &OrdinaryFit,
    rows: &[OrdinaryRecord],
    inventory: &[String],
) -> Result<f64, AnalysisError> {
    let mut total = 0.0;
    for row in rows {
        total += brier(
            &score_ordinary_fit(id, fit, &row.candidate_type_codes, &row.delivered_token, inventory)?,
            row.actual_selected_candidate_index,
        );
    }
    Ok(total / rows.len() as f64)
}

fn softmax_fit(
    _role: &str,
    rows: &[OrdinaryRecord],
    inventory: &[String],
) -> Result<OrdinaryFit, AnalysisError> {
    let mut weights = vec![0.0; COEFFICIENTS];
    let mut gradient = vec![0.0; COEFFICIENTS];
    for _ in 0..2000 {
        gradient.fill(0.0);
        for row in rows {
            let logits: Vec<f64> = (0..CANDIDATES)
                .map(|pos| {
                    feature_indices(&row.candidate_type_codes, &row.delivered_token, pos, inventory)
                        .map(|idx| idx.iter().map(|&i| weights[i]).sum())
                        .unwrap_or(0.0)
                })
                .collect();
            let probs = softmax(&logits);
            for pos in 0..CANDIDATES {
                let err = probs[pos] - if pos == row.actual_selected_candidate_index { 1.0 } else { 0.0 };
                if let Ok(idx) =
                    feature_indices(&row.candidate_type_codes, &row.delivered_token, pos, inventory)
                {
                    for i in idx {
                        gradient[i] += err;
                    }
                }
            }
        }
        let n = rows.len() as f64;
        for (i, w) in weights.iter_mut().enumerate() {
            let reg = if i == 0 { 0.0 } else { 0.01 * *w };
            *w -= 0.05 * (gradient[i] / n + reg);
        }
    }
    Ok(OrdinaryFit::Softmax { weights })
}

pub fn score_prediction_native_brier(
    native_distribution: &[f64],
    actual_index: usize,
) -> f64 {
    brier(native_distribution, actual_index)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inv() -> Vec<String> {
        (0..32).map(|i| format!("tok{i:02}")).collect()
    }

    fn row(case: &str, action: usize) -> OrdinaryRecord {
        OrdinaryRecord {
            case_id: case.to_string(),
            receiver_role: "baby-a".to_string(),
            delivered_token: None,
            candidate_type_codes: vec![0, 1, 2, 3],
            actual_selected_candidate_index: action,
        }
    }

    #[test]
    fn uniform_scores_flat() {
        let d = score_ordinary_fit(
            PredictorId::Uniform,
            &OrdinaryFit::Uniform,
            &[0, 1, 2, 3],
            &None,
            &inv(),
        )
        .unwrap();
        assert_eq!(d, vec![0.25; 4]);
    }

    #[test]
    fn selection_picks_majority() {
        let fit_rows: Vec<OrdinaryRecord> =
            (0..20).map(|i| row(&format!("f{i}"), 0)).collect();
        let sel_rows: Vec<OrdinaryRecord> =
            (0..10).map(|i| row(&format!("s{i}"), 0)).collect();
        let sel = select_ordinary_predictor(&inv(), &[], &fit_rows, &sel_rows).unwrap();
        assert_eq!(sel.selected, PredictorId::ValidationMajority);
    }
}
