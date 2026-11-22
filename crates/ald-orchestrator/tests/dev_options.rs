//! Development-options integration tests.
#![allow(clippy::cast_precision_loss)] // small counts/indices; exact for realistic inputs
#![allow(clippy::cast_possible_truncation)] // small counts/indices; bounded by construction
#![allow(clippy::cast_sign_loss)] // non-negative by construction (validated ranges, draws in [0, 1))

mod common;

use ald_orchestrator::{
    candidate_types, development_config_hash, development_config_short_id, drop_draw_less_than,
    parse_active_sender_tokens, parse_drop_probability, parse_held_out_types, parse_learning_mode,
    parse_training_interactions, resolve_development_options, resolved_is_default, run_dyad,
    stable_f64, DevelopmentOptions, DropProbability, DyadRequest, LearningMode,
};
use common::{fresh_temp_dir, test_inventory, test_manifest_text};

fn dev_test_req(
    dir: &std::path::Path,
    manifest_text: &str,
    cohort: &str,
    index: u32,
    dev: Option<DevelopmentOptions>,
) -> (DyadRequest, std::path::PathBuf) {
    let manifest_path = dir.join(format!("manifest-{index}.json"));
    std::fs::write(&manifest_path, manifest_text).unwrap();
    let evidence_root = dir.join("evidence");
    (
        DyadRequest {
            manifest_path: manifest_path.to_string_lossy().into_owned(),
            cohort: cohort.to_string(),
            kind: "primary".to_string(),
            index,
            evidence_root: evidence_root.to_string_lossy().into_owned(),
            treatment_batch_version: 2,
            protocol_version: 2,
            development: dev,
        },
        evidence_root,
    )
}

fn small_manifest_text() -> String {
    test_manifest_text(16, 8, 8, 8)
}

