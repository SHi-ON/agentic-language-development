//! ALA dyad orchestration: seeds, workload, manifest, and the dyad runner.
//!
//! Seeds, workload, manifest, and runner, plus the LV01 experiment
//! wiring used by the ALA path. The runner executes one full dyad
//! deterministically from the frozen
//! manifest and writes the evidence database, journal lines, and receipt.
#![allow(clippy::cast_precision_loss)] // small counts/indices; exact for realistic inputs
#![allow(clippy::cast_possible_truncation)] // small counts/indices; bounded by construction
#![allow(clippy::cast_sign_loss)] // non-negative by construction (validated ranges, draws in [0, 1))

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
    #[error("learner: {0}")]
    Learner(#[from] ald_learners::LearnerError),
}

/// Pretty JSON for persisted artifacts. Serialization of these shapes is
/// infallible in practice, but the error is typed rather than unwrapped so
/// no artifact write can panic.
fn to_pretty<T: serde::Serialize>(v: &T) -> Result<String, OrchestratorError> {
    serde_json::to_string_pretty(v).map_err(|e| OrchestratorError::Domain(e.to_string()))
}

// ---------------------------------------------------------------------------
// Seeds
// ---------------------------------------------------------------------------

pub const ALA_SEED_ROOT: &str = "ala-ledger-value-v1";
pub const ALA_SEED_ROOT_V2: &str = "ala-ledger-value-v2";
pub const ALA_STUDY_ID: &str = "ALA-LV";

pub const SEED_PURPOSES: [&str; 8] = [
    "scenario",
    "learner/agent-0",
    "learner/agent-1",
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
///
/// # Errors
///
/// Returns an error when a required field is empty or NUL-bearing, the slot index is not zero-padded, or the seed root is unknown.
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
// Workload
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudyCounts {
    pub training_cases: u64,
    pub validation_fit_cases: u64,
    pub validation_selection_cases: u64,
    pub within_support_test_cases: u64,
}
///
/// # Errors
///
/// Returns an error for invalid JSON, or missing/non-positive partition counts.
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
///
/// # Errors
///
/// Returns an error when a total is odd (roles split evenly).
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
// Development options (cohort-gated ablations)
// ---------------------------------------------------------------------------

/// Cohort allowed to run with non-default development options.
pub const DEVELOPMENT_COHORT: &str = "development";
/// Domain separating the development-config binding hash.
pub const DEVELOPMENT_CONFIG_DOMAIN: &str = "ala-development-config-v1";
/// Default channel-drop probability: the legacy 1/8 channel.
pub const DEFAULT_DROP_NUMERATOR: u64 = 1;
/// Default channel-drop denominator (see [`DEFAULT_DROP_NUMERATOR`]).
pub const DEFAULT_DROP_DENOMINATOR: u64 = 8;
/// Default sender width: the full 32-token inventory.
pub const DEFAULT_ACTIVE_SENDER_TOKENS: u32 = 32;
/// Permitted `--active-sender-tokens` values.
pub const ACTIVE_SENDER_TOKEN_CHOICES: [u32; 3] = [8, 16, 32];

/// Who learns during joint-training episodes. `Joint` is the legacy path:
/// sender and receiver both REINFORCE. `None` freezes every logit, baseline,
/// and count while consuming the identical sampling draws. `ReceiverOnly`
/// freezes the sender (exactly uniform) while the receiver learns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LearningMode {
    Joint,
    None,
    ReceiverOnly,
}

/// Exact rational channel-drop probability in `[0, 1]`, written `NUM/DEN`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DropProbability {
    pub numerator: u64,
    pub denominator: u64,
}

/// Requested development options: every field is optional and `None` means
/// the frozen default. The CLI builds this from the `--training-...` /
/// `--learning-mode` / `--drop-probability` / `--active-sender-tokens` /
/// `--held-out-types` flags; [`resolve_development_options`] validates and
/// fills defaults against the manifest.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevelopmentOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub training_interactions_per_direction: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub learning_mode: Option<LearningMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drop_probability: Option<DropProbability>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_sender_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub held_out_types: Option<Vec<u32>>,
}

/// Fully validated development configuration with defaults resolved.
/// `training_interactions_total` is exactly `2 * per_direction` (one
/// episode per direction per step). `held_out_types` is sorted ascending.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedDevelopment {
    pub training_interactions_per_direction: u64,
    pub training_interactions_total: u64,
    pub learning_mode: LearningMode,
    pub drop_probability: DropProbability,
    pub active_sender_tokens: u32,
    pub held_out_types: Vec<u32>,
}

/// Parse `--training-interactions-per-direction N`: a positive integer.
/// Rejects zero, non-numeric input, and anything else without fallback.
///
/// # Errors
///
/// Returns an error for zero, non-numeric, or otherwise malformed input.
pub fn parse_training_interactions(s: &str) -> Result<u64, OrchestratorError> {
    let n: u64 = s.parse().map_err(|_| {
        OrchestratorError::Domain(format!(
            "development options: --training-interactions-per-direction must be a positive integer, got {s:?}"
        ))
    })?;
    if n < 1 {
        return Err(OrchestratorError::Domain(
            "development options: --training-interactions-per-direction must be >= 1".to_string(),
        ));
    }
    Ok(n)
}

