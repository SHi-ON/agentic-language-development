//! Seeds, workload, manifest, and journal integration tests.

mod common;

use ald_orchestrator::{
    derive_ala_seed, execution_counts_from_manifest, journal_append_entry, prototype_workload,
    verify_journal_chain, AlaSeedInput, StudyCounts,
};
use common::{fresh_temp_dir, test_manifest_text};

#[test]
fn seed_layout_matches_ts() {
    let s = derive_ala_seed(&AlaSeedInput {
        cohort: "A".to_string(),
        slot_kind: "primary".to_string(),
        slot_index: "0300".to_string(),
        purpose: "scenario".to_string(),
        role: "agent-0-receiver".to_string(),
        partition: Some("training".to_string()),
        case_id: None,
        branch: None,
        seed_root: None,
    })
    .unwrap();
    assert_eq!(s.len(), 64);
}

#[test]
fn workload_splits_evenly() {
    let w = prototype_workload(&StudyCounts {
        training_cases: 240,
        validation_fit_cases: 240,
        validation_selection_cases: 240,
        within_support_test_cases: 240,
    })
    .unwrap();
    assert_eq!(w.validation_fit_cases, 120);
}

#[test]
fn rejects_odd_workload() {
    assert!(prototype_workload(&StudyCounts {
        training_cases: 3,
        validation_fit_cases: 2,
        validation_selection_cases: 2,
        within_support_test_cases: 2,
    })
    .is_err());
}
fn journal_body(kind: &str) -> serde_json::Map<String, serde_json::Value> {
    let mut m = serde_json::Map::new();
    m.insert("kind".to_string(), serde_json::json!(kind));
    m
}

#[test]
fn journal_round_trip_stays_chained() {
    let dir = fresh_temp_dir("journal-ok");
    let path = dir.join("journal.jsonl");
    let p = path.to_str().unwrap();
    let h0 = journal_append_entry(p, journal_body("attempted")).unwrap();
    let h1 = journal_append_entry(p, journal_body("terminal")).unwrap();
    let h2 = journal_append_entry(p, journal_body("terminal")).unwrap();
    let (next, head) = verify_journal_chain(p).unwrap();
    assert_eq!((next, head.as_str()), (3, h2.as_str()));
    let lines: Vec<String> = std::fs::read_to_string(&path)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect();
    assert_eq!(lines.len(), 3);
    let v0: serde_json::Value = serde_json::from_str(&lines[0]).unwrap();
    assert_eq!(v0["seq"], 0);
    assert_eq!(v0["previousHash"], ald_types::GENESIS_HASH);
    assert_eq!(v0["entryHash"], h0);
    let v1: serde_json::Value = serde_json::from_str(&lines[1]).unwrap();
    assert_eq!(
        (
            v1["seq"].as_u64().unwrap(),
            v1["previousHash"].as_str().unwrap()
        ),
        (1, h0.as_str())
    );
    assert_eq!(v1["entryHash"], h1);
}

#[test]
fn journal_append_rejects_tampered_entry_hash() {
    let dir = fresh_temp_dir("journal-tamper");
    let path = dir.join("journal.jsonl");
    let p = path.to_str().unwrap();
    journal_append_entry(p, journal_body("attempted")).unwrap();
    journal_append_entry(p, journal_body("terminal")).unwrap();
    let before = std::fs::read_to_string(&path).unwrap();
    // Flip one hex char of the second line's entryHash.
    let mut lines: Vec<String> = before.lines().map(str::to_string).collect();
    let mut v1: serde_json::Value = serde_json::from_str(&lines[1]).unwrap();
    let orig = v1["entryHash"].as_str().unwrap().to_string();
    let mut tampered = orig.clone();
    let flip_at = tampered.len() - 1;
    tampered.replace_range(flip_at.., if orig.ends_with('0') { "1" } else { "0" });
    assert_ne!(tampered, orig);
    v1["entryHash"] = serde_json::json!(tampered);
    lines[1] = serde_json::to_string(&v1).unwrap();
    std::fs::write(&path, lines.join("\n") + "\n").unwrap();
    let err = journal_append_entry(p, journal_body("terminal"))
        .unwrap_err()
        .to_string();
    assert!(err.contains("entryHash"), "unexpected error: {err}");
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        lines.join("\n") + "\n",
        "failed append must not write"
    );
}

