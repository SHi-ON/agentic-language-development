//! Learner adapters: transcript/task-history training index and prediction.
//!
//! Rust port of the learner surface used by the LV01 path
//! (`indexLv01TrainingLedger` in `packages/learners/src`): a count-based
//! training ledger over delivered tokens, plus transcript-only and
//! task-history prediction from the fitted counts.

use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LearnerError {
    #[error("learner domain error: {0}")]
    Domain(String),
}

#[derive(Debug, Clone, Default)]
pub struct TrainingLedger {
    /// token -> action counts (Laplace +1 handled at predict time).
    pub by_token: HashMap<String, [f64; 4]>,
    pub marginal: [f64; 4],
    pub total: u64,
}

impl TrainingLedger {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn index(&mut self, token: Option<&str>, action: usize) {
        if action >= 4 {
            return;
        }
        self.marginal[action] += 1.0;
        self.total += 1;
        let key = token.unwrap_or("<disabled>").to_string();
        self.by_token.entry(key).or_insert([0.0; 4])[action] += 1.0;
    }

    /// Transcript-only prediction: token-conditional distribution, Laplace-smoothed.
    pub fn predict_transcript(&self, token: Option<&str>) -> [f64; 4] {
        let key = token.unwrap_or("<disabled>");
        let counts = self.by_token.get(key).copied().unwrap_or([0.0; 4]);
        let total: f64 = counts.iter().sum::<f64>() + 4.0;
        let mut out = [0.0; 4];
        for (i, o) in out.iter_mut().enumerate() {
            *o = (counts[i] + 1.0) / total;
        }
        out
    }

    /// Task-history prediction: marginal distribution, Laplace-smoothed.
    pub fn predict_marginal(&self) -> [f64; 4] {
        let total: f64 = self.marginal.iter().sum::<f64>() + 4.0;
        let mut out = [0.0; 4];
        for (i, o) in out.iter_mut().enumerate() {
            *o = (self.marginal[i] + 1.0) / total;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcript_prediction_sums_to_one() {
        let mut l = TrainingLedger::new();
        for _ in 0..10 {
            l.index(Some("tok01"), 0);
        }
        let p = l.predict_transcript(Some("tok01"));
        assert!((p.iter().sum::<f64>() - 1.0).abs() < 1e-12);
        assert!(p[0] > p[1]);
    }

    #[test]
    fn unknown_token_falls_back_to_uniform() {
        let l = TrainingLedger::new();
        assert_eq!(l.predict_transcript(Some("nope")), [0.25; 4]);
    }
}