/// Parse `--learning-mode joint|none|receiver-only` exactly.
///
/// # Errors
///
/// Returns an error for anything but `joint|none|receiver-only`.
pub fn parse_learning_mode(s: &str) -> Result<LearningMode, OrchestratorError> {
    match s {
        "joint" => Ok(LearningMode::Joint),
        "none" => Ok(LearningMode::None),
        "receiver-only" => Ok(LearningMode::ReceiverOnly),
        _ => Err(OrchestratorError::Domain(format!(
            "development options: --learning-mode must be joint|none|receiver-only, got {s:?}"
        ))),
    }
}

/// Parse `--drop-probability NUM/DEN`: an exact rational in `[0, 1]`.
/// Both parts must be bare non-negative integers with `DEN >= 1` and
/// `NUM <= DEN`; malformed or out-of-range input is rejected outright.
///
/// # Errors
///
/// Returns an error for malformed or out-of-range rationals.
pub fn parse_drop_probability(s: &str) -> Result<DropProbability, OrchestratorError> {
    let bad = || {
        OrchestratorError::Domain(format!(
            "development options: --drop-probability must be NUM/DEN in [0,1], got {s:?}"
        ))
    };
    let (num_s, den_s) = s.split_once('/').ok_or_else(bad)?;
    if num_s.is_empty() || den_s.is_empty() {
        return Err(bad());
    }
    let num: u64 = num_s.parse().map_err(|_| bad())?;
    let den: u64 = den_s.parse().map_err(|_| bad())?;
    if den < 1 || num > den {
        return Err(bad());
    }
    Ok(DropProbability {
        numerator: num,
        denominator: den,
    })
}

/// Parse `--active-sender-tokens K`: exactly 8, 16, or 32.
///
/// # Errors
///
/// Returns an error for anything but 8, 16, or 32.
pub fn parse_active_sender_tokens(s: &str) -> Result<u32, OrchestratorError> {
    let k: u32 = s.parse().map_err(|_| {
        OrchestratorError::Domain(format!(
            "development options: --active-sender-tokens must be one of 8, 16, 32, got {s:?}"
        ))
    })?;
    if !ACTIVE_SENDER_TOKEN_CHOICES.contains(&k) {
        return Err(OrchestratorError::Domain(format!(
            "development options: --active-sender-tokens must be one of 8, 16, 32, got {k}"
        )));
    }
    Ok(k)
}

/// Parse `--held-out-types`, e.g. `12,13,14,15`: distinct type codes below
/// 16 leaving at least 4 permitted types. The empty string selects the empty
/// set. Duplicates, out-of-range codes, and empty entries are rejected.
///
/// # Errors
///
/// Returns an error for duplicates, out-of-range codes, empty entries, or fewer than 4 remaining types.
pub fn parse_held_out_types(s: &str) -> Result<Vec<u32>, OrchestratorError> {
    let bad = |why: &str| {
        OrchestratorError::Domain(format!(
            "development options: --held-out-types {why}, got {s:?}"
        ))
    };
    if s.is_empty() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in s.split(',') {
        let trimmed = entry.trim();
        if trimmed.is_empty() {
            return Err(bad("must not contain empty entries"));
        }
        let t: u32 = trimmed
            .parse()
            .map_err(|_| bad("must be comma-separated type codes below 16"))?;
        if t >= ald_types::signaling::TYPES as u32 {
            return Err(bad("must be comma-separated type codes below 16"));
        }
        if out.contains(&t) {
            return Err(bad("must not repeat a type code"));
        }
        out.push(t);
    }
    if out.len() > ald_types::signaling::TYPES - ald_types::signaling::CANDIDATES {
        return Err(bad("must leave at least 4 permitted types"));
    }
    Ok(out)
}

/// Validate requested options and fill defaults. `manifest_per_role_training`
/// is the manifest's per-role training count (the default interactions per
/// direction). Every malformed or out-of-range value is an error; nothing
/// falls back silently.
///
/// # Errors
///
/// Returns an error when a requested value is malformed or out of range.
pub fn resolve_development_options(
    requested: &DevelopmentOptions,
    manifest_per_role_training: u64,
) -> Result<ResolvedDevelopment, OrchestratorError> {
    let per_direction = requested
        .training_interactions_per_direction
        .unwrap_or(manifest_per_role_training);
    if per_direction < 1 {
        return Err(OrchestratorError::Domain(
            "development options: training interactions per direction must be >= 1".to_string(),
        ));
    }
    let total = per_direction.checked_mul(2).ok_or_else(|| {
        OrchestratorError::Domain(
            "development options: training interactions per direction is out of range".to_string(),
        )
    })?;
    if usize::try_from(per_direction).is_err() {
        return Err(OrchestratorError::Domain(
            "development options: training interactions per direction is out of range".to_string(),
        ));
    }
    let drop = requested.drop_probability.unwrap_or(DropProbability {
        numerator: DEFAULT_DROP_NUMERATOR,
        denominator: DEFAULT_DROP_DENOMINATOR,
    });
    if drop.denominator < 1 || drop.numerator > drop.denominator {
        return Err(OrchestratorError::Domain(
            "development options: drop probability must be NUM/DEN in [0,1]".to_string(),
        ));
    }
    let k = requested
        .active_sender_tokens
        .unwrap_or(DEFAULT_ACTIVE_SENDER_TOKENS);
    if !ACTIVE_SENDER_TOKEN_CHOICES.contains(&k) {
        return Err(OrchestratorError::Domain(
            "development options: active sender tokens must be one of 8, 16, 32".to_string(),
        ));
    }
    let mut held_out = requested.held_out_types.clone().unwrap_or_default();
    {
        let mut seen = std::collections::HashSet::new();
        for t in &held_out {
            if *t >= ald_types::signaling::TYPES as u32 || !seen.insert(*t) {
                return Err(OrchestratorError::Domain(
                    "development options: held-out types must be distinct codes below 16"
                        .to_string(),
                ));
            }
        }
        if held_out.len() > ald_types::signaling::TYPES - ald_types::signaling::CANDIDATES {
            return Err(OrchestratorError::Domain(
                "development options: held-out types must leave at least 4 permitted types"
                    .to_string(),
            ));
        }
    }
    held_out.sort_unstable();
    Ok(ResolvedDevelopment {
        training_interactions_per_direction: per_direction,
        training_interactions_total: total,
        learning_mode: requested.learning_mode.unwrap_or(LearningMode::Joint),
        drop_probability: drop,
        active_sender_tokens: k,
        held_out_types: held_out,
    })
}

