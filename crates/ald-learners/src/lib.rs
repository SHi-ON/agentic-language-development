//! Learner adapters: emergent tabular sender, count-based type ledger, and
//! the legacy transcript/task-history training index.
//!
//! The LV01 path historically used [`TrainingLedger`] (a count-based ledger
//! over delivered string tokens). The emergent redesign mirrors the proven
//! feasibility probe (`/tmp/emerge_probe.py`: single-role emergence with
//! sender sharpness 0.93 and Brier advantage +0.56):
//!
//! - [`TabularSender`]: per-type logits over a token vocabulary, softmax
//!   sampling via [`ald_hashing::SeededPrng`] streams, and a REINFORCE
//!   update with a per-type running baseline.
//! - [`TypeLedger`]: count-based receiver mapping delivered tokens to type
//!   posteriors and position posteriors over candidate codes.
//!
//! Determinism conventions (shared with the orchestrator): floats that enter
//! persisted artifacts go through [`stable_f64`], and every serialized map
//! is a `BTreeMap` so bytes are byte-identical across runs.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Number of sender types / receiver type hypotheses (LV01 single-role).
pub const NUM_TYPES: usize = 16;

/// REINFORCE step size for [`TabularSender::update`]. Matches the probe.
pub const SENDER_LEARNING_RATE: f64 = 1.0;

#[derive(Debug, Error)]
pub enum LearnerError {
    #[error("learner domain error: {0}")]
    Domain(String),
}

/// Round-trip stabilization for floats that enter hashed records.
///
/// Copy of the orchestrator convention: `serde_json`'s default float parser
/// is not correctly rounded, so `parse(format(v))` can differ from `v` by 1
/// ulp. Iterate to the parse/format fixpoint so every hashed float survives
/// a store/load round-trip bit-identically.
fn stable_f64(v: f64) -> f64 {
    let mut cur = v;
    for _ in 0..16 {
        let s = serde_json::Value::from(cur).to_string();
        let next: f64 = serde_json::from_str(&s).unwrap_or(cur);
        if next.to_bits() == cur.to_bits() {
            return cur;
        }
        cur = next;
    }
    cur
}

fn softmax(logits: &[f64]) -> Vec<f64> {
    let max = logits.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let mut out = Vec::with_capacity(logits.len());
    let mut sum = 0.0;
    for &l in logits {
        let e = (l - max).exp();
        out.push(e);
        sum += e;
    }
    for p in out.iter_mut() {
        *p /= sum;
    }
    out
}

/// Tabular emergent sender: `NUM_TYPES x num_tokens` logits.
///
/// Sampling draws from the per-type softmax via the caller's
/// [`ald_hashing::SeededPrng`] stream (derive one stream per sender, e.g.
/// `rng.derive("sender")`, so draws are reproducible). Learning is
/// REINFORCE with a per-type running-mean baseline, exactly as in the
/// feasibility probe.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TabularSender {
    num_tokens: usize,
    /// Logits indexed `[type_idx][token]`; every row has `num_tokens` entries.
    logits: Vec<Vec<f64>>,
    /// Running-mean reward baseline per type.
    baseline: [f64; NUM_TYPES],
    /// Observation counts backing the running-mean baselines.
    baseline_counts: [u64; NUM_TYPES],
}

impl TabularSender {
    /// Fresh sender with uniform rows (all-zero logits).
    ///
    /// # Panics
    ///
    /// Panics when `num_tokens` is zero: sampling from an empty vocabulary
    /// is undefined.
    #[must_use]
    pub fn new(num_tokens: usize) -> Self {
        assert!(num_tokens > 0, "TabularSender needs a non-empty vocabulary");
        Self {
            num_tokens,
            logits: vec![vec![0.0; num_tokens]; NUM_TYPES],
            baseline: [0.0; NUM_TYPES],
            baseline_counts: [0; NUM_TYPES],
        }
    }

    /// Vocabulary size.
    #[must_use]
    pub fn num_tokens(&self) -> usize {
        self.num_tokens
    }

    /// Softmax token distribution for one type (sums to 1).
    #[must_use]
    pub fn probs(&self, type_idx: usize) -> Vec<f64> {
        softmax(&self.logits[type_idx])
    }

    /// Running-mean reward baseline for one type.
    #[must_use]
    pub fn baseline(&self, type_idx: usize) -> f64 {
        self.baseline[type_idx]
    }

