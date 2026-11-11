//! Merkle leaf/node hashing and inclusion/consistency proofs.
//!
//! Ports `packages/merkle/src/{hashes,tree,verify}.ts`:
//! leaf = SHA-256(merkle-leaf-domain || 0x00 || uint64BE(seq) || entryHashBytes)
//! node = SHA-256(merkle-node-domain || 0x01 || leftBytes || rightBytes).

use ald_hashing::{decode_hash, domain_hash, encode_hash, sha256_bytes, HashingError};
use ald_types::hash_domains;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MerkleError {
    #[error(transparent)]
    Hash(#[from] HashingError),
    #[error("invalid sequence number")]
    Sequence,
    #[error("invalid proof")]
    Proof,
}

#[must_use]
pub fn leaf_hash(sequence: u64, entry_hash: &str) -> Result<String, MerkleError> {
    let entry = decode_hash(entry_hash)?;
    let seq = sequence.to_be_bytes();
    let mut h = sha2::Sha256::new();
    use sha2::Digest as _;
    h.update(hash_domains::MERKLE_LEAF.as_bytes());
    h.update([0u8]);
    h.update(seq);
    h.update(entry);
    Ok(encode_hash(&h.finalize().into()))
}

#[must_use]
pub fn node_hash(left: &str, right: &str) -> Result<String, MerkleError> {
    let l = decode_hash(left)?;
    let r = decode_hash(right)?;
    let mut h = sha2::Sha256::new();
    use sha2::Digest as _;
    h.update(hash_domains::MERKLE_NODE.as_bytes());
    h.update([1u8]);
    h.update(l);
    h.update(r);
    Ok(encode_hash(&h.finalize().into()))
}

#[must_use]
pub const fn leaf_domain() -> &'static str {
    hash_domains::MERKLE_LEAF
}

#[must_use]
pub const fn node_domain() -> &'static str {
    hash_domains::MERKLE_NODE
}

/// Unused-compat alias kept for TS `hashCarrierMark` callers.
#[must_use]
pub fn carrier_mark(carrier_mode: &str, artifact_canonical: &str) -> String {
    domain_hash(
        hash_domains::CARRIER_MARK,
        &[carrier_mode.as_bytes(), &[0u8], artifact_canonical.as_bytes()],
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InclusionProof {
    pub leaf: String,
    pub siblings: Vec<String>,
    pub root: String,
}

pub fn verify_inclusion(proof: &InclusionProof, mut index: u64) -> Result<bool, MerkleError> {
    let mut acc = decode_hash(&proof.leaf).map_err(MerkleError::Hash)?;
    for sib in &proof.siblings {
        let s = decode_hash(sib).map_err(MerkleError::Hash)?;
        let (l, r) = if index % 2 == 0 { (acc, s) } else { (s, acc) };
        let mut h = sha2::Sha256::new();
        use sha2::Digest as _;
        h.update(hash_domains::MERKLE_NODE.as_bytes());
        h.update([1u8]);
        h.update(l);
        h.update(r);
        acc = h.finalize().into();
        index /= 2;
    }
    Ok(encode_hash(&acc) == proof.root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaf_is_deterministic() {
        let a = leaf_hash(7, &("sha256:".to_string() + &"ab".repeat(32))).unwrap();
        let b = leaf_hash(7, &("sha256:".to_string() + &"ab".repeat(32))).unwrap();
        assert_eq!(a, b);
        assert!(a.starts_with("sha256:"));
    }

    #[test]
    fn node_order_matters() {
        let x = "sha256:".to_string() + &"11".repeat(32);
        let y = "sha256:".to_string() + &"22".repeat(32);
        assert_ne!(node_hash(&x, &y).unwrap(), node_hash(&y, &x).unwrap());
    }
}
