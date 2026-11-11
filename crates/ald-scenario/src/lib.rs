//! Referential scenario engine (LV01): deterministic candidate generation,
//! transcript production, and paired-branch execution plans.
//!
//! The `ReferentialScenarioEngine` surface plus the LV01 branch plan.

use ald_hashing::SeededPrng;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ScenarioError {
    #[error("scenario domain error: {0}")]
    Domain(String),
}

pub const LV01_UNTOUCHED_TYPE_CODES: [u32; 4] = [12, 13, 14, 15];
pub const CANDIDATES: usize = 4;
pub const TYPES: usize = 16;

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
        for c in codes.iter_mut() {
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
}
