//! Learner adapters: one symmetric emergent agent policy.
//!
//! [`AgentPolicy`] owns BOTH a sender table and a receiver table,
//! mirroring the unsupervised probe v4 (`/tmp/emerge_probe_v4.py`: two
//! agents alternating roles, receiver never sees the true type, REINFORCE
//! on the sampled candidate position; Brier advantage 0.52 @1500
//! episodes/direction, 0.62 @3000/direction).
//!
//! Determinism conventions (shared with the orchestrator): floats that enter
//! persisted artifacts go through [`stable_f64`] so bytes are
//! byte-identical across runs.
#![allow(clippy::cast_precision_loss)] // small counts/indices; exact for realistic inputs
#![allow(clippy::cast_possible_truncation)] // small counts/indices; bounded by construction
#![allow(clippy::cast_sign_loss)] // non-negative by construction (validated ranges, draws in [0, 1))

use ald_types::signaling::{TOKENS, TYPES};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// REINFORCE step size for [`AgentPolicy::update_sender`]. Matches probe v4.
pub const SENDER_LEARNING_RATE: f64 = 1.0;

/// REINFORCE step size for [`AgentPolicy::update_receiver`]. Matches probe v4.
pub const RECEIVER_LEARNING_RATE: f64 = 1.0;

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
    let max = logits.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let mut out = Vec::with_capacity(logits.len());
    let mut sum = 0.0;
    for &l in logits {
        let e = (l - max).exp();
        out.push(e);
        sum += e;
    }
    for p in &mut out {
        *p /= sum;
    }
    out
}

/// Sample one index from `probs` with a single `next_float` draw walking the
/// cumulative distribution, so the sample sequence is fully determined by
/// the stream state.
fn sample_from_probs(probs: &[f64], rng: &mut ald_hashing::SeededPrng) -> usize {
    let u = rng.next_float();
    let mut cum = 0.0;
    for (i, &p) in probs.iter().enumerate() {
        cum += p;
        if u < cum {
            return i;
        }
    }
    probs.len() - 1
}

/// One REINFORCE step on a logit row: advance the running-mean baseline
/// toward `reward`, then move the row along
/// `lr * advantage * (one_hot(target) - softmax(row))`. The softmax is taken
/// before the row moves, exactly as in probe v4.
fn reinforce_row(
    row: &mut [f64],
    baseline: &mut f64,
    count: &mut u64,
    target: usize,
    reward: f64,
    lr: f64,
) {
    let p = softmax(row);
    *count += 1;
    let n = *count as f64;
    *baseline = stable_f64(*baseline + (reward - *baseline) / n);
    let adv = reward - *baseline;
    for (j, slot) in row.iter_mut().enumerate() {
        let one_hot = if j == target { 1.0 } else { 0.0 };
        *slot = stable_f64(*slot + lr * adv * (one_hot - p[j]));
    }
}

/// Optional persisted policy metadata. Absent on every legacy policy, so
/// legacy [`AgentPolicy::to_bytes`] output is byte-identical with or without
/// this field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentPolicyMetadata {
    /// Sender vocabulary constraint: sampling, normalization, and updates
    /// use only tokens `0..active_sender_tokens`. The persisted tables keep
    /// the full 32-token shape regardless.
    pub active_sender_tokens: u32,
}

/// Symmetric emergent agent: sender logits `[TYPES][TOKENS]` plus
/// receiver logits `[TOKENS][TYPES]`, mirroring probe v4 exactly.
///
/// Unsupervised co-training alternates two instances as sender/receiver.
/// The receiver NEVER sees the true type: it samples a candidate position
/// from [`AgentPolicy::receiver_position_posterior`], both sides share the
/// episode reward, the sender REINFORCEs its sampled token with a per-type
/// baseline, and the receiver REINFORCEs toward the CHOSEN candidate's type
/// only with a per-token baseline. A dropped (`None`) token yields an exact
/// uniform posterior and no receiver update; the sender still updates on the
/// outcome, as in probe v4.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentPolicy {
    /// Sender logits indexed `[type_idx][token]`.
    sender_logits: Vec<Vec<f64>>,
    /// Receiver logits indexed `[token][type_idx]`.
    receiver_logits: Vec<Vec<f64>>,
    /// Running-mean reward baseline per sender type.
    sender_baseline: [f64; TYPES],
    /// Observation counts backing the sender baselines.
    sender_baseline_counts: [u64; TYPES],
    /// Running-mean reward baseline per received token.
    receiver_baseline: [f64; TOKENS],
    /// Observation counts backing the receiver baselines.
    receiver_baseline_counts: [u64; TOKENS],
    /// Development-only constraint record; `None` means the full 32-token
    /// sender. Skipped on the wire when absent, so default serialization is
    /// unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    metadata: Option<AgentPolicyMetadata>,
}