/// True when a resolved configuration equals the frozen defaults (so the run
/// takes the legacy path byte-identically: no sidecar, no slot suffix, no
/// extra receipt/journal fields).
#[must_use]
pub fn resolved_is_default(
    resolved: &ResolvedDevelopment,
    manifest_per_role_training: u64,
) -> bool {
    resolved.training_interactions_per_direction == manifest_per_role_training
        && resolved.learning_mode == LearningMode::Joint
        && resolved.drop_probability.numerator == DEFAULT_DROP_NUMERATOR
        && resolved.drop_probability.denominator == DEFAULT_DROP_DENOMINATOR
        && resolved.active_sender_tokens == DEFAULT_ACTIVE_SENDER_TOKENS
        && resolved.held_out_types.is_empty()
}

/// Binding hash over a resolved development configuration (canonical JSON
/// under [`DEVELOPMENT_CONFIG_DOMAIN`]). Non-default runs carry this hash in
/// the receipt, the journal lines, and the `development-config.json` sidecar.
///
/// # Errors
///
/// Returns an error when the config cannot be serialized or canonicalized.
pub fn development_config_hash(
    resolved: &ResolvedDevelopment,
) -> Result<String, OrchestratorError> {
    let v = serde_json::to_value(resolved).map_err(|e| OrchestratorError::Domain(e.to_string()))?;
    ald_hashing::hash_canonical(DEVELOPMENT_CONFIG_DOMAIN, &v)
        .map_err(|e| OrchestratorError::Domain(e.to_string()))
}

/// Short config id (8 hex chars) suffixing non-default run roots.
#[must_use]
pub fn development_config_short_id(hash: &str) -> String {
    hash.chars().skip(7).take(8).collect()
}

/// Exact rational channel-drop decision for one unit draw `u`: drop iff
/// `u < numerator/denominator`, computed in integer arithmetic so thirds and
/// other non-dyadic probabilities are exact. `u` is a 53-bit `next_float`
/// draw, i.e. exactly `m / 2^53` for an integer `m`, so scaling by `2^53` is
/// exact and the comparison `m*den < num*2^53` is bit-exact. At `(1, 8)` this
/// agrees with the legacy `u < 1.0 / 8.0` comparison for every `u`.
#[must_use]
pub fn drop_draw_less_than(u: f64, numerator: u64, denominator: u64) -> bool {
    const SCALE: f64 = 9_007_199_254_740_992.0; // 2^53
    const SCALE_U128: u128 = 9_007_199_254_740_992;
    let m = (u * SCALE) as u64;
    u128::from(m) * u128::from(denominator) < u128::from(numerator) * SCALE_U128
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub development: Option<DevelopmentOptions>,
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
    /// Development-config binding hash ([`development_config_hash`]) for
    /// non-default runs. Absent on legacy runs, so default receipt bytes are
    /// unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub development_config_sha256: Option<String>,
}

/// Parse workload counts from an ALA execution manifest.
///
/// `manifest_path` points at the execution manifest (which pins the study
/// protocol by hash); the frozen per-partition case totals live in its
/// `workload` block. Refuses anything that is not valid JSON, does not
/// carry the frozen manifest version / study id, or lacks positive counts.
///
/// # Errors
///
/// Returns an error for invalid JSON, a wrong manifest version/study id, or non-positive counts.
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

/// Chain fields assigned by the journal itself; callers must not supply
/// them (a caller-supplied `seq`/`previousHash`/`entryHash` would fork
/// verification).
const JOURNAL_CHAIN_FIELDS: [&str; 3] = ["seq", "previousHash", "entryHash"];

