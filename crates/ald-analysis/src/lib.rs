//! ALD analysis toolkit: descriptive stats, special functions, bootstrap,
//! LV01 predictors, ALA-LV dyad estimation, and V3 control contrasts.
//!
//! Estimation-only: no supported labels, no power claims.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AnalysisError {
    #[error("analysis domain error: {0}")]
    Domain(String),
    #[error("analysis empty sample: {0}")]
    EmptySample(String),
    #[error("analysis length mismatch: {0}")]
    LengthMismatch(String),
}

pub mod artifacts;
pub mod contrasts;
pub mod dyad;
pub mod ledger;
pub mod special;
pub mod stats;

pub use artifacts::{branch_cases, BranchOutcomeArtifact, CaseProbabilities, ROLES};
pub use contrasts::{
    brier_control_contrasts, fidelity, invalidity_bounds, simultaneous_bounds,
    success_control_contrasts, CONTRAST_IDS, CONTROL_BRANCHES, FAMILY_ALPHA, FAMILY_SIZE,
    PRIMARY_ADMISSIBLE, SUCCESS_CONTRAST_ADMISSIBLE,
};
pub use dyad::{analyze_dyad, analyze_role, cohort_interval};
pub use ledger::{brier, select_ordinary_predictor, PredictorId, PREDICTOR_IDS};
pub use special::{normal_cdf, normal_quantile, student_t_cdf, student_t_quantile};
pub use stats::{
    bootstrap_mean_ci, bootstrap_mean_replicates, holm_bonferroni, mean, one_sample_t,
    percentile_interval, quantile_sorted, summarize, wilson_interval,
};
