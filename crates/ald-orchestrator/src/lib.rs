//! ALA dyad orchestration: seeds, workload, manifest, and the dyad runner.
//!
//! Rust port of `packages/orchestrator/src/ala/` (seeds, workload,
//! manifest, runner) plus the LV01 experiment wiring used by the ALA path.
//! The runner executes one full dyad deterministically from the frozen
//! manifest and writes the evidence database, journal lines, and receipt.

use ald_hashing::derive_seed_hex;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OrchestratorError {
    #[error("orchestrator domain error: {0}")]
    Domain(String),
    #[error("io: {0}")]
    Io(String),
    #[error("evidence: {0}")]
    Evidence(#[from] ald_evidence::EvidenceError),
    #[error("analysis: {0}")]
    Analysis(#[from] ald_analysis::AnalysisError),
}

// ---------------------------------------------------------------------------
// Seeds (ala-seeds.ts)
// ---------------------------------------------------------------------------

pub const ALA_SEED_ROOT: &str = "ala-ledger-value-v1";
pub const ALA_SEED_ROOT_V2: &str = "ala-ledger-value-v2";
pub const ALA_STUDY_ID: &str = "ALA-LV";

pub const SEED_PURPOSES: [&str; 8] = [
    "scenario",
    "learner/baby-a",
    "learner/baby-b",
    "channel",
    "action-draw",
    "intervention-shuffle",
    "branch-order",
    "analysis",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlaSeedInput {
    pub cohort: String,
    pub slot_kind: String,
    pub slot_index: String,
    pub purpose: String,
    pub role: String,
    pub partition: Option<String>,
    pub case_id: Option<String>,
    pub branch: Option<String>,
    pub seed_root: Option<String>,
}

/// Derive one ALA seed: root/study/cohort/kind/index/purpose/role[/partition/case/branch].
pub fn derive_ala_seed(input: &AlaSeedInput) -> Result<String, OrchestratorError> {
    for (name, value) in [
        ("cohort", &input.cohort),
        ("slotKind", &input.slot_kind),
        ("slotIndex", &input.slot_index),
        ("purpose", &input.purpose),
        ("role", &input.role),
    ] {
        if value.is_empty() || value.contains('\0') {
            return Err(OrchestratorError::Domain(format!(
                "ALA seeds: {name} is empty"
            )));
        }
    }
    if input.slot_index.len() != 4 || !input.slot_index.bytes().all(|b| b.is_ascii_digit()) {
        return Err(OrchestratorError::Domain(
            "ALA seeds: slotIndex must be zero-padded".to_string(),
        ));
    }
    let root = input.seed_root.as_deref().unwrap_or(ALA_SEED_ROOT);
    if root != ALA_SEED_ROOT && root != ALA_SEED_ROOT_V2 {
        return Err(OrchestratorError::Domain(
            "ALA seeds: seedRoot is not a known ALA namespace root".to_string(),
        ));
    }
    let part_owned;
    let case_owned;
    let branch_owned;
    let mut parts: Vec<&str> = vec![
        root,
        ALA_STUDY_ID,
        &input.cohort,
        &input.slot_kind,
        &input.slot_index,
        &input.purpose,
        &input.role,
    ];
    if let Some(p) = &input.partition {
        part_owned = p.clone();
        parts.push(Box::leak(part_owned.into_boxed_str()));
    }
    if let Some(c) = &input.case_id {
        case_owned = c.clone();
        parts.push(Box::leak(case_owned.into_boxed_str()));
    }
    if let Some(b) = &input.branch {
        branch_owned = b.clone();
        parts.push(Box::leak(branch_owned.into_boxed_str()));
    }
    Ok(derive_seed_hex(&parts))
}

// ---------------------------------------------------------------------------
// Workload (ala-workload.ts)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudyCounts {
    pub training_cases: u64,
    pub validation_fit_cases: u64,
    pub validation_selection_cases: u64,
    pub within_support_test_cases: u64,
}

pub fn study_counts_from_protocol(text: &str) -> Result<StudyCounts, OrchestratorError> {
    let v: serde_json::Value =
        serde_json::from_str(text).map_err(|e| OrchestratorError::Domain(e.to_string()))?;
    let at = |partition: &str| -> Result<u64, OrchestratorError> {
        v.pointer(&format!("/task/partitions/{partition}/cases"))
            .and_then(serde_json::Value::as_u64)
            .filter(|&n| n >= 1)
            .ok_or_else(|| {
                OrchestratorError::Domain(format!(
                    "ALA workload: study protocol has no positive {partition} cases"
                ))
            })
    };
    Ok(StudyCounts {
        training_cases: at("training")?,
        validation_fit_cases: at("validationFit")?,
        validation_selection_cases: at("validationSelection")?,
        within_support_test_cases: at("withinSupportTest")?,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionWorkload {
    pub profile: String,
    pub training_turns: u64,
    pub validation_fit_cases: u64,
    pub validation_selection_cases: u64,
    pub within_support_test_cases: u64,
}

/// Prototype workload: study totals split evenly across the two roles.
pub fn prototype_workload(counts: &StudyCounts) -> Result<CollectionWorkload, OrchestratorError> {
    for (name, value) in [
        ("trainingCases", counts.training_cases),
        ("validationFitCases", counts.validation_fit_cases),
        (
            "validationSelectionCases",
            counts.validation_selection_cases,
        ),
        ("withinSupportTestCases", counts.within_support_test_cases),
    ] {
        if value % 2 != 0 {
            return Err(OrchestratorError::Domain(format!(
                "ALA workload: study {name} is not evenly split across roles"
            )));
        }
    }
    Ok(CollectionWorkload {
        profile: "prototype".to_string(),
        training_turns: counts.training_cases,
        validation_fit_cases: counts.validation_fit_cases / 2,
        validation_selection_cases: counts.validation_selection_cases / 2,
        within_support_test_cases: counts.within_support_test_cases / 2,
    })
}

// ---------------------------------------------------------------------------
// Dyad runner
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DyadRequest {
    pub manifest_path: String,
    pub cohort: String,
    pub kind: String,
    pub index: u32,
    pub evidence_root: String,
    pub treatment_batch_version: u32,
    pub protocol_version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DyadReceipt {
    pub cohort: String,
    pub kind: String,
    pub index: u32,
    pub state: String,
    pub cases_attempted: u64,
    pub cases_valid: u64,
    pub manifest_sha256: String,
    #[serde(default)]
    pub protocol_version: u32,
    #[serde(default)]
    pub treatment_batch_version: u32,
    /// Dyad primary: mean per-role ordinary-minus-native Brier gap.
    #[serde(default)]
    pub primary: f64,
    /// Measured wall-clock cost of the run in seconds.
    #[serde(default)]
    pub wall_seconds: f64,
    /// Hash-chained root-journal entry hashes bounding this run.
    #[serde(default)]
    pub attempted_hash: String,
    #[serde(default)]
    pub terminal_hash: String,
}

/// Parse workload counts from an ALA execution manifest.
///
/// `manifest_path` points at the execution manifest (which pins the study
/// protocol by hash); the frozen per-partition case totals live in its
/// `workload` block. Refuses anything that is not valid JSON, does not
/// carry the frozen manifest version / study id, or lacks positive counts.
pub fn execution_counts_from_manifest(text: &str) -> Result<StudyCounts, OrchestratorError> {
    let v: serde_json::Value = serde_json::from_str(text)
        .map_err(|e| OrchestratorError::Domain(format!("ALA manifest: invalid JSON: {e}")))?;
    let version = v
        .get("manifestVersion")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    if version != "ala-lv-execution-manifest/v1" {
        return Err(OrchestratorError::Domain(format!(
            "ALA manifest: manifestVersion must be ala-lv-execution-manifest/v1, got {version:?}"
        )));
    }
    let study = v
        .get("studyId")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    if study != "ALA-LV" {
        return Err(OrchestratorError::Domain(format!(
            "ALA manifest: studyId must be ALA-LV, got {study:?}"
        )));
    }
    let at = |name: &str| -> Result<u64, OrchestratorError> {
        v.pointer(&format!("/workload/{name}"))
            .and_then(serde_json::Value::as_u64)
            .filter(|&n| n >= 1)
            .ok_or_else(|| {
                OrchestratorError::Domain(format!("ALA manifest: workload has no positive {name}"))
            })
    };
    Ok(StudyCounts {
        training_cases: at("trainingCases")?,
        validation_fit_cases: at("validationFitCases")?,
        validation_selection_cases: at("validationSelectionCases")?,
        within_support_test_cases: at("withinSupportTestCases")?,
    })
}

fn manifest_sha256_hex(raw: &[u8]) -> String {
    use sha2::Digest as _;
    let mut h = sha2::Sha256::new();
    h.update(raw);
    format!("sha256:{}", hex::encode(h.finalize()))
}

/// P5 admission gate: study cohorts A/B refuse to run without an admission
/// record at `<evidence_root>/admission.json` whose decision is GO.
/// Development (and any non-study cohort) runs without admission.
fn check_admission(evidence_root: &str, cohort: &str) -> Result<(), OrchestratorError> {
    if cohort != "A" && cohort != "B" {
        return Ok(());
    }
    let path = format!("{evidence_root}/admission.json");
    let text = std::fs::read_to_string(&path).map_err(|_| {
        OrchestratorError::Domain(format!(
            "ALA admission: cohort {cohort} requires {path} with a GO decision; refusing study cohort without admission"
        ))
    })?;
    let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| {
        OrchestratorError::Domain(format!("ALA admission: {path} is invalid JSON: {e}"))
    })?;
    let decision_go = v
        .get("decision")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|d| d.eq_ignore_ascii_case("GO"));
    let admitted_flag = v
        .get("admitted")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    if !(decision_go || admitted_flag) {
        return Err(OrchestratorError::Domain(format!(
            "ALA admission: {path} does not admit cohort {cohort} (need {{\"decision\": \"GO\"}})"
        )));
    }
    Ok(())
}

pub const JOURNAL_DOMAIN: &str = "ala-journal-entry-v1";

/// Append one hash-chained line to the root journal
/// (`<evidence_root>/journal.jsonl`). Lines carry no timestamps so identical
/// runs hash identically; the chain links every dyad attempt in one root.
fn journal_append(
    journal_path: &str,
    mut body: serde_json::Map<String, serde_json::Value>,
) -> Result<String, OrchestratorError> {
    use std::io::Write as _;
    let mut seq = 0u64;
    let mut prev = ald_types::GENESIS_HASH.to_string();
    if std::path::Path::new(journal_path).exists() {
        let text = std::fs::read_to_string(journal_path)
            .map_err(|e| OrchestratorError::Io(format!("journal unreadable: {e}")))?;
        for line in text.lines() {
            let v: serde_json::Value = serde_json::from_str(line).map_err(|e| {
                OrchestratorError::Domain(format!("ALA journal: corrupt line: {e}"))
            })?;
            let entry = v
                .get("entryHash")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| {
                    OrchestratorError::Domain("ALA journal: line without entryHash".to_string())
                })?;
            let s = v
                .get("seq")
                .and_then(serde_json::Value::as_u64)
                .ok_or_else(|| {
                    OrchestratorError::Domain("ALA journal: line without seq".to_string())
                })?;
            prev = entry.to_string();
            seq = s + 1;
        }
    } else if let Some(parent) = std::path::Path::new(journal_path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| OrchestratorError::Io(e.to_string()))?;
        }
    }
    body.insert("seq".to_string(), serde_json::Value::from(seq));
    body.insert("previousHash".to_string(), serde_json::Value::String(prev));
    let canonical = ald_hashing::canonical_json(&serde_json::Value::Object(body.clone()))
        .map_err(|e| OrchestratorError::Domain(e.to_string()))?;
    let hash = ald_hashing::domain_hash(JOURNAL_DOMAIN, &[canonical.as_bytes()]);
    body.insert(
        "entryHash".to_string(),
        serde_json::Value::String(hash.clone()),
    );
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(journal_path)
        .map_err(|e| OrchestratorError::Io(format!("journal unwritable: {e}")))?;
    writeln!(f, "{}", serde_json::Value::Object(body))
        .map_err(|e| OrchestratorError::Io(format!("journal unwritable: {e}")))?;
    Ok(hash)
}

fn valid_distribution(d: &[f64]) -> bool {
    d.len() == 4
        && d.iter().all(|v| v.is_finite() && *v >= 0.0)
        && (d.iter().sum::<f64>() - 1.0).abs() < 1e-9
}

/// Round-trip stabilization for floats that enter hashed records.
///
/// `serde_json`'s default float parser is not correctly rounded, so
/// `parse(format(v))` can differ from `v` by 1 ulp — and evidence entry
/// hashes are recomputed from the stored text. Iterate to the parse/format
/// fixpoint so every hashed float survives a store/load round-trip
/// bit-identically. The adjustment is at most a few ulps (relative ~1e-16),
/// applied once at the source so every artifact agrees exactly.
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BranchOutcome {
    probabilities: Vec<f64>,
    success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CaseProbabilities {
    case_id: String,
    target_index: usize,
    delivered_token: Option<String>,
    native_missing: bool,
    native: Vec<f64>,
    replay: Vec<f64>,
    shared_draw: f64,
    draw_bits_hex: String,
    // BTreeMap: serde streams map entries in iteration order, and HashMap
    // iteration is per-instance random, so a HashMap here would make the
    // persisted bytes (and any hash over them) nondeterministic.
    branches: std::collections::BTreeMap<String, BranchOutcome>,
}

struct DyadInner {
    primary: f64,
    cases_attempted: u64,
    cases_valid: u64,
}

fn journal_slot_identity(
    req: &DyadRequest,
    pad: &str,
    manifest_sha256: &str,
) -> serde_json::Map<String, serde_json::Value> {
    let mut m = serde_json::Map::new();
    m.insert("cohort".to_string(), serde_json::json!(req.cohort));
    m.insert("slotKind".to_string(), serde_json::json!(req.kind));
    m.insert("slotIndex".to_string(), serde_json::json!(pad));
    m.insert(
        "manifestSha256".to_string(),
        serde_json::json!(manifest_sha256),
    );
    m
}

/// Execute one full ALA dyad: validate the execution manifest and P5
/// admission, journal the attempt, generate LV01 cases per role from the
/// manifest workload, train a per-role [`ald_learners::TrainingLedger`] on
/// the generated training cases, score every test case with ledger-native
/// predictions and exact paired-draw replay, evaluate every branch via
/// [`ald_interventions::substitute_branch`], persist per-case branch
/// probabilities per role plus evidence-stream events, journal the terminal
/// line, and return an honest receipt with measured cost.
///
/// The evidence layout (slot dirs, receipt, root journal, admission record)
/// matches the frozen v2 layout so existing post-check tooling keeps
/// working during migration.
pub fn run_dyad(req: &DyadRequest, inventory: &[String]) -> Result<DyadReceipt, OrchestratorError> {
    let started = std::time::Instant::now();
    if req.cohort.is_empty() || req.kind.is_empty() || req.evidence_root.is_empty() {
        return Err(OrchestratorError::Domain(
            "ALA dyad: cohort, kind, and evidenceRoot must be non-empty".to_string(),
        ));
    }
    if req.index > 9999 {
        return Err(OrchestratorError::Domain(
            "ALA dyad: index must fit a zero-padded 4-digit slot".to_string(),
        ));
    }
    let distinct = {
        let mut s = std::collections::HashSet::new();
        inventory.iter().all(|t| s.insert(t))
    };
    if inventory.len() != 32 || !distinct {
        return Err(OrchestratorError::Domain(
            "ALA dyad: LV01 requires 32 distinct inventory tokens".to_string(),
        ));
    }
    let pad = format!("{:04}", req.index);

    // Gate 1: execution manifest (identity + workload).
    let manifest_raw = std::fs::read(&req.manifest_path)
        .map_err(|e| OrchestratorError::Io(format!("manifest unreadable: {e}")))?;
    let manifest_text = String::from_utf8(manifest_raw.clone())
        .map_err(|e| OrchestratorError::Domain(format!("ALA manifest: not UTF-8: {e}")))?;
    let counts = execution_counts_from_manifest(&manifest_text)?;
    let workload = prototype_workload(&counts)?;
    let manifest_sha256 = manifest_sha256_hex(&manifest_raw);

    // Gate 2: P5 admission for study cohorts.
    check_admission(&req.evidence_root, &req.cohort)?;

    let slot_dir = format!("{}/{}/{}-{}", req.evidence_root, req.cohort, req.kind, pad);
    std::fs::create_dir_all(&slot_dir).map_err(|e| OrchestratorError::Io(e.to_string()))?;
    let db_path = format!("{slot_dir}/evidence.sqlite");
    let conn = ald_evidence::open(&db_path)?;
    let journal_path = format!("{}/journal.jsonl", req.evidence_root);

    // Journal the attempt before doing work.
    let mut attempted = journal_slot_identity(req, &pad, &manifest_sha256);
    attempted.insert("kind".to_string(), serde_json::json!("attempted"));
    attempted.insert(
        "protocolVersion".to_string(),
        serde_json::json!(req.protocol_version),
    );
    attempted.insert(
        "treatmentBatchVersion".to_string(),
        serde_json::json!(req.treatment_batch_version),
    );
    let attempted_hash = journal_append(&journal_path, attempted)?;

    let inner = match run_dyad_inner(
        req,
        inventory,
        &counts,
        &workload,
        &manifest_sha256,
        &slot_dir,
        &conn,
        &pad,
    ) {
        Ok(ok) => ok,
        Err(e) => {
            let mut terminal = journal_slot_identity(req, &pad, &manifest_sha256);
            terminal.insert("kind".to_string(), serde_json::json!("terminal"));
            terminal.insert("state".to_string(), serde_json::json!("invalid"));
            terminal.insert("error".to_string(), serde_json::json!(e.to_string()));
            let _ = journal_append(&journal_path, terminal);
            return Err(e);
        }
    };

    let mut terminal = journal_slot_identity(req, &pad, &manifest_sha256);
    terminal.insert("kind".to_string(), serde_json::json!("terminal"));
    terminal.insert("state".to_string(), serde_json::json!("valid"));
    terminal.insert(
        "casesAttempted".to_string(),
        serde_json::json!(inner.cases_attempted),
    );
    terminal.insert(
        "casesValid".to_string(),
        serde_json::json!(inner.cases_valid),
    );
    terminal.insert("primary".to_string(), serde_json::json!(inner.primary));
    let terminal_hash = journal_append(&journal_path, terminal)?;

    let wall_seconds = started.elapsed().as_secs_f64();
    let receipt = DyadReceipt {
        cohort: req.cohort.clone(),
        kind: req.kind.clone(),
        index: req.index,
        state: "valid".to_string(),
        cases_attempted: inner.cases_attempted,
        cases_valid: inner.cases_valid,
        manifest_sha256,
        protocol_version: req.protocol_version,
        treatment_batch_version: req.treatment_batch_version,
        primary: inner.primary,
        wall_seconds,
        attempted_hash,
        terminal_hash,
    };
    let body =
        serde_json::to_value(&receipt).map_err(|e| OrchestratorError::Domain(e.to_string()))?;
    std::fs::write(
        format!("{slot_dir}/dyad-receipt.json"),
        serde_json::to_string_pretty(&body).unwrap(),
    )
    .map_err(|e| OrchestratorError::Io(e.to_string()))?;

    Ok(receipt)
}

#[allow(clippy::too_many_arguments)]
fn run_dyad_inner(
    req: &DyadRequest,
    inventory: &[String],
    counts: &StudyCounts,
    workload: &CollectionWorkload,
    manifest_sha256: &str,
    slot_dir: &str,
    conn: &rusqlite::Connection,
    pad: &str,
) -> Result<DyadInner, OrchestratorError> {
    use ald_analysis::analyze_dyad;
    use ald_analysis::dyad::{RoleFolds, TestRow};
    use ald_analysis::ledger::OrdinaryRecord;
    use ald_learners::TrainingLedger;
    use ald_scenario::{generate_cases, plan_paired_case};

    let seed_root = if req.protocol_version >= 2 {
        ALA_SEED_ROOT_V2
    } else {
        ALA_SEED_ROOT
    };
    // Manifest totals are per dyad; each role generates half. `training_turns`
    // is the dyad total, so halve the study count the same way the workload
    // halves the validation partitions.
    let per_role_train = (counts.training_cases / 2) as usize;
    let per_role_fit = workload.validation_fit_cases as usize;
    let per_role_sel = workload.validation_selection_cases as usize;
    let per_role_test = workload.within_support_test_cases as usize;

    let mut audit_attempted = journal_slot_identity(req, pad, manifest_sha256);
    audit_attempted.insert("kind".to_string(), serde_json::json!("attempted"));
    ald_evidence::append_event(conn, ald_types::EventStream::Audit, audit_attempted)?;

    let mut roles = Vec::new();
    let mut cases_attempted = 0u64;
    let mut cases_valid = 0u64;
    for role in ["baby-a", "baby-b"] {
        let mk = |part: &str| AlaSeedInput {
            cohort: req.cohort.clone(),
            slot_kind: req.kind.clone(),
            slot_index: pad.to_string(),
            purpose: "scenario".to_string(),
            role: format!("{role}-receiver"),
            partition: Some(part.to_string()),
            case_id: None,
            branch: None,
            seed_root: Some(seed_root.to_string()),
        };
        let train_seed = derive_ala_seed(&mk("training"))?;
        let fit_seed = derive_ala_seed(&mk("validation-fit"))?;
        let sel_seed = derive_ala_seed(&mk("validation-selection"))?;
        let test_seed = derive_ala_seed(&mk("within-support-test"))?;

        let to_records = |seed: &str, n: usize, off: u64| -> Vec<OrdinaryRecord> {
            generate_cases(seed, off, role, n, inventory)
                .into_iter()
                .map(|c| OrdinaryRecord {
                    case_id: c.case_id,
                    receiver_role: c.receiver_role,
                    delivered_token: c.delivered_token,
                    candidate_type_codes: c.candidate_type_codes.to_vec(),
                    actual_selected_candidate_index: c.target_index,
                })
                .collect()
        };
        let training = to_records(&train_seed, per_role_train, 0);
        let validation_fit = to_records(&fit_seed, per_role_fit, 1);
        let validation_selection = to_records(&sel_seed, per_role_sel, 2);
        let test_cases = generate_cases(&test_seed, 3, role, per_role_test, inventory);

        // Native predictor: train the ledger on this role's generated
        // training cases, then predict each test case from its token.
        let mut ledger = TrainingLedger::new();
        for r in &training {
            ledger.index(
                r.delivered_token.as_deref(),
                r.actual_selected_candidate_index,
            );
        }

        let mut test = Vec::with_capacity(test_cases.len());
        let mut probs = Vec::with_capacity(test_cases.len());
        for c in &test_cases {
            let plan = plan_paired_case(&test_seed, &c.case_id);
            let native: Vec<f64> = ledger
                .predict_transcript(c.delivered_token.as_deref())
                .iter()
                .map(|v| stable_f64(*v))
                .collect();
            let native_missing = c.delivered_token.is_none();
            // One stabilized draw drives replay, branch shuffling, and every
            // persisted copy; the bit-hex is recomputed from it.
            let shared_draw = stable_f64(plan.shared_draw);
            let draw_bits_hex = hex::encode(shared_draw.to_bits().to_be_bytes());
            // Replay: exact one-hot on the paired draw's action.
            let draw_idx = ((shared_draw * 4.0).floor() as usize).min(3);
            let mut replay = vec![0.0; 4];
            replay[draw_idx] = 1.0;
            if !valid_distribution(&native) || !valid_distribution(&replay) {
                return Err(OrchestratorError::Domain(format!(
                    "ALA dyad: invalid native/replay distributions for {}",
                    c.case_id
                )));
            }
            let mut branch_rng =
                ald_hashing::SeededPrng::new(&test_seed).derive(&format!("branch/{}", c.case_id));
            let mut branch_distributions = std::collections::BTreeMap::new();
            let mut branch_success = std::collections::BTreeMap::new();
            let mut outcomes = std::collections::BTreeMap::new();
            for b in ald_scenario::LV01_BRANCHES {
                let d =
                    ald_interventions::substitute_branch(b, &native, shared_draw, &mut branch_rng)
                        .map_err(|e| OrchestratorError::Domain(e.to_string()))?;
                if !valid_distribution(&d) {
                    return Err(OrchestratorError::Domain(format!(
                        "ALA dyad: invalid {b} distribution for {}",
                        c.case_id
                    )));
                }
                let ok = d
                    .iter()
                    .enumerate()
                    .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
                    .map(|(i, _)| i)
                    == Some(c.target_index);
                branch_distributions.insert(b.to_string(), d.clone());
                branch_success.insert(b.to_string(), ok);
                outcomes.insert(
                    b.to_string(),
                    BranchOutcome {
                        probabilities: d,
                        success: ok,
                    },
                );
            }
            cases_attempted += 1;
            cases_valid += 1;
            probs.push(CaseProbabilities {
                case_id: c.case_id.clone(),
                target_index: c.target_index,
                delivered_token: c.delivered_token.clone(),
                native_missing,
                native: native.clone(),
                replay: replay.clone(),
                shared_draw,
                draw_bits_hex: draw_bits_hex.clone(),
                branches: outcomes,
            });
            let mut ev = serde_json::Map::new();
            ev.insert("caseId".to_string(), serde_json::json!(c.case_id));
            ev.insert("role".to_string(), serde_json::json!(role));
            ev.insert("targetIndex".to_string(), serde_json::json!(c.target_index));
            ev.insert(
                "deliveredToken".to_string(),
                c.delivered_token
                    .clone()
                    .map(serde_json::Value::String)
                    .unwrap_or(serde_json::Value::Null),
            );
            ev.insert(
                "nativeMissing".to_string(),
                serde_json::json!(native_missing),
            );
            ev.insert("native".to_string(), serde_json::json!(native));
            ev.insert("replay".to_string(), serde_json::json!(replay));
            ev.insert("sharedDraw".to_string(), serde_json::json!(shared_draw));
            ev.insert("drawBitsHex".to_string(), serde_json::json!(draw_bits_hex));
            ev.insert(
                "branchSuccess".to_string(),
                serde_json::json!(branch_success),
            );
            ald_evidence::append_event(conn, ald_types::EventStream::Turns, ev)?;
            test.push(TestRow {
                record: OrdinaryRecord {
                    case_id: c.case_id.clone(),
                    receiver_role: c.receiver_role.clone(),
                    delivered_token: c.delivered_token.clone(),
                    candidate_type_codes: c.candidate_type_codes.to_vec(),
                    actual_selected_candidate_index: c.target_index,
                },
                native_distribution: native,
                replay_distribution: replay,
                target_index: c.target_index,
                native_missing,
                branch_distributions: branch_distributions.into_iter().collect(),
                branch_success: branch_success.into_iter().collect(),
            });
        }
        std::fs::write(
            format!("{slot_dir}/branch-probabilities-{role}.json"),
            serde_json::to_string_pretty(&probs).unwrap(),
        )
        .map_err(|e| OrchestratorError::Io(e.to_string()))?;
        roles.push(RoleFolds {
            role: role.to_string(),
            training,
            validation_fit,
            validation_selection,
            test,
        });
    }

    let mut result = analyze_dyad(&format!("{}-{:04}", req.kind, req.index), &roles, inventory)?;
    if !result.primary.is_finite() {
        return Err(OrchestratorError::Domain(
            "ALA dyad: non-finite dyad primary".to_string(),
        ));
    }
    // One stabilized primary for every artifact (receipt, journal, audit,
    // primary.json, dyad-result.json) so hashed and plain copies agree.
    result.primary = stable_f64(result.primary);
    std::fs::write(
        format!("{slot_dir}/primary.json"),
        result.primary.to_string(),
    )
    .map_err(|e| OrchestratorError::Io(e.to_string()))?;
    std::fs::write(
        format!("{slot_dir}/dyad-result.json"),
        serde_json::to_string_pretty(&result).unwrap(),
    )
    .map_err(|e| OrchestratorError::Io(e.to_string()))?;

    let mut audit_terminal = journal_slot_identity(req, pad, manifest_sha256);
    audit_terminal.insert("kind".to_string(), serde_json::json!("terminal"));
    audit_terminal.insert("state".to_string(), serde_json::json!("valid"));
    audit_terminal.insert(
        "casesAttempted".to_string(),
        serde_json::json!(cases_attempted),
    );
    audit_terminal.insert("casesValid".to_string(), serde_json::json!(cases_valid));
    audit_terminal.insert("primary".to_string(), serde_json::json!(result.primary));
    ald_evidence::append_event(conn, ald_types::EventStream::Audit, audit_terminal)?;

    Ok(DyadInner {
        primary: result.primary,
        cases_attempted,
        cases_valid,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_layout_matches_ts() {
        let s = derive_ala_seed(&AlaSeedInput {
            cohort: "A".to_string(),
            slot_kind: "primary".to_string(),
            slot_index: "0300".to_string(),
            purpose: "scenario".to_string(),
            role: "baby-a-receiver".to_string(),
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

    // -- run_dyad integration coverage -------------------------------------

    static TEST_CTR: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    fn test_inventory() -> Vec<String> {
        (0..32).map(|i| format!("tok{i:02}")).collect()
    }

    fn test_manifest_text(training: u64, fit: u64, sel: u64, test: u64) -> String {
        format!(
            "{{\"manifestVersion\":\"ala-lv-execution-manifest/v1\",\"studyId\":\"ALA-LV\",\
             \"workload\":{{\"trainingCases\":{training},\"validationFitCases\":{fit},\
             \"validationSelectionCases\":{sel},\"withinSupportTestCases\":{test}}}}}"
        )
    }

    fn fresh_temp_dir(tag: &str) -> std::path::PathBuf {
        let n = TEST_CTR.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("ald-orch-{}-{tag}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn journal_hashes(journal_path: &std::path::Path) -> Vec<(String, String, String)> {
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

    #[test]
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
        };
        let receipt = run_dyad(&req, &inventory).unwrap();

        // Honest receipt: measured counts, real manifest hash, measured cost.
        use sha2::Digest as _;
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
            "branch-probabilities-baby-a.json",
            "branch-probabilities-baby-b.json",
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
        for role in ["baby-a", "baby-b"] {
            let text =
                std::fs::read_to_string(slot.join(format!("branch-probabilities-{role}.json")))
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

        // Native distributions come from a TrainingLedger over the generated
        // training cases: rebuild the ledger independently and compare.
        for role in ["baby-a", "baby-b"] {
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
            let train_seed = derive_ala_seed(&mk("training")).unwrap();
            let test_seed = derive_ala_seed(&mk("within-support-test")).unwrap();
            let mut ledger = ald_learners::TrainingLedger::new();
            for c in ald_scenario::generate_cases(&train_seed, 0, role, 1500, &inventory) {
                ledger.index(c.delivered_token.as_deref(), c.target_index);
            }
            let text =
                std::fs::read_to_string(slot.join(format!("branch-probabilities-{role}.json")))
                    .unwrap();
            let cases: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
            let first_test = &ald_scenario::generate_cases(&test_seed, 3, role, 120, &inventory)[0];
            let expect = ledger.predict_transcript(first_test.delivered_token.as_deref());
            let got: Vec<f64> = serde_json::from_value(cases[0]["native"].clone()).unwrap();
            for (a, b) in expect.iter().zip(got.iter()) {
                assert!((a - b).abs() < 1e-12, "native is not the ledger prediction");
            }
            // At least one missing-token case is honestly flagged.
            assert!(cases
                .iter()
                .any(|c| c["nativeMissing"] == serde_json::json!(true)));
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
            };
            let receipt = run_dyad(&req, &test_inventory()).unwrap();
            journals.push(journal_hashes(&evidence_root.join("journal.jsonl")));
            primaries.push(receipt.primary);
            branch_bytes.push(
                std::fs::read(
                    evidence_root.join("development/primary-0007/branch-probabilities-baby-a.json"),
                )
                .unwrap(),
            );
            let _ = std::fs::remove_dir_all(&dir);
        }
        assert_eq!(journals[0], journals[1]);
        assert_eq!(journals[0].len(), 2);
        assert!((primaries[0] - primaries[1]).abs() < 1e-15);
        assert_eq!(branch_bytes[0], branch_bytes[1]);
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
}
