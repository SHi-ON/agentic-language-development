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

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Number of sender types / receiver type hypotheses (LV01 single-role).
pub const NUM_TYPES: usize = 16;

/// REINFORCE step size for [`AgentPolicy::update_sender`]. Matches probe v4.
pub const SENDER_LEARNING_RATE: f64 = 1.0;

/// Token vocabulary size for [`AgentPolicy`] (probe v4 `NTOK`).
pub const NUM_TOKENS: usize = 32;

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

/// Symmetric emergent agent: sender logits `[NUM_TYPES][NUM_TOKENS]` plus
/// receiver logits `[NUM_TOKENS][NUM_TYPES]`, mirroring probe v4 exactly.
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
    sender_baseline: [f64; NUM_TYPES],
    /// Observation counts backing the sender baselines.
    sender_baseline_counts: [u64; NUM_TYPES],
    /// Running-mean reward baseline per received token.
    receiver_baseline: [f64; NUM_TOKENS],
    /// Observation counts backing the receiver baselines.
    receiver_baseline_counts: [u64; NUM_TOKENS],
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
            sender_logits: vec![vec![0.0; NUM_TOKENS]; NUM_TYPES],
            receiver_logits: vec![vec![0.0; NUM_TYPES]; NUM_TOKENS],
            sender_baseline: [0.0; NUM_TYPES],
            sender_baseline_counts: [0; NUM_TYPES],
            receiver_baseline: [0.0; NUM_TOKENS],
            receiver_baseline_counts: [0; NUM_TOKENS],
        }
    }

    /// Softmax token distribution the sender uses for one type (sums to 1).
    ///
    /// # Panics
    ///
    /// Panics when `type_idx` is out of range.
    #[must_use]
    pub fn sender_probs(&self, type_idx: usize) -> Vec<f64> {
        softmax(&self.sender_logits[type_idx])
    }

    /// Sample a token from the type's sender softmax using `rng` (one
    /// `next_float` draw, so the sequence is fully stream-determined).
    pub fn sample_token(&self, type_idx: usize, rng: &mut ald_hashing::SeededPrng) -> usize {
        sample_from_probs(&self.sender_probs(type_idx), rng)
    }

    /// Running-mean reward baseline for one sender type.
    #[must_use]
    pub fn sender_baseline(&self, type_idx: usize) -> f64 {
        self.sender_baseline[type_idx]
    }

    /// Running-mean reward baseline for one received token.
    #[must_use]
    pub fn receiver_baseline(&self, token: usize) -> f64 {
        self.receiver_baseline[token]
    }

    /// Receiver type posterior `softmax(receiver_logits[token])`. `None`
    /// (dropped token) yields the exact uniform distribution.
    ///
    /// # Panics
    ///
    /// Panics when `token` is `Some(t)` with `t` out of range.
    #[must_use]
    pub fn receiver_type_posterior(&self, token: Option<usize>) -> [f64; NUM_TYPES] {
        let Some(tok) = token else {
            return [1.0 / NUM_TYPES as f64; NUM_TYPES];
        };
        softmax(&self.receiver_logits[tok])
            .try_into()
            .expect("receiver rows have NUM_TYPES entries")
    }

    /// Posterior over the 4 candidate positions of a case: the receiver
    /// type posterior restricted to the candidate types, renormalized.
    /// `None` yields exact `[0.25; 4]`.
    ///
    /// # Panics
    ///
    /// Panics when `token` or any candidate type is out of range; callers
    /// pass validated candidate codes (4 distinct types below
    /// [`NUM_TYPES`]), exactly as in probe v4.
    #[must_use]
    pub fn receiver_position_posterior(
        &self,
        token: Option<usize>,
        candidates: &[usize; 4],
    ) -> [f64; 4] {
        if token.is_none() {
            return [0.25; 4];
        }
        let pt = self.receiver_type_posterior(token);
        let raw = candidates.map(|c| pt[c]);
        let sum: f64 = raw.iter().sum();
        raw.map(|x| x / sum)
    }

    /// Sample a candidate position from
    /// [`AgentPolicy::receiver_position_posterior`] using `rng`. This is the
    /// receiver's action during unsupervised training: the true type is
    /// never consulted.
    pub fn sample_position(
        &self,
        token: Option<usize>,
        candidates: &[usize; 4],
        rng: &mut ald_hashing::SeededPrng,
    ) -> usize {
        sample_from_probs(&self.receiver_position_posterior(token, candidates), rng)
    }

    /// Sender REINFORCE update for the sampled `(type_idx, token)` pair with
    /// the per-type running baseline and [`SENDER_LEARNING_RATE`] (= 1.0,
    /// probe v4). Applied on every training episode, including dropped
    /// tokens: the sender still learns from the shared outcome. Non-finite
    /// rewards are ignored so a broken reward signal can never poison the
    /// persisted policy.
    ///
    /// # Panics
    ///
    /// Panics when `type_idx` or `token` is out of range.
    pub fn update_sender(&mut self, type_idx: usize, token: usize, reward: f64) {
        if !reward.is_finite() {
            return;
        }
        assert!(type_idx < NUM_TYPES, "AgentPolicy sender type out of range");
        assert!(token < NUM_TOKENS, "AgentPolicy sender token out of range");
        let row = &mut self.sender_logits[type_idx];
        reinforce_row(
            row,
            &mut self.sender_baseline[type_idx],
            &mut self.sender_baseline_counts[type_idx],
            token,
            reward,
            SENDER_LEARNING_RATE,
        );
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
    /// # Panics
    ///
    /// Panics when `token` is `Some(t)` with `t` out of range, when
    /// `choice >= 4`, or when the chosen candidate type is out of range.
    pub fn update_receiver(
        &mut self,
        token: Option<usize>,
        candidates: &[usize; 4],
        choice: usize,
        reward: f64,
    ) {
        if !reward.is_finite() {
            return;
        }
        let Some(tok) = token else {
            return;
        };
        assert!(tok < NUM_TOKENS, "AgentPolicy receiver token out of range");
        assert!(choice < 4, "AgentPolicy receiver choice out of range");
        let chosen_type = candidates[choice];
        assert!(
            chosen_type < NUM_TYPES,
            "AgentPolicy chosen candidate type out of range"
        );
        let row = &mut self.receiver_logits[tok];
        reinforce_row(
            row,
            &mut self.receiver_baseline[tok],
            &mut self.receiver_baseline_counts[tok],
            chosen_type,
            reward,
            RECEIVER_LEARNING_RATE,
        );
    }

    /// Deterministic policy bytes for artifact persistence.
    ///
    /// Field order is fixed and logits serialize in row-major order, so the
    /// same policy always yields identical bytes.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("AgentPolicy serialization is infallible")
    }

    /// Restore a policy persisted with [`AgentPolicy::to_bytes`].
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, LearnerError> {
        let policy: Self =
            serde_json::from_slice(bytes).map_err(|e| LearnerError::Domain(e.to_string()))?;
        let sender_ok = policy.sender_logits.len() == NUM_TYPES
            && policy
                .sender_logits
                .iter()
                .all(|row| row.len() == NUM_TOKENS);
        let receiver_ok = policy.receiver_logits.len() == NUM_TOKENS
            && policy
                .receiver_logits
                .iter()
                .all(|row| row.len() == NUM_TYPES);
        let finite = policy
            .sender_logits
            .iter()
            .flatten()
            .chain(policy.receiver_logits.iter().flatten())
            .chain(policy.sender_baseline.iter())
            .chain(policy.receiver_baseline.iter())
            .all(|v| v.is_finite());
        if !sender_ok || !receiver_ok || !finite {
            return Err(LearnerError::Domain(
                "AgentPolicy bytes have a bad shape or non-finite floats".to_string(),
            ));
        }
        Ok(policy)
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

    /// One probe-style case: a target type plus 3 distinct distractors,
    /// shuffled. Returns `(candidates, target_position)`.
    fn draw_case(rng: &mut ald_hashing::SeededPrng) -> ([usize; 4], usize) {
        let ty = rng.next_int(NUM_TYPES as u32).unwrap() as usize;
        let mut others = Vec::new();
        while others.len() < 3 {
            let c = rng.next_int(NUM_TYPES as u32).unwrap() as usize;
            if c != ty && !others.contains(&c) {
                others.push(c);
            }
        }
        let mut cands = [ty, others[0], others[1], others[2]];
        for i in (1..4).rev() {
            let j = rng.next_int(i as u32 + 1).unwrap() as usize;
            cands.swap(i, j);
        }
        let pos = cands.iter().position(|&c| c == ty).unwrap();
        (cands, pos)
    }

    #[test]
    fn agent_policy_none_token_is_exact_uniform_and_never_updated() {
        let mut a = AgentPolicy::new();
        assert_eq!(
            a.receiver_type_posterior(None),
            [1.0 / NUM_TYPES as f64; NUM_TYPES]
        );
        assert_eq!(
            a.receiver_position_posterior(None, &[0, 1, 2, 3]),
            [0.25; 4]
        );

        // Receiver REINFORCE moves toward the CHOSEN candidate's type:
        // choosing position 0 (type 5) pays 1, position 1 (type 1) pays 0,
        // so P(type 5 | tok 3) must end above P(type 1 | tok 3).
        let cands = [5, 1, 2, 9];
        for _ in 0..100 {
            a.update_receiver(Some(3), &cands, 0, 1.0);
            a.update_receiver(Some(3), &cands, 1, 0.0);
        }
        let pt = a.receiver_type_posterior(Some(3));
        assert!((pt.iter().sum::<f64>() - 1.0).abs() < 1e-12);
        assert!(
            pt[5] > pt[1],
            "chosen type should gain mass: {} vs {}",
            pt[5],
            pt[1]
        );
        let pp = a.receiver_position_posterior(Some(3), &cands);
        assert!((pp.iter().sum::<f64>() - 1.0).abs() < 1e-12);
        assert_eq!(argmax(&pp), 0);

        // After training, None is STILL exact uniform, and a None update is a
        // full no-op (bytes cover logits, baselines, and counts).
        assert_eq!(
            a.receiver_type_posterior(None),
            [1.0 / NUM_TYPES as f64; NUM_TYPES]
        );
        assert_eq!(a.receiver_position_posterior(None, &cands), [0.25; 4]);
        let before = a.to_bytes();
        a.update_receiver(None, &cands, 0, 1.0);
        a.update_receiver(None, &cands, 2, 0.0);
        assert_eq!(a.to_bytes(), before, "None must never update the receiver");

        // Non-finite rewards are ignored on both sides.
        a.update_sender(0, 0, f64::NAN);
        a.update_receiver(Some(3), &cands, 0, f64::INFINITY);
        assert_eq!(a.to_bytes(), before);
    }

    #[test]
    fn agent_policy_is_deterministic_per_seed() {
        let run = |seed: &str| {
            let root = ald_hashing::SeededPrng::new(seed);
            let mut rng = root.derive("agents");
            let mut agents = [AgentPolicy::new(), AgentPolicy::new()];
            let mut trace = Vec::new();
            for ep in 0..80 {
                let (cands, pos) = draw_case(&mut rng);
                let ty = cands[pos];
                let [a0, a1] = &mut agents;
                let (sender, receiver): (&mut AgentPolicy, &mut AgentPolicy) =
                    if ep % 2 == 0 { (a0, a1) } else { (a1, a0) };
                let k = sender.sample_token(ty, &mut rng);
                let tok = if rng.next_float() < 1.0 / 8.0 {
                    None
                } else {
                    Some(k)
                };
                let choice = receiver.sample_position(tok, &cands, &mut rng);
                let reward = if choice == pos { 1.0 } else { 0.0 };
                sender.update_sender(ty, k, reward);
                receiver.update_receiver(tok, &cands, choice, reward);
                trace.push((k, choice));
            }
            (trace, agents[0].to_bytes(), agents[1].to_bytes())
        };
        let a = run("seed-11");
        let b = run("seed-11");
        assert_eq!(a.0, b.0, "same seed must sample identically");
        assert_eq!(a.1, b.1, "same seed must serialize identically");
        assert_eq!(a.2, b.2, "same seed must serialize identically");
        // Round-trips are lossless and stable.
        let p0 = AgentPolicy::from_bytes(&a.1).unwrap();
        assert_eq!(p0.to_bytes(), a.1);
        assert_eq!(AgentPolicy::from_bytes(&p0.to_bytes()).unwrap(), p0);
        // Different seeds diverge.
        assert_ne!(a.0, run("other-seed").0);
        // Bad bytes are rejected: garbage and wrong-shape JSON alike.
        assert!(AgentPolicy::from_bytes(b"not json").is_err());
        let mut v: serde_json::Value = serde_json::from_slice(&a.1).unwrap();
        v["sender_logits"].as_array_mut().unwrap().pop();
        assert!(AgentPolicy::from_bytes(&serde_json::to_vec(&v).unwrap()).is_err());
    }

    #[test]
    fn agent_policies_co_train_unsupervised_like_probe_v4() {
        // Exact mirror of /tmp/emerge_probe_v4.py: two agents alternate
        // sender/receiver for 3000 episodes (1500 per direction), 1/8 token
        // drops, receiver acts by SAMPLING its posterior and never sees the
        // true type. Probe at this budget: advantage 0.52, sharpness 0.83.
        let mut rng = ald_hashing::SeededPrng::new("agent-policy-emergence").derive("loop");
        let mut agents = [AgentPolicy::new(), AgentPolicy::new()];
        for ep in 0..3000 {
            let (cands, pos) = draw_case(&mut rng);
            let ty = cands[pos];
            let [a0, a1] = &mut agents;
            let (sender, receiver): (&mut AgentPolicy, &mut AgentPolicy) =
                if ep % 2 == 0 { (a0, a1) } else { (a1, a0) };
            let k = sender.sample_token(ty, &mut rng);
            let tok = if rng.next_float() < 1.0 / 8.0 {
                None
            } else {
                Some(k)
            };
            let choice = receiver.sample_position(tok, &cands, &mut rng);
            let reward = if choice == pos { 1.0 } else { 0.0 };
            sender.update_sender(ty, k, reward);
            receiver.update_receiver(tok, &cands, choice, reward);
        }
        // Deterministic scoring pass (probe eval): token still sampled and
        // dropped w.p. 1/8; Brier of the receiver position posterior.
        let n_eval = 2000;
        let mut brier = 0.0;
        for i in 0..n_eval {
            let (cands, pos) = draw_case(&mut rng);
            let ty = cands[pos];
            let sender = &agents[i % 2];
            let receiver = &agents[1 - (i % 2)];
            let k = sender.sample_token(ty, &mut rng);
            let tok = if rng.next_float() < 1.0 / 8.0 {
                None
            } else {
                Some(k)
            };
            let post = receiver.receiver_position_posterior(tok, &cands);
            brier += post
                .iter()
                .enumerate()
                .map(|(j, &p)| (p - if j == pos { 1.0 } else { 0.0 }).powi(2))
                .sum::<f64>();
        }
        brier /= n_eval as f64;
        // Uniform posterior Brier is 0.75, so advantage mirrors the probe.
        let advantage = 0.75 - brier;
        let sharp: f64 = agents
            .iter()
            .flat_map(|a| (0..NUM_TYPES).map(move |ty| a.sender_probs(ty)))
            .map(|p| p.into_iter().fold(0.0, f64::max))
            .sum::<f64>()
            / (2 * NUM_TYPES) as f64;
        eprintln!("probe-v4 mirror @3000 total: brier={brier:.4} advantage={advantage:.4} sharpness={sharp:.4}");
        assert!(
            sharp > 0.8,
            "sender rows should sharpen like the probe, got {sharp}"
        );
        assert!(
            advantage > 0.4,
            "Brier advantage over uniform should exceed 0.4, got {advantage} (brier {brier})"
        );
    }
}