    /// Sample a token from the type's softmax using `rng`.
    ///
    /// A single `next_float` draw walks the cumulative distribution, so the
    /// sample sequence is fully determined by the stream state.
    pub fn sample(&self, type_idx: usize, rng: &mut ald_hashing::SeededPrng) -> usize {
        let p = self.probs(type_idx);
        let u = rng.next_float();
        let mut cum = 0.0;
        for (i, &pi) in p.iter().enumerate() {
            cum += pi;
            if u < cum {
                return i;
            }
        }
        self.num_tokens - 1
    }

    /// REINFORCE update for the sampled `(type_idx, token)` pair.
    ///
    /// Advances the type's running baseline toward `reward`, then moves the
    /// logits along `advantage * (one_hot(token) - probs)` with
    /// [`SENDER_LEARNING_RATE`]. Non-finite rewards are ignored so a broken
    /// reward signal can never poison the persisted policy.
    ///
    /// # Panics
    ///
    /// Panics when `type_idx` or `token` is out of range.
    pub fn update(&mut self, type_idx: usize, token: usize, reward: f64) {
        if !reward.is_finite() {
            return;
        }
        assert!(type_idx < NUM_TYPES, "TabularSender type out of range");
        assert!(token < self.num_tokens, "TabularSender token out of range");
        let p = self.probs(type_idx);
        self.baseline_counts[type_idx] += 1;
        let n = self.baseline_counts[type_idx] as f64;
        let base = self.baseline[type_idx] + (reward - self.baseline[type_idx]) / n;
        self.baseline[type_idx] = stable_f64(base);
        let adv = reward - self.baseline[type_idx];
        let row = &mut self.logits[type_idx];
        for (j, slot) in row.iter_mut().enumerate() {
            let target = if j == token { 1.0 } else { 0.0 };
            *slot = stable_f64(*slot + SENDER_LEARNING_RATE * adv * (target - p[j]));
        }
    }

    /// Deterministic policy bytes for artifact persistence.
    ///
    /// Field order is fixed and logits serialize in row-major order, so the
    /// same policy always yields identical bytes.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("TabularSender serialization is infallible")
    }

    /// Restore a policy persisted with [`TabularSender::to_bytes`].
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, LearnerError> {
        let sender: Self =
            serde_json::from_slice(bytes).map_err(|e| LearnerError::Domain(e.to_string()))?;
        if sender.num_tokens == 0
            || sender.logits.len() != NUM_TYPES
            || sender
                .logits
                .iter()
                .any(|row| row.len() != sender.num_tokens)
            || sender
                .logits
                .iter()
                .flatten()
                .chain(sender.baseline.iter())
                .any(|v| !v.is_finite())
        {
            return Err(LearnerError::Domain(
                "TabularSender bytes have a bad shape or non-finite floats".to_string(),
            ));
        }
        Ok(sender)
    }
}

/// Count-based receiver: `counts[token][type]` with Laplace-smoothed
/// posteriors.
///
/// Mirrors the probe receiver: token-conditional type distributions with a
/// +1 prior, plus a position posterior over the 4 candidate codes of a case.
/// Dropped (`None`) tokens are never fitted and always yield exact uniform
/// posteriors (transcript semantics shared with [`TrainingLedger`]).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TypeLedger {
    /// Token id -> observation counts per type. `BTreeMap` keeps persisted
    /// bytes byte-identical across runs.
    counts: BTreeMap<usize, [u64; NUM_TYPES]>,
}

impl TypeLedger {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Fit one delivered observation. `None` (dropped token) is never
    /// fitted; out-of-range type indices are ignored like
    /// [`TrainingLedger::index`] ignores out-of-range actions.
    pub fn observe(&mut self, token: Option<usize>, type_idx: usize) {
        if type_idx >= NUM_TYPES {
            return;
        }
        if let Some(tok) = token {
            self.counts.entry(tok).or_insert([0; NUM_TYPES])[type_idx] += 1;
        }
    }

    /// Raw per-type counts for one token (all zeros when unseen).
    #[must_use]
    pub fn counts(&self, token: usize) -> [u64; NUM_TYPES] {
        self.counts.get(&token).copied().unwrap_or([0; NUM_TYPES])
    }

