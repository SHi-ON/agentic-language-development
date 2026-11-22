//! SQLite evidence store: hash-chained event streams, checkpoints, export.
//!
//! Database and export surface: one SQLite file, one table per concern;
//! every event carries `entryHash =
//! SHA-256(stream-domain || 0x00 || canonical(unsigned event))` and chains to
//! the previous hash. Single-writer discipline is the caller's contract.
//!
//! Float encoding: appended bodies store f64 values as their IEEE-754 bit
//! patterns (u64 integers; decode with `f64::from_bits`), because decimal
//! floats do not survive store -> parse -> rehash bit-exactly for all
//! doubles. Bodies written before this encoding keep decimal floats and
//! continue to verify; the two forms never mix within one event.
#![allow(clippy::cast_precision_loss)] // small counts/indices; exact for realistic inputs
#![allow(clippy::cast_possible_truncation)] // small counts/indices; bounded by construction
#![allow(clippy::cast_sign_loss)] // non-negative by construction (validated ranges, draws in [0, 1))

use ald_hashing::{compute_entry_hash, float_bits_value, HashingError};
use ald_types::EventStream;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EvidenceError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("hashing: {0}")]
    Hash(#[from] HashingError),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("io: {0}")]
    Io(String),
    #[error("unknown run: {0}")]
    UnknownRun(String),
    #[error("stream closed or missing: {0}")]
    Stream(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredEvent {
    pub sequence: u64,
    pub stream: String,
    pub entry_hash: String,
    pub previous_hash: String,
    pub body: serde_json::Value,
}

/// Open (creating parent dirs and schema) the evidence database.
///
/// # Errors
///
/// Returns an error when parent directories cannot be created, or SQLite fails to open or initialize.
pub fn open(path: &str) -> Result<Connection, EvidenceError> {
    if let Some(parent) = std::path::Path::new(path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|e| EvidenceError::Io(format!("{path}: {e}")))?;
        }
    }
    let conn = Connection::open(path)?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS events(
            stream TEXT NOT NULL,
            sequence INTEGER NOT NULL,
            entry_hash TEXT NOT NULL,
            previous_hash TEXT NOT NULL,
            body TEXT NOT NULL,
            PRIMARY KEY(stream, sequence)
        );
        CREATE TABLE IF NOT EXISTS checkpoints(
            sequence INTEGER PRIMARY KEY,
            body TEXT NOT NULL,
            checkpoint_hash TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS runs(
            run_id TEXT PRIMARY KEY,
            manifest TEXT NOT NULL
        );",
    )?;
    Ok(conn)
}

fn last_hash(conn: &Connection, stream: &str) -> Result<String, EvidenceError> {
    let mut stmt = conn.prepare(
        "SELECT entry_hash FROM events WHERE stream = ?1 ORDER BY sequence DESC LIMIT 1",
    )?;
    let h: Result<String, _> = stmt.query_row(params![stream], |r| r.get(0));
    match h {
        Ok(v) => Ok(v),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(ald_types::GENESIS_HASH.to_string()),
        Err(e) => Err(e.into()),
    }
}

fn next_seq(conn: &Connection, stream: &str) -> Result<u64, EvidenceError> {
    let mut stmt =
        conn.prepare("SELECT COALESCE(MAX(sequence), -1) + 1 FROM events WHERE stream = ?1")?;
    Ok(stmt.query_row(params![stream], |r| r.get(0))?)
}

/// Append one unsigned event body to a stream; returns the stored event.
///
/// Floats are bit-encoded before hashing and storing (see module docs),
/// so the stored text re-parses to the exact hashed values.
///
/// # Errors
///
/// Returns an error when the body cannot be hashed, or SQLite fails to insert.
pub fn append_event(
    conn: &Connection,
    stream: EventStream,
    mut unsigned: serde_json::Map<String, serde_json::Value>,
) -> Result<StoredEvent, EvidenceError> {
    let stream_name = stream.as_str();
    let prev = last_hash(conn, stream_name)?;
    let seq = next_seq(conn, stream_name)?;
    unsigned.insert(
        "previousHash".to_string(),
        serde_json::Value::String(prev.clone()),
    );
    let encoded = float_bits_value(&serde_json::Value::Object(unsigned));
    let mut unsigned = encoded.as_object().cloned().unwrap_or_default();
    let entry_hash = compute_entry_hash(stream, &unsigned)?;
    unsigned.insert(
        "entryHash".to_string(),
        serde_json::Value::String(entry_hash.clone()),
    );
    let body = serde_json::Value::Object(unsigned);
    conn.execute(
        "INSERT INTO events(stream, sequence, entry_hash, previous_hash, body) VALUES(?1,?2,?3,?4,?5)",
        params![stream_name, seq.cast_signed(), entry_hash, prev, body.to_string()],
    )?;
    Ok(StoredEvent {
        sequence: seq,
        stream: stream_name.to_string(),
        entry_hash,
        previous_hash: prev,
        body,
    })
}

