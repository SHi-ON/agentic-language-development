//! Fail-closed CLI number parsing (P0-FORK-01) and manifest validation
//! (P0-FORK-02): malformed numbers exit 2 naming the flag before any
//! writes; `ala manifest` accepts only protocol versions 1 and 2.

use std::path::PathBuf;
use std::process::Command;

fn ala() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ala"))
}

fn scratch_dir(test: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ala-parse-{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn small_manifest(dir: &std::path::Path) -> String {
    let path = dir.join("manifest.json");
    std::fs::write(
        &path,
        r#"{"manifestVersion":"ala-lv-execution-manifest/v1","studyId":"ALA-LV","workload":{"trainingCases":16,"validationFitCases":8,"validationSelectionCases":8,"withinSupportTestCases":8}}"#,
    )
    .unwrap();
    path.to_str().unwrap().to_string()
}

fn base_run_args(manifest: &str, root: &std::path::Path) -> Vec<String> {
    vec![
        "run".to_string(),
        "--manifest".to_string(),
        manifest.to_string(),
        "--cohort".to_string(),
        "development".to_string(),
        "--kind".to_string(),
        "primary".to_string(),
        "--index".to_string(),
        "0".to_string(),
        "--evidence-root".to_string(),
        root.to_str().unwrap().to_string(),
    ]
}

fn fixture_dyad(cohort: &str) -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(format!("cohort-{cohort}/primary-0000"))
        .to_str()
        .unwrap()
        .to_string()
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

#[test]
fn run_rejects_non_numeric_protocol_version() {
    let dir = scratch_dir("run-proto");
    let manifest = small_manifest(&dir);
    let root = dir.join("evidence");
    let mut args = base_run_args(&manifest, &root);
    args.push("--protocol-version".to_string());
    args.push("x".to_string());
    let out = ala().args(&args).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--protocol-version"),
        "stderr must name the flag, got: {stderr}"
    );
    assert!(
        !root.exists(),
        "rejected run must not create the evidence root"
    );
}

#[test]
fn run_rejects_non_numeric_treatment_batch_version() {
    let dir = scratch_dir("run-tbv");
    let manifest = small_manifest(&dir);
    let root = dir.join("evidence");
    let mut args = base_run_args(&manifest, &root);
    args.push("--treatment-batch-version".to_string());
    args.push("x".to_string());
    let out = ala().args(&args).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--treatment-batch-version"),
        "stderr must name the flag, got: {stderr}"
    );
    assert!(!root.exists(), "rejected run must not write");
}

#[test]
fn run_rejects_non_numeric_index() {
    let dir = scratch_dir("run-index");
    let manifest = small_manifest(&dir);
    let root = dir.join("evidence");
    let mut args = base_run_args(&manifest, &root);
    let pos = args.iter().position(|a| a == "--index").unwrap() + 1;
    args[pos] = "x".to_string();
    let out = ala().args(&args).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--index"),
        "stderr must name the flag, got: {stderr}"
    );
    assert!(!root.exists(), "rejected run must not write");
}

#[test]
fn run_rejects_out_of_range_protocol_version() {
    let dir = scratch_dir("run-proto-range");
    let manifest = small_manifest(&dir);
    let root = dir.join("evidence");
    let mut args = base_run_args(&manifest, &root);
    args.push("--protocol-version".to_string());
    args.push("99".to_string());
    let out = ala().args(&args).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--protocol-version"),
        "stderr must name the flag, got: {stderr}"
    );
    assert!(!root.exists(), "rejected run must not write");
}

fn score_with_bad_flag(flag: &str) -> (Option<i32>, String, bool) {
    let dir = scratch_dir(&format!("score-{}", flag.trim_start_matches("--")));
    let selection = dir.join("selection.json");
    let out = dir.join("scored.json");
    run_select("A", selection.to_str().unwrap());
    let scored = ala()
        .args([
            "analyze",
            "score",
            "--dyad",
            &fixture_dyad("A"),
            "--cohort",
            "A",
            "--out",
            out.to_str().unwrap(),
            "--selection",
            selection.to_str().unwrap(),
            "--bootstrap-seed",
            "s",
            flag,
            "x",
        ])
        .output()
        .unwrap();
    (
        scored.status.code(),
        String::from_utf8_lossy(&scored.stderr).into_owned(),
        out.exists(),
    )
}

#[test]
fn score_rejects_non_numeric_coverage() {
    let (code, stderr, wrote) = score_with_bad_flag("--coverage");
    assert_eq!(code, Some(2));
    assert!(
        stderr.contains("--coverage"),
        "stderr must name the flag, got: {stderr}"
    );
    assert!(!wrote, "rejected score must not write output");
}

#[test]
fn score_rejects_non_numeric_resamples() {
    let (code, stderr, wrote) = score_with_bad_flag("--bootstrap-resamples");
    assert_eq!(code, Some(2));
    assert!(
        stderr.contains("--bootstrap-resamples"),
        "stderr must name the flag, got: {stderr}"
    );
    assert!(!wrote, "rejected score must not write output");
}

