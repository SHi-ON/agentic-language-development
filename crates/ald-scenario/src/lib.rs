//! Referential scenario engine (LV01): deterministic candidate generation,
//! transcript production, and paired-branch execution plans.
//!
//! The `ReferentialScenarioEngine` surface plus the LV01 branch plan.
#![allow(clippy::cast_precision_loss)] // small counts/indices; exact for realistic inputs
#![allow(clippy::cast_possible_truncation)] // small counts/indices; bounded by construction
#![allow(clippy::cast_sign_loss)] // non-negative by construction (validated ranges, draws in [0, 1))

use ald_hashing::SeededPrng;
use ald_types::signaling::{CANDIDATES, TYPES};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ScenarioError {
    #[error("scenario domain error: {0}")]
    Domain(String),
}

pub const LV01_UNTOUCHED_TYPE_CODES: [u32; 4] = [12, 13, 14, 15];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lv01Case {
    pub case_id: String,
    pub receiver_role: String,
    pub delivered_token: Option<String>,
    pub candidate_type_codes: [u32; 4],
    /// Researcher-only target; never delivered to the receiver.
    pub target_index: usize,
}

/// Deterministic LV01 case generator: `seeded stream per (runSeed, slot)`.
#[must_use]
pub fn generate_cases(
    seed: &str,
    slot: u64,
    role: &str,
    count: usize,
    inventory: &[String],
) -> Vec<Lv01Case> {
    let mut rng = SeededPrng::new(seed).derive(&format!("lv01/{slot}/{role}"));
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let mut codes = [0u32; 4];
        let mut used = [false; TYPES];
        for c in &mut codes {
            loop {
                let t = rng.next_int(TYPES as u32).unwrap_or(0) as usize;
                if !used[t] {
                    used[t] = true;
                    *c = t as u32;
                    break;
                }
            }
        }
        let target = rng.next_int(CANDIDATES as u32).unwrap_or(0) as usize;
        let token = if rng.next_int(8).unwrap_or(1) == 0 {
            None
        } else {
            Some(inventory[rng.next_int(inventory.len() as u32).unwrap_or(0) as usize].clone())
        };
        out.push(Lv01Case {
            case_id: format!("{role}-{slot}-{i:04}"),
            receiver_role: role.to_string(),
            delivered_token: token,
            candidate_type_codes: codes,
            target_index: target,
        });
    }
    out
}

/// Deterministic LV01 case generator with held-out type exclusion.
///
/// Generates exactly as [`generate_cases`] (identical stream, identical draw
/// order) and then replaces every held-out type code with a deterministic
/// replacement fill: the smallest-numbered permitted type is NOT used;
/// instead each replaced position draws from a dedicated per-case fill
/// stream (`heldout-fill/{case_id}`) until it lands on a permitted type not
/// already present in the case. Cases keep 4 distinct candidate codes, the
/// target index and placeholder token draws are untouched, and an empty
/// `held_out` returns bytes identical to [`generate_cases`].
///
/// Callers must guarantee at least [`CANDIDATES`] permitted types remain
/// (i.e. `held_out.len() <= TYPES - CANDIDATES` after dedup); with fewer,
/// distinctness is unfillable and the fill loop never terminates. The
/// orchestrator validates this before calling.
#[must_use]
pub fn generate_cases_excluding(
    seed: &str,
    slot: u64,
    role: &str,
    count: usize,
    inventory: &[String],
    held_out: &[u32],
) -> Vec<Lv01Case> {
    let mut cases = generate_cases(seed, slot, role, count, inventory);
    if held_out.is_empty() {
        return cases;
    }
    let held = |t: u32| held_out.contains(&t);
    for case in &mut cases {
        if !case.candidate_type_codes.iter().any(|c| held(*c)) {
            continue;
        }
        let mut fill_rng = SeededPrng::new(seed).derive(&format!("heldout-fill/{}", case.case_id));
        let mut used = [false; TYPES];
        for c in &case.candidate_type_codes {
            if !held(*c) {
                used[*c as usize] = true;
            }
        }
        for c in &mut case.candidate_type_codes {
            if held(*c) {
                loop {
                    let t = fill_rng.next_int(TYPES as u32).unwrap_or(0);
                    if !held(t) && !used[t as usize] {
                        used[t as usize] = true;
                        *c = t;
                        break;
                    }
                }
            }
        }
    }
    cases
}

/// LV01 branches: normal plus four ablations.
pub const LV01_BRANCHES: [&str; 5] = ["normal", "disabled", "constant", "random", "shuffled"];

/// Paired execution plan for one case: the shared unit draw and per-branch
/// probability vectors are produced from one stream so audit reproduces them.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairedCasePlan {
    pub case_id: String,
    pub shared_draw: f64,
    pub draw_bits_hex: String,
}

