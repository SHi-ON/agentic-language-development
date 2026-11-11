//! SQLite evidence store: hash-chained event streams, checkpoints, export.
//!
//! Rust port of `packages/evidence/src/database.ts` + `export.ts` surface:
//! one SQLite file, one table per concern; every event carries `entryHash =
//! SHA-256(stream-domain || 0x00 || canonical(unsigned event))` and chains to
//! the previous hash. Single-writer discipline is the caller's contract.

use ald_hashing::{compute_entry_hash, HashingError};
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
    let mut stmt = conn.prepare("SELECT COALESCE(MAX(sequence), -1) + 1 FROM events WHERE stream = ?1")?;
    Ok(stmt.query_row(params![stream], |r| r.get(0))?)
}

/// Append one unsigned event body to a stream; returns the stored event.
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
    let entry_hash = compute_entry_hash(stream, &unsigned)?;
    unsigned.insert(
        "entryHash".to_string(),
        serde_json::Value::String(entry_hash.clone()),
    );
    let body = serde_json::Value::Object(unsigned);
    conn.execute(
        "INSERT INTO events(stream, sequence, entry_hash, previous_hash, body) VALUES(?1,?2,?3,?4,?5)",
        params![stream_name, seq as i64, entry_hash, prev, body.to_string()],
    )?;
    Ok(StoredEvent { sequence: seq, stream: stream_name.to_string(), entry_hash, previous_hash: prev, body })
}

/// Read a full stream in sequence order.
pub fn read_stream(conn: &Connection, stream: EventStream) -> Result<Vec<StoredEvent>, EvidenceError> {
    let mut stmt = conn.prepare(
        "SELECT sequence, entry_hash, previous_hash, body FROM events WHERE stream = ?1 ORDER BY sequence ASC",
    )?;
    let rows = stmt.query_map(params![stream.as_str()], |r| {
        let body_text: String = r.get(3)?;
        Ok(StoredEvent {
            sequence: r.get::<_, i64>(0)? as u64,
            stream: stream.as_str().to_string(),
            entry_hash: r.get(1)?,
            previous_hash: r.get(2)?,
            body: serde_json::from_str(&body_text).unwrap_or(serde_json::Value::Null),
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(EvidenceError::from)
}

/// Verify a stream's hash chain from genesis.
pub fn verify_stream(conn: &Connection, stream: EventStream) -> Result<u64, EvidenceError> {
    let events = read_stream(conn, stream)?;
    let mut prev = ald_types::GENESIS_HASH.to_string();
    for e in &events {
        if e.previous_hash != prev {
            return Err(EvidenceError::Stream(format!("chain break at seq {}", e.sequence)));
        }
        let body = e.body.as_object().cloned().unwrap_or_default();
        let recomputed = compute_entry_hash(stream, &body)?;
        if recomputed != e.entry_hash {
            return Err(EvidenceError::Stream(format!("entry hash mismatch at seq {}", e.sequence)));
        }
        prev = e.entry_hash.clone();
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
        conn.execute("UPDATE events SET body = '{}' WHERE sequence = 0", []).unwrap();
        assert!(verify_stream(&conn, EventStream::Channel).is_err());
    }
}
