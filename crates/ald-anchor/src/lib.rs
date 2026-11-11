//! Anchor publisher: simulated-chain anchor receipts and verification.
//!
//! Rust port of `packages/anchor/src` (simulated transport): an anchor
//! receipt binds a checkpoint hash to a monotonically increasing height;
//! verification checks the binding hash.

use ald_hashing::{domain_hash, HashingError};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AnchorError {
    #[error("hashing: {0}")]
    Hash(#[from] HashingError),
    #[error("anchor domain error: {0}")]
    Domain(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnchorReceipt {
    pub checkpoint_hash: String,
    pub height: u64,
    pub anchor_hash: String,
    pub transport: String,
}

pub fn publish_simulated(checkpoint_hash: &str, height: u64) -> Result<AnchorReceipt, AnchorError> {
    if !ald_hashing::is_sha256_hash(checkpoint_hash) {
        return Err(AnchorError::Domain("checkpoint hash must be sha256:<hex>".to_string()));
    }
    Ok(AnchorReceipt {
        checkpoint_hash: checkpoint_hash.to_string(),
        height,
        anchor_hash: domain_hash(
            "dtsf-anchor-simulated-v1",
            &[checkpoint_hash.as_bytes(), &height.to_be_bytes()],
        ),
        transport: "simulated".to_string(),
    })
}

pub fn verify_receipt(receipt: &AnchorReceipt) -> Result<bool, AnchorError> {
    Ok(publish_simulated(&receipt.checkpoint_hash, receipt.height)?.anchor_hash == receipt.anchor_hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publish_verify_roundtrip() {
        let h = "sha256:".to_string() + &"ab".repeat(32);
        let r = publish_simulated(&h, 41).unwrap();
        assert!(verify_receipt(&r).unwrap());
    }

    #[test]
    fn rejects_bad_hash() {
        assert!(publish_simulated("nope", 1).is_err());
    }
}