impl Default for AgentPolicy {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentPolicy {
    /// Fresh agent with uniform rows (all-zero logits).
    #[must_use]
    pub fn new() -> Self {
        Self {
            sender_logits: vec![vec![0.0; TOKENS]; TYPES],
            receiver_logits: vec![vec![0.0; TYPES]; TOKENS],
            sender_baseline: [0.0; TYPES],
            sender_baseline_counts: [0; TYPES],
            receiver_baseline: [0.0; TOKENS],
            receiver_baseline_counts: [0; TOKENS],
            metadata: None,
        }
    }

    /// Fresh agent whose sender is constrained to the first `k` tokens.
    /// Sampling, normalization ([`AgentPolicy::sender_probs`]), and sender
    /// updates all operate on tokens `0..k` only; probabilities outside are
    /// exact `0.0` and logits outside stay exactly `0.0`. The persisted
    /// tables keep the full 32-token shape, and `k` is recorded in metadata.
    /// The CLI/orchestrator restrict `k` to 8/16/32; this constructor accepts
    /// any `1..=TOKENS` and rejects anything else without fallback.
    ///
    /// # Errors
    ///
    /// Returns an error when `k` is outside `1..=TOKENS`.
    pub fn with_active_sender_tokens(k: usize) -> Result<Self, LearnerError> {
        if !(1..=TOKENS).contains(&k) {
            return Err(LearnerError::Domain(format!(
                "active sender tokens must be within 1..={TOKENS}, got {k}"
            )));
        }
        let mut policy = Self::new();
        if k != TOKENS {
            policy.metadata = Some(AgentPolicyMetadata {
                active_sender_tokens: k as u32,
            });
        }
        Ok(policy)
    }

    /// Number of sender tokens this policy samples from (`TOKENS` unless
    /// a development constraint is recorded in metadata).
    #[must_use]
    pub fn active_sender_tokens(&self) -> usize {
        self.metadata
            .as_ref()
            .map_or(TOKENS, |m| m.active_sender_tokens as usize)
    }

    /// Softmax token distribution the sender uses for one type (sums to 1).
    /// Under an active-token constraint the softmax normalizes over the
    /// first `k` logits only and pads exact `0.0` beyond; unconstrained
    /// policies take the legacy full-row path bit-identically.
    ///
    /// # Errors
    ///
    /// Returns [`LearnerError::Domain`] when `type_idx` is out of range.
    pub fn sender_probs(&self, type_idx: usize) -> Result<Vec<f64>, LearnerError> {
        if type_idx >= TYPES {
            return Err(LearnerError::Domain(format!(
                "sender type index {type_idx} out of range (TYPES = {TYPES})"
            )));
        }
        let k = self.active_sender_tokens();
        if k == TOKENS {
            return Ok(softmax(&self.sender_logits[type_idx]));
        }
        let mut probs = softmax(&self.sender_logits[type_idx][..k]);
        probs.resize(TOKENS, 0.0);
        Ok(probs)
    }

    /// Sample a token from the type's sender softmax using `rng` (one
    /// `next_float` draw, so the sequence is fully stream-determined).
    ///
    /// # Errors
    ///
    /// Returns [`LearnerError::Domain`] when `type_idx` is out of range.
    pub fn sample_token(
        &self,
        type_idx: usize,
        rng: &mut ald_hashing::SeededPrng,
    ) -> Result<usize, LearnerError> {
        Ok(sample_from_probs(&self.sender_probs(type_idx)?, rng))
    }

    /// Running-mean reward baseline for one sender type.
    ///
    /// # Errors
    ///
    /// Returns [`LearnerError::Domain`] when `type_idx` is out of range.
    pub fn sender_baseline(&self, type_idx: usize) -> Result<f64, LearnerError> {
        self.sender_baseline.get(type_idx).copied().ok_or_else(|| {
            LearnerError::Domain(format!(
                "sender type index {type_idx} out of range (TYPES = {TYPES})"
            ))
        })
    }

    /// Running-mean reward baseline for one received token.
    ///
    /// # Errors
    ///
    /// Returns [`LearnerError::Domain`] when `token` is out of range.
    pub fn receiver_baseline(&self, token: usize) -> Result<f64, LearnerError> {
        self.receiver_baseline.get(token).copied().ok_or_else(|| {
            LearnerError::Domain(format!(
                "receiver token index {token} out of range (TOKENS = {TOKENS})"
            ))
        })
    }

