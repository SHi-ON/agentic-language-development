//! ALA seed derivation.

use crate::OrchestratorError;
use ald_hashing::derive_seed_hex;
use serde::{Deserialize, Serialize};

pub const ALA_SEED_ROOT: &str = "ala-ledger-value-v1";
pub const ALA_SEED_ROOT_V2: &str = "ala-ledger-value-v2";
pub const ALA_STUDY_ID: &str = "ALA-LV";

pub const SEED_PURPOSES: [&str; 8] = [
    "scenario",
    "learner/agent-0",
    "learner/agent-1",
    "channel",
    "action-draw",
    "intervention-shuffle",
    "branch-order",
    "analysis",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlaSeedInput {
    pub cohort: String,
    pub slot_kind: String,
    pub slot_index: String,
    pub purpose: String,
    pub role: String,
    pub partition: Option<String>,
    pub case_id: Option<String>,
    pub branch: Option<String>,
    pub seed_root: Option<String>,
}

/// Derive one ALA seed: root/study/cohort/kind/index/purpose/role[/partition/case/branch].
///
/// # Errors
///
/// Returns an error when a required field is empty or NUL-bearing, the slot index is not zero-padded, or the seed root is unknown.
pub fn derive_ala_seed(input: &AlaSeedInput) -> Result<String, OrchestratorError> {
    for (name, value) in [
        ("cohort", &input.cohort),
        ("slotKind", &input.slot_kind),
        ("slotIndex", &input.slot_index),
        ("purpose", &input.purpose),
        ("role", &input.role),
    ] {
        if value.is_empty() || value.contains('\0') {
            return Err(OrchestratorError::Domain(format!(
                "ALA seeds: {name} is empty"
            )));
        }
    }
    if input.slot_index.len() != 4 || !input.slot_index.bytes().all(|b| b.is_ascii_digit()) {
        return Err(OrchestratorError::Domain(
            "ALA seeds: slotIndex must be zero-padded".to_string(),
        ));
    }
    let root = input.seed_root.as_deref().unwrap_or(ALA_SEED_ROOT);
    if root != ALA_SEED_ROOT && root != ALA_SEED_ROOT_V2 {
        return Err(OrchestratorError::Domain(
            "ALA seeds: seedRoot is not a known ALA namespace root".to_string(),
        ));
    }
    let part_owned;
    let case_owned;
    let branch_owned;
    let mut parts: Vec<&str> = vec![
        root,
        ALA_STUDY_ID,
        &input.cohort,
        &input.slot_kind,
        &input.slot_index,
        &input.purpose,
        &input.role,
    ];
    if let Some(p) = &input.partition {
        part_owned = p.clone();
        parts.push(Box::leak(part_owned.into_boxed_str()));
    }
    if let Some(c) = &input.case_id {
        case_owned = c.clone();
        parts.push(Box::leak(case_owned.into_boxed_str()));
    }
    if let Some(b) = &input.branch {
        branch_owned = b.clone();
        parts.push(Box::leak(branch_owned.into_boxed_str()));
    }
    Ok(derive_seed_hex(&parts))
}
