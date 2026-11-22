//! Development-only run options (cohort-gated ablations).

#![allow(clippy::cast_precision_loss)] // small counts/indices; exact for realistic inputs
#![allow(clippy::cast_possible_truncation)] // small counts/indices; bounded by construction
#![allow(clippy::cast_sign_loss)] // non-negative by construction (validated ranges, draws in [0, 1))

use crate::OrchestratorError;
use serde::{Deserialize, Serialize};

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
