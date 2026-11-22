//! Hash-chained run journal.

use crate::OrchestratorError;

pub const JOURNAL_DOMAIN: &str = "ala-journal-entry-v1";

/// Chain fields assigned by the journal itself; callers must not supply
/// them (a caller-supplied `seq`/`previousHash`/`entryHash` would fork
/// verification).
const JOURNAL_CHAIN_FIELDS: [&str; 3] = ["seq", "previousHash", "entryHash"];

/// Verify every line of an existing journal and return the next `seq` plus
/// the head `entryHash` to link. A missing journal is the empty chain
/// (`(0, GENESIS_HASH)`). Any violation — non-JSON line, missing chain
/// field, `seq` gap/replay, broken `previousHash` link, or `entryHash`
/// mismatch — fails BEFORE any write, so a corrupt chain is never extended.
///
/// Note: `kind`/`state` values are NOT restricted here. `run_dyad` writes
/// `attempted`/`terminal` entries; operator tooling (e.g. the watchdog)
/// appends `aborted`/`note` states through the same chain.
///
/// # Errors
///
/// Returns an error naming the first corrupt line (bad JSON, missing chain field, `seq` gap/replay, broken link, or `entryHash` mismatch), or when the file is unreadable.
pub fn verify_journal_chain(journal_path: &str) -> Result<(u64, String), OrchestratorError> {
    let mut expected_seq = 0u64;
    let mut prev = ald_types::GENESIS_HASH.to_string();
    if !std::path::Path::new(journal_path).exists() {
        return Ok((expected_seq, prev));
    }
    let text = std::fs::read_to_string(journal_path)
        .map_err(|e| OrchestratorError::Io(format!("journal unreadable: {e}")))?;
    for (idx, line) in text.lines().enumerate() {
        let lineno = idx + 1;
        let v: serde_json::Value = serde_json::from_str(line).map_err(|e| {
            OrchestratorError::Domain(format!("ALA journal line {lineno}: corrupt entry: {e}"))
        })?;
        let obj = v.as_object().ok_or_else(|| {
            OrchestratorError::Domain(format!("ALA journal line {lineno}: entry is not an object"))
        })?;
        let seq = obj
            .get("seq")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| {
                OrchestratorError::Domain(format!("ALA journal line {lineno}: entry without seq"))
            })?;
        if seq != expected_seq {
            return Err(OrchestratorError::Domain(format!(
                "ALA journal line {lineno}: seq {seq} breaks the chain (expected {expected_seq})"
            )));
        }
        let previous = obj
            .get("previousHash")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                OrchestratorError::Domain(format!(
                    "ALA journal line {lineno}: entry without previousHash"
                ))
            })?;
        if previous != prev {
            return Err(OrchestratorError::Domain(format!(
                "ALA journal line {lineno}: previousHash does not link the prior entryHash"
            )));
        }
        let entry = obj
            .get("entryHash")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                OrchestratorError::Domain(format!(
                    "ALA journal line {lineno}: entry without entryHash"
                ))
            })?;
        let mut body = obj.clone();
        body.remove("entryHash");
        let canonical = ald_hashing::canonical_json(&serde_json::Value::Object(body))
            .map_err(|e| OrchestratorError::Domain(format!("ALA journal line {lineno}: {e}")))?;
        let recomputed = ald_hashing::domain_hash(JOURNAL_DOMAIN, &[canonical.as_bytes()]);
        if recomputed != entry {
            return Err(OrchestratorError::Domain(format!(
                "ALA journal line {lineno}: entryHash mismatch (tampered entry)"
            )));
        }
        prev = entry.to_string();
        expected_seq += 1;
    }
    Ok((expected_seq, prev))
}

/// Append one caller-supplied body to a journal through the same verified
/// chain [`journal_append`] uses. Chain fields (`seq`, `previousHash`,
/// `entryHash`) are assigned here; a body that already carries them is
/// rejected so callers cannot fork verification.
///
/// # Errors
///
/// Returns an error when the body carries chain fields, the existing chain is corrupt, or the file is unwritable.
pub fn journal_append_entry(
    journal_path: &str,
    body: serde_json::Map<String, serde_json::Value>,
) -> Result<String, OrchestratorError> {
    for f in JOURNAL_CHAIN_FIELDS {
        if body.contains_key(f) {
            return Err(OrchestratorError::Domain(format!(
                "ALA journal: body must not carry chain field {f:?} (assigned on append)"
            )));
        }
    }
    journal_append(journal_path, body)
}

/// Append one hash-chained line to the root journal
/// (`<evidence_root>/journal.jsonl`). Lines carry no timestamps so identical
/// runs hash identically; the chain links every dyad attempt in one root.
pub(crate) fn journal_append(
    journal_path: &str,
    mut body: serde_json::Map<String, serde_json::Value>,
) -> Result<String, OrchestratorError> {
    use std::io::Write as _;
    let (seq, prev) = verify_journal_chain(journal_path)?;
    if !std::path::Path::new(journal_path).exists() {
        if let Some(parent) = std::path::Path::new(journal_path).parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| OrchestratorError::Io(e.to_string()))?;
            }
        }
    }
    body.insert("seq".to_string(), serde_json::Value::from(seq));
    body.insert("previousHash".to_string(), serde_json::Value::String(prev));
    let canonical = ald_hashing::canonical_json(&serde_json::Value::Object(body.clone()))
        .map_err(|e| OrchestratorError::Domain(e.to_string()))?;
    let hash = ald_hashing::domain_hash(JOURNAL_DOMAIN, &[canonical.as_bytes()]);
    body.insert(
        "entryHash".to_string(),
        serde_json::Value::String(hash.clone()),
    );
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(journal_path)
        .map_err(|e| OrchestratorError::Io(format!("journal unwritable: {e}")))?;
    writeln!(f, "{}", serde_json::Value::Object(body))
        .map_err(|e| OrchestratorError::Io(format!("journal unwritable: {e}")))?;
    Ok(hash)
}
