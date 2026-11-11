//! Canonical JSON (RFC 8785 JCS), SHA-256 domain hashing, and the
//! deterministic `SeededPrng` (SHA-256 counter mode).
//!
//! JCS is implemented directly (sorted keys, minimal escaping) so the
//! crate has no non-audited canonicalization dependency.

use ald_types::SIGNATURE_FIELDS;
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum HashingError {
    #[error("Value cannot be represented as canonical JSON: {0}")]
    Unrepresentable(String),
}

pub const HASH_PREFIX: &str = "sha256:";

/// Serialize to RFC 8785 canonical JSON. Rejects values JSON cannot
/// represent (undefined/NaN/Infinity have no Rust equivalents; reject
/// non-finite f64, and require objects/arrays only).
pub fn canonical_json(value: &Value) -> Result<String, HashingError> {
    assert_canonical_value(value, "")?;
    let mut out = String::new();
    write_canonical(value, &mut out);
    Ok(out)
}

fn assert_canonical_value(value: &Value, path: &str) -> Result<(), HashingError> {
    match value {
        Value::Null | Value::Bool(_) | Value::String(_) => Ok(()),
        Value::Number(n) => {
            if n.as_f64().is_some_and(|f| !f.is_finite()) {
                return Err(HashingError::Unrepresentable(format!(
                    "non-finite number at {}",
                    if path.is_empty() { "<root>" } else { path }
                )));
            }
            Ok(())
        }
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                assert_canonical_value(item, &format!("{path}[{i}]"))?;
            }
            Ok(())
        }
        Value::Object(map) => {
            for (k, v) in map {
                let child = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                assert_canonical_value(v, &child)?;
            }
            Ok(())
        }
    }
}

fn write_canonical(value: &Value, out: &mut String) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => out.push_str(&n.to_string()),
        Value::String(s) => {
            out.push('"');
            for c in s.chars() {
                match c {
                    '"' => out.push_str("\\\""),
                    '\\' => out.push_str("\\\\"),
                    '\u{08}' => out.push_str("\\b"),
                    '\u{09}' => out.push_str("\\t"),
                    '\u{0A}' => out.push_str("\\n"),
                    '\u{0C}' => out.push_str("\\f"),
                    '\u{0D}' => out.push_str("\\r"),
                    c if (c as u32) < 0x20 => {
                        out.push_str(&format!("\\u{:04x}", c as u32));
                    }
                    c => out.push(c),
                }
            }
            out.push('"');
        }
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_canonical(item, out);
            }
            out.push(']');
        }
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (i, k) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                let mut key = String::new();
                write_canonical(&Value::String((*k).clone()), &mut key);
                out.push_str(&key);
                out.push(':');
                write_canonical(&map[*k], out);
            }
            out.push('}');
        }
    }
}

#[must_use]
pub fn sha256_bytes(parts: &[&[u8]]) -> [u8; 32] {
    let mut h = Sha256::new();
    for part in parts {
        h.update(part);
    }
    h.finalize().into()
}

#[must_use]
pub fn encode_hash(digest: &[u8; 32]) -> String {
    format!("{HASH_PREFIX}{}", hex::encode(digest))
}

pub fn is_sha256_hash(value: &str) -> bool {
    value.len() == 7 + 64
        && value.starts_with(HASH_PREFIX)
        && value[7..].chars().all(|c| c.is_ascii_hexdigit())
}

pub fn decode_hash(hash: &str) -> Result<[u8; 32], HashingError> {
    if !is_sha256_hash(hash) {
        return Err(HashingError::Unrepresentable(format!(
            "invalid hash encoding: {}",
            hash.chars().take(80).collect::<String>()
        )));
    }
    let mut out = [0u8; 32];
    hex::decode_to_slice(&hash[7..], &mut out)
        .map_err(|e| HashingError::Unrepresentable(e.to_string()))?;
    Ok(out)
}