    /// Receiver type posterior `softmax(receiver_logits[token])`. `None`
    /// (dropped token) yields the exact uniform distribution.
    ///
    /// # Errors
    ///
    /// Returns [`LearnerError::Domain`] when `token` is `Some(t)` with `t`
    /// out of range.
    pub fn receiver_type_posterior(
        &self,
        token: Option<usize>,
    ) -> Result<[f64; TYPES], LearnerError> {
        let Some(tok) = token else {
            return Ok([1.0 / TYPES as f64; TYPES]);
        };
        let row = self.receiver_logits.get(tok).ok_or_else(|| {
            LearnerError::Domain(format!(
                "receiver token index {tok} out of range (TOKENS = {TOKENS})"
            ))
        })?;
        // SAFETY: every receiver row has TYPES entries by construction
        // (new/with_active_sender_tokens build full rows; from_bytes rejects
        // any other shape), so this conversion cannot fail on a live policy.
        let out: [f64; TYPES] = softmax(row)
            .try_into()
            .map_err(|_| LearnerError::Domain("receiver row has wrong length".to_string()))?;
        Ok(out)
    }

    /// Posterior over the 4 candidate positions of a case: the receiver
    /// type posterior restricted to the candidate types, renormalized.
    /// `None` yields exact `[0.25; 4]`.
    ///
    /// # Errors
    ///
    /// Returns [`LearnerError::Domain`] when `token` or any candidate type
    /// is out of range.
    pub fn receiver_position_posterior(
        &self,
        token: Option<usize>,
        candidates: &[usize; 4],
    ) -> Result<[f64; 4], LearnerError> {
        if token.is_none() {
            return Ok([0.25; 4]);
        }
        for (i, &c) in candidates.iter().enumerate() {
            if c >= TYPES {
                return Err(LearnerError::Domain(format!(
                    "candidate {i} type {c} out of range (TYPES = {TYPES})"
                )));
            }
        }
        let pt = self.receiver_type_posterior(token)?;
        let raw = candidates.map(|c| pt[c]);
        let sum: f64 = raw.iter().sum();
        Ok(raw.map(|x| x / sum))
    }

    /// Sample a candidate position from
    /// [`AgentPolicy::receiver_position_posterior`] using `rng`. This is the
    /// receiver's action during unsupervised training: the true type is
    /// never consulted.
    ///
    /// # Errors
    ///
    /// Returns [`LearnerError::Domain`] when `token` or any candidate type
    /// is out of range.
    pub fn sample_position(
        &self,
        token: Option<usize>,
        candidates: &[usize; 4],
        rng: &mut ald_hashing::SeededPrng,
    ) -> Result<usize, LearnerError> {
        Ok(sample_from_probs(
            &self.receiver_position_posterior(token, candidates)?,
            rng,
        ))
    }

    /// Sender REINFORCE update for the sampled `(type_idx, token)` pair with
    /// the per-type running baseline and [`SENDER_LEARNING_RATE`] (= 1.0,
    /// probe v4). Applied on every training episode, including dropped
    /// tokens: the sender still learns from the shared outcome. Non-finite
    /// rewards are ignored so a broken reward signal can never poison the
    /// persisted policy. Under an active-token constraint the REINFORCE
    /// step runs on the first-`k` sub-row only (baseline/counts still
    /// advance); logits at `k..` are never touched. Unconstrained policies
    /// take the legacy full-row path bit-identically.
    ///
    /// # Errors
    ///
    /// Returns [`LearnerError::Domain`] when `type_idx` or `token` is out
    /// of range, or when `token` is outside the active set under a
    /// constraint.
    pub fn update_sender(
        &mut self,
        type_idx: usize,
        token: usize,
        reward: f64,
    ) -> Result<(), LearnerError> {
        if !reward.is_finite() {
            return Ok(());
        }
        if type_idx >= TYPES {
            return Err(LearnerError::Domain(format!(
                "sender type index {type_idx} out of range (TYPES = {TYPES})"
            )));
        }
        if token >= TOKENS {
            return Err(LearnerError::Domain(format!(
                "sender token index {token} out of range (TOKENS = {TOKENS})"
            )));
        }
        let k = self.active_sender_tokens();
        if token >= k {
            return Err(LearnerError::Domain(format!(
                "sender token {token} outside the active set of {k}"
            )));
        }
        if k == TOKENS {
            let row = &mut self.sender_logits[type_idx];
            reinforce_row(
                row,
                &mut self.sender_baseline[type_idx],
                &mut self.sender_baseline_counts[type_idx],
                token,
                reward,
                SENDER_LEARNING_RATE,
            );
        } else {
            let row = &mut self.sender_logits[type_idx][..k];
            reinforce_row(
                row,
                &mut self.sender_baseline[type_idx],
                &mut self.sender_baseline_counts[type_idx],
                token,
                reward,
                SENDER_LEARNING_RATE,
            );
        }
        Ok(())
    }

