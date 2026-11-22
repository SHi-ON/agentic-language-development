//! Shared run plumbing: distributions, folds, bundles, identities.

use crate::manifest::DyadRequest;
use serde::{Deserialize, Serialize};

pub(crate) fn valid_distribution(d: &[f64]) -> bool {
    d.len() == 4
        && d.iter().all(|v| v.is_finite() && *v >= 0.0)
        && (d.iter().sum::<f64>() - 1.0).abs() < 1e-9
}

/// Round-trip stabilization for floats that enter hashed records.
///
/// `serde_json`'s default float parser is not correctly rounded, so
/// `parse(format(v))` can differ from `v` by 1 ulp — and evidence entry
/// hashes are recomputed from the stored text. Iterate to the parse/format
/// fixpoint so every hashed float survives a store/load round-trip
/// bit-identically. The adjustment is at most a few ulps (relative ~1e-16),
/// applied once at the source so every artifact agrees exactly.
#[must_use]
pub fn stable_f64(v: f64) -> f64 {
    let mut cur = v;
    for _ in 0..16 {
        let s = serde_json::Value::from(cur).to_string();
        let next: f64 = serde_json::from_str(&s).unwrap_or(cur);
        if next.to_bits() == cur.to_bits() {
            return cur;
        }
        cur = next;
    }
    cur
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BranchOutcome {
    pub(crate) probabilities: Vec<f64>,
    pub(crate) success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CaseProbabilities {
    pub(crate) case_id: String,
    pub(crate) target_index: usize,
    pub(crate) delivered_token: Option<String>,
    pub(crate) native_missing: bool,
    pub(crate) native: Vec<f64>,
    pub(crate) replay: Vec<f64>,
    pub(crate) shared_draw: f64,
    pub(crate) draw_bits_hex: String,
    // BTreeMap: serde streams map entries in iteration order, and HashMap
    // iteration is per-instance random, so a HashMap here would make the
    // persisted bytes (and any hash over them) nondeterministic.
    pub(crate) branches: std::collections::BTreeMap<String, BranchOutcome>,
}

/// Deterministic on-disk mirror of [`ald_analysis::dyad::RoleFolds`].
///
/// Field declaration order matches the committed `role-folds-*.json`
/// fixtures exactly (`role`, `training`, `validation_fit`,
/// `validation_selection`, `test`; per test row `record`,
/// `native_distribution`, `replay_distribution`, `target_index`,
/// `native_missing`, `branch_distributions`, `branch_success`). The branch
/// maps use `BTreeMap` so the persisted bytes are byte-identical across
/// runs; the in-memory `TestRow` maps are `HashMap` and must never be
/// serialized directly.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct PersistTestRow<'a> {
    pub(crate) record: &'a ald_analysis::ledger::OrdinaryRecord,
    pub(crate) native_distribution: &'a Vec<f64>,
    pub(crate) replay_distribution: &'a Vec<f64>,
    pub(crate) target_index: usize,
    pub(crate) native_missing: bool,
    pub(crate) branch_distributions: std::collections::BTreeMap<&'a str, &'a Vec<f64>>,
    pub(crate) branch_success: std::collections::BTreeMap<&'a str, bool>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PersistRoleFolds<'a> {
    pub(crate) role: &'a str,
    pub(crate) training: &'a [ald_analysis::ledger::OrdinaryRecord],
    pub(crate) validation_fit: &'a [ald_analysis::ledger::OrdinaryRecord],
    pub(crate) validation_selection: &'a [ald_analysis::ledger::OrdinaryRecord],
    pub(crate) test: Vec<PersistTestRow<'a>>,
}

pub(crate) fn persistable_role_folds(
    folds: &ald_analysis::dyad::RoleFolds,
) -> PersistRoleFolds<'_> {
    PersistRoleFolds {
        role: &folds.role,
        training: &folds.training,
        validation_fit: &folds.validation_fit,
        validation_selection: &folds.validation_selection,
        test: folds
            .test
            .iter()
            .map(|t| PersistTestRow {
                record: &t.record,
                native_distribution: &t.native_distribution,
                replay_distribution: &t.replay_distribution,
                target_index: t.target_index,
                native_missing: t.native_missing,
                branch_distributions: t
                    .branch_distributions
                    .iter()
                    .map(|(k, v)| (k.as_str(), v))
                    .collect(),
                branch_success: t
                    .branch_success
                    .iter()
                    .map(|(k, v)| (k.as_str(), *v))
                    .collect(),
            })
            .collect(),
    }
}

pub(crate) struct DyadInner {
    pub(crate) primary: f64,
    pub(crate) cases_attempted: u64,
    pub(crate) cases_valid: u64,
}

/// Per-split per-type exposure census for one role: candidate-set appearances
/// of each of the 16 types. The training census covers the `N` episode
/// records per direction the agents actually trained on (a prefix of — or,
/// for `N` above the manifest count, an extension of — the persisted fold);
/// the other three cover the persisted folds.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RoleExposure {
    pub(crate) role: String,
    pub(crate) training: [u64; 16],
    pub(crate) validation_fit: [u64; 16],
    pub(crate) validation_selection: [u64; 16],
    pub(crate) within_support_test: [u64; 16],
}

pub(crate) fn exposure_histogram(codes: &[Vec<u32>]) -> [u64; 16] {
    let mut h = [0u64; 16];
    for row in codes {
        for c in row {
            h[*c as usize] += 1;
        }
    }
    h
}

/// Candidate type codes as agent-policy indices. Callers only pass
/// validated generated cases (4 distinct codes below 16).
/// Convert four candidate type codes to `usize` indices.
///
/// # Panics
///
/// Panics when fewer than four codes are given.
#[must_use]
pub fn candidate_types(codes: &[u32]) -> [usize; 4] {
    [
        codes[0] as usize,
        codes[1] as usize,
        codes[2] as usize,
        codes[3] as usize,
    ]
}

/// One role's generated folds plus the seeds they were generated from. The
/// training records carry the target types + candidate codes the emergent
/// training episodes are built from; test tokens are resampled from the
/// frozen opposite agent (as sender) at scoring time. `training` holds
/// `max(N, fold)` records: the persisted fold is its manifest-count prefix
/// and the episodes run over its `N`-prefix.
pub(crate) struct RoleBundle {
    pub(crate) role: String,
    pub(crate) train_seed: String,
    pub(crate) fit_seed: String,
    pub(crate) sel_seed: String,
    pub(crate) test_seed: String,
    pub(crate) training: Vec<ald_analysis::ledger::OrdinaryRecord>,
    pub(crate) validation_fit: Vec<ald_analysis::ledger::OrdinaryRecord>,
    pub(crate) validation_selection: Vec<ald_analysis::ledger::OrdinaryRecord>,
    pub(crate) test_cases: Vec<ald_scenario::Lv01Case>,
}

pub(crate) fn journal_slot_identity(
    req: &DyadRequest,
    pad: &str,
    manifest_sha256: &str,
) -> serde_json::Map<String, serde_json::Value> {
    let mut m = serde_json::Map::new();
    m.insert("cohort".to_string(), serde_json::json!(req.cohort));
    m.insert("slotKind".to_string(), serde_json::json!(req.kind));
    m.insert("slotIndex".to_string(), serde_json::json!(pad));
    m.insert(
        "manifestSha256".to_string(),
        serde_json::json!(manifest_sha256),
    );
    m
}