/// Verify every line of an existing journal and return the next `seq` plus
/// the head `entryHash` to link. A missing journal is the empty chain
/// (`(0, GENESIS_HASH)`). Any violation — non-JSON line, missing chain
/// field, `seq` gap/replay, broken `previousHash` link, or `entryHash`
/// mismatch — fails BEFORE any write, so a corrupt chain is never extended.
///
/// Note: `kind`/`state` values are NOT restricted here. `run_dyad` writes
/// `attempted`/`terminal` entries; operator tooling (e.g. the watchdog)
/// appends `aborted`/`note` states through the same chain.
///
/// # Errors
///
/// Returns an error naming the first corrupt line (bad JSON, missing chain field, `seq` gap/replay, broken link, or `entryHash` mismatch), or when the file is unreadable.
pub fn verify_journal_chain(journal_path: &str) -> Result<(u64, String), OrchestratorError> {
    let mut expected_seq = 0u64;
    let mut prev = ald_types::GENESIS_HASH.to_string();
    if !std::path::Path::new(journal_path).exists() {
        return Ok((expected_seq, prev));
    }
    let text = std::fs::read_to_string(journal_path)
        .map_err(|e| OrchestratorError::Io(format!("journal unreadable: {e}")))?;
    for (idx, line) in text.lines().enumerate() {
        let lineno = idx + 1;
        let v: serde_json::Value = serde_json::from_str(line).map_err(|e| {
            OrchestratorError::Domain(format!("ALA journal line {lineno}: corrupt entry: {e}"))
        })?;
        let obj = v.as_object().ok_or_else(|| {
            OrchestratorError::Domain(format!("ALA journal line {lineno}: entry is not an object"))
        })?;
        let seq = obj
            .get("seq")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| {
                OrchestratorError::Domain(format!("ALA journal line {lineno}: entry without seq"))
            })?;
        if seq != expected_seq {
            return Err(OrchestratorError::Domain(format!(
                "ALA journal line {lineno}: seq {seq} breaks the chain (expected {expected_seq})"
            )));
        }
        let previous = obj
            .get("previousHash")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                OrchestratorError::Domain(format!(
                    "ALA journal line {lineno}: entry without previousHash"
                ))
            })?;
        if previous != prev {
            return Err(OrchestratorError::Domain(format!(
                "ALA journal line {lineno}: previousHash does not link the prior entryHash"
            )));
        }
        let entry = obj
            .get("entryHash")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                OrchestratorError::Domain(format!(
                    "ALA journal line {lineno}: entry without entryHash"
                ))
            })?;
        let mut body = obj.clone();
        body.remove("entryHash");
        let canonical = ald_hashing::canonical_json(&serde_json::Value::Object(body))
            .map_err(|e| OrchestratorError::Domain(format!("ALA journal line {lineno}: {e}")))?;
        let recomputed = ald_hashing::domain_hash(JOURNAL_DOMAIN, &[canonical.as_bytes()]);
        if recomputed != entry {
            return Err(OrchestratorError::Domain(format!(
                "ALA journal line {lineno}: entryHash mismatch (tampered entry)"
            )));
        }
        prev = entry.to_string();
        expected_seq += 1;
    }
    Ok((expected_seq, prev))
}

/// Append one caller-supplied body to a journal through the same verified
/// chain [`journal_append`] uses. Chain fields (`seq`, `previousHash`,
/// `entryHash`) are assigned here; a body that already carries them is
/// rejected so callers cannot fork verification.
///
/// # Errors
///
/// Returns an error when the body carries chain fields, the existing chain is corrupt, or the file is unwritable.
pub fn journal_append_entry(
    journal_path: &str,
    body: serde_json::Map<String, serde_json::Value>,
) -> Result<String, OrchestratorError> {
    for f in JOURNAL_CHAIN_FIELDS {
        if body.contains_key(f) {
            return Err(OrchestratorError::Domain(format!(
                "ALA journal: body must not carry chain field {f:?} (assigned on append)"
            )));
        }
    }
    journal_append(journal_path, body)
}

/// Append one hash-chained line to the root journal
/// (`<evidence_root>/journal.jsonl`). Lines carry no timestamps so identical
/// runs hash identically; the chain links every dyad attempt in one root.
fn journal_append(
    journal_path: &str,
    mut body: serde_json::Map<String, serde_json::Value>,
) -> Result<String, OrchestratorError> {
    use std::io::Write as _;
    let (seq, prev) = verify_journal_chain(journal_path)?;
    if !std::path::Path::new(journal_path).exists() {
        if let Some(parent) = std::path::Path::new(journal_path).parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| OrchestratorError::Io(e.to_string()))?;
            }
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

/// Deterministic on-disk mirror of [`ald_analysis::dyad::RoleFolds`].
///
/// Field declaration order matches the committed `role-folds-*.json`
/// fixtures exactly (`role`, `training`, `validation_fit`,
/// `validation_selection`, `test`; per test row `record`,
/// `native_distribution`, `replay_distribution`, `target_index`,
/// `native_missing`, `branch_distributions`, `branch_success`). The branch
/// maps use `BTreeMap` so the persisted bytes are byte-identical across
/// runs; the in-memory `TestRow` maps are `HashMap` and must never be
/// serialized directly.
#[derive(Debug, Clone, Serialize)]
struct PersistTestRow<'a> {
    record: &'a ald_analysis::ledger::OrdinaryRecord,
    native_distribution: &'a Vec<f64>,
    replay_distribution: &'a Vec<f64>,
    target_index: usize,
    native_missing: bool,
    branch_distributions: std::collections::BTreeMap<&'a str, &'a Vec<f64>>,
    branch_success: std::collections::BTreeMap<&'a str, bool>,
}

#[derive(Debug, Clone, Serialize)]
struct PersistRoleFolds<'a> {
    role: &'a str,
    training: &'a [ald_analysis::ledger::OrdinaryRecord],
    validation_fit: &'a [ald_analysis::ledger::OrdinaryRecord],
    validation_selection: &'a [ald_analysis::ledger::OrdinaryRecord],
    test: Vec<PersistTestRow<'a>>,
}

fn persistable_role_folds(folds: &ald_analysis::dyad::RoleFolds) -> PersistRoleFolds<'_> {
    PersistRoleFolds {
        role: &folds.role,
        training: &folds.training,
        validation_fit: &folds.validation_fit,
        validation_selection: &folds.validation_selection,
        test: folds
            .test
            .iter()
            .map(|t| PersistTestRow {
                record: &t.record,
                native_distribution: &t.native_distribution,
                replay_distribution: &t.replay_distribution,
                target_index: t.target_index,
                native_missing: t.native_missing,
                branch_distributions: t
                    .branch_distributions
                    .iter()
                    .map(|(k, v)| (k.as_str(), v))
                    .collect(),
                branch_success: t
                    .branch_success
                    .iter()
                    .map(|(k, v)| (k.as_str(), *v))
                    .collect(),
            })
            .collect(),
    }
}