    /// `P(type | token)` with a Laplace +1 prior. `None` yields the exact
    /// uniform distribution.
    #[must_use]
    pub fn type_posterior(&self, token: Option<usize>) -> [f64; NUM_TYPES] {
        let uniform = 1.0 / NUM_TYPES as f64;
        let Some(tok) = token else {
            return [uniform; NUM_TYPES];
        };
        let row = self.counts(tok);
        let total = row.iter().sum::<u64>() as f64 + NUM_TYPES as f64;
        let mut out = [0.0; NUM_TYPES];
        for (i, o) in out.iter_mut().enumerate() {
            *o = (row[i] as f64 + 1.0) / total;
        }
        out
    }

    /// Posterior over the 4 candidate positions of a case: each position's
    /// mass is the Laplace-smoothed count of its candidate type under the
    /// delivered token, normalized. `None` yields exact `[0.25; 4]`.
    /// Out-of-range candidate codes carry only the prior mass.
    #[must_use]
    pub fn position_posterior(&self, token: Option<usize>, candidates: &[usize; 4]) -> [f64; 4] {
        if token.is_none() {
            return [0.25; 4];
        }
        let row = self.counts(token.unwrap());
        let mut mass = [0.0; 4];
        for (i, &c) in candidates.iter().enumerate() {
            mass[i] = if c < NUM_TYPES {
                row[c] as f64 + 1.0
            } else {
                1.0
            };
        }
        let sum: f64 = mass.iter().sum();
        mass.map(|m| m / sum)
    }

