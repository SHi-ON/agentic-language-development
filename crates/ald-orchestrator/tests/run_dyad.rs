//! Full-dyad run integration tests.
#![allow(clippy::cast_precision_loss)] // small counts/indices; exact for realistic inputs
#![allow(clippy::cast_possible_truncation)] // small counts/indices; bounded by construction
#![allow(clippy::cast_sign_loss)] // non-negative by construction (validated ranges, draws in [0, 1))

mod common;

use ald_orchestrator::{
    candidate_types, derive_ala_seed, run_dyad, stable_f64, AlaSeedInput, DyadRequest,
    ALA_SEED_ROOT_V2,
};
use common::{fresh_temp_dir, journal_hashes, test_inventory, test_manifest_text};
use sha2::Digest as _;

#[test]
#[allow(clippy::too_many_lines)] // fixture-driven end-to-end assertions.
#[allow(clippy::float_cmp)] // replay one-hot is constructed exact; == 1.0 checks that.
fn integration_full_dev_dyad_verifies() {
    // Frozen workload totals: 1500 training + 120/120/120 per role.
    let dir = fresh_temp_dir("full");
    let manifest_path = dir.join("manifest.json");
    let manifest_text = test_manifest_text(3000, 240, 240, 240);
    std::fs::write(&manifest_path, &manifest_text).unwrap();
    let evidence_root = dir.join("evidence");
    let inventory = test_inventory();

    let req = DyadRequest {
        manifest_path: manifest_path.to_string_lossy().into_owned(),
        cohort: "development".to_string(),
        kind: "primary".to_string(),
        index: 0,
        evidence_root: evidence_root.to_string_lossy().into_owned(),
        treatment_batch_version: 2,
        protocol_version: 2,
        development: None,
    };
    let receipt = run_dyad(&req, &inventory).unwrap();

    // Honest receipt: measured counts, real manifest hash, measured cost.
    let mut h = sha2::Sha256::new();
    h.update(manifest_text.as_bytes());
    let expect_sha = format!("sha256:{}", hex::encode(h.finalize()));
    assert_eq!(receipt.state, "valid");
    assert_eq!((receipt.cases_attempted, receipt.cases_valid), (240, 240));
    assert_eq!(receipt.manifest_sha256, expect_sha);
    assert!(receipt.primary.is_finite());
    assert!(receipt.wall_seconds >= 0.0);
    assert!(receipt.attempted_hash.starts_with("sha256:"));
    assert!(receipt.terminal_hash.starts_with("sha256:"));

    let slot = evidence_root.join("development/primary-0000");
    for name in [
        "dyad-receipt.json",
        "primary.json",
        "dyad-result.json",
        "agent-policy-agent-0.json",
        "agent-policy-agent-1.json",
        "branch-probabilities-agent-0.json",
        "branch-probabilities-agent-1.json",
        "role-folds-agent-0.json",
        "role-folds-agent-1.json",
        "evidence.sqlite",
    ] {
        assert!(slot.join(name).exists(), "missing slot artifact {name}");
    }
    let stored_primary: f64 = std::fs::read_to_string(slot.join("primary.json"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert!((stored_primary - receipt.primary).abs() < 1e-15);

    // Root journal: attempted then terminal, hash-chained from genesis.
    let journal = journal_hashes(&evidence_root.join("journal.jsonl"));
    assert_eq!(journal.len(), 2);
    assert_eq!(journal[0].0, "attempted");
    assert_eq!(journal[1].0, "terminal");
    assert_eq!(journal[0].1, ald_types::GENESIS_HASH);
    assert_eq!(journal[1].1, journal[0].2);
    assert_eq!(receipt.attempted_hash, journal[0].2);
    assert_eq!(receipt.terminal_hash, journal[1].2);

    // Evidence streams verify through ald-evidence with real contents.
    let conn = ald_evidence::open(slot.join("evidence.sqlite").to_str().unwrap()).unwrap();
    assert_eq!(
        ald_evidence::verify_stream(&conn, ald_types::EventStream::Turns).unwrap(),
        240
    );
    assert_eq!(
        ald_evidence::verify_stream(&conn, ald_types::EventStream::Audit).unwrap(),
        2
    );

    // Per-case probabilities: full branch maps, valid distributions,
    // exact one-hot replay on the paired draw.
    for role in ["agent-0", "agent-1"] {
        let text = std::fs::read_to_string(slot.join(format!("branch-probabilities-{role}.json")))
            .unwrap();
        let cases: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
        assert_eq!(cases.len(), 120);
        for c in &cases {
            let native: Vec<f64> = serde_json::from_value(c["native"].clone()).unwrap();
            let replay: Vec<f64> = serde_json::from_value(c["replay"].clone()).unwrap();
            assert!((native.iter().sum::<f64>() - 1.0).abs() < 1e-9);
            assert_eq!(replay.iter().filter(|v| **v == 1.0).count(), 1);
            assert!((replay.iter().sum::<f64>() - 1.0).abs() < 1e-12);
            let draw = c["sharedDraw"].as_f64().unwrap();
            let expect_idx = ((draw * 4.0).floor() as usize).min(3);
            assert_eq!(replay[expect_idx], 1.0);
            assert_eq!(
                c["nativeMissing"],
                serde_json::json!(c["deliveredToken"].is_null())
            );
            let branches = c["branches"].as_object().unwrap();
            assert_eq!(branches.len(), 5);
            for b in ald_scenario::LV01_BRANCHES {
                let probs: Vec<f64> =
                    serde_json::from_value(branches[b]["probabilities"].clone()).unwrap();
                assert!((probs.iter().sum::<f64>() - 1.0).abs() < 1e-9, "branch {b}");
            }
        }
    }

    // Native distributions come from the frozen emergent policies:
    // reload both persisted full agent policies and reproduce the
    // first test case's token and native prediction exactly (the
    // opposite agent sends, the role agent receives).
    let policies: Vec<ald_learners::AgentPolicy> = ["agent-0", "agent-1"]
        .iter()
        .map(|role| {
            ald_learners::AgentPolicy::from_bytes(
                &std::fs::read(slot.join(format!("agent-policy-{role}.json"))).unwrap(),
            )
            .unwrap()
        })
        .collect();
    for (receiver_idx, role) in ["agent-0", "agent-1"].iter().enumerate() {
        let role = *role;
        let sender = &policies[1 - receiver_idx];
        let receiver = &policies[receiver_idx];
        let mk = |part: &str| AlaSeedInput {
            cohort: "development".to_string(),
            slot_kind: "primary".to_string(),
            slot_index: "0000".to_string(),
            purpose: "scenario".to_string(),
            role: format!("{role}-receiver"),
            partition: Some(part.to_string()),
            case_id: None,
            branch: None,
            seed_root: Some(ALA_SEED_ROOT_V2.to_string()),
        };
        let test_seed = derive_ala_seed(&mk("within-support-test")).unwrap();
        let text = std::fs::read_to_string(slot.join(format!("branch-probabilities-{role}.json")))
            .unwrap();
        let cases: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
        let first_test = &ald_scenario::generate_cases(&test_seed, 3, role, 120, &inventory)[0];
        let cands = candidate_types(&first_test.candidate_type_codes);
        let target_type = cands[first_test.target_index];
        let mut rng = ald_hashing::SeededPrng::new(&test_seed).derive("emergent-test");
        let token = sender.sample_token(target_type, &mut rng).unwrap();
        let delivered = if rng.next_float() < 1.0 / 8.0 {
            None
        } else {
            Some(token)
        };
        let expect_token = delivered.map(|id| inventory[id].clone());
        let expect_native: Vec<f64> = receiver
            .receiver_position_posterior(delivered, &cands)
            .unwrap()
            .iter()
            .map(|v| stable_f64(*v))
            .collect();
        match expect_token {
            None => assert!(
                cases[0]["deliveredToken"].is_null(),
                "first test token is not the frozen opposite-agent sample"
            ),
            Some(t) => assert_eq!(cases[0]["deliveredToken"], serde_json::json!(t)),
        }
        let got: Vec<f64> = serde_json::from_value(cases[0]["native"].clone()).unwrap();
        for (a, b) in expect_native.iter().zip(got.iter()) {
            assert!(
                (a - b).abs() < 1e-12,
                "native is not the frozen receiver posterior"
            );
        }
        // At least one missing-token case is honestly flagged, and every
        // dropped token predicts EXACT uniform in the artifacts.
        assert!(cases
            .iter()
            .any(|c| c["nativeMissing"] == serde_json::json!(true)));
        for c in &cases {
            if c["deliveredToken"].is_null() {
                assert_eq!(c["nativeMissing"], serde_json::json!(true));
                let native: Vec<f64> = serde_json::from_value(c["native"].clone()).unwrap();
                assert_eq!(native, vec![0.25; 4], "None token must be exact uniform");
            }
        }
    }

    // Role folds: one file per role in the committed fixture schema,
    // with per-case probabilities agreeing with branch-probabilities.
    for role in ["agent-0", "agent-1"] {
        let text = std::fs::read_to_string(slot.join(format!("role-folds-{role}.json"))).unwrap();
        let folds: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(folds["role"], serde_json::json!(role));
        let top = folds.as_object().unwrap();
        assert_eq!(top.len(), 5);
        for k in [
            "role",
            "training",
            "validation_fit",
            "validation_selection",
            "test",
        ] {
            assert!(top.contains_key(k), "role folds missing {k}");
        }
        assert_eq!(folds["training"].as_array().unwrap().len(), 1500);
        assert_eq!(folds["validation_fit"].as_array().unwrap().len(), 120);
        assert_eq!(folds["validation_selection"].as_array().unwrap().len(), 120);
        let fit0 = &folds["validation_fit"][0];
        let fit_keys = fit0.as_object().unwrap();
        assert_eq!(fit_keys.len(), 5);
        for k in [
            "case_id",
            "receiver_role",
            "delivered_token",
            "candidate_type_codes",
            "actual_selected_candidate_index",
        ] {
            assert!(fit_keys.contains_key(k), "fold record missing {k}");
        }
        let test_rows = folds["test"].as_array().unwrap();
        assert_eq!(test_rows.len(), 120);
        let branch_text =
            std::fs::read_to_string(slot.join(format!("branch-probabilities-{role}.json")))
                .unwrap();
        let branch_cases: Vec<serde_json::Value> = serde_json::from_str(&branch_text).unwrap();
        for (row, probs) in test_rows.iter().zip(branch_cases.iter()) {
            let keys = row.as_object().unwrap();
            assert_eq!(keys.len(), 7);
            for k in [
                "record",
                "native_distribution",
                "replay_distribution",
                "target_index",
                "native_missing",
                "branch_distributions",
                "branch_success",
            ] {
                assert!(keys.contains_key(k), "test row missing {k}");
            }
            assert_eq!(row["record"]["case_id"], probs["caseId"]);
            assert_eq!(row["native_distribution"], probs["native"]);
            assert_eq!(row["replay_distribution"], probs["replay"]);
            assert_eq!(row["target_index"], probs["targetIndex"]);
            assert_eq!(row["native_missing"], probs["nativeMissing"]);
            let dists = row["branch_distributions"].as_object().unwrap();
            let succ = row["branch_success"].as_object().unwrap();
            assert_eq!(dists.len(), 5);
            assert_eq!(succ.len(), 5);
            for b in ald_scenario::LV01_BRANCHES {
                assert_eq!(dists[b], probs["branches"][b]["probabilities"]);
                assert_eq!(succ[b], probs["branches"][b]["success"]);
            }
        }
        // The persisted folds deserialize as RoleFolds for select|score.
        let parsed: ald_analysis::dyad::RoleFolds = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed.role, role);
        assert_eq!(parsed.test.len(), 120);
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn determinism_same_seed_identical_journal_hashes() {
    // Small workload keeps this fast; the code path is identical.
    let manifest_text = test_manifest_text(8, 4, 4, 4);
    let mut journals = Vec::new();
    let mut primaries = Vec::new();
    let mut branch_bytes = Vec::new();
    let mut folds_bytes = Vec::new();
    let mut policy_bytes = Vec::new();
    for tag in ["det-a", "det-b"] {
        let dir = fresh_temp_dir(tag);
        let manifest_path = dir.join("manifest.json");
        std::fs::write(&manifest_path, &manifest_text).unwrap();
        let evidence_root = dir.join("evidence");
        let req = DyadRequest {
            manifest_path: manifest_path.to_string_lossy().into_owned(),
            cohort: "development".to_string(),
            kind: "primary".to_string(),
            index: 7,
            evidence_root: evidence_root.to_string_lossy().into_owned(),
            treatment_batch_version: 2,
            protocol_version: 2,
            development: None,
        };
        let receipt = run_dyad(&req, &test_inventory()).unwrap();
        journals.push(journal_hashes(&evidence_root.join("journal.jsonl")));
        primaries.push(receipt.primary);
        branch_bytes.push(
            std::fs::read(
                evidence_root.join("development/primary-0007/branch-probabilities-agent-0.json"),
            )
            .unwrap(),
        );
        folds_bytes.push(
            std::fs::read(evidence_root.join("development/primary-0007/role-folds-agent-0.json"))
                .unwrap(),
        );
        // Frozen emergent agent policies are byte-identical across
        // runs too.
        for role in ["agent-0", "agent-1"] {
            policy_bytes.push(
                std::fs::read(
                    evidence_root
                        .join(format!("development/primary-0007/agent-policy-{role}.json")),
                )
                .unwrap(),
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
    assert_eq!(journals[0], journals[1]);
    assert_eq!(journals[0].len(), 2);
    assert!((primaries[0] - primaries[1]).abs() < 1e-15);
    assert_eq!(branch_bytes[0], branch_bytes[1]);
    assert_eq!(folds_bytes[0], folds_bytes[1]);
    assert_eq!(policy_bytes[0], policy_bytes[2]);
    assert_eq!(policy_bytes[1], policy_bytes[3]);
}

#[test]
fn emergence_native_beats_uniform_by_margin() {
    // End-to-end emergence on the frozen workload: 1500 unsupervised
    // training episodes per direction symmetrically co-train the two
    // agents, so each role's frozen native Brier must beat uniform
    // (0.75) by over 0.3.
    let dir = fresh_temp_dir("emerge");
    let manifest_path = dir.join("manifest.json");
    std::fs::write(&manifest_path, test_manifest_text(3000, 240, 240, 240)).unwrap();
    let evidence_root = dir.join("evidence");
    let req = DyadRequest {
        manifest_path: manifest_path.to_string_lossy().into_owned(),
        cohort: "development".to_string(),
        kind: "primary".to_string(),
        index: 0,
        evidence_root: evidence_root.to_string_lossy().into_owned(),
        treatment_batch_version: 2,
        protocol_version: 2,
        development: None,
    };
    let receipt = run_dyad(&req, &test_inventory()).unwrap();
    assert_eq!(receipt.state, "valid");
    let text =
        std::fs::read_to_string(evidence_root.join("development/primary-0000/dyad-result.json"))
            .unwrap();
    let result: ald_analysis::dyad::DyadResult = serde_json::from_str(&text).unwrap();
    assert_eq!(result.roles.len(), 2);
    for r in &result.roles {
        let advantage = 0.75 - r.native_brier;
        assert!(
            advantage > 0.3,
            "role {} shows no emergence: native_brier={:.4} advantage={:.4}",
            r.role,
            r.native_brier,
            advantage
        );
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn pipeline_run_select_score_matches_receipt() {
    // Full pipeline on a small temp dev dyad: run -> select -> score.
    // Selection and scoring read the persisted role-folds files exactly
    // as `ala analyze select|score` does; scoring must succeed and
    // reproduce the receipt primary.
    let dir = fresh_temp_dir("pipeline");
    let manifest_path = dir.join("manifest.json");
    std::fs::write(&manifest_path, test_manifest_text(8, 4, 4, 4)).unwrap();
    let evidence_root = dir.join("evidence");
    let inventory = test_inventory();
    let req = DyadRequest {
        manifest_path: manifest_path.to_string_lossy().into_owned(),
        cohort: "development".to_string(),
        kind: "primary".to_string(),
        index: 0,
        evidence_root: evidence_root.to_string_lossy().into_owned(),
        treatment_batch_version: 2,
        protocol_version: 2,
        development: None,
    };
    let receipt = run_dyad(&req, &inventory).unwrap();
    let slot = evidence_root.join("development/primary-0000");

    let mut folds = Vec::new();
    let mut selected = Vec::new();
    for role in ["agent-0", "agent-1"] {
        let text = std::fs::read_to_string(slot.join(format!("role-folds-{role}.json"))).unwrap();
        let f: ald_analysis::dyad::RoleFolds = serde_json::from_str(&text).unwrap();
        assert_eq!(f.role, role);
        let sel = ald_analysis::select_ordinary_predictor(
            &inventory,
            &f.training,
            &f.validation_fit,
            &f.validation_selection,
        )
        .unwrap();
        assert!(ald_analysis::PREDICTOR_IDS.contains(&sel.selected));
        selected.push(sel.selected);
        folds.push(f);
    }

    let result = ald_analysis::analyze_dyad("primary-0000", &folds, &inventory).unwrap();
    for (role_result, want) in result.roles.iter().zip(selected.iter()) {
        assert_eq!(&role_result.selected_predictor_id, want);
    }
    assert!(
        (result.primary - receipt.primary).abs() < 1e-9,
        "scored primary {} differs from receipt {}",
        result.primary,
        receipt.primary
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn artifact_consolidation_under_50_files() {
    fn count_files(path: &std::path::Path) -> usize {
        let mut n = 0;
        for entry in std::fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            let file_type = entry.file_type().unwrap();
            if file_type.is_dir() {
                n += count_files(&entry.path());
            } else if file_type.is_file() {
                n += 1;
            }
        }
        n
    }
    // A full frozen-workload dyad must stay consolidated: v2 wrote
    // ~163k files/dyad (per-case artifacts); v3 writes a handful of
    // per-role files. Fails closed if per-case writes ever recur.
    let dir = fresh_temp_dir("consol");
    let manifest_path = dir.join("manifest.json");
    std::fs::write(&manifest_path, test_manifest_text(3000, 240, 240, 240)).unwrap();
    let evidence_root = dir.join("evidence");
    let req = DyadRequest {
        manifest_path: manifest_path.to_string_lossy().into_owned(),
        cohort: "development".to_string(),
        kind: "primary".to_string(),
        index: 0,
        evidence_root: evidence_root.to_string_lossy().into_owned(),
        treatment_batch_version: 2,
        protocol_version: 2,
        development: None,
    };
    run_dyad(&req, &test_inventory()).unwrap();

    let total = count_files(&evidence_root);
    assert!(
        total < 50,
        "a full dyad wrote {total} files; artifact consolidation regressed"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn failure_bad_manifest() {
    let dir = fresh_temp_dir("badmanifest");
    for (name, body) in [
        ("garbage.json", "this is not json".to_string()),
        ("empty.json", "{}".to_string()),
        (
            "wrong-study.json",
            test_manifest_text(8, 4, 4, 4).replace("ALA-LV", "NOPE"),
        ),
    ] {
        let manifest_path = dir.join(name);
        std::fs::write(&manifest_path, &body).unwrap();
        let req = DyadRequest {
            manifest_path: manifest_path.to_string_lossy().into_owned(),
            cohort: "development".to_string(),
            kind: "primary".to_string(),
            index: 0,
            evidence_root: dir.join("evidence").to_string_lossy().into_owned(),
            treatment_batch_version: 2,
            protocol_version: 2,
            development: None,
        };
        let err = run_dyad(&req, &test_inventory()).unwrap_err().to_string();
        assert!(err.contains("manifest"), "unexpected error: {err}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn failure_missing_admission() {
    let dir = fresh_temp_dir("noadmit");
    let manifest_path = dir.join("manifest.json");
    std::fs::write(&manifest_path, test_manifest_text(8, 4, 4, 4)).unwrap();
    let evidence_root = dir.join("evidence");
    let req = DyadRequest {
        manifest_path: manifest_path.to_string_lossy().into_owned(),
        cohort: "A".to_string(),
        kind: "primary".to_string(),
        index: 0,
        evidence_root: evidence_root.to_string_lossy().into_owned(),
        treatment_batch_version: 2,
        protocol_version: 2,
        development: None,
    };
    // No admission.json: study cohort refused before any work.
    let err = run_dyad(&req, &test_inventory()).unwrap_err().to_string();
    assert!(err.contains("admission"), "unexpected error: {err}");
    assert!(!evidence_root.join("journal.jsonl").exists());

    // Explicit refusal is also refused.
    std::fs::create_dir_all(&evidence_root).unwrap();
    std::fs::write(
        evidence_root.join("admission.json"),
        r#"{"decision":"NO-GO"}"#,
    )
    .unwrap();
    let err = run_dyad(&req, &test_inventory()).unwrap_err().to_string();
    assert!(err.contains("admission"), "unexpected error: {err}");

    // GO admission lets the study cohort run.
    std::fs::write(evidence_root.join("admission.json"), r#"{"decision":"GO"}"#).unwrap();
    let receipt = run_dyad(&req, &test_inventory()).unwrap();
    assert_eq!(receipt.state, "valid");
    assert_eq!((receipt.cases_attempted, receipt.cases_valid), (4, 4));
    let _ = std::fs::remove_dir_all(&dir);
}
