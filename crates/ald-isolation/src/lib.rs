//! Isolation modes: prototype (in-process, logical separation) and
//! research-grade (separate-process boundary contract).
//!
//! Rust port of `packages/isolation/src`: the mode contract table plus the
//! transport boundary check. Research-grade process spawning is expressed as
//! an explicit `Boundary` the runtime must satisfy; the prototype transport
//! is in-process channels.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum IsolationError {
    #[error("isolation violation: {0}")]
    Violation(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IsolationMode {
    Prototype,
    ResearchGrade,
}

impl IsolationMode {
    #[must_use]
    pub const fn process_boundary_between_babies(self) -> &'static str {
        match self {
            Self::Prototype => "none-shared-process",
            Self::ResearchGrade => "separate-process-or-container-per-baby",
        }
    }

    #[must_use]
    pub const fn claim_boundary(self) -> &'static str {
        match self {
            Self::Prototype => "This run used Prototype Mode isolation. It demonstrates protocol, ledger, and orchestration correctness. It does not support a channel-isolation or side-channel-resistance claim, because both Babies executed in the same process.",
            Self::ResearchGrade => "This run used Research-Grade Mode isolation under the documented threat model. It supports a practical side-channel-resistance claim against the enumerated channels. It is not a formally verified isolation proof.",
        }
    }
}

/// Logical in-process transport: two mailboxes, no shared mutable state
/// except through this explicit channel record.
#[derive(Debug, Default)]
pub struct PrototypeTransport {
    pub a_to_b: Vec<Vec<u8>>,
    pub b_to_a: Vec<Vec<u8>>,
}

impl PrototypeTransport {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn send_a_to_b(&mut self, msg: Vec<u8>) {
        self.a_to_b.push(msg);
    }

    pub fn send_b_to_a(&mut self, msg: Vec<u8>) {
        self.b_to_a.push(msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prototype_disclaims_isolation() {
        assert!(IsolationMode::Prototype.claim_boundary().contains("does not support"));
    }

    #[test]
    fn transport_is_directional() {
        let mut t = PrototypeTransport::new();
        t.send_a_to_b(vec![1]);
        assert_eq!(t.a_to_b.len(), 1);
        assert!(t.b_to_a.is_empty());
    }
}