/// The single run root under `<root>/development` (fresh-root tests run
/// exactly one dyad).
fn only_slot(evidence_root: &std::path::Path) -> std::path::PathBuf {
    let entries: Vec<_> = std::fs::read_dir(evidence_root.join("development"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(entries.len(), 1, "expected exactly one run root");
    entries.into_iter().next().unwrap()
}

fn read_json(path: &std::path::Path) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn dev_opts(
    n: Option<u64>,
    mode: Option<LearningMode>,
    drop: Option<DropProbability>,
    k: Option<u32>,
    held: Option<Vec<u32>>,
) -> DevelopmentOptions {
    DevelopmentOptions {
        training_interactions_per_direction: n,
        learning_mode: mode,
        drop_probability: drop,
        active_sender_tokens: k,
        held_out_types: held,
    }
}

#[test]
#[allow(clippy::too_many_lines)] // tabular parse/resolve matrix.
fn dev_parse_and_resolve_matrix() {
    // Valid parses.
    assert_eq!(parse_training_interactions("1").unwrap(), 1);
    assert_eq!(parse_training_interactions("1500").unwrap(), 1500);
    assert_eq!(parse_learning_mode("joint").unwrap(), LearningMode::Joint);
    assert_eq!(parse_learning_mode("none").unwrap(), LearningMode::None);
    assert_eq!(
        parse_learning_mode("receiver-only").unwrap(),
        LearningMode::ReceiverOnly
    );
    assert_eq!(
        parse_drop_probability("1/8").unwrap(),
        DropProbability {
            numerator: 1,
            denominator: 8
        }
    );
    assert_eq!(
        parse_drop_probability("0/1").unwrap(),
        DropProbability {
            numerator: 0,
            denominator: 1
        }
    );
    assert_eq!(
        parse_drop_probability("1/1").unwrap(),
        DropProbability {
            numerator: 1,
            denominator: 1
        }
    );
    assert_eq!(parse_active_sender_tokens("8").unwrap(), 8);
    assert_eq!(parse_active_sender_tokens("16").unwrap(), 16);
    assert_eq!(parse_active_sender_tokens("32").unwrap(), 32);
    assert_eq!(parse_held_out_types("").unwrap(), Vec::<u32>::new());
    assert_eq!(
        parse_held_out_types("12,13,14,15").unwrap(),
        vec![12, 13, 14, 15]
    );
    // Malformed / out-of-range input is rejected, never defaulted.
    for bad in ["0", "abc", "-3", "1.5", ""] {
        assert!(parse_training_interactions(bad).is_err(), "N={bad:?}");
    }
    for bad in ["", "JOINT", "both", "receiver", "sender-only"] {
        assert!(parse_learning_mode(bad).is_err(), "mode={bad:?}");
    }
    for bad in [
        "", "1", "1/", "/8", "1/0", "2/1", "9/8", "a/b", "1/8/2", "-1/8", " 1/8",
    ] {
        assert!(parse_drop_probability(bad).is_err(), "drop={bad:?}");
    }
    for bad in ["", "0", "7", "31", "33", "64", "abc"] {
        assert!(parse_active_sender_tokens(bad).is_err(), "K={bad:?}");
    }
    for bad in [
        "16",
        "1,1",
        "0,16",
        "a",
        "12,,13",
        "0,1,2,3,4,5,6,7,8,9,10,11,12",
    ] {
        assert!(parse_held_out_types(bad).is_err(), "held={bad:?}");
    }
    // Resolution fills defaults and records 2N/N.
    let d = resolve_development_options(&DevelopmentOptions::default(), 1500).unwrap();
    assert_eq!(d.training_interactions_per_direction, 1500);
    assert_eq!(d.training_interactions_total, 3000);
    assert_eq!(d.learning_mode, LearningMode::Joint);
    assert_eq!(
        d.drop_probability,
        DropProbability {
            numerator: 1,
            denominator: 8
        }
    );
    assert_eq!(d.active_sender_tokens, 32);
    assert_eq!(d.held_out_types, [] as [u32; 0]);
    assert!(resolved_is_default(&d, 1500));
    assert!(!resolved_is_default(&d, 8));
    let non = resolve_development_options(
        &dev_opts(
            Some(8),
            Some(LearningMode::None),
            None,
            None,
            Some(vec![15, 12]),
        ),
        1500,
    )
    .unwrap();
    assert_eq!(non.training_interactions_total, 16);
    assert_eq!(non.held_out_types, vec![12, 15]);
    assert!(!resolved_is_default(&non, 1500));
    // Struct-level validation (bypassing the parsers) rejects too.
    assert!(resolve_development_options(&dev_opts(Some(0), None, None, None, None), 8).is_err());
    assert!(resolve_development_options(
        &dev_opts(
            None,
            None,
            Some(DropProbability {
                numerator: 2,
                denominator: 1
            }),
            None,
            None
        ),
        8
    )
    .is_err());
    assert!(resolve_development_options(&dev_opts(None, None, None, Some(7), None), 8).is_err());
    assert!(
        resolve_development_options(&dev_opts(None, None, None, None, Some(vec![3, 3])), 8)
            .is_err()
    );
    // Config hash is stable and binds the resolved values.
    let h1 = development_config_hash(&non).unwrap();
    let h2 = development_config_hash(&non).unwrap();
    assert_eq!(h1, h2);
    assert!(h1.starts_with("sha256:"));
    assert_eq!(development_config_short_id(&h1).len(), 8);
    let mut other = non.clone();
    other.training_interactions_per_direction = 9;
    assert_ne!(development_config_hash(&other).unwrap(), h1);
}

#[test]
fn dev_drop_draw_is_exact_rational() {
    const SCALE: f64 = 9_007_199_254_740_992.0;
    // Endpoints decide without ambiguity (one draw is still consumed by
    // the caller in every case).
    assert!(!drop_draw_less_than(0.0, 0, 1));
    assert!(!drop_draw_less_than(0.999_999, 0, 1));
    assert!(drop_draw_less_than(0.0, 1, 1));
    assert!(drop_draw_less_than(0.999_999, 1, 1));
    // (1,8) agrees with the legacy float comparison for EVERY draw,
    // including the exact boundary m = 2^50.
    for m in [
        0u64,
        1,
        (1 << 50) - 1,
        1 << 50,
        (1 << 50) + 1,
        (1 << 53) - 1,
    ] {
        let u = m as f64 / SCALE;
        assert_eq!(
            drop_draw_less_than(u, 1, 8),
            u < 1.0 / 8.0,
            "rational (1,8) must match legacy at m={m}"
        );
    }
    let mut rng = ald_hashing::SeededPrng::new("drop-sweep").derive("draws");
    for _ in 0..50_000 {
        let u = rng.next_float();
        assert_eq!(drop_draw_less_than(u, 1, 8), u < 1.0 / 8.0);
    }
    // Non-dyadic probabilities are exact: (1,3) drops iff m < 2^53/3.
    let third = (1u128 << 53) / 3; // 3002399751580330 (floor), threshold is m <= this
    for (m, expect) in [
        (third as u64 - 1, true),
        (third as u64, true),
        (third as u64 + 1, false),
        (0u64, true),
        ((1u64 << 53) - 1, false),
    ] {
        let u = m as f64 / SCALE;
        assert_eq!(drop_draw_less_than(u, 1, 3), expect, "m={m}");
    }
    // Half-open sanity: P(drop) over the stream approximates NUM/DEN.
    let mut rng = ald_hashing::SeededPrng::new("drop-rate").derive("draws");
    let n = 20_000u64;
    let mut dropped = 0u64;
    for _ in 0..n {
        if drop_draw_less_than(rng.next_float(), 1, 4) {
            dropped += 1;
        }
    }
    let rate = dropped as f64 / n as f64;
    assert!((rate - 0.25).abs() < 0.02, "rate {rate} far from 1/4");
}

struct TrainNRuns {
    train_fold: serde_json::Value,
    test_records: Vec<serde_json::Value>,
    sidecar: Option<serde_json::Value>,
    slot_name: String,
    policy0: Vec<u8>,
}

fn run_train_n(tag: &str, n: u64) -> (TrainNRuns, std::path::PathBuf) {
    let dir = fresh_temp_dir(tag);
    let (req, root) = dev_test_req(
        &dir,
        &small_manifest_text(),
        "development",
        0,
        Some(dev_opts(Some(n), None, None, None, None)),
    );
    let receipt = run_dyad(&req, &test_inventory()).unwrap();
    assert_eq!(receipt.state, "valid");
    let slot = only_slot(&root);
    let folds = read_json(&slot.join("role-folds-agent-0.json"));
    let sidecar_path = slot.join("development-config.json");
    let runs = TrainNRuns {
        train_fold: folds["training"].clone(),
        test_records: folds["test"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["record"].clone())
            .collect(),
        sidecar: std::fs::metadata(&sidecar_path)
            .is_ok()
            .then(|| read_json(&sidecar_path)),
        slot_name: slot.file_name().unwrap().to_string_lossy().into_owned(),
        policy0: std::fs::read(slot.join("agent-policy-agent-0.json")).unwrap(),
    };
    (runs, dir)
}

#[test]
fn dev_training_interactions_prefix_folds_and_eval_streams() {
    // Manifest fold is 8/direction; N=3 trains on the fold prefix, N=12
    // extends the same stream with fresh draws. N=8 is covered by the
    // explicit-default identity test below.
    let (r3, d3) = run_train_n("n3", 3);
    let (r12, d12) = run_train_n("n12", 12);
    // Folds unchanged: identical persisted training folds at both N.
    assert_eq!(r3.train_fold, r12.train_fold);
    assert_eq!(r3.train_fold.as_array().unwrap().len(), 8);
    // Identical eval streams: generated test records agree exactly (only
    // the trained policies' delivered tokens may differ).
    assert_eq!(r3.test_records, r12.test_records);
    assert_eq!(r3.test_records.len(), 4);
    // Both runs record 2N and N.
    for (r, n) in [(&r3, 3u64), (&r12, 12)] {
        let s = r.sidecar.as_ref().expect("non-default N writes a sidecar");
        assert_eq!(s["counts"]["trainingInteractionsPerDirection"], n);
        assert_eq!(s["counts"]["trainingInteractionsTotal"], 2 * n);
        assert_eq!(s["resolved"]["trainingInteractionsPerDirection"], n);
        assert_eq!(s["counts"]["perRoleTrainFold"], 8);
        assert!(r.slot_name.contains("-dev-"), "dev runs suffix the root");
    }
    // Prefix identity, N=3: the episode census equals the histogram of
    // the fold's first 3 records.
    let census3 = &r3.sidecar.as_ref().unwrap()["exposureCounts"]["agent-0"]["training"];
    let mut expect = [0u64; 16];
    for rec in r3.train_fold.as_array().unwrap().iter().take(3) {
        for c in rec["candidate_type_codes"].as_array().unwrap() {
            expect[c.as_u64().unwrap() as usize] += 1;
        }
    }
    assert_eq!(
        census3.as_array().unwrap(),
        &expect
            .iter()
            .map(|c| serde_json::json!(c))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        census3
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c.as_u64().unwrap())
            .sum::<u64>(),
        12,
        "3 episodes x 4 candidates"
    );
    // Prefix identity, N=12: the episode census equals the histogram of
    // the first 12 records regenerated from the sidecar train seed (the
    // 8 fold records plus 4 fresh stream draws, no duplication).
    let s12 = r12.sidecar.as_ref().unwrap();
    let seed = s12["seeds"]["roles"]["agent-0"]["training"]
        .as_str()
        .unwrap();
    let regen = ald_scenario::generate_cases(seed, 0, "agent-0", 12, &test_inventory());
    let mut expect12 = [0u64; 16];
    for c in &regen {
        for t in c.candidate_type_codes {
            expect12[t as usize] += 1;
        }
    }
    let census12 = s12["exposureCounts"]["agent-0"]["training"]
        .as_array()
        .unwrap();
    assert_eq!(
        census12,
        &expect12
            .iter()
            .map(|c| serde_json::json!(c))
            .collect::<Vec<_>>()
    );
    // ... and the regenerated 8-prefix is exactly the persisted fold.
    for (rec, case) in r12
        .train_fold
        .as_array()
        .unwrap()
        .iter()
        .zip(regen.iter().take(8))
    {
        let codes: Vec<u32> = rec["candidate_type_codes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c.as_u64().unwrap() as u32)
            .collect();
        assert_eq!(&codes, &case.candidate_type_codes.to_vec());
        assert_eq!(rec["case_id"], case.case_id);
    }
    // More training moves the policies (and deterministically so).
    assert_ne!(r3.policy0, r12.policy0);
    let (r12b, d12b) = run_train_n("n12b", 12);
    assert_eq!(r12.policy0, r12b.policy0);
    for d in [d3, d12, d12b] {
        let _ = std::fs::remove_dir_all(&d);
    }
}

#[test]
#[allow(clippy::too_many_lines)] // fixture-driven end-to-end assertions.
fn dev_learning_mode_none_freezes_and_zero_primary() {
    // Small workload, index 2: ordinary selection is uniform on both
    // roles (probed on the legacy path), so frozen-uniform natives give
    // primary exactly 0.0 case-by-case.
    let dir = fresh_temp_dir("none");
    let (req, root) = dev_test_req(
        &dir,
        &small_manifest_text(),
        "development",
        2,
        Some(dev_opts(None, Some(LearningMode::None), None, None, None)),
    );
    let receipt = run_dyad(&req, &test_inventory()).unwrap();
    assert_eq!(receipt.state, "valid");
    let slot = only_slot(&root);
    // Logits, baselines, and counts frozen: bytes equal fresh policies.
    let fresh = ald_learners::AgentPolicy::new().to_bytes();
    for role in ["agent-0", "agent-1"] {
        let bytes = std::fs::read(slot.join(format!("agent-policy-{role}.json"))).unwrap();
        assert_eq!(bytes, fresh, "{role} policy must be bit-frozen");
    }
    // Primary exactly 0.0, case-by-case.
    assert_eq!(receipt.primary, 0.0);
    let result: ald_analysis::dyad::DyadResult =
        serde_json::from_str(&std::fs::read_to_string(slot.join("dyad-result.json")).unwrap())
            .unwrap();
    assert_eq!(result.primary, 0.0);
    let inventory = test_inventory();
    for role in ["agent-0", "agent-1"] {
        let folds_text =
            std::fs::read_to_string(slot.join(format!("role-folds-{role}.json"))).unwrap();
        let folds: ald_analysis::dyad::RoleFolds = serde_json::from_str(&folds_text).unwrap();
        let sel = ald_analysis::select_ordinary_predictor(
            &inventory,
            &folds.training,
            &folds.validation_fit,
            &folds.validation_selection,
        )
        .unwrap();
        assert_eq!(sel.selected, ald_analysis::PredictorId::Uniform);
        let probs: Vec<serde_json::Value> = serde_json::from_str(
            &std::fs::read_to_string(slot.join(format!("branch-probabilities-{role}.json")))
                .unwrap(),
        )
        .unwrap();
        for (row, case) in folds.test.iter().zip(probs.iter()) {
            let ordinary = ald_analysis::ledger::score_ordinary_fit(
                sel.selected,
                &sel.refit,
                &row.record.candidate_type_codes,
                &row.record.delivered_token,
                &inventory,
            )
            .unwrap();
            let native: Vec<f64> = serde_json::from_value(case["native"].clone()).unwrap();
            assert_eq!(native, vec![0.25; 4]);
            assert_eq!(ordinary, native, "per-case gap must be exactly 0");
        }
    }
    // Same sampling draws consumed: delivered test tokens equal an
    // independent uniform-stream resimulation (token draw + channel draw
    // per case, in order).
    let sidecar = read_json(&slot.join("development-config.json"));
    for (receiver_idx, role) in ["agent-0", "agent-1"].iter().enumerate() {
        let role = *role;
        let _ = receiver_idx;
        let test_seed = sidecar["seeds"]["roles"][role]["withinSupportTest"]
            .as_str()
            .unwrap();
        let sender = ald_learners::AgentPolicy::new();
        let mut rng = ald_hashing::SeededPrng::new(test_seed).derive("emergent-test");
        let cases = ald_scenario::generate_cases(test_seed, 3, role, 4, &inventory);
        let probs: Vec<serde_json::Value> = serde_json::from_str(
            &std::fs::read_to_string(slot.join(format!("branch-probabilities-{role}.json")))
                .unwrap(),
        )
        .unwrap();
        for (c, case) in cases.iter().zip(probs.iter()) {
            let cands = candidate_types(&c.candidate_type_codes);
            let token = sender
                .sample_token(cands[c.target_index], &mut rng)
                .unwrap();
            let delivered = if drop_draw_less_than(rng.next_float(), 1, 8) {
                None
            } else {
                Some(token)
            };
            let expect = delivered.map(|id| inventory[id].clone());
            match expect {
                None => assert!(case["deliveredToken"].is_null()),
                Some(t) => assert_eq!(case["deliveredToken"], serde_json::json!(t)),
            }
        }
    }
    // Binding: receipt, journal, and sidecar carry the same config hash.
    let config_hash = sidecar["digests"]["developmentConfigSha256"]
        .as_str()
        .unwrap();
    assert_eq!(
        receipt.development_config_sha256.as_deref(),
        Some(config_hash)
    );
    assert_eq!(
        sidecar["resolved"]["learningMode"],
        serde_json::json!("none")
    );
    assert_eq!(
        sidecar["requested"],
        serde_json::json!({"learningMode": "none"}),
        "sidecar requested records only the flags actually passed"
    );
    let journal: Vec<serde_json::Value> = std::fs::read_to_string(root.join("journal.jsonl"))
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(journal.len(), 2);
    for line in &journal {
        assert_eq!(line["developmentConfigSha256"], config_hash);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn dev_learning_mode_receiver_only_sender_uniform() {
    // N=40 over the fold of 8 gives the receiver plenty of episodes while
    // the sender must stay bit-frozen.
    let dir = fresh_temp_dir("recvonly");
    let (req, root) = dev_test_req(
        &dir,
        &small_manifest_text(),
        "development",
        0,
        Some(dev_opts(
            Some(40),
            Some(LearningMode::ReceiverOnly),
            None,
            None,
            None,
        )),
    );
    let receipt = run_dyad(&req, &test_inventory()).unwrap();
    assert!(receipt.primary.is_finite());
    let slot = only_slot(&root);
    let fresh = ald_learners::AgentPolicy::new();
    let inventory = test_inventory();
    for role in ["agent-0", "agent-1"] {
        let policy = ald_learners::AgentPolicy::from_bytes(
            &std::fs::read(slot.join(format!("agent-policy-{role}.json"))).unwrap(),
        )
        .unwrap();
        // Sender exactly uniform: every logit, baseline, and count zero.
        for ty in 0..ald_types::signaling::TYPES {
            assert!(policy.sender_logits(ty).unwrap().iter().all(|v| *v == 0.0));
            assert_eq!(policy.sender_baseline(ty).unwrap(), 0.0);
            assert_eq!(policy.sender_baseline_count(ty).unwrap(), 0);
            assert_eq!(
                policy.sender_probs(ty).unwrap(),
                fresh.sender_probs(ty).unwrap()
            );
        }
        // ... while the receiver learned.
        assert_ne!(
            policy.to_bytes(),
            fresh.to_bytes(),
            "{role} receiver must move"
        );
    }
    // Sender-uniform at test too: delivered tokens equal the frozen
    // uniform-sender resimulation exactly.
    let sidecar = read_json(&slot.join("development-config.json"));
    for role in ["agent-0", "agent-1"] {
        let test_seed = sidecar["seeds"]["roles"][role]["withinSupportTest"]
            .as_str()
            .unwrap();
        let mut rng = ald_hashing::SeededPrng::new(test_seed).derive("emergent-test");
        let cases = ald_scenario::generate_cases(test_seed, 3, role, 4, &inventory);
        let probs: Vec<serde_json::Value> = serde_json::from_str(
            &std::fs::read_to_string(slot.join(format!("branch-probabilities-{role}.json")))
                .unwrap(),
        )
        .unwrap();
        for (c, case) in cases.iter().zip(probs.iter()) {
            let cands = candidate_types(&c.candidate_type_codes);
            let token = fresh.sample_token(cands[c.target_index], &mut rng).unwrap();
            let delivered = if drop_draw_less_than(rng.next_float(), 1, 8) {
                None
            } else {
                Some(token)
            };
            match delivered.map(|id| inventory[id].clone()) {
                None => assert!(case["deliveredToken"].is_null()),
                Some(t) => assert_eq!(case["deliveredToken"], serde_json::json!(t)),
            }
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
#[allow(clippy::too_many_lines)] // tabular endpoint/rational matrix.
fn dev_drop_probability_endpoints_and_rational() {
    let inventory = test_inventory();
    // 0/1: nothing is ever dropped, in training or at test.
    let dir0 = fresh_temp_dir("drop0");
    let (req0, root0) = dev_test_req(
        &dir0,
        &small_manifest_text(),
        "development",
        0,
        Some(dev_opts(
            None,
            None,
            Some(DropProbability {
                numerator: 0,
                denominator: 1,
            }),
            None,
            None,
        )),
    );
    run_dyad(&req0, &inventory).unwrap();
    let slot0 = only_slot(&root0);
    for role in ["agent-0", "agent-1"] {
        let probs: Vec<serde_json::Value> = serde_json::from_str(
            &std::fs::read_to_string(slot0.join(format!("branch-probabilities-{role}.json")))
                .unwrap(),
        )
        .unwrap();
        assert_ne!(probs, [] as [serde_json::Value; 0]);
        for case in &probs {
            assert!(case["deliveredToken"].is_string(), "0/1 must never drop");
            assert_eq!(case["nativeMissing"], serde_json::json!(false));
        }
        // Training side: with no drops the receiver saw every episode.
        let policy = ald_learners::AgentPolicy::from_bytes(
            &std::fs::read(slot0.join(format!("agent-policy-{role}.json"))).unwrap(),
        )
        .unwrap();
        assert_ne!(
            policy.to_bytes(),
            ald_learners::AgentPolicy::new().to_bytes()
        );
    }
    // 1/1: everything is dropped (endpoints still consume their draw, so
    // the stream stays aligned) and every native is exact uniform.
    let dir1 = fresh_temp_dir("drop1");
    let (req1, root1) = dev_test_req(
        &dir1,
        &small_manifest_text(),
        "development",
        0,
        Some(dev_opts(
            None,
            None,
            Some(DropProbability {
                numerator: 1,
                denominator: 1,
            }),
            None,
            None,
        )),
    );
    run_dyad(&req1, &inventory).unwrap();
    let slot1 = only_slot(&root1);
    for role in ["agent-0", "agent-1"] {
        let probs: Vec<serde_json::Value> = serde_json::from_str(
            &std::fs::read_to_string(slot1.join(format!("branch-probabilities-{role}.json")))
                .unwrap(),
        )
        .unwrap();
        for case in &probs {
            assert!(case["deliveredToken"].is_null(), "1/1 must always drop");
            let native: Vec<f64> = serde_json::from_value(case["native"].clone()).unwrap();
            assert_eq!(native, vec![0.25; 4]);
        }
        // Training side: all dropped, so the receiver never updated (no
        // logits, no baselines, no counts) while the sender still ran its
        // update on every one of its 8 episodes (counts advance even when
        // a zero advantage leaves the logits untouched).
        let policy = ald_learners::AgentPolicy::from_bytes(
            &std::fs::read(slot1.join(format!("agent-policy-{role}.json"))).unwrap(),
        )
        .unwrap();
        let fresh_bytes = ald_learners::AgentPolicy::new().to_bytes();
        let got: serde_json::Value = serde_json::from_slice(&policy.to_bytes()).unwrap();
        let new: serde_json::Value = serde_json::from_slice(&fresh_bytes).unwrap();
        assert_eq!(got["receiver_logits"], new["receiver_logits"]);
        assert_eq!(got["receiver_baseline"], new["receiver_baseline"]);
        assert_eq!(
            got["receiver_baseline_counts"],
            new["receiver_baseline_counts"]
        );
        let sender_updates: u64 = (0..ald_types::signaling::TYPES)
            .map(|ty| policy.sender_baseline_count(ty).unwrap())
            .sum();
        assert_eq!(
            sender_updates, 8,
            "sender updates on every episode even when dropped"
        );
    }
    // 1/3: exact rational in train and test; test deliveries match an
    // independent resimulation draw-for-draw, and reruns are identical.
    let run_third = |tag: &str| {
        let dir = fresh_temp_dir(tag);
        let (req, root) = dev_test_req(
            &dir,
            &small_manifest_text(),
            "development",
            0,
            Some(dev_opts(
                None,
                None,
                Some(DropProbability {
                    numerator: 1,
                    denominator: 3,
                }),
                None,
                None,
            )),
        );
        run_dyad(&req, &inventory).unwrap();
        (dir, only_slot(&root))
    };
    let (d3a, s3a) = run_third("drop3a");
    let (d3b, s3b) = run_third("drop3b");
    for role in ["agent-0", "agent-1"] {
        assert_eq!(
            std::fs::read(s3a.join(format!("agent-policy-{role}.json"))).unwrap(),
            std::fs::read(s3b.join(format!("agent-policy-{role}.json"))).unwrap(),
            "rational channel must be deterministic"
        );
    }
    let sidecar = read_json(&s3a.join("development-config.json"));
    assert_eq!(
        sidecar["resolved"]["dropProbability"],
        serde_json::json!({"numerator": 1, "denominator": 3})
    );
    // Resimulate the test phase of the 1/3 run from the persisted frozen
    // policies: exact match proves one channel draw per interaction with
    // the resolved rational, in train-independent eval streams.
    for (receiver_idx, role) in ["agent-0", "agent-1"].iter().enumerate() {
        let role = *role;
        let policies: Vec<ald_learners::AgentPolicy> = ["agent-0", "agent-1"]
            .iter()
            .map(|r| {
                ald_learners::AgentPolicy::from_bytes(
                    &std::fs::read(s3a.join(format!("agent-policy-{r}.json"))).unwrap(),
                )
                .unwrap()
            })
            .collect();
        let sender = &policies[1 - receiver_idx];
        let receiver = &policies[receiver_idx];
        let test_seed = sidecar["seeds"]["roles"][role]["withinSupportTest"]
            .as_str()
            .unwrap();
        let mut rng = ald_hashing::SeededPrng::new(test_seed).derive("emergent-test");
        let cases = ald_scenario::generate_cases(test_seed, 3, role, 4, &inventory);
        let probs: Vec<serde_json::Value> = serde_json::from_str(
            &std::fs::read_to_string(s3a.join(format!("branch-probabilities-{role}.json")))
                .unwrap(),
        )
        .unwrap();
        for (c, case) in cases.iter().zip(probs.iter()) {
            let cands = candidate_types(&c.candidate_type_codes);
            let token = sender
                .sample_token(cands[c.target_index], &mut rng)
                .unwrap();
            let delivered = if drop_draw_less_than(rng.next_float(), 1, 3) {
                None
            } else {
                Some(token)
            };
            match delivered.map(|id| inventory[id].clone()) {
                None => assert!(case["deliveredToken"].is_null()),
                Some(t) => assert_eq!(case["deliveredToken"], serde_json::json!(t)),
            }
            let expect_native: Vec<f64> = receiver
                .receiver_position_posterior(delivered, &cands)
                .unwrap()
                .iter()
                .map(|v| stable_f64(*v))
                .collect();
            let got: Vec<f64> = serde_json::from_value(case["native"].clone()).unwrap();
            assert_eq!(got, expect_native);
        }
    }
    for d in [dir0, dir1, d3a, d3b] {
        let _ = std::fs::remove_dir_all(&d);
    }
}

#[test]
fn dev_active_sender_tokens_end_to_end() {
    let inventory = test_inventory();
    for k in [8u32, 16] {
        let dir = fresh_temp_dir("acttok");
        let (req, root) = dev_test_req(
            &dir,
            &small_manifest_text(),
            "development",
            0,
            Some(dev_opts(None, None, None, Some(k), None)),
        );
        let receipt = run_dyad(&req, &inventory).unwrap();
        assert_eq!(receipt.state, "valid");
        let slot = only_slot(&root);
        // Support: every delivered test token is inside the active set.
        for role in ["agent-0", "agent-1"] {
            let probs: Vec<serde_json::Value> = serde_json::from_str(
                &std::fs::read_to_string(slot.join(format!("branch-probabilities-{role}.json")))
                    .unwrap(),
            )
            .unwrap();
            let mut delivered_any = false;
            for case in &probs {
                if let Some(tok) = case["deliveredToken"].as_str() {
                    delivered_any = true;
                    let id: usize = tok.strip_prefix("tok").unwrap().parse().unwrap();
                    assert!(id < k as usize, "token {tok} escapes the active set");
                }
            }
            assert!(delivered_any, "expected some delivered tokens");
            // Normalization + update constraint + 32-token format + K in
            // metadata.
            let bytes = std::fs::read(slot.join(format!("agent-policy-{role}.json"))).unwrap();
            let policy = ald_learners::AgentPolicy::from_bytes(&bytes).unwrap();
            assert_eq!(policy.active_sender_tokens(), k as usize);
            for ty in 0..ald_types::signaling::TYPES {
                let p = policy.sender_probs(ty).unwrap();
                assert_eq!(p.len(), 32);
                assert!((p[..k as usize].iter().sum::<f64>() - 1.0).abs() < 1e-12);
                assert!(p[k as usize..].iter().all(|v| *v == 0.0));
                assert!(policy.sender_logits(ty).unwrap()[k as usize..]
                    .iter()
                    .all(|v| *v == 0.0));
            }
            let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(v["metadata"]["active_sender_tokens"], serde_json::json!(k));
            assert_eq!(v["sender_logits"].as_array().unwrap().len(), 16);
            assert_eq!(v["sender_logits"][0].as_array().unwrap().len(), 32);
        }
        let sidecar = read_json(&slot.join("development-config.json"));
        assert_eq!(
            sidecar["resolved"]["activeSenderTokens"],
            serde_json::json!(k)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn dev_held_out_types_excluded_and_census() {
    let inventory = test_inventory();
    let held = vec![12u32, 13, 14, 15];
    let dir = fresh_temp_dir("heldout");
    let (req, root) = dev_test_req(
        &dir,
        &small_manifest_text(),
        "development",
        0,
        Some(dev_opts(None, None, None, None, Some(held.clone()))),
    );
    run_dyad(&req, &inventory).unwrap();
    let slot = only_slot(&root);
    let sidecar = read_json(&slot.join("development-config.json"));
    assert_eq!(
        sidecar["resolved"]["heldOutTypes"],
        serde_json::json!([12, 13, 14, 15])
    );
    for role in ["agent-0", "agent-1"] {
        // No held-out code in any pre-test fold, either direction.
        let folds = read_json(&slot.join(format!("role-folds-{role}.json")));
        for split in ["training", "validation_fit", "validation_selection"] {
            for rec in folds[split].as_array().unwrap() {
                for c in rec["candidate_type_codes"].as_array().unwrap() {
                    let t = c.as_u64().unwrap() as u32;
                    assert!(!held.contains(&t), "{role}/{split} exposes held-out {t}");
                }
            }
        }
        // Test folds cover all 16: they equal the unfiltered regeneration
        // from the sidecar test seed exactly.
        let test_seed = sidecar["seeds"]["roles"][role]["withinSupportTest"]
            .as_str()
            .unwrap();
        let regen = ald_scenario::generate_cases(test_seed, 3, role, 4, &inventory);
        let test = folds["test"].as_array().unwrap();
        assert_eq!(test.len(), regen.len());
        for (row, case) in test.iter().zip(regen.iter()) {
            let codes: Vec<u32> = row["record"]["candidate_type_codes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| c.as_u64().unwrap() as u32)
                .collect();
            assert_eq!(&codes, &case.candidate_type_codes.to_vec());
        }
        // The sidecar census reproduces from the sidecar seeds: pre-test
        // splits via the excluding generator, test via the plain one.
        let seeds = &sidecar["seeds"]["roles"][role];
        let counts = |seed: &str, off: u64, n: usize, filtered: bool| {
            let cases = if filtered {
                ald_scenario::generate_cases_excluding(seed, off, role, n, &inventory, &held)
            } else {
                ald_scenario::generate_cases(seed, off, role, n, &inventory)
            };
            let mut h = [0u64; 16];
            for c in &cases {
                for t in c.candidate_type_codes {
                    h[t as usize] += 1;
                }
            }
            h.iter().map(|c| serde_json::json!(c)).collect::<Vec<_>>()
        };
        let census = &sidecar["exposureCounts"][role];
        assert_eq!(
            census["training"].as_array().unwrap(),
            &counts(seeds["training"].as_str().unwrap(), 0, 8, true)
        );
        assert_eq!(
            census["validationFit"].as_array().unwrap(),
            &counts(seeds["validationFit"].as_str().unwrap(), 1, 4, true)
        );
        assert_eq!(
            census["validationSelection"].as_array().unwrap(),
            &counts(seeds["validationSelection"].as_str().unwrap(), 2, 4, true)
        );
        assert_eq!(
            census["withinSupportTest"].as_array().unwrap(),
            &counts(seeds["withinSupportTest"].as_str().unwrap(), 3, 4, false)
        );
        // Held-out types have zero pre-test exposure in the census.
        for split in ["training", "validationFit", "validationSelection"] {
            for t in &held {
                assert_eq!(census[split][*t as usize], serde_json::json!(0));
            }
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn dev_cohort_gate_and_no_partial_artifacts() {
    // Every single non-default option is refused outside cohort
    // development, before any directory, journal, or database exists.
    let singles = [
        dev_opts(Some(3), None, None, None, None),
        dev_opts(None, Some(LearningMode::None), None, None, None),
        dev_opts(None, Some(LearningMode::ReceiverOnly), None, None, None),
        dev_opts(
            None,
            None,
            Some(DropProbability {
                numerator: 1,
                denominator: 3,
            }),
            None,
            None,
        ),
        dev_opts(None, None, None, Some(8), None),
        dev_opts(None, None, None, None, Some(vec![15])),
    ];
    for (i, dev) in singles.iter().enumerate() {
        for cohort in ["A", "B", "staging"] {
            let dir = fresh_temp_dir("gate");
            let (req, root) =
                dev_test_req(&dir, &small_manifest_text(), cohort, 0, Some(dev.clone()));
            // Even a GO admission must not admit dev options elsewhere.
            if cohort == "A" && i == 0 {
                std::fs::create_dir_all(&root).unwrap();
                std::fs::write(root.join("admission.json"), r#"{"decision":"GO"}"#).unwrap();
                let err = run_dyad(&req, &test_inventory()).unwrap_err().to_string();
                assert!(err.contains("development"), "unexpected error: {err}");
                assert!(!root.join("journal.jsonl").exists());
                assert!(!root.join("A").exists());
            } else {
                let err = run_dyad(&req, &test_inventory()).unwrap_err().to_string();
                assert!(err.contains("development"), "unexpected error: {err}");
                assert!(
                    !root.exists(),
                    "rejected run must not create any dir/journal/db"
                );
            }
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
    // Malformed resolved structs are rejected artifact-free too.
    let bads = vec![
        dev_opts(Some(0), None, None, None, None),
        dev_opts(None, None, None, Some(7), None),
        dev_opts(
            None,
            None,
            Some(DropProbability {
                numerator: 5,
                denominator: 4,
            }),
            None,
            None,
        ),
        dev_opts(None, None, None, None, Some(vec![1, 1])),
    ];
    for dev in &bads {
        let dir = fresh_temp_dir("baddev");
        let (req, root) = dev_test_req(
            &dir,
            &small_manifest_text(),
            "development",
            0,
            Some(dev.clone()),
        );
        assert!(run_dyad(&req, &test_inventory()).is_err());
        assert!(!root.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn dev_explicit_default_identity() {
    // Bare defaults and explicitly-passed defaults take the identical
    // legacy path: byte-identical artifacts, journals, and receipts.
    let manifest = small_manifest_text();
    let explicit = dev_opts(
        Some(8),
        Some(LearningMode::Joint),
        Some(DropProbability {
            numerator: 1,
            denominator: 8,
        }),
        Some(32),
        Some(Vec::new()),
    );
    let dir_a = fresh_temp_dir("idef-a");
    let (req_a, root_a) = dev_test_req(&dir_a, &manifest, "development", 0, None);
    let r_a = run_dyad(&req_a, &test_inventory()).unwrap();
    let dir_b = fresh_temp_dir("idef-b");
    let (req_b, root_b) = dev_test_req(&dir_b, &manifest, "development", 0, Some(explicit));
    let r_b = run_dyad(&req_b, &test_inventory()).unwrap();
    assert_eq!(r_a.development_config_sha256, None);
    assert_eq!(r_b.development_config_sha256, None);
    let slot_a = only_slot(&root_a);
    let slot_b = only_slot(&root_b);
    assert_eq!(
        slot_a.file_name().unwrap(),
        std::ffi::OsStr::new("primary-0000")
    );
    assert_eq!(
        slot_b.file_name().unwrap(),
        std::ffi::OsStr::new("primary-0000"),
        "explicit defaults take the legacy (unsuffixed) root"
    );
    assert!(!slot_a.join("development-config.json").exists());
    assert!(!slot_b.join("development-config.json").exists());
    let files_a: Vec<_> = std::fs::read_dir(&slot_a)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    for name in &files_a {
        // evidence.sqlite embeds allocator/page-layout bytes that are
        // content-identical but not byte-stable; the receipt carries
        // measured wall-clock seconds. Everything else must be
        // byte-identical.
        if name == std::ffi::OsStr::new("evidence.sqlite") {
            continue;
        }
        if name == std::ffi::OsStr::new("dyad-receipt.json") {
            let mut ra: serde_json::Value =
                serde_json::from_slice(&std::fs::read(slot_a.join(name)).unwrap()).unwrap();
            let mut rb: serde_json::Value =
                serde_json::from_slice(&std::fs::read(slot_b.join(name)).unwrap()).unwrap();
            ra.as_object_mut().unwrap().remove("wallSeconds");
            rb.as_object_mut().unwrap().remove("wallSeconds");
            assert_eq!(ra, rb, "receipts differ beyond wall-clock");
            continue;
        }
        assert_eq!(
            std::fs::read(slot_a.join(name)).unwrap(),
            std::fs::read(slot_b.join(name)).unwrap(),
            "artifact {name:?} differs between default and explicit-default"
        );
    }
    assert_eq!(
        std::fs::read(root_a.join("journal.jsonl")).unwrap(),
        std::fs::read(root_b.join("journal.jsonl")).unwrap()
    );
    let mut va = serde_json::to_value(&r_a).unwrap();
    let mut vb = serde_json::to_value(&r_b).unwrap();
    va.as_object_mut().unwrap().remove("wallSeconds");
    vb.as_object_mut().unwrap().remove("wallSeconds");
    assert_eq!(va, vb);
    // Both evidence stores verify with identical stream counts.
    for slot in [&slot_a, &slot_b] {
        let conn = ald_evidence::open(slot.join("evidence.sqlite").to_str().unwrap()).unwrap();
        assert_eq!(
            ald_evidence::verify_stream(&conn, ald_types::EventStream::Turns).unwrap(),
            8
        );
        assert_eq!(
            ald_evidence::verify_stream(&conn, ald_types::EventStream::Audit).unwrap(),
            2
        );
    }
    for d in [dir_a, dir_b] {
        let _ = std::fs::remove_dir_all(&d);
    }
}

#[test]
fn dev_run_roots_separate_per_config_and_refuse_overwrite() {
    let manifest = small_manifest_text();
    let dir = fresh_temp_dir("roots");
    let manifest_path = dir.join("manifest.json");
    std::fs::write(&manifest_path, &manifest).unwrap();
    let root = dir.join("evidence");
    let mk = |n: u64| DyadRequest {
        manifest_path: manifest_path.to_string_lossy().into_owned(),
        cohort: "development".to_string(),
        kind: "primary".to_string(),
        index: 0,
        evidence_root: root.to_string_lossy().into_owned(),
        treatment_batch_version: 2,
        protocol_version: 2,
        development: Some(dev_opts(Some(n), None, None, None, None)),
    };
    let r3 = run_dyad(&mk(3), &test_inventory()).unwrap();
    let r5 = run_dyad(&mk(5), &test_inventory()).unwrap();
    assert_ne!(
        r3.development_config_sha256, r5.development_config_sha256,
        "distinct configs bind distinct hashes"
    );
    // Separate run roots per config.
    let mut roots: Vec<String> = std::fs::read_dir(root.join("development"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    roots.sort();
    assert_eq!(roots.len(), 2);
    assert!(roots[0].starts_with("primary-0000-dev-"));
    assert!(roots[1].starts_with("primary-0000-dev-"));
    assert_ne!(roots[0], roots[1]);
    for name in &roots {
        let sidecar = read_json(&root.join(format!("development/{name}/development-config.json")));
        let n = sidecar["counts"]["trainingInteractionsPerDirection"]
            .as_u64()
            .unwrap();
        let receipt = if n == 3 { &r3 } else { &r5 };
        assert!(n == 3 || n == 5);
        assert_eq!(
            sidecar["digests"]["developmentConfigSha256"]
                .as_str()
                .unwrap(),
            receipt.development_config_sha256.as_deref().unwrap()
        );
        assert!(name.ends_with(&development_config_short_id(
            receipt.development_config_sha256.as_deref().unwrap()
        )));
    }
    // The shared journal chains all four lines with per-line config tags.
    let journal: Vec<serde_json::Value> = std::fs::read_to_string(root.join("journal.jsonl"))
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(journal.len(), 4);
    let hashes: std::collections::HashSet<String> = journal
        .iter()
        .map(|l| l["developmentConfigSha256"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(hashes.len(), 2);
    // Rerunning the same config refuses to overwrite and changes nothing.
    let n3_root = roots
        .iter()
        .find(|name| {
            read_json(&root.join(format!("development/{name}/development-config.json")))["counts"]
                ["trainingInteractionsPerDirection"]
                == serde_json::json!(3u64)
        })
        .unwrap()
        .clone();
    let before =
        std::fs::read(root.join(format!("development/{n3_root}/development-config.json"))).unwrap();
    let err = run_dyad(&mk(3), &test_inventory()).unwrap_err().to_string();
    assert!(err.contains("overwrite"), "unexpected error: {err}");
    let after =
        std::fs::read(root.join(format!("development/{n3_root}/development-config.json"))).unwrap();
    assert_eq!(before, after);
    assert_eq!(
        std::fs::read_to_string(root.join("journal.jsonl"))
            .unwrap()
            .lines()
            .count(),
        4,
        "refused rerun appends no journal lines"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
