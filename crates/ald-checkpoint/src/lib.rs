//! Checkpoint manifest generation (LEDGER §7–§9).
//!
//! A checkpoint binds every hash-chained stream of one run at one instant:
//! per-stream tree size, RFC 6962 root, and last entry hash, chained to the
//! previous checkpoint hash.

use ald_hashing::{hash_canonical, HashingError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeReference {
    pub tree: String,
    pub size: u64,
    pub root: String,
    pub last_entry_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnsignedCheckpointManifest {
    pub run_id: String,
    pub sequence: u64,
    pub previous_checkpoint_hash: String,
    pub trees: Vec<TreeReference>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointManifest {
    #[serde(flatten)]
    pub unsigned: UnsignedCheckpointManifest,
    pub checkpoint_hash: String,
}

pub fn checkpoint_hash(
    unsigned: &UnsignedCheckpointManifest,
) -> Result<String, HashingError> {
    let value = serde_json::to_value(unsigned)
        .map_err(|e| HashingError::Unrepresentable(e.to_string()))?;
    hash_canonical(ald_types::hash_domains::CHECKPOINT, &value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_stable() {
        let m = UnsignedCheckpointManifest {
            run_id: "r".to_string(),
            sequence: 0,
            previous_checkpoint_hash: ald_types::GENESIS_HASH.to_string(),
            trees: vec![],
            created_at: "2026-01-01T00:00:00Z".to_string(),
        };
        assert_eq!(checkpoint_hash(&m).unwrap(), checkpoint_hash(&m).unwrap());
    }
}
