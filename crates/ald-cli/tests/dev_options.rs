//! CLI gating for development flags: duplicates, malformed values, and
//! non-development cohorts are refused before anything is written; valid
//! development runs and explicit defaults behave.

use std::path::PathBuf;
use std::process::Command;

fn ala() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ala"))
}

fn scratch_dir(test: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ala-dev-opts-{test}-{}", std::process::id()));
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

fn base_args(manifest: &str, cohort: &str, root: &std::path::Path) -> Vec<String> {
    vec![
        "run".to_string(),
        "--manifest".to_string(),
        manifest.to_string(),
        "--cohort".to_string(),
        cohort.to_string(),
        "--kind".to_string(),
        "primary".to_string(),
        "--index".to_string(),
        "0".to_string(),
        "--evidence-root".to_string(),
        root.to_str().unwrap().to_string(),
    ]
}

#[test]
fn run_rejects_duplicate_flags_before_writing() {
    for dup in [
        "--manifest",
        "--cohort",
        "--training-interactions-per-direction",
        "--learning-mode",
        "--drop-probability",
        "--active-sender-tokens",
        "--held-out-types",
    ] {
        let dir = scratch_dir("dup");
        let manifest = small_manifest(&dir);
        let root = dir.join("evidence");
        let mut args = base_args(&manifest, "development", &root);
        let value = match dup {
            "--manifest" => manifest.clone(),
            "--cohort" => "development".to_string(),
            "--training-interactions-per-direction" => "3".to_string(),
            "--learning-mode" => "none".to_string(),
            "--drop-probability" => "1/2".to_string(),
            "--active-sender-tokens" => "8".to_string(),
            _ => "15".to_string(),
        };
        // --manifest/--cohort are already in the base args (push once to
        // duplicate); development flags are absent (push twice).
        let repeats = if dup == "--manifest" || dup == "--cohort" {
            1
        } else {
            2
        };
        for _ in 0..repeats {
            args.push(dup.to_string());
            args.push(value.clone());
        }
        let out = ala().args(&args).output().unwrap();
        assert_eq!(out.status.code(), Some(2), "duplicate {dup} must exit 2");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("duplicate"),
            "stderr names the fault: {stderr}"
        );
        assert!(!root.exists(), "rejected run must not create {root:?}");
    }
}

#[test]
fn run_rejects_malformed_development_values_before_writing() {
    for (flag, value) in [
        ("--training-interactions-per-direction", "0"),
        ("--training-interactions-per-direction", "abc"),
        ("--learning-mode", "sometimes"),
        ("--learning-mode", ""),
        ("--drop-probability", "1/0"),
        ("--drop-probability", "2/1"),
        ("--drop-probability", "half"),
        ("--active-sender-tokens", "7"),
        ("--active-sender-tokens", "32 "),
        ("--held-out-types", "16"),
        ("--held-out-types", "1,1"),
        ("--held-out-types", "nope"),
    ] {
        let dir = scratch_dir("malformed");
        let manifest = small_manifest(&dir);
        let root = dir.join("evidence");
        let mut args = base_args(&manifest, "development", &root);
        args.push(flag.to_string());
        args.push(value.to_string());
        let out = ala().args(&args).output().unwrap();
        assert_eq!(
            out.status.code(),
            Some(2),
            "{flag} {value:?} must exit 2, got {:?}: {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(!root.exists(), "rejected run must not create {root:?}");
    }
}

#[test]
fn run_rejects_development_options_outside_development_cohort() {
    for (flag, value) in [
        ("--training-interactions-per-direction", "3"),
        ("--learning-mode", "none"),
        ("--drop-probability", "1/3"),
        ("--active-sender-tokens", "8"),
        ("--held-out-types", "15"),
    ] {
        for cohort in ["A", "B"] {
            let dir = scratch_dir("cohort");
            let manifest = small_manifest(&dir);
            let root = dir.join("evidence");
            let mut args = base_args(&manifest, cohort, &root);
            args.push(flag.to_string());
            args.push(value.to_string());
            let out = ala().args(&args).output().unwrap();
            assert!(
                !out.status.success(),
                "{flag} with cohort {cohort} must fail"
            );
            let stderr = String::from_utf8_lossy(&out.stderr);
            assert!(
                stderr.contains("development"),
                "stderr names the cohort gate: {stderr}"
            );
            assert!(!root.exists(), "rejected run must not create {root:?}");
        }
    }
}

#[test]
fn run_development_happy_path_writes_sidecar_in_suffixed_root() {
    let dir = scratch_dir("happy");
    let manifest = small_manifest(&dir);
    let root = dir.join("evidence");
    let mut args = base_args(&manifest, "development", &root);
    args.extend([
        "--training-interactions-per-direction".to_string(),
        "3".to_string(),
        "--learning-mode".to_string(),
        "joint".to_string(),
    ]);
    let out = ala().args(&args).output().unwrap();
    assert!(
        out.status.success(),
        "dev run failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let receipt: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(receipt["state"], "valid");
    assert!(receipt["developmentConfigSha256"].is_string());
    let entries: Vec<_> = std::fs::read_dir(root.join("development"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(entries.len(), 1);
    assert!(
        entries[0].starts_with("primary-0000-dev-"),
        "suffixed root: {:?}",
        entries[0]
    );
    let sidecar: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join(format!(
            "development/{}/development-config.json",
            entries[0]
        )))
        .unwrap(),
    )
    .unwrap();
    assert_eq!(sidecar["counts"]["trainingInteractionsPerDirection"], 3);
    assert_eq!(
        sidecar["digests"]["developmentConfigSha256"],
        receipt["developmentConfigSha256"]
    );
}

#[test]
fn run_explicit_defaults_take_the_legacy_path() {
    let dir = scratch_dir("explicit");
    let manifest = small_manifest(&dir);
    let root = dir.join("evidence");
    let mut args = base_args(&manifest, "development", &root);
    // The small manifest carries 8 training cases per direction.
    args.extend([
        "--training-interactions-per-direction".to_string(),
        "8".to_string(),
        "--learning-mode".to_string(),
        "joint".to_string(),
        "--drop-probability".to_string(),
        "1/8".to_string(),
        "--active-sender-tokens".to_string(),
        "32".to_string(),
        "--held-out-types".to_string(),
        "".to_string(),
    ]);
    let out = ala().args(&args).output().unwrap();
    assert!(
        out.status.success(),
        "explicit-default run failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let receipt: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert!(receipt.get("developmentConfigSha256").is_none());
    let slot = root.join("development/primary-0000");
    assert!(slot.join("dyad-receipt.json").exists());
    assert!(!slot.join("development-config.json").exists());
}
