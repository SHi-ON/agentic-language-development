//! Operations: failure correlation digests and retention policy.
//!
//! Rust port of `packages/ops/src`: operator-only failure digests
//! (`SHA-256("dtsf-ops-failure-message-v1" || 0x00 || message)`) and the
//! retention window check.

use ald_hashing::domain_hash;

#[must_use]
pub fn failure_correlation_digest(message: &str) -> String {
    domain_hash("dtsf-ops-failure-message-v1", &[message.as_bytes()])
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetentionDecision {
    Retain,
    Expire,
}

/// Retain artifacts newer than `window_days`; expire older ones.
#[must_use]
pub const fn retention_decision(age_days: u64, window_days: u64) -> RetentionDecision {
    if age_days <= window_days {
        RetentionDecision::Retain
    } else {
        RetentionDecision::Expire
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_stable_and_prefixed() {
        let a = failure_correlation_digest("boom");
        assert!(a.starts_with("sha256:"));
        assert_eq!(a, failure_correlation_digest("boom"));
    }

    #[test]
    fn retention_window() {
        assert_eq!(retention_decision(3, 30), RetentionDecision::Retain);
        assert_eq!(retention_decision(31, 30), RetentionDecision::Expire);
    }
}
