//! Shared ALD domain separators, hash/stream contracts, and ALA-LV types.
//!
//! Port of `packages/types/src/domains.ts` + the ALA-relevant subset of
//! contracts/schemas: every hash is `sha256:<64 hex>`, construction is
//! `SHA-256(domain || 0x00 || payload)` unless noted.

use serde::{Deserialize, Serialize};

pub const GENESIS_HASH: &str =
    "sha256:0000000000000000000000000000000000000000000000000000000000000000";

pub mod hash_domains {
    pub const LEDGER_ENTRY: &str = "dtsf-agent-ledger-entry-v1";
    pub const MERKLE_LEAF: &str = "dtsf-merkle-leaf-v1";
    pub const MERKLE_NODE: &str = "dtsf-merkle-node-v1";
    pub const CHECKPOINT: &str = "dtsf-ledger-checkpoint-v1";
    pub const CARRIER_MARK: &str = "dtsf-carrier-mark-v1";
    pub const REPLAY_DIGEST: &str = "dtsf-replay-digest-v1";
    pub const CHANNEL_EVENT: &str = "dtsf-channel-event-v1";
    pub const AFFECT_EVENT: &str = "dtsf-affect-event-v1";
    pub const AUDIT_LEDGER_ENTRY: &str = "dtsf-audit-ledger-entry-v1";
    pub const INTERVENTION_EVENT: &str = "dtsf-intervention-event-v1";
    pub const TURN_RECORD: &str = "dtsf-turn-record-v1";
    pub const AGENT_PROPOSAL: &str = "dtsf-agent-proposal-v1";
    pub const REJECTED_PAYLOAD: &str = "dtsf-rejected-payload-v1";
    pub const RUN_CONFIG: &str = "dtsf-run-config-v1";
    pub const RUN_ID: &str = "dtsf-run-id-v1";
    pub const RUN_MANIFEST: &str = "dtsf-run-manifest-v1";
    pub const PRE_REGISTRATION: &str = "dtsf-preregistration-v1";
    pub const SCENARIO_BUNDLE: &str = "dtsf-scenario-bundle-v1";
    pub const SCENARIO_STATE: &str = "dtsf-scenario-state-v1";
    pub const OBSERVATION: &str = "dtsf-observation-v1";
    pub const ACTION: &str = "dtsf-action-v1";
    pub const OUTCOME: &str = "dtsf-outcome-v1";
    pub const SEED: &str = "dtsf-seed-v1";
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EventStream {
    #[serde(rename = "agent-0-ledger")]
    Agent0Ledger,
    #[serde(rename = "agent-1-ledger")]
    Agent1Ledger,
    #[serde(rename = "channel")]
    Channel,
    #[serde(rename = "affect")]
    Affect,
    #[serde(rename = "audit")]
    Audit,
    #[serde(rename = "turns")]
    Turns,
    #[serde(rename = "intervention")]
    Intervention,
}

impl EventStream {
    #[must_use]
    pub const fn hash_domain(self) -> &'static str {
        match self {
            Self::Agent0Ledger | Self::Agent1Ledger => hash_domains::LEDGER_ENTRY,
            Self::Channel => hash_domains::CHANNEL_EVENT,
            Self::Affect => hash_domains::AFFECT_EVENT,
            Self::Audit => hash_domains::AUDIT_LEDGER_ENTRY,
            Self::Turns => hash_domains::TURN_RECORD,
            Self::Intervention => hash_domains::INTERVENTION_EVENT,
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Agent0Ledger => "agent-0-ledger",
            Self::Agent1Ledger => "agent-1-ledger",
            Self::Channel => "channel",
            Self::Affect => "affect",
            Self::Audit => "audit",
            Self::Turns => "turns",
            Self::Intervention => "intervention",
        }
    }
}

pub const SIGNATURE_FIELDS: [&str; 2] = ["entryHash", "writerSignature"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AgentRole {
    #[serde(rename = "agent-0")]
    Agent0,
    #[serde(rename = "agent-1")]
    Agent1,
}

impl AgentRole {
    #[must_use]
    pub const fn agent_id(self) -> char {
        match self {
            Self::Agent0 => '0',
            Self::Agent1 => '1',
        }
    }

    #[must_use]
    pub const fn other(self) -> Self {
        match self {
            Self::Agent0 => Self::Agent1,
            Self::Agent1 => Self::Agent0,
        }
    }
}

/// Minimal RunConfig subset governing deterministic ALA replay.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunConfig {
    pub random_seed: String,
    #[serde(default)]
    pub deployment_mode: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AlaCohort {
    A,
    B,
    #[serde(rename = "development")]
    Development,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AlaSlotKind {
    Primary,
    Reserve,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AlaTerminalState {
    Valid,
    Invalid,
    Aborted,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_domains_match_ts() {
        assert_eq!(
            EventStream::Agent0Ledger.hash_domain(),
            "dtsf-agent-ledger-entry-v1"
        );
        assert_eq!(
            EventStream::Channel.hash_domain(),
            "dtsf-channel-event-v1"
        );
        assert_eq!(EventStream::Turns.hash_domain(), "dtsf-turn-record-v1");
    }

    #[test]
    fn role_helpers() {
        assert_eq!(AgentRole::Agent0.agent_id(), '0');
        assert_eq!(AgentRole::Agent0.other(), AgentRole::Agent1);
    }
}