    /// Receiver REINFORCE update on the SAMPLED candidate position with the
    /// per-token running baseline and [`RECEIVER_LEARNING_RATE`] (= 1.0,
    /// probe v4). The row moves toward `candidates[choice]` — the chosen
    /// candidate's type ONLY; the true type is never an input, so a wrong
    /// guess is reinforced exactly as sampled and only the shared reward
    /// (through the advantage) steers learning.
    ///
    /// `None` (dropped token) performs NO receiver update at all: logits,
    /// baseline, and counts are untouched. Non-finite rewards are ignored.
    ///
    /// # Errors
    ///
    /// Returns [`LearnerError::Domain`] when `token` is `Some(t)` with `t`
    /// out of range, when `choice >= 4`, or when the chosen candidate type
    /// is out of range.
    pub fn update_receiver(
        &mut self,
        token: Option<usize>,
        candidates: &[usize; 4],
        choice: usize,
        reward: f64,
    ) -> Result<(), LearnerError> {
        if !reward.is_finite() {
            return Ok(());
        }
        let Some(tok) = token else {
            return Ok(());
        };
        if tok >= TOKENS {
            return Err(LearnerError::Domain(format!(
                "receiver token index {tok} out of range (TOKENS = {TOKENS})"
            )));
        }
        if choice >= 4 {
            return Err(LearnerError::Domain(format!(
                "receiver choice {choice} out of range (4 candidates)"
            )));
        }
        let chosen_type = candidates[choice];
        if chosen_type >= TYPES {
            return Err(LearnerError::Domain(format!(
                "chosen candidate type {chosen_type} out of range (TYPES = {TYPES})"
            )));
        }
        let row = &mut self.receiver_logits[tok];
        reinforce_row(
            row,
            &mut self.receiver_baseline[tok],
            &mut self.receiver_baseline_counts[tok],
            chosen_type,
            reward,
            RECEIVER_LEARNING_RATE,
        );
        Ok(())
    }

    /// Deterministic policy bytes for artifact persistence.
    ///
    /// Field order is fixed and logits serialize in row-major order, so the
    /// same policy always yields identical bytes.
    ///
    /// # Panics
    ///
    /// Only if serializing the policy fails, which cannot happen for these
    /// field types (`serde_json` renders non-finite floats as null rather
    /// than erroring); the `expect` documents that invariant.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        // SAFETY: serializing this struct cannot fail — all fields are
        // natively serializable (Vecs, arrays, Option) and serde_json
        // renders non-finite floats as null rather than erroring.
        serde_json::to_vec(self).expect("AgentPolicy serialization is infallible")
    }

    /// Sender logit row for one type (read-only inspection).
    ///
    /// # Errors
    ///
    /// Returns [`LearnerError::Domain`] when `type_idx` is out of range.
    pub fn sender_logits(&self, type_idx: usize) -> Result<&[f64], LearnerError> {
        self.sender_logits
            .get(type_idx)
            .map(Vec::as_slice)
            .ok_or_else(|| {
                LearnerError::Domain(format!(
                    "sender type index {type_idx} out of range (TYPES = {TYPES})"
                ))
            })
    }

    /// Observation count backing one sender type's baseline.
    ///
    /// # Errors
    ///
    /// Returns [`LearnerError::Domain`] when `type_idx` is out of range.
    pub fn sender_baseline_count(&self, type_idx: usize) -> Result<u64, LearnerError> {
        self.sender_baseline_counts
            .get(type_idx)
            .copied()
            .ok_or_else(|| {
                LearnerError::Domain(format!(
                    "sender type index {type_idx} out of range (TYPES = {TYPES})"
                ))
            })
    }

    /// Restore a policy persisted with [`AgentPolicy::to_bytes`].
    ///
    /// # Errors
    ///
    /// Returns an error when the bytes are not valid policy JSON, or fail shape, finiteness, or metadata checks.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, LearnerError> {
        let policy: Self =
            serde_json::from_slice(bytes).map_err(|e| LearnerError::Domain(e.to_string()))?;
        let sender_ok = policy.sender_logits.len() == TYPES
            && policy.sender_logits.iter().all(|row| row.len() == TOKENS);
        let receiver_ok = policy.receiver_logits.len() == TOKENS
            && policy.receiver_logits.iter().all(|row| row.len() == TYPES);
        let finite = policy
            .sender_logits
            .iter()
            .flatten()
            .chain(policy.receiver_logits.iter().flatten())
            .chain(policy.sender_baseline.iter())
            .chain(policy.receiver_baseline.iter())
            .all(|v| v.is_finite());
        let meta_ok = policy
            .metadata
            .as_ref()
            .is_none_or(|m| (1..=TOKENS as u32).contains(&m.active_sender_tokens));
        if !sender_ok || !receiver_ok || !finite || !meta_ok {
            return Err(LearnerError::Domain(
                "AgentPolicy bytes have a bad shape, bad metadata, or non-finite floats"
                    .to_string(),
            ));
        }
        Ok(policy)
    }
}