/// Read a full stream in sequence order.
///
/// # Errors
///
/// Returns an error when SQLite fails to prepare, read, or decode a row.
pub fn read_stream(
    conn: &Connection,
    stream: EventStream,
) -> Result<Vec<StoredEvent>, EvidenceError> {
    let mut stmt = conn.prepare(
        "SELECT sequence, entry_hash, previous_hash, body FROM events WHERE stream = ?1 ORDER BY sequence ASC",
    )?;
    let rows = stmt.query_map(params![stream.as_str()], |r| {
        let body_text: String = r.get(3)?;
        Ok(StoredEvent {
            sequence: r.get::<_, i64>(0)?.cast_unsigned(),
            stream: stream.as_str().to_string(),
            entry_hash: r.get(1)?,
            previous_hash: r.get(2)?,
            body: serde_json::from_str(&body_text).unwrap_or(serde_json::Value::Null),
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(EvidenceError::from)
}

/// One stored checkpoint row: `body` is the JSON of an
/// `ald_checkpoint::UnsignedCheckpointManifest` (parsed by the verifier,
/// which owns the checkpoint contract), `checkpoint_hash` its claimed hash.
#[derive(Debug, Clone)]
pub struct StoredCheckpoint {
    pub sequence: u64,
    pub body: String,
    pub checkpoint_hash: String,
}

/// Read all checkpoint rows in sequence order.
///
/// # Errors
///
/// Returns an error when SQLite fails to prepare, read, or decode a row.
pub fn read_checkpoints(conn: &Connection) -> Result<Vec<StoredCheckpoint>, EvidenceError> {
    let mut stmt = conn
        .prepare("SELECT sequence, body, checkpoint_hash FROM checkpoints ORDER BY sequence ASC")?;
    let rows = stmt.query_map([], |r| {
        Ok(StoredCheckpoint {
            sequence: r.get::<_, i64>(0)?.cast_unsigned(),
            body: r.get(1)?,
            checkpoint_hash: r.get(2)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(EvidenceError::from)
}

/// Verify a stream's hash chain from genesis.
///
/// # Errors
///
/// Returns an error when the chain is broken (sequence, link, or hash mismatch), or SQLite fails.
pub fn verify_stream(conn: &Connection, stream: EventStream) -> Result<u64, EvidenceError> {
    let events = read_stream(conn, stream)?;
    let mut prev = ald_types::GENESIS_HASH.to_string();
    for e in &events {
        if e.previous_hash != prev {
            return Err(EvidenceError::Stream(format!(
                "chain break at seq {}",
                e.sequence
            )));
        }
        let body = e.body.as_object().cloned().unwrap_or_default();
        let recomputed = compute_entry_hash(stream, &body)?;
        if recomputed != e.entry_hash {
            return Err(EvidenceError::Stream(format!(
                "entry hash mismatch at seq {}",
                e.sequence
            )));
        }
        prev.clone_from(&e.entry_hash);
    }
    Ok(events.len() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_and_verify_chain() {
        let conn = open(":memory:").unwrap();
        for i in 0..3 {
            let mut m = serde_json::Map::new();
            m.insert("i".to_string(), serde_json::json!(i));
            append_event(&conn, EventStream::Channel, m).unwrap();
        }
        assert_eq!(verify_stream(&conn, EventStream::Channel).unwrap(), 3);
    }

    #[test]
    fn tamper_detected() {
        let conn = open(":memory:").unwrap();
        let mut m = serde_json::Map::new();
        m.insert("i".to_string(), serde_json::json!(1));
        append_event(&conn, EventStream::Channel, m).unwrap();
        conn.execute("UPDATE events SET body = '{}' WHERE sequence = 0", [])
            .unwrap();
        assert!(verify_stream(&conn, EventStream::Channel).is_err());
    }

    #[test]
    fn checkpoints_read_in_sequence_order() {
        let conn = open(":memory:").unwrap();
        assert!(read_checkpoints(&conn).unwrap().is_empty());
        for seq in [1u64, 0] {
            conn.execute(
                "INSERT INTO checkpoints(sequence, body, checkpoint_hash) VALUES(?1,?2,?3)",
                params![
                    seq.cast_signed(),
                    format!("{{\"s\":{seq}}}"),
                    format!("h{seq}")
                ],
            )
            .unwrap();
        }
        let rows = read_checkpoints(&conn).unwrap();
        assert_eq!(
            rows.iter().map(|r| r.sequence).collect::<Vec<_>>(),
            vec![0, 1]
        );
        assert_eq!(rows[0].checkpoint_hash, "h0");
    }

    #[test]
    fn float_cycle_values_verify() {
        // Regression: f64 values whose legacy decimal print does not
        // parse back to the same double (k8.idx215 seq 142/233) must
        // still produce verifiable evidence. f_a prints legacy-short as
        // "1.929800299119496e-9" but parses to a neighboring double.
        let fa: f64 = serde_json::from_str::<serde_json::Value>("1.9298002991194962e-9")
            .unwrap()
            .as_f64()
            .unwrap();
        assert_eq!(
            serde_json::Value::from(fa).to_string(),
            "1.929800299119496e-9",
            "precondition: legacy printer emits the unstable form"
        );
        let fb: f64 = serde_json::from_str::<serde_json::Value>("1.929800299119496e-9")
            .unwrap()
            .as_f64()
            .unwrap();
        assert_ne!(fa.to_bits(), fb.to_bits(), "precondition: 2-cycle");
        let conn = open(":memory:").unwrap();
        let mut m = serde_json::Map::new();
        m.insert("native".to_string(), serde_json::json!([fa, fb, 0.25]));
        append_event(&conn, EventStream::Turns, m).unwrap();
        assert_eq!(verify_stream(&conn, EventStream::Turns).unwrap(), 1);
    }
}