#[must_use]
pub fn plan_paired_case(seed: &str, case_id: &str) -> PairedCasePlan {
    let mut rng = SeededPrng::new(seed).derive(&format!("lv01-draw/{case_id}"));
    let u = rng.next_float();
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&u.to_bits().to_be_bytes());
    PairedCasePlan {
        case_id: case_id.to_string(),
        shared_draw: u,
        draw_bits_hex: hex::encode(buf),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generation_is_deterministic() {
        let inv: Vec<String> = (0..32).map(|i| format!("tok{i:02}")).collect();
        let a = generate_cases("s", 7, "agent-0", 50, &inv);
        let b = generate_cases("s", 7, "agent-0", 50, &inv);
        assert_eq!(a.len(), 50);
        assert_eq!(
            a.iter().map(|c| c.case_id.clone()).collect::<Vec<_>>(),
            b.iter().map(|c| c.case_id.clone()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn draw_bits_roundtrip() {
        let p = plan_paired_case("s", "agent-0-7-0001");
        assert!(p.shared_draw >= 0.0 && p.shared_draw < 1.0);
        assert_eq!(p.draw_bits_hex.len(), 16);
    }

    #[test]
    fn generation_prefix_is_stable_over_counts() {
        // Training-length overrides rely on this: generating N2 > N1 cases
        // yields the N1-prefix plus fresh stream draws, never duplication.
        let inv: Vec<String> = (0..32).map(|i| format!("tok{i:02}")).collect();
        let short = generate_cases("prefix-seed", 0, "agent-0", 5, &inv);
        let long = generate_cases("prefix-seed", 0, "agent-0", 40, &inv);
        for (i, c) in short.iter().enumerate() {
            assert_eq!(c.case_id, long[i].case_id);
            assert_eq!(c.candidate_type_codes, long[i].candidate_type_codes);
            assert_eq!(c.target_index, long[i].target_index);
            assert_eq!(c.delivered_token, long[i].delivered_token);
        }
        // Same prefix stability holds for the held-out variant.
        let held = [12u32, 13, 14, 15];
        let short_f = generate_cases_excluding("prefix-seed", 0, "agent-0", 5, &inv, &held);
        let long_f = generate_cases_excluding("prefix-seed", 0, "agent-0", 40, &inv, &held);
        for (i, c) in short_f.iter().enumerate() {
            assert_eq!(c.case_id, long_f[i].case_id);
            assert_eq!(c.candidate_type_codes, long_f[i].candidate_type_codes);
        }
    }

    #[test]
    fn excluding_empty_matches_legacy_exactly() {
        let inv: Vec<String> = (0..32).map(|i| format!("tok{i:02}")).collect();
        let a = generate_cases("s", 0, "agent-1", 60, &inv);
        let b = generate_cases_excluding("s", 0, "agent-1", 60, &inv, &[]);
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            serde_json::to_string(&b).unwrap()
        );
    }

    #[test]
    fn excluding_removes_held_out_and_preserves_shape() {
        let inv: Vec<String> = (0..32).map(|i| format!("tok{i:02}")).collect();
        let held = [12u32, 13, 14, 15];
        let base = generate_cases("fill-seed", 0, "agent-0", 200, &inv);
        let filtered = generate_cases_excluding("fill-seed", 0, "agent-0", 200, &inv, &held);
        assert!(base
            .iter()
            .any(|c| c.candidate_type_codes.iter().any(|t| held.contains(t))));
        for (b, f) in base.iter().zip(filtered.iter()) {
            // Identity, target, and token draws are untouched; only held-out
            // candidate codes are replaced.
            assert_eq!(f.case_id, b.case_id);
            assert_eq!(f.target_index, b.target_index);
            assert_eq!(f.delivered_token, b.delivered_token);
            assert_eq!(f.receiver_role, b.receiver_role);
            assert!(!f.candidate_type_codes.iter().any(|t| held.contains(t)));
            let mut sorted = f.candidate_type_codes;
            sorted.sort_unstable();
            assert_eq!(sorted.len(), 4, "fills must keep 4 distinct codes");
            for w in sorted.windows(2) {
                assert_ne!(w[0], w[1], "fills must keep 4 distinct codes");
            }
            for (bc, fc) in b
                .candidate_type_codes
                .iter()
                .zip(f.candidate_type_codes.iter())
            {
                if !held.contains(bc) {
                    assert_eq!(fc, bc, "permitted codes are never rewritten");
                }
            }
        }
        // Deterministic: same inputs, identical fills.
        let again = generate_cases_excluding("fill-seed", 0, "agent-0", 200, &inv, &held);
        assert_eq!(
            serde_json::to_string(&filtered).unwrap(),
            serde_json::to_string(&again).unwrap()
        );
        // Minimum permitted set (4 types left) still fills distinctly.
        let many: Vec<u32> = (4..16).collect();
        let min = generate_cases_excluding("fill-seed", 1, "agent-1", 50, &inv, &many);
        for c in &min {
            let mut sorted = c.candidate_type_codes;
            sorted.sort_unstable();
            assert_eq!(sorted, [0, 1, 2, 3]);
        }
    }
}
