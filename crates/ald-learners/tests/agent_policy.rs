//! `AgentPolicy` integration tests: probe-v4 mirror, determinism, and active-token constraints.

#![allow(clippy::cast_precision_loss)] // small counts/indices; exact for realistic inputs
#![allow(clippy::cast_possible_truncation)] // small counts/indices; bounded by construction
#![allow(clippy::cast_sign_loss)] // non-negative by construction (validated ranges, draws in [0, 1))

use ald_learners::AgentPolicy;
use ald_types::signaling::{TOKENS, TYPES};

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
    let ty = rng.next_int(TYPES as u32).unwrap() as usize;
    let mut others = Vec::new();
    while others.len() < 3 {
        let c = rng.next_int(TYPES as u32).unwrap() as usize;
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
        a.receiver_type_posterior(None).unwrap(),
        [1.0 / TYPES as f64; TYPES]
    );
    assert_eq!(
        a.receiver_position_posterior(None, &[0, 1, 2, 3]).unwrap(),
        [0.25; 4]
    );

    // Receiver REINFORCE moves toward the CHOSEN candidate's type:
    // choosing position 0 (type 5) pays 1, position 1 (type 1) pays 0,
    // so P(type 5 | tok 3) must end above P(type 1 | tok 3).
    let cands = [5, 1, 2, 9];
    for _ in 0..100 {
        a.update_receiver(Some(3), &cands, 0, 1.0).unwrap();
        a.update_receiver(Some(3), &cands, 1, 0.0).unwrap();
    }
    let pt = a.receiver_type_posterior(Some(3)).unwrap();
    assert!((pt.iter().sum::<f64>() - 1.0).abs() < 1e-12);
    assert!(
        pt[5] > pt[1],
        "chosen type should gain mass: {} vs {}",
        pt[5],
        pt[1]
    );
    let pp = a.receiver_position_posterior(Some(3), &cands).unwrap();
    assert!((pp.iter().sum::<f64>() - 1.0).abs() < 1e-12);
    assert_eq!(argmax(&pp), 0);

    // After training, None is STILL exact uniform, and a None update is a
    // full no-op (bytes cover logits, baselines, and counts).
    assert_eq!(
        a.receiver_type_posterior(None).unwrap(),
        [1.0 / TYPES as f64; TYPES]
    );
    assert_eq!(
        a.receiver_position_posterior(None, &cands).unwrap(),
        [0.25; 4]
    );
    let before = a.to_bytes();
    a.update_receiver(None, &cands, 0, 1.0).unwrap();
    a.update_receiver(None, &cands, 2, 0.0).unwrap();
    assert_eq!(a.to_bytes(), before, "None must never update the receiver");

    // Non-finite rewards are ignored on both sides.
    a.update_sender(0, 0, f64::NAN).unwrap();
    a.update_receiver(Some(3), &cands, 0, f64::INFINITY)
        .unwrap();
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
            let k = sender.sample_token(ty, &mut rng).unwrap();
            let tok = if rng.next_float() < 1.0 / 8.0 {
                None
            } else {
                Some(k)
            };
            let choice = receiver.sample_position(tok, &cands, &mut rng).unwrap();
            let reward = if choice == pos { 1.0 } else { 0.0 };
            sender.update_sender(ty, k, reward).unwrap();
            receiver
                .update_receiver(tok, &cands, choice, reward)
                .unwrap();
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
        let k = sender.sample_token(ty, &mut rng).unwrap();
        let tok = if rng.next_float() < 1.0 / 8.0 {
            None
        } else {
            Some(k)
        };
        let choice = receiver.sample_position(tok, &cands, &mut rng).unwrap();
        let reward = if choice == pos { 1.0 } else { 0.0 };
        sender.update_sender(ty, k, reward).unwrap();
        receiver
            .update_receiver(tok, &cands, choice, reward)
            .unwrap();
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
        let k = sender.sample_token(ty, &mut rng).unwrap();
        let tok = if rng.next_float() < 1.0 / 8.0 {
            None
        } else {
            Some(k)
        };
        let post = receiver.receiver_position_posterior(tok, &cands).unwrap();
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
        .flat_map(|a| (0..TYPES).map(move |ty| a.sender_probs(ty).unwrap()))
        .map(|p| p.into_iter().fold(0.0, f64::max))
        .sum::<f64>()
        / (2 * TYPES) as f64;
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

