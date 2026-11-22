//! Shared integration-test helpers.

#![allow(dead_code)] // each test binary uses a subset.

pub static TEST_CTR: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn test_inventory() -> Vec<String> {
    (0..32).map(|i| format!("tok{i:02}")).collect()
}

pub fn test_manifest_text(training: u64, fit: u64, sel: u64, test: u64) -> String {
    format!(
        "{{\"manifestVersion\":\"ala-lv-execution-manifest/v1\",\"studyId\":\"ALA-LV\",\
         \"workload\":{{\"trainingCases\":{training},\"validationFitCases\":{fit},\
         \"validationSelectionCases\":{sel},\"withinSupportTestCases\":{test}}}}}"
    )
}

pub fn fresh_temp_dir(tag: &str) -> std::path::PathBuf {
    let n = TEST_CTR.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("ald-orch-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

pub fn journal_hashes(journal_path: &std::path::Path) -> Vec<(String, String, String)> {
    // (kind, previousHash, entryHash) per line.
    std::fs::read_to_string(journal_path)
        .unwrap()
        .lines()
        .map(|line| {
            let v: serde_json::Value = serde_json::from_str(line).unwrap();
            (
                v["kind"].as_str().unwrap().to_string(),
                v["previousHash"].as_str().unwrap().to_string(),
                v["entryHash"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}
