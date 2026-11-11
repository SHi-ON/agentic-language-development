//! Regression: score the committed locked-mean-anchored fixture dyads and
//! assert the cohort interval means match the locked v2 cohort means.

use std::path::PathBuf;
use std::process::Command;

const LOCKED_MEANS: [(&str, f64); 2] = [("A", 0.12497749132421103), ("B", 0.12302667184362459)];

fn ala() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ala"))
}

fn fixture_dyad(cohort: &str) -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(format!("cohort-{cohort}/primary-0000"))
        .to_str()
        .unwrap()
        .to_string()
}

fn scratch_dir(test: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("ala-score-fixture-{}-{}", test, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run_select(cohort: &str, out: &str) {
    let status = ala()
        .args([
            "analyze",
            "select",
            "--dyad",
            &fixture_dyad(cohort),
            "--cohort",
            cohort,
            "--out",
            out,
        ])
        .status()
        .unwrap();
    assert!(status.success(), "select failed for cohort {cohort}");
}

fn run_score(cohort: &str, selection: &str, out: &str) {
    let status = ala()
        .args([
            "analyze",
            "score",
            "--dyad",
            &fixture_dyad(cohort),
            "--cohort",
            cohort,
            "--out",
            out,
            "--selection",
            selection,
            "--bootstrap-seed",
            "fixture-seed",
            "--bootstrap-resamples",
            "100",
            "--attempted",
            "1",
        ])
        .status()
        .unwrap();
    assert!(status.success(), "score failed for cohort {cohort}");
}

#[test]
fn score_fixture_matches_locked_v2_cohort_means() {
    for (cohort, locked) in LOCKED_MEANS {
        let dir = scratch_dir(&format!("means-{cohort}"));
        let selection = dir.join("selection.json");
        let scored = dir.join("scored.json");
        run_select(cohort, selection.to_str().unwrap());
        run_score(
            cohort,
            selection.to_str().unwrap(),
            scored.to_str().unwrap(),
        );

        let doc: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&scored).unwrap()).unwrap();
        assert_eq!(doc["stage"], "score");
        assert_eq!(doc["cohort"], cohort);
        assert_eq!(doc["n"], 1);
        let mean = doc["interval"]["mean"].as_f64().unwrap();
        assert!(
            (mean - locked).abs() < 1e-9,
            "cohort {cohort}: interval mean {mean} differs from locked {locked}"
        );
        // Full estimation block is present: per-dyad contrasts, simultaneous
        // bounds, invalidity bounds, fidelity, bootstrap.
        assert!(doc["dyads"][0]["brierContrasts"].is_object());
        assert!(doc["dyads"][0]["fidelity"]["mean_squared_distance"].is_f64());
        assert!(doc["controlBounds"]["normal-minus-disabled"].is_object());
        assert!(doc["invalidityBounds"]["primary"].is_object());
        assert!(doc["bootstrap"]["estimate"].is_f64());
        assert!(doc["docSha256"].is_string());
    }
}

#[test]
fn select_records_uniform_with_lock_hashes() {
    let dir = scratch_dir("select");
    let selection = dir.join("selection.json");
    run_select("A", selection.to_str().unwrap());
    let doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&selection).unwrap()).unwrap();
    let records = doc["records"].as_array().unwrap();
    assert_eq!(records.len(), 2);
    for r in records {
        assert_eq!(r["selectedPredictorId"], "uniform");
        assert!(r["recordSha256"].as_str().unwrap().len() > 16);
    }
}

#[test]
fn analyze_refuses_to_overwrite_outputs() {
    let dir = scratch_dir("overwrite");
    let selection = dir.join("selection.json");
    let scored = dir.join("scored.json");
    std::fs::write(&selection, "{}").unwrap();
    std::fs::write(&scored, "{}").unwrap();
    let sel_status = ala()
        .args([
            "analyze",
            "select",
            "--dyad",
            &fixture_dyad("A"),
            "--cohort",
            "A",
            "--out",
            selection.to_str().unwrap(),
        ])
        .status()
        .unwrap();
    assert!(!sel_status.success(), "select overwrote an existing output");
    let score_status = ala()
        .args([
            "analyze",
            "score",
            "--dyad",
            &fixture_dyad("A"),
            "--cohort",
            "A",
            "--out",
            scored.to_str().unwrap(),
            "--selection",
            selection.to_str().unwrap(),
            "--bootstrap-seed",
            "s",
            "--bootstrap-resamples",
            "10",
        ])
        .status()
        .unwrap();
    assert!(
        !score_status.success(),
        "score overwrote an existing output"
    );
}