#[test]
#[allow(clippy::float_cmp)] // exact != proves movement away from exact-uniform init.
fn active_sender_tokens_constrain_support_normalization_and_update() {
    for k in [8usize, 16] {
        let mut a = AgentPolicy::with_active_sender_tokens(k).unwrap();
        assert_eq!(a.active_sender_tokens(), k);
        // Normalization: first-k sums to 1, exact zero outside.
        for ty in 0..TYPES {
            let p = a.sender_probs(ty).unwrap();
            assert_eq!(p.len(), TOKENS);
            assert!((p[..k].iter().sum::<f64>() - 1.0).abs() < 1e-12);
            assert!(
                p[k..].iter().all(|v| *v == 0.0),
                "probs outside k must be exact 0.0"
            );
        }
        // Support: sampling never leaves the active set.
        let mut rng = ald_hashing::SeededPrng::new("support").derive("loop");
        for _ in 0..2000 {
            let ty = rng.next_int(TYPES as u32).unwrap() as usize;
            let t = a.sample_token(ty, &mut rng).unwrap();
            assert!(t < k, "sampled token {t} escapes the active set of {k}");
        }
        // Update: REINFORCE moves only the first-k sub-row; the rest
        // stays exactly 0.0 while baselines/counts still advance.
        for i in 0..300 {
            let ty = rng.next_int(TYPES as u32).unwrap() as usize;
            let t = a.sample_token(ty, &mut rng).unwrap();
            // Mixed rewards: a constant reward would zero the advantage
            // and move nothing.
            a.update_sender(ty, t, f64::from(u8::from(i % 2 == 1)))
                .unwrap();
        }
        for ty in 0..TYPES {
            assert!(a.sender_logits(ty).unwrap()[k..].iter().all(|v| *v == 0.0));
        }
        assert!(a.sender_probs(3).unwrap()[..k]
            .iter()
            .any(|v| *v != 1.0 / k as f64));
        let total: u64 = (0..TYPES)
            .map(|ty| a.sender_baseline_count(ty).unwrap())
            .sum();
        assert_eq!(
            total, 300,
            "every sender update must advance exactly one baseline count"
        );
        // Metadata round-trips with the 32-token tables intact.
        let bytes = a.to_bytes();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            v["metadata"]["active_sender_tokens"],
            serde_json::json!(k as u32)
        );
        assert_eq!(v["sender_logits"].as_array().unwrap().len(), TYPES);
        assert_eq!(v["sender_logits"][0].as_array().unwrap().len(), TOKENS);
        let back = AgentPolicy::from_bytes(&bytes).unwrap();
        assert_eq!(back, a);
        assert_eq!(back.to_bytes(), bytes);
    }
}

#[test]
fn active_sender_tokens_full_width_is_legacy() {
    // k = 32 carries no metadata and serializes exactly like new().
    let full = AgentPolicy::with_active_sender_tokens(32).unwrap();
    assert_eq!(full.active_sender_tokens(), 32);
    assert_eq!(full.to_bytes(), AgentPolicy::new().to_bytes());
    let v: serde_json::Value = serde_json::from_slice(&full.to_bytes()).unwrap();
    assert!(v.get("metadata").is_none());
    // Out-of-range widths are rejected, never clamped.
    assert!(AgentPolicy::with_active_sender_tokens(0).is_err());
    assert!(AgentPolicy::with_active_sender_tokens(33).is_err());
    let mut bad: serde_json::Value =
        serde_json::from_slice(&AgentPolicy::new().to_bytes()).unwrap();
    bad["metadata"] = serde_json::json!({"active_sender_tokens": 0});
    assert!(AgentPolicy::from_bytes(&serde_json::to_vec(&bad).unwrap()).is_err());
    bad["metadata"] = serde_json::json!({"active_sender_tokens": 99});
    assert!(AgentPolicy::from_bytes(&serde_json::to_vec(&bad).unwrap()).is_err());
}

#[test]
fn sender_probs_rejects_type_out_of_range() {
    let a = AgentPolicy::new();
    assert!(a.sender_probs(TYPES).is_err());
    assert!(a.sender_baseline(TYPES).is_err());
    assert!(a.sender_baseline_count(TYPES).is_err());
    assert!(a.sender_logits(TYPES).is_err());
    assert!(a.receiver_baseline(TOKENS).is_err());
    assert!(a.receiver_type_posterior(Some(TOKENS)).is_err());
}

#[test]
fn update_sender_rejects_token_outside_active_set() {
    let mut a = AgentPolicy::with_active_sender_tokens(8).unwrap();
    assert!(a.update_sender(0, 8, 1.0).is_err());
    assert!(a.update_sender(TYPES, 0, 1.0).is_err());
    assert!(a.update_sender(0, TOKENS, 1.0).is_err());
    assert!(a.update_sender(0, 0, 1.0).is_ok());
}

#[test]
fn receiver_position_posterior_rejects_bad_candidate() {
    let a = AgentPolicy::new();
    assert!(a
        .receiver_position_posterior(Some(0), &[0, 1, 2, TYPES])
        .is_err());
    assert!(a
        .receiver_position_posterior(Some(TOKENS), &[0, 1, 2, 3])
        .is_err());
    assert!(a
        .receiver_position_posterior(Some(0), &[0, 1, 2, 3])
        .is_ok());
    let mut rng = ald_hashing::SeededPrng::new("x");
    assert!(a.sample_token(TYPES, &mut rng).is_err());
    assert!(a
        .sample_position(Some(0), &[0, 1, 2, TYPES], &mut rng)
        .is_err());
}

#[test]
fn update_receiver_rejects_bad_inputs() {
    let mut a = AgentPolicy::new();
    let cands = [0, 1, 2, 3];
    assert!(a.update_receiver(Some(TOKENS), &cands, 0, 1.0).is_err());
    assert!(a.update_receiver(Some(0), &cands, 4, 1.0).is_err());
    assert!(a
        .update_receiver(Some(0), &[0, 1, 2, TYPES], 3, 1.0)
        .is_err());
    assert!(a.update_receiver(Some(0), &cands, 0, 1.0).is_ok());
}