struct DyadInner {
    primary: f64,
    cases_attempted: u64,
    cases_valid: u64,
}

/// Per-split per-type exposure census for one role: candidate-set appearances
/// of each of the 16 types. The training census covers the `N` episode
/// records per direction the agents actually trained on (a prefix of — or,
/// for `N` above the manifest count, an extension of — the persisted fold);
/// the other three cover the persisted folds.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RoleExposure {
    role: String,
    training: [u64; 16],
    validation_fit: [u64; 16],
    validation_selection: [u64; 16],
    within_support_test: [u64; 16],
}

fn exposure_histogram(codes: &[Vec<u32>]) -> [u64; 16] {
    let mut h = [0u64; 16];
    for row in codes {
        for c in row {
            h[*c as usize] += 1;
        }
    }
    h
}

/// Candidate type codes as agent-policy indices. Callers only pass
/// validated generated cases (4 distinct codes below 16).
fn candidate_types(codes: &[u32]) -> [usize; 4] {
    [
        codes[0] as usize,
        codes[1] as usize,
        codes[2] as usize,
        codes[3] as usize,
    ]
}

/// One role's generated folds plus the seeds they were generated from. The
/// training records carry the target types + candidate codes the emergent
/// training episodes are built from; test tokens are resampled from the
/// frozen opposite agent (as sender) at scoring time. `training` holds
/// `max(N, fold)` records: the persisted fold is its manifest-count prefix
/// and the episodes run over its `N`-prefix.
struct RoleBundle {
    role: String,
    train_seed: String,
    fit_seed: String,
    sel_seed: String,
    test_seed: String,
    training: Vec<ald_analysis::ledger::OrdinaryRecord>,
    validation_fit: Vec<ald_analysis::ledger::OrdinaryRecord>,
    validation_selection: Vec<ald_analysis::ledger::OrdinaryRecord>,
    test_cases: Vec<ald_scenario::Lv01Case>,
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
/// manifest workload, symmetrically co-train two
/// [`ald_learners::AgentPolicy`] agents unsupervised (alternating
/// sender/receiver per step, receiver samples its candidate position and
/// never sees the true type, shared episode reward, 1/8 token-drop
/// channel, all randomness from derived seed streams), freeze the
/// policies at test (tokens from the opposite agent as sender, native
/// predictions from the role agent's receiver position posteriors), score
/// every test case with emergent-native predictions and exact paired-draw
/// replay, evaluate every branch via
/// [`ald_interventions::substitute_branch`], persist per-role branch
/// probabilities and role folds (`role-folds-{agent-0,agent-1}.json`, the
/// exact input schema `ala analyze select|score` reads) plus the frozen
/// full agent policies (`agent-policy-{agent-0,agent-1}.json`), and
/// evidence-stream events, journal the terminal line, and return an
/// honest receipt with measured cost.
///
/// The evidence layout (slot dirs, receipt, root journal, admission record)
/// matches the frozen v2 layout so existing post-check tooling keeps
/// working during migration.
///
/// Development options ride along in `req.development`: they are resolved
/// and gated BEFORE any directory, journal, or database is created, so a
/// rejected run leaves no partial artifacts. Resolved-default runs take the
/// legacy path byte-identically; non-default runs require
/// `--cohort development`, write to a config-suffixed run root, refuse to
/// overwrite an existing root, and bind the config hash into the receipt,
/// the journal lines, and a `development-config.json` sidecar.
///
/// # Errors
///
/// Returns an error for invalid manifests, inventories, or admission, for IO failures, or for non-finite results; rejected runs leave no partial artifacts.
#[allow(clippy::too_many_lines)] // straight-line run pipeline; P0-FORK-11 splits modules, not this function.
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

    // Gate 1b: development options (resolve + cohort gate). Runs before any
    // directory, journal, or database exists, so rejections are artifact-free.
    let manifest_per_role_training = counts.training_cases / 2;
    let requested = req.development.clone().unwrap_or_default();
    let resolved = resolve_development_options(&requested, manifest_per_role_training)?;
    let dev_active = !resolved_is_default(&resolved, manifest_per_role_training);
    let dev_hash: Option<String> = if dev_active {
        Some(development_config_hash(&resolved)?)
    } else {
        None
    };
    if dev_active && req.cohort != DEVELOPMENT_COHORT {
        return Err(OrchestratorError::Domain(format!(
            "development options: non-default options require --cohort {DEVELOPMENT_COHORT}, got {:?}",
            req.cohort
        )));
    }

    // Gate 2: P5 admission for study cohorts.
    check_admission(&req.evidence_root, &req.cohort)?;