#[test]
fn score_rejects_non_numeric_attempted() {
    let (code, stderr, wrote) = score_with_bad_flag("--attempted");
    assert_eq!(code, Some(2));
    assert!(
        stderr.contains("--attempted"),
        "stderr must name the flag, got: {stderr}"
    );
    assert!(!wrote, "rejected score must not write output");
}

#[test]
fn score_reports_bad_flag_even_when_out_exists() {
    // Parse errors precede the overwrite guard: exit 2, not exit 1.
    let dir = scratch_dir("score-out-exists");
    let selection = dir.join("selection.json");
    let out = dir.join("scored.json");
    run_select("A", selection.to_str().unwrap());
    std::fs::write(&out, "{}").unwrap();
    let scored = ala()
        .args([
            "analyze",
            "score",
            "--dyad",
            &fixture_dyad("A"),
            "--cohort",
            "A",
            "--out",
            out.to_str().unwrap(),
            "--selection",
            selection.to_str().unwrap(),
            "--bootstrap-seed",
            "s",
            "--coverage",
            "x",
        ])
        .output()
        .unwrap();
    assert_eq!(scored.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&scored.stderr);
    assert!(
        stderr.contains("--coverage"),
        "stderr must name the flag, got: {stderr}"
    );
}

#[test]
fn manifest_rejects_non_numeric_version() {
    let out = ala()
        .args(["manifest", "--protocol-version", "x"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--protocol-version"),
        "stderr must name the flag, got: {stderr}"
    );
}

#[test]
fn manifest_rejects_version_99() {
    let out = ala()
        .args(["manifest", "--protocol-version", "99"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(
        out.stdout.is_empty(),
        "rejected manifest must print nothing to stdout"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--protocol-version"),
        "stderr must name the flag, got: {stderr}"
    );
}

#[test]
fn manifest_rejects_version_0() {
    let out = ala()
        .args(["manifest", "--protocol-version", "0"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn manifest_accepts_v1_and_v2() {
    for (flag, want) in [("1", 1), ("2", 2)] {
        let out = ala()
            .args(["manifest", "--protocol-version", flag])
            .output()
            .unwrap();
        assert!(out.status.success(), "manifest v{flag} must succeed");
        let doc: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(doc["protocolVersion"], want);
        assert!(doc["manifest"]
            .as_str()
            .unwrap()
            .contains(&format!(".v{flag}.json")));
    }
    // Default is v2.
    let out = ala().args(["manifest"]).output().unwrap();
    assert!(out.status.success(), "default manifest must succeed");
    let doc: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(doc["protocolVersion"], 2);
}

#[test]
fn journal_rejects_non_json_line() {
    let dir = scratch_dir("journal-nonjson");
    let journal = dir.join("journal.jsonl");
    let out = ala()
        .args([
            "journal",
            "--journal",
            journal.to_str().unwrap(),
            "--line",
            "not json",
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(
        !journal.exists(),
        "rejected journal append must not create the file"
    );
}

#[test]
fn journal_rejects_caller_chain_fields() {
    let dir = scratch_dir("journal-chainfields");
    let journal = dir.join("journal.jsonl");
    let out = ala()
        .args([
            "journal",
            "--journal",
            journal.to_str().unwrap(),
            "--line",
            r#"{"kind":"terminal","seq":0,"previousHash":"x","entryHash":"y"}"#,
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(!journal.exists());
}

#[test]
fn journal_chains_two_appends() {
    let dir = scratch_dir("journal-chain");
    let journal = dir.join("journal.jsonl");
    let j = journal.to_str().unwrap();
    for line in [
        r#"{"kind":"attempted"}"#,
        r#"{"kind":"terminal","state":"valid"}"#,
    ] {
        let out = ala()
            .args(["journal", "--journal", j, "--line", line])
            .output()
            .unwrap();
        assert!(out.status.success());
        let hash = String::from_utf8_lossy(&out.stdout);
        assert!(hash.trim_start().starts_with("sha256:"), "got: {hash}");
    }
    let text = std::fs::read_to_string(&journal).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2);
    let v0: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    let v1: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
    assert_eq!(v0["seq"], 0);
    assert_eq!(v1["seq"], 1);
    assert_eq!(v1["previousHash"], v0["entryHash"]);
}

#[test]
fn journal_io_error_exits_nonzero_without_panic() {
    // --journal under a path whose parent is a FILE: creation fails.
    let dir = scratch_dir("journal-io");
    let blocker = dir.join("blocker");
    std::fs::write(&blocker, "x").unwrap();
    let journal = blocker.join("journal.jsonl");
    let out = ala()
        .args([
            "journal",
            "--journal",
            journal.to_str().unwrap(),
            "--line",
            r#"{"kind":"terminal"}"#,
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("ala journal"), "got: {stderr}");
    assert!(
        !stderr.contains("panicked"),
        "must not panic, got: {stderr}"
    );
}

#[test]
fn manifest_succeeds_from_any_working_directory() {
    // The embedded-manifest fallback: no protocols/ dir under CWD, yet the
    // shipped bytes still validate.
    let dir = scratch_dir("manifest-cwd");
    let out = ala()
        .args(["manifest", "--protocol-version", "2"])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "manifest must work outside the repo root"
    );
}
