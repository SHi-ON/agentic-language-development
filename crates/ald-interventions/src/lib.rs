//! Intervention events and gateway branch substitution.
//!
//! Typed intervention events
//! with entry hashes, and the four control-branch substitutions
//! (disabled / constant / random / shuffled) applied to a normal
//! distribution.

use ald_hashing::{hash_canonical, HashingError};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum InterventionError {
    #[error("hashing: {0}")]
    Hash(#[from] HashingError),
    #[error("intervention domain error: {0}")]
    Domain(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InterventionEvent {
    pub event_id: String,
    pub kind: String,
    pub case_id: String,
    pub branch: String,
    pub created_at: String,
}

pub fn intervention_entry_hash(event: &InterventionEvent) -> Result<String, InterventionError> {
    let v = serde_json::to_value(event)
        .map_err(|e| InterventionError::Domain(e.to_string()))?;
    Ok(hash_canonical(ald_types::hash_domains::INTERVENTION_EVENT, &v)?)
}

/// Apply a control-branch substitution to a normal distribution.
pub fn substitute_branch(
    branch: &str,
    normal: &[f64],
    draw: f64,
    seed_stream: &mut ald_hashing::SeededPrng,
) -> Result<Vec<f64>, InterventionError> {
    match branch {
        "normal" => Ok(normal.to_vec()),
        "disabled" => Ok(vec![0.25, 0.25, 0.25, 0.25]),
        "constant" => Ok(vec![1.0, 0.0, 0.0, 0.0]),
        "random" => {
            let k = seed_stream.next_int(4).map_err(|e| InterventionError::Domain(e.to_string()))?;
            let mut out = vec![0.0; 4];
            out[k as usize] = 1.0;
            Ok(out)
        }
        "shuffled" => {
            let mut idx = [0usize, 1, 2, 3];
            // Fisher-Yates with the shared draw sequence.
            let mut u = draw;
            for i in (1..4).rev() {
                u = (u * 7.13).fract();
                let j = (u * (i + 1) as f64).floor() as usize % (i + 1);
                idx.swap(i, j);
            }
            Ok(idx.iter().map(|&i| normal[i]).collect())
        }
        other => Err(InterventionError::Domain(format!("unknown branch {other}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_is_uniform() {
        let mut rng = ald_hashing::SeededPrng::new("s");
        assert_eq!(
            substitute_branch("disabled", &[0.0, 1.0, 0.0, 0.0], 0.5, &mut rng).unwrap(),
            vec![0.25; 4]
        );
    }

    #[test]
    fn shuffled_preserves_mass() {
        let mut rng = ald_hashing::SeededPrng::new("s");
        let out =
            substitute_branch("shuffled", &[0.1, 0.2, 0.3, 0.4], 0.5, &mut rng).unwrap();
        assert!((out.iter().sum::<f64>() - 1.0).abs() < 1e-12);
    }
}
