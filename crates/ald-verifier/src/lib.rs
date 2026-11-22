//! Independent bundle verifier: chain-walk, checkpoint, and inclusion checks.
//!
//! Verify a
//! stream's hash chain (via `ald-evidence`), re-check checkpoint hashes, and
//! validate Merkle inclusion proofs.

use ald_evidence::{read_stream, verify_stream};
use ald_merkle::verify_inclusion;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum VerifierError {
    #[error("evidence: {0}")]
    Evidence(#[from] ald_evidence::EvidenceError),
    #[error("merkle: {0}")]
    Merkle(#[from] ald_merkle::MerkleError),
    #[error("verification failed: {0}")]
    Failed(String),
}

pub const VERIFIER_VERSION: &str = "ald-verifier-rs/0.1.0";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationReport {
    pub verifier_version: String,
    pub streams_checked: u64,
    pub events_checked: u64,
    pub ok: bool,
}

/// Verify every stream's chain in an evidence database.
///
/// # Errors
///
/// Returns an error when any stream chain fails to verify, when a stored
/// checkpoint body is not a valid unsigned manifest or its hash mismatches,
/// or when SQLite fails.
pub fn verify_bundle(conn: &Connection) -> Result<VerificationReport, VerifierError> {
    let mut events = 0u64;
    let mut streams = 0u64;
    for stream in [
        ald_types::EventStream::Agent0Ledger,
        ald_types::EventStream::Agent1Ledger,
        ald_types::EventStream::Channel,
        ald_types::EventStream::Affect,
        ald_types::EventStream::Audit,
        ald_types::EventStream::Turns,
        ald_types::EventStream::Intervention,
    ] {
        let n = verify_stream(conn, stream)?;
        if n > 0 {
            streams += 1;
            events += n;
        }
    }
    for cp in ald_evidence::read_checkpoints(conn)? {
        let unsigned: ald_checkpoint::UnsignedCheckpointManifest = serde_json::from_str(&cp.body)
            .map_err(|e| {
            VerifierError::Failed(format!(
                "checkpoint {} body is not a valid unsigned manifest: {e}",
                cp.sequence
            ))
        })?;
        let recomputed = ald_checkpoint::checkpoint_hash(&unsigned).map_err(|e| {
            VerifierError::Failed(format!("checkpoint {} unhashable: {e}", cp.sequence))
        })?;
        if recomputed != cp.checkpoint_hash {
            return Err(VerifierError::Failed(format!(
                "checkpoint {} hash mismatch",
                cp.sequence
            )));
        }
    }
    Ok(VerificationReport {
        verifier_version: VERIFIER_VERSION.to_string(),
        streams_checked: streams,
        events_checked: events,
        ok: true,
    })
}

/// Verify a Merkle inclusion proof against its root.
///
/// # Errors
///
/// Returns an error when the proof carries malformed hashes.
pub fn verify_inclusion_proof(proof: &ald_merkle::InclusionProof) -> Result<bool, VerifierError> {
    Ok(verify_inclusion(proof, 0)?)
}

#[allow(unused_imports)]
fn _use_read_stream(conn: &Connection) -> Result<(), VerifierError> {
    let _ = read_stream(conn, ald_types::EventStream::Channel)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_bundle_verifies() {
        let conn = ald_evidence::open(":memory:").unwrap();
        let r = verify_bundle(&conn).unwrap();
        assert!(r.ok);
        assert_eq!(r.events_checked, 0);
    }

    fn test_manifest() -> ald_checkpoint::UnsignedCheckpointManifest {
        ald_checkpoint::UnsignedCheckpointManifest {
            run_id: "r".to_string(),
            sequence: 0,
            previous_checkpoint_hash: ald_types::GENESIS_HASH.to_string(),
            trees: vec![],
            created_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn stored_checkpoint_reverified() {
        let conn = ald_evidence::open(":memory:").unwrap();
        let m = test_manifest();
        let hash = ald_checkpoint::checkpoint_hash(&m).unwrap();
        conn.execute(
            "INSERT INTO checkpoints(sequence, body, checkpoint_hash) VALUES(?1,?2,?3)",
            rusqlite::params![0, serde_json::to_string(&m).unwrap(), hash],
        )
        .unwrap();
        assert!(verify_bundle(&conn).unwrap().ok);
    }

    #[test]
    fn tampered_chain_fails_verify() {
        let conn = ald_evidence::open(":memory:").unwrap();
        for i in 0..2 {
            let mut m = serde_json::Map::new();
            m.insert("i".to_string(), serde_json::json!(i));
            ald_evidence::append_event(&conn, ald_types::EventStream::Channel, m).unwrap();
        }
        conn.execute("UPDATE events SET body = '{}' WHERE sequence = 1", [])
            .unwrap();
        assert!(verify_bundle(&conn).is_err());
    }

    #[test]
    fn entry_hash_mismatch_fails_verify() {
        let conn = ald_evidence::open(":memory:").unwrap();
        let mut m = serde_json::Map::new();
        m.insert("i".to_string(), serde_json::json!(0));
        ald_evidence::append_event(&conn, ald_types::EventStream::Channel, m).unwrap();
        conn.execute(
            "UPDATE events SET entry_hash = 'sha256:00' WHERE sequence = 0",
            [],
        )
        .unwrap();
        assert!(verify_bundle(&conn).is_err());
    }

    #[test]
    fn inclusion_proof_negative() {
        let entry = "sha256:".to_string() + &"ab".repeat(32);
        let leaf = ald_merkle::leaf_hash(0, &entry).unwrap();
        let sib = ald_merkle::leaf_hash(1, &entry).unwrap();
        let root = ald_merkle::node_hash(&leaf, &sib).unwrap();
        let good = ald_merkle::InclusionProof {
            leaf: leaf.clone(),
            siblings: vec![sib.clone()],
            root: root.clone(),
        };
        assert!(ald_merkle::verify_inclusion(&good, 0).unwrap());
        let mut bad_sib = sib.clone();
        bad_sib.replace_range(10..11, if &sib[10..11] == "0" { "1" } else { "0" });
        let bad = ald_merkle::InclusionProof {
            leaf,
            siblings: vec![bad_sib],
            root,
        };
        // Either outcome is fail-closed: Err on malformed input, Ok(false)
        // on a well-formed but non-matching proof.
        assert!(!ald_merkle::verify_inclusion(&bad, 0).unwrap_or(false));
    }

    #[test]
    fn mutated_checkpoint_fails() {
        let conn = ald_evidence::open(":memory:").unwrap();
        let m = test_manifest();
        let hash = ald_checkpoint::checkpoint_hash(&m).unwrap();
        conn.execute(
            "INSERT INTO checkpoints(sequence, body, checkpoint_hash) VALUES(?1,?2,?3)",
            rusqlite::params![0, serde_json::to_string(&m).unwrap(), hash],
        )
        .unwrap();
        conn.execute(
            "UPDATE checkpoints SET body = ?1 WHERE sequence = 0",
            rusqlite::params![
                serde_json::to_string(&ald_checkpoint::UnsignedCheckpointManifest {
                    run_id: "tampered".to_string(),
                    ..m
                })
                .unwrap()
            ],
        )
        .unwrap();
        let err = verify_bundle(&conn).unwrap_err().to_string();
        assert!(err.contains("checkpoint 0"), "unexpected error: {err}");
    }
}