    /// Deterministic ledger bytes for artifact persistence.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("TypeLedger serialization is infallible")
    }

    /// Restore a ledger persisted with [`TypeLedger::to_bytes`].
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, LearnerError> {
        serde_json::from_slice(bytes).map_err(|e| LearnerError::Domain(e.to_string()))
    }
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
        // Dropped (None) tokens are never fitted into the token-conditional
        // model: there is no "<disabled>" pseudo-token.
        let Some(tok) = token else {
            return;
        };
        self.by_token.entry(tok.to_string()).or_insert([0.0; 4])[action] += 1.0;
    }

    /// Transcript-only prediction: token-conditional distribution, Laplace-smoothed.
    /// A missing token predicts exact uniform.
    pub fn predict_transcript(&self, token: Option<&str>) -> [f64; 4] {
        if token.is_none() {
            return [0.25; 4];
        }
        let counts = self
            .by_token
            .get(token.unwrap())
            .copied()
            .unwrap_or([0.0; 4]);
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

    fn argmax(xs: &[f64]) -> usize {
        xs.iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(i, _)| i)
            .unwrap()
    }

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

    #[test]
    fn none_token_is_exact_uniform_and_never_fitted() {
        let mut l = TrainingLedger::new();
        for _ in 0..10 {
            l.index(None, 0);
        }
        // No "<disabled>" pseudo-token is fitted ...
        assert!(l.by_token.is_empty());
        // ... and None predicts EXACT uniform.
        assert_eq!(l.predict_transcript(None), [0.25; 4]);

        let mut t = TypeLedger::new();
        t.observe(None, 0);
        assert_eq!(t.type_posterior(None), [1.0 / NUM_TYPES as f64; NUM_TYPES]);
        assert_eq!(t.position_posterior(None, &[0, 1, 2, 3]), [0.25; 4]);
    }

    #[test]
    fn sender_concentrates_after_scripted_reward() {
        // Scripted contrast: token 5 of type 3 always succeeds, token 7
        // always fails. The running baseline settles near 0.5, so token 5
        // earns a sustained positive advantage and must concentrate.
        let mut s = TabularSender::new(32);
        for _ in 0..200 {
            s.update(3, 5, 1.0);
            s.update(3, 7, 0.0);
        }
        let p = s.probs(3);
        assert_eq!(argmax(&p), 5);
        assert!(
            p[5] > 0.9,
            "rewarded token should concentrate, got {}",
            p[5]
        );
        assert!((p.iter().sum::<f64>() - 1.0).abs() < 1e-12);
        // Untouched rows stay uniform.
        assert!((s.probs(0)[0] - 1.0 / 32.0).abs() < 1e-12);
    }

    #[test]
    fn sender_and_ledger_are_deterministic_per_seed() {
        let run = |seed: &str| {
            let root = ald_hashing::SeededPrng::new(seed);
            let mut rng = root.derive("sender");
            let mut s = TabularSender::new(32);
            let mut t = TypeLedger::new();
            let mut samples = Vec::new();
            for step in 0..50 {
                let ty = step % NUM_TYPES;
                let tok = s.sample(ty, &mut rng);
                samples.push(tok);
                let reward = if tok == ty { 1.0 } else { 0.0 };
                s.update(ty, tok, reward);
                t.observe(Some(tok), ty);
            }
            (samples, s.to_bytes(), t.to_bytes())
        };
        let a = run("seed-7");
        let b = run("seed-7");
        assert_eq!(a.0, b.0, "same seed must sample identically");
        assert_eq!(a.1, b.1, "same seed must serialize identically");
        assert_eq!(a.2, b.2, "same seed must serialize identically");
        // Round-trips are lossless.
        let s2 = TabularSender::from_bytes(&a.1).unwrap();
        assert_eq!(serde_json::to_vec(&s2).unwrap(), a.1);
        let t2 = TypeLedger::from_bytes(&a.2).unwrap();
        assert_eq!(t2.to_bytes(), a.2);
        // Different seeds diverge.
        let c = run("other-seed");
        assert_ne!(a.0, c.0);
    }

    #[test]
    fn ledger_posteriors_normalize_and_match_laplace() {
        let mut t = TypeLedger::new();
        for _ in 0..3 {
            t.observe(Some(2), 1);
        }
        // P(type|tok2): type 1 has (3+1)/(3+16), the rest 1/19.
        let pt = t.type_posterior(Some(2));
        assert!((pt.iter().sum::<f64>() - 1.0).abs() < 1e-12);
        for (i, &v) in pt.iter().enumerate() {
            let expect = if i == 1 { 4.0 / 19.0 } else { 1.0 / 19.0 };
            assert!((v - expect).abs() < 1e-12, "type {i}: {v} != {expect}");
        }
        // Unseen token falls back to uniform.
        assert_eq!(
            t.type_posterior(Some(9)),
            [1.0 / NUM_TYPES as f64; NUM_TYPES]
        );
        // Position posterior over candidates [1,0,5,6]: masses 4,1,1,1.
        let pp = t.position_posterior(Some(2), &[1, 0, 5, 6]);
        assert!((pp.iter().sum::<f64>() - 1.0).abs() < 1e-12);
        for (got, expect) in pp.iter().zip([4.0 / 7.0, 1.0 / 7.0, 1.0 / 7.0, 1.0 / 7.0]) {
            assert!((got - expect).abs() < 1e-12);
        }
    }

    #[test]
    fn sender_and_ledger_co_training_sharpens_like_probe() {
        // End-to-end mirror of the feasibility probe: single-role loop with
        // a 1/8 drop rate, REINFORCE on receiver success. The senders must
        // sharpen well above the uniform 1/32 row max.
        let mut rng = ald_hashing::SeededPrng::new("emergence").derive("loop");
        let mut s = TabularSender::new(32);
        let mut t = TypeLedger::new();
        for _ in 0..1500 {
            let ty = rng.next_int(NUM_TYPES as u32).unwrap() as usize;
            let mut others = Vec::new();
            while others.len() < 3 {
                let c = rng.next_int(NUM_TYPES as u32).unwrap() as usize;
                if c != ty && !others.contains(&c) {
                    others.push(c);
                }
            }
            let mut cands = [ty, others[0], others[1], others[2]];
            // Fisher-Yates with the same stream.
            for i in (1..4).rev() {
                let j = rng.next_int(i as u32 + 1).unwrap() as usize;
                cands.swap(i, j);
            }
            let pos = cands.iter().position(|&c| c == ty).unwrap();
            let k = s.sample(ty, &mut rng);
            let tok = if rng.next_float() < 1.0 / 8.0 {
                None
            } else {
                Some(k)
            };
            let post = t.position_posterior(tok, &cands);
            let succ = if argmax(&post) == pos { 1.0 } else { 0.0 };
            if tok.is_some() {
                t.observe(tok, ty);
            }
            s.update(ty, k, succ);
        }
        let sharp: f64 = (0..NUM_TYPES)
            .map(|ty| s.probs(ty).into_iter().fold(0.0, f64::max))
            .sum::<f64>()
            / NUM_TYPES as f64;
        assert!(
            sharp > 0.5,
            "sender should sharpen above uniform 0.031, got {sharp}"
        );
    }
}