    // Non-default configs run in a separate config-suffixed root so different
    // ablations never share (or silently overwrite) one slot directory.
    let slot_dir = match &dev_hash {
        Some(h) => format!(
            "{}/{}/{}-{}-dev-{}",
            req.evidence_root,
            req.cohort,
            req.kind,
            pad,
            development_config_short_id(h)
        ),
        None => format!("{}/{}/{}-{}", req.evidence_root, req.cohort, req.kind, pad),
    };
    if dev_active && std::path::Path::new(&slot_dir).exists() {
        return Err(OrchestratorError::Domain(format!(
            "development options: refusing to overwrite existing run root {slot_dir}"
        )));
    }
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
    if let Some(h) = &dev_hash {
        attempted.insert("developmentConfigSha256".to_string(), serde_json::json!(h));
    }
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
        &requested,
        &resolved,
        dev_hash.as_ref(),
    ) {
        Ok(ok) => ok,
        Err(e) => {
            let mut terminal = journal_slot_identity(req, &pad, &manifest_sha256);
            terminal.insert("kind".to_string(), serde_json::json!("terminal"));
            terminal.insert("state".to_string(), serde_json::json!("invalid"));
            terminal.insert("error".to_string(), serde_json::json!(e.to_string()));
            if let Some(h) = &dev_hash {
                terminal.insert("developmentConfigSha256".to_string(), serde_json::json!(h));
            }
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
    if let Some(h) = &dev_hash {
        terminal.insert("developmentConfigSha256".to_string(), serde_json::json!(h));
    }
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
        development_config_sha256: dev_hash.clone(),
    };
    let body =
        serde_json::to_value(&receipt).map_err(|e| OrchestratorError::Domain(e.to_string()))?;
    std::fs::write(format!("{slot_dir}/dyad-receipt.json"), to_pretty(&body)?)
        .map_err(|e| OrchestratorError::Io(e.to_string()))?;

    Ok(receipt)
}