#[must_use]
pub fn uint64_be(value: u64) -> [u8; 8] {
    value.to_be_bytes()
}

/// `SHA-256(domain || 0x00 || payload...)` as `sha256:<hex>`.
#[must_use]
pub fn domain_hash(domain: &str, payload: &[&[u8]]) -> String {
    let mut parts: Vec<&[u8]> = vec![domain.as_bytes(), &[0u8]];
    parts.extend_from_slice(payload);
    encode_hash(&sha256_bytes(&parts))
}

/// Domain-separated hash of the canonical form of `value`.
pub fn hash_canonical(domain: &str, value: &Value) -> Result<String, HashingError> {
    Ok(domain_hash(domain, &[canonical_json(value)?.as_bytes()]))
}

pub fn omit_signature_fields(
    event: &serde_json::Map<String, Value>,
) -> serde_json::Map<String, Value> {
    event
        .iter()
        .filter(|(k, _)| !SIGNATURE_FIELDS.contains(&k.as_str()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// Recompute an entry hash: canonicalize minus signature fields under the
/// stream domain.
pub fn compute_entry_hash(
    stream: ald_types::EventStream,
    event: &serde_json::Map<String, Value>,
) -> Result<String, HashingError> {
    let body = Value::Object(omit_signature_fields(event));
    hash_canonical(stream.hash_domain(), &body)
}

/// Deterministic SHA-256 counter-mode PRNG (`SeededPrng` stream
/// layout): key = SHA-256(seed-domain || 0x00 || utf8(seed)),
/// block = SHA-256(key || uint64BE(counter)).
pub struct SeededPrng {
    key: [u8; 32],
    counter: u64,
    block: [u8; 32],
    offset: usize,
    fresh: bool,
}

impl SeededPrng {
    #[must_use]
    pub fn new(seed: &str) -> Self {
        Self {
            key: sha256_bytes(&[ald_types::hash_domains::SEED.as_bytes(), &[0u8], seed.as_bytes()]),
            counter: 0,
            block: [0u8; 32],
            offset: 32,
            fresh: true,
        }
    }

    /// Independent child stream; same label always yields the same stream.
    /// Construction: `SeededPrng::new(hex(key) + "/" + label)`.
    #[must_use]
    pub fn derive(&self, label: &str) -> Self {
        Self::new(&format!("{}/{}", hex::encode(self.key), label))
    }

    fn refill(&mut self) {
        self.block = sha256_bytes(&[&self.key, &uint64_be(self.counter)]);
        self.counter += 1;
        self.offset = 0;
        self.fresh = false;
    }

    pub fn next_u32(&mut self) -> u32 {
        if self.offset + 4 > 32 {
            self.refill();
        }
        let mut b = [0u8; 4];
        b.copy_from_slice(&self.block[self.offset..self.offset + 4]);
        self.offset += 4;
        u32::from_be_bytes(b)
    }

    /// Uniform float in [0, 1) with 53 bits of precision.
    pub fn next_float(&mut self) -> f64 {
        let high = f64::from(self.next_u32() >> 5);
        let low = f64::from(self.next_u32() >> 6);
        (high * 67_108_864.0 + low) / 9_007_199_254_740_992.0
    }

    pub fn next_int(&mut self, max_exclusive: u32) -> Result<u32, HashingError> {
        if max_exclusive == 0 {
            return Err(HashingError::Unrepresentable(
                "maxExclusive must be a positive integer".to_string(),
            ));
        }
        let range = 4_294_967_296u64;
        let limit = range - (range % u64::from(max_exclusive));
        loop {
            let v = u64::from(self.next_u32());
            if v < limit {
                return Ok((v % u64::from(max_exclusive)) as u32);
            }
        }
    }
}

/// RESEARCH.md Appendix D: lowercase hex SHA-256 of parts joined by 0x00.
#[must_use]
pub fn derive_seed_hex(parts: &[&str]) -> String {
    let mut h = Sha256::new();
    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            h.update([0u8]);
        }
        h.update(part.as_bytes());
    }
    hex::encode(h.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn canonical_sorts_keys() {
        assert_eq!(
            canonical_json(&json!({"b": 1, "a": [2, 1]})).unwrap(),
            r#"{"a":[2,1],"b":1}"#
        );
    }

    #[test]
    fn non_finite_unrepresentable_by_construction() {
        // serde_json::Number guarantees finiteness: from_f64 returns None
        // for NaN/±Infinity, so such values can never reach canonical_json.
        // The is_finite guard in assert_canonical_value is defense-in-depth
        // for values built through other paths.
        assert!(serde_json::Number::from_f64(f64::INFINITY).is_none());
        assert!(serde_json::Number::from_f64(f64::NAN).is_none());
        let v = Value::from(f64::MAX);
        assert!(canonical_json(&v).is_ok());
    }

    #[test]
    fn prng_matches_ts_layout() {
        // key = SHA-256("dtsf-seed-v1" || 0x00 || "x"); first block must equal
        // SHA-256(key || uint64BE(0)).
        let mut rng = SeededPrng::new("x");
        let _ = rng.next_u32();
        let key = sha256_bytes(&[b"dtsf-seed-v1".as_slice(), &[0u8], b"x".as_slice()]);
        let expect = sha256_bytes(&[&key, &0u64.to_be_bytes()]);
        assert_eq!(rng.block, expect);
        assert_eq!(rng.counter, 1);
    }

    #[test]
    fn next_int_rejects_zero() {
        assert!(SeededPrng::new("s").next_int(0).is_err());
    }

    #[test]
    fn prng_matches_ts_oracle_seed_x() {
        // Reference oracle values for seed "x": three uint32s, then
        // next_float, then next_int(700).
        let mut rng = SeededPrng::new("x");
        assert_eq!(rng.next_u32(), 3255788481);
        assert_eq!(rng.next_u32(), 3306436351);
        assert_eq!(rng.next_u32(), 1446772966);
        assert_eq!(format!("{:.17}", rng.next_float()), "0.42330249837794243");
        assert_eq!(rng.next_int(700).unwrap(), 495);
    }

    #[test]
    fn canonical_matches_ts_oracle() {
        // Canonical form of {b:1,a:[2,1]} is '{"a":[2,1],"b":1}'.
        let v: Value = serde_json::from_str(r#"{"b":1,"a":[2,1]}"#).unwrap();
        assert_eq!(canonical_json(&v).unwrap(), r#"{"a":[2,1],"b":1}"#);
    }

    #[test]
    fn prng_matches_ts_oracle_d6_seed() {
        // Reference stream for seed "a136f247...df357a": first uint32,
        // first float, next_int(10000).
        let mut rng = SeededPrng::new(
            "a136f247e0b12755021e5027a4e7b5f41d3b630e418a609c2d30cfd995df357a",
        );
        assert_eq!(rng.next_u32(), 1574471825);
        assert_eq!(format!("{:.17}", rng.next_float()), "0.28304296965537734");
        assert_eq!(rng.next_int(10000).unwrap(), 8006);
    }

    #[test]
    fn ala_seed_matches_ts_oracle() {
        // Reference seed derivation
        // (A/primary/0300/scenario/agent-0-receiver, v2 root).
        assert_eq!(
            derive_seed_hex(&[
                "ala-ledger-value-v2",
                "ALA-LV",
                "A",
                "primary",
                "0300",
                "scenario",
                "agent-0-receiver"
            ]),
            "7fff5297553ff5a3b209a57a73b784984e9c4d1231d1999362220566d7fe8cbc"
        );
    }

    #[test]
    fn derive_seed_hex_joins_with_nul() {
        let mut h = Sha256::new();
        h.update(b"ald-e03-v1");
        h.update([0u8]);
        h.update(b"7");
        assert_eq!(derive_seed_hex(&["ald-e03-v1", "7"]), hex::encode(h.finalize()));
    }
}
