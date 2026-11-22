//! ALA dyad orchestration: seeds, workload, manifest, and the dyad runner.
//!
//! Seeds, workload, manifest, and runner, plus the LV01 experiment
//! wiring used by the ALA path. The runner executes one full dyad
//! deterministically from the frozen
//! manifest and writes the evidence database, journal lines, and receipt.

pub mod dev_options;
pub mod journal;
pub mod manifest;
pub mod run;
pub mod run_support;
pub mod seeds;
pub mod workload;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum OrchestratorError {
    #[error("orchestrator domain error: {0}")]
    Domain(String),
    #[error("io: {0}")]
    Io(String),
    #[error("evidence: {0}")]
    Evidence(#[from] ald_evidence::EvidenceError),
    #[error("analysis: {0}")]
    Analysis(#[from] ald_analysis::AnalysisError),
    #[error("learner: {0}")]
    Learner(#[from] ald_learners::LearnerError),
}

pub use dev_options::{
    development_config_hash, development_config_short_id, drop_draw_less_than,
    parse_active_sender_tokens, parse_drop_probability, parse_held_out_types, parse_learning_mode,
    parse_training_interactions, resolve_development_options, resolved_is_default,
    DevelopmentOptions, DropProbability, LearningMode, ResolvedDevelopment,
    ACTIVE_SENDER_TOKEN_CHOICES, DEFAULT_ACTIVE_SENDER_TOKENS, DEFAULT_DROP_DENOMINATOR,
    DEFAULT_DROP_NUMERATOR, DEVELOPMENT_COHORT, DEVELOPMENT_CONFIG_DOMAIN,
};
pub use journal::{journal_append_entry, verify_journal_chain, JOURNAL_DOMAIN};
pub use manifest::{execution_counts_from_manifest, DyadReceipt, DyadRequest};
pub use run::run_dyad;
pub use run_support::{candidate_types, stable_f64};
pub use seeds::{
    derive_ala_seed, AlaSeedInput, ALA_SEED_ROOT, ALA_SEED_ROOT_V2, ALA_STUDY_ID, SEED_PURPOSES,
};
pub use workload::{
    prototype_workload, study_counts_from_protocol, CollectionWorkload, StudyCounts,
};