#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_lines)] // straight-line dyad pipeline; P0-FORK-11 splits modules, not this function.
fn run_dyad_inner(
    req: &DyadRequest,
    inventory: &[String],
    counts: &StudyCounts,
    workload: &CollectionWorkload,
    manifest_sha256: &str,
    slot_dir: &str,
    conn: &rusqlite::Connection,
    pad: &str,
    requested: &DevelopmentOptions,
    resolved: &ResolvedDevelopment,
    dev_hash: Option<&String>,
) -> Result<DyadInner, OrchestratorError> {
    use ald_analysis::analyze_dyad;
    use ald_analysis::dyad::{RoleFolds, TestRow};
    use ald_analysis::ledger::OrdinaryRecord;
    use ald_learners::AgentPolicy;
    use ald_scenario::{generate_cases, generate_cases_excluding, plan_paired_case};

    let dev_active = dev_hash.is_some();
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
    // Episodes per direction: the manifest count on legacy runs, the
    // `--training-interactions-per-direction` override otherwise. Training
    // records are generated as one deterministic `max(N, fold)` stream, so an
    // override below the fold trains on the fold's prefix and an override
    // above it extends the same stream with fresh draws (never duplication).
    let episodes_per_direction = usize::try_from(resolved.training_interactions_per_direction)
        .map_err(|_| {
            OrchestratorError::Domain(
                "development options: training interactions per direction is out of range"
                    .to_string(),
            )
        })?;
    let generated_train = episodes_per_direction.max(per_role_train);
    let channel_dropped = |draw: f64| {
        if dev_active {
            drop_draw_less_than(
                draw,
                resolved.drop_probability.numerator,
                resolved.drop_probability.denominator,
            )
        } else {
            draw < 1.0 / 8.0
        }
    };

    let mut audit_attempted = journal_slot_identity(req, pad, manifest_sha256);
    audit_attempted.insert("kind".to_string(), serde_json::json!("attempted"));
    ald_evidence::append_event(conn, ald_types::EventStream::Audit, audit_attempted)?;

    // Phase 1: generate every role's folds from the manifest workload.
    // Training records carry the target types + candidate codes the
    // emergent episodes are built from. Non-default runs exclude held-out
    // types from every pre-test fold in both directions; test folds always
    // cover all 16 types.
    let mut bundles = Vec::new();
    for role in ["agent-0", "agent-1"] {
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
            let cases = if dev_active {
                generate_cases_excluding(seed, off, role, n, inventory, &resolved.held_out_types)
            } else {
                generate_cases(seed, off, role, n, inventory)
            };
            cases
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
        let training = to_records(&train_seed, generated_train, 0);
        let validation_fit = to_records(&fit_seed, per_role_fit, 1);
        let validation_selection = to_records(&sel_seed, per_role_sel, 2);
        let test_cases = generate_cases(&test_seed, 3, role, per_role_test, inventory);
        bundles.push(RoleBundle {
            role: role.to_string(),
            train_seed,
            fit_seed,
            sel_seed,
            test_seed,
            training,
            validation_fit,
            validation_selection,
            test_cases,
        });
    }

    // Phase 2: symmetric unsupervised co-training — two agents alternate
    // sender/receiver, one episode per direction per step over the
    // receiver role's training folds. Mirrors probe v4 exactly: the
    // sender agent samples a token for the true target type, the 1/8
    // channel may drop it, the receiver agent SAMPLES a candidate
    // position from its posterior (it never sees the true type), both
    // share the episode reward, the sender REINFORCEs its sampled token
    // (even when dropped), and the receiver REINFORCEs toward the chosen
    // candidate's type only (a dropped token is a receiver no-op). Every
    // draw comes from per-role derived seed streams, so training is
    // bit-deterministic per dyad. Development runs may constrain the sender
    // width, replace the channel probability, and freeze one or both sides
    // (`--learning-mode` gates ONLY the update calls: sampling draws are
    // consumed identically in every mode).
    let mut agents = if resolved.active_sender_tokens == DEFAULT_ACTIVE_SENDER_TOKENS {
        [AgentPolicy::new(), AgentPolicy::new()]
    } else {
        [
            AgentPolicy::with_active_sender_tokens(resolved.active_sender_tokens as usize)
                .map_err(|e| OrchestratorError::Domain(e.to_string()))?,
            AgentPolicy::with_active_sender_tokens(resolved.active_sender_tokens as usize)
                .map_err(|e| OrchestratorError::Domain(e.to_string()))?,
        ]
    };
    let mut train_rngs: Vec<ald_hashing::SeededPrng> = bundles
        .iter()
        .map(|b| ald_hashing::SeededPrng::new(&b.train_seed).derive("emergent-train"))
        .collect();
    for step in 0..episodes_per_direction {
        for receiver_idx in 0..bundles.len() {
            let sender_idx = 1 - receiver_idx;
            let rec = &bundles[receiver_idx].training[step];
            let cands = candidate_types(&rec.candidate_type_codes);
            let target_type = cands[rec.actual_selected_candidate_index];
            let rng = &mut train_rngs[receiver_idx];
            let token = agents[sender_idx].sample_token(target_type, rng)?;
            let channel_draw = rng.next_float();
            let delivered = if channel_dropped(channel_draw) {
                None
            } else {
                Some(token)
            };
            let choice = agents[receiver_idx].sample_position(delivered, &cands, rng)?;
            let reward = if choice == rec.actual_selected_candidate_index {
                1.0
            } else {
                0.0
            };
            match resolved.learning_mode {
                LearningMode::Joint => {
                    agents[sender_idx].update_sender(target_type, token, reward)?;
                    agents[receiver_idx].update_receiver(delivered, &cands, choice, reward)?;
                }
                LearningMode::None => {}
                LearningMode::ReceiverOnly => {
                    agents[receiver_idx].update_receiver(delivered, &cands, choice, reward)?;
                }
            }
        }
    }

    // Phase 3: persist the frozen full agent policies (sender + receiver
    // tables) as dyad artifacts. The serializer is deterministic (fixed
    // field order, stabilized floats), so the bytes are byte-identical
    // across runs.
    for (bundle, agent) in bundles.iter().zip(agents.iter()) {
        std::fs::write(
            format!("{}/agent-policy-{}.json", slot_dir, bundle.role),
            agent.to_bytes(),
        )
        .map_err(|e| OrchestratorError::Io(e.to_string()))?;
    }

    // Phase 4: frozen-policy scoring per role. Test tokens come from the
    // OPPOSITE agent as sender through the same 1/8 channel; native
    // predictions are the role agent's frozen receiver position
    // posteriors (argmax scoring downstream). Dropped tokens predict
    // exact uniform. Branch probabilities, role folds, receipt, and
    // journal keep their exact shapes.
    let mut roles = Vec::new();
    let mut cases_attempted = 0u64;
    let mut cases_valid = 0u64;
    for (receiver_idx, bundle) in bundles.iter().enumerate() {
        let receiver = &agents[receiver_idx];
        let sender = &agents[1 - receiver_idx];
        let role = bundle.role.as_str();
        let test_seed = bundle.test_seed.as_str();
        let mut test_rng = ald_hashing::SeededPrng::new(test_seed).derive("emergent-test");

        let mut test = Vec::with_capacity(bundle.test_cases.len());
        let mut probs = Vec::with_capacity(bundle.test_cases.len());
        for c in &bundle.test_cases {
            let plan = plan_paired_case(test_seed, &c.case_id);
            // Frozen policies: the opposite agent as sender emits the test
            // token through the same 1/8 drop channel, and the role agent
            // as receiver scores its position posterior. The generated
            // placeholder token is discarded.
            let cands = candidate_types(&c.candidate_type_codes);
            let target_type = cands[c.target_index];
            let token = sender.sample_token(target_type, &mut test_rng)?;
            let channel_draw = test_rng.next_float();
            let delivered = if channel_dropped(channel_draw) {
                None
            } else {
                Some(token)
            };
            let delivered_token = delivered.map(|id| inventory[id].clone());
            let native: Vec<f64> = receiver
                .receiver_position_posterior(delivered, &cands)?
                .iter()
                .map(|v| stable_f64(*v))
                .collect();
            let native_missing = delivered_token.is_none();
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
                ald_hashing::SeededPrng::new(test_seed).derive(&format!("branch/{}", c.case_id));
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
                // total_cmp: identical argmax on the finite distributions
                // valid_distribution enforces above, and panic-free.
                let ok = d
                    .iter()
                    .enumerate()
                    .max_by(|a, b| a.1.total_cmp(b.1))
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
                delivered_token: delivered_token.clone(),
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
                delivered_token
                    .clone()
                    .map_or(serde_json::Value::Null, serde_json::Value::String),
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
                    delivered_token: delivered_token.clone(),
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
            to_pretty(&probs)?,
        )
        .map_err(|e| OrchestratorError::Io(e.to_string()))?;
        let folds = RoleFolds {
            role: role.to_string(),
            // The persisted training fold is always the manifest-count
            // prefix, however many episodes ran over the bundle.
            training: bundle.training[..per_role_train].to_vec(),
            validation_fit: bundle.validation_fit.clone(),
            validation_selection: bundle.validation_selection.clone(),
            test,
        };
        // Role folds for `ala analyze select|score`: the generated
        // training/validation/test folds with emergent-native, replay, and
        // per-branch probabilities, in the committed fixture schema.
        std::fs::write(
            format!("{slot_dir}/role-folds-{role}.json"),
            to_pretty(&persistable_role_folds(&folds))?,
        )
        .map_err(|e| OrchestratorError::Io(e.to_string()))?;
        roles.push(folds);
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
    std::fs::write(format!("{slot_dir}/dyad-result.json"), to_pretty(&result)?)
        .map_err(|e| OrchestratorError::Io(e.to_string()))?;

    // Non-default runs record the full development configuration alongside
    // the dyad artifacts: requested + resolved options, digests, the seeds
    // every split was generated from, the 2N/N counts, and the per-split
    // per-type exposure census. Legacy runs write no sidecar.
    if let Some(hash) = dev_hash {
        let mut seeds = serde_json::Map::new();
        let mut exposure = serde_json::Map::new();
        for b in &bundles {
            let mut role_seeds = serde_json::Map::new();
            role_seeds.insert("training".to_string(), serde_json::json!(b.train_seed));
            role_seeds.insert("validationFit".to_string(), serde_json::json!(b.fit_seed));
            role_seeds.insert(
                "validationSelection".to_string(),
                serde_json::json!(b.sel_seed),
            );
            role_seeds.insert(
                "withinSupportTest".to_string(),
                serde_json::json!(b.test_seed),
            );
            seeds.insert(b.role.clone(), serde_json::Value::Object(role_seeds));
            let train_codes: Vec<Vec<u32>> = b.training[..episodes_per_direction]
                .iter()
                .map(|r| r.candidate_type_codes.clone())
                .collect();
            let fit_codes: Vec<Vec<u32>> = b
                .validation_fit
                .iter()
                .map(|r| r.candidate_type_codes.clone())
                .collect();
            let sel_codes: Vec<Vec<u32>> = b
                .validation_selection
                .iter()
                .map(|r| r.candidate_type_codes.clone())
                .collect();
            let test_codes: Vec<Vec<u32>> = b
                .test_cases
                .iter()
                .map(|c| c.candidate_type_codes.to_vec())
                .collect();
            exposure.insert(
                b.role.clone(),
                serde_json::to_value(RoleExposure {
                    role: b.role.clone(),
                    training: exposure_histogram(&train_codes),
                    validation_fit: exposure_histogram(&fit_codes),
                    validation_selection: exposure_histogram(&sel_codes),
                    within_support_test: exposure_histogram(&test_codes),
                })
                .map_err(|e| OrchestratorError::Domain(e.to_string()))?,
            );
        }
        let sidecar = serde_json::json!({
            "requested": requested,
            "resolved": resolved,
            "digests": {
                "manifestSha256": manifest_sha256,
                "developmentConfigSha256": hash,
            },
            "seeds": {
                "seedRoot": seed_root,
                "roles": seeds,
            },
            "counts": {
                "perRoleTrainFold": per_role_train,
                "trainingInteractionsPerDirection": resolved.training_interactions_per_direction,
                "trainingInteractionsTotal": resolved.training_interactions_total,
                "perRoleValidationFit": per_role_fit,
                "perRoleValidationSelection": per_role_sel,
                "perRoleTest": per_role_test,
            },
            "exposureCounts": exposure,
            "versions": {
                "protocolVersion": req.protocol_version,
                "treatmentBatchVersion": req.treatment_batch_version,
                "manifestVersion": "ala-lv-execution-manifest/v1",
            },
        });
        std::fs::write(
            format!("{slot_dir}/development-config.json"),
            to_pretty(&sidecar)?,
        )
        .map_err(|e| OrchestratorError::Io(e.to_string()))?;
    }

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
    use sha2::Digest as _;

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
            let text =
                std::fs::read_to_string(slot.join(format!("branch-probabilities-{role}.json")))
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
            let text =
                std::fs::read_to_string(slot.join(format!("role-folds-{role}.json"))).unwrap();
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
                    evidence_root
                        .join("development/primary-0007/branch-probabilities-agent-0.json"),
                )
                .unwrap(),
            );
            folds_bytes.push(
                std::fs::read(
                    evidence_root.join("development/primary-0007/role-folds-agent-0.json"),
                )
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
        let text = std::fs::read_to_string(
            evidence_root.join("development/primary-0000/dyad-result.json"),
        )
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
            let text =
                std::fs::read_to_string(slot.join(format!("role-folds-{role}.json"))).unwrap();
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

    // -- development options -------------------------------------------------

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
        assert!(
            resolve_development_options(&dev_opts(Some(0), None, None, None, None), 8).is_err()
        );
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
        assert!(
            resolve_development_options(&dev_opts(None, None, None, Some(7), None), 8).is_err()
        );
        assert!(resolve_development_options(
            &dev_opts(None, None, None, None, Some(vec![3, 3])),
            8
        )
        .is_err());
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
                    &std::fs::read_to_string(
                        slot.join(format!("branch-probabilities-{role}.json")),
                    )
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
            let sidecar =
                read_json(&root.join(format!("development/{name}/development-config.json")));
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
                read_json(&root.join(format!("development/{name}/development-config.json")))
                    ["counts"]["trainingInteractionsPerDirection"]
                    == serde_json::json!(3u64)
            })
            .unwrap()
            .clone();
        let before =
            std::fs::read(root.join(format!("development/{n3_root}/development-config.json")))
                .unwrap();
        let err = run_dyad(&mk(3), &test_inventory()).unwrap_err().to_string();
        assert!(err.contains("overwrite"), "unexpected error: {err}");
        let after =
            std::fs::read(root.join(format!("development/{n3_root}/development-config.json")))
                .unwrap();
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
}