#[test]
fn journal_append_rejects_broken_previous_hash_link() {
    let dir = fresh_temp_dir("journal-link");
    let path = dir.join("journal.jsonl");
    let p = path.to_str().unwrap();
    journal_append_entry(p, journal_body("attempted")).unwrap();
    journal_append_entry(p, journal_body("terminal")).unwrap();
    let before = std::fs::read_to_string(&path).unwrap();
    let mut lines: Vec<String> = before.lines().map(str::to_string).collect();
    let mut v1: serde_json::Value = serde_json::from_str(&lines[1]).unwrap();
    v1["previousHash"] = serde_json::json!("sha256:00");
    lines[1] = serde_json::to_string(&v1).unwrap();
    std::fs::write(&path, lines.join("\n") + "\n").unwrap();
    let err = journal_append_entry(p, journal_body("terminal"))
        .unwrap_err()
        .to_string();
    assert!(err.contains("previousHash"), "unexpected error: {err}");
}

#[test]
fn journal_append_rejects_seq_gap_and_replay() {
    for (tag, seqs) in [("gap", [0u64, 2]), ("replay", [0, 0])] {
        let dir = fresh_temp_dir(&format!("journal-seq-{tag}"));
        let path = dir.join("journal.jsonl");
        let p = path.to_str().unwrap();
        journal_append_entry(p, journal_body("attempted")).unwrap();
        journal_append_entry(p, journal_body("terminal")).unwrap();
        let before = std::fs::read_to_string(&path).unwrap();
        let mut lines: Vec<String> = before.lines().map(str::to_string).collect();
        for (line, want) in lines.iter_mut().zip(seqs) {
            let mut v: serde_json::Value = serde_json::from_str(line).unwrap();
            v["seq"] = serde_json::json!(want);
            *line = serde_json::to_string(&v).unwrap();
        }
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();
        let err = journal_append_entry(p, journal_body("terminal"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("seq"), "unexpected error for {tag}: {err}");
    }
}

#[test]
fn journal_append_rejects_non_json_line() {
    let dir = fresh_temp_dir("journal-garbage");
    let path = dir.join("journal.jsonl");
    let p = path.to_str().unwrap();
    journal_append_entry(p, journal_body("attempted")).unwrap();
    std::fs::write(&path, "not json\n").unwrap();
    let err = journal_append_entry(p, journal_body("terminal"))
        .unwrap_err()
        .to_string();
    assert!(err.contains("line 1"), "unexpected error: {err}");
}

#[test]
fn journal_append_entry_rejects_caller_chain_fields() {
    let dir = fresh_temp_dir("journal-chainfields");
    let path = dir.join("journal.jsonl");
    let p = path.to_str().unwrap();
    for f in ["seq", "previousHash", "entryHash"] {
        let mut m = journal_body("terminal");
        m.insert(f.to_string(), serde_json::json!(0));
        assert!(
            journal_append_entry(p, m).is_err(),
            "caller chain field {f} must be rejected"
        );
    }
    assert!(!path.exists(), "rejected append must not create the file");
}

#[test]
fn manifest_counts_parse_and_reject() {
    let ok = execution_counts_from_manifest(&test_manifest_text(3000, 240, 240, 240)).unwrap();
    assert_eq!(
        (ok.training_cases, ok.within_support_test_cases),
        (3000, 240)
    );
    assert!(execution_counts_from_manifest("not json").is_err());
    assert!(execution_counts_from_manifest("{}").is_err());
    assert!(execution_counts_from_manifest(
        &test_manifest_text(3000, 240, 240, 240).replace("ALA-LV", "WRONG")
    )
    .is_err());
    assert!(execution_counts_from_manifest(&test_manifest_text(0, 240, 240, 240)).is_err());
}
