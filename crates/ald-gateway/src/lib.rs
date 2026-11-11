//! Symbol gateway: delivery routing and control-branch substitution point.
//!
//! ALA surface: route a
//! delivered token to the receiver role, and expose the branch at which
//! control substitution happens (the substitution math itself lives in
//! `ald-interventions`).

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GatewayError {
    #[error("gateway domain error: {0}")]
    Domain(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Delivery {
    pub case_id: String,
    pub receiver_role: String,
    pub token: Option<String>,
    pub branch: String,
}

/// Route a case to its receiver; control branches carry the same token as
/// normal (substitution happens at scoring, never at delivery).
pub fn route(case_id: &str, role: &str, token: Option<&str>, branch: &str) -> Result<Delivery, GatewayError> {
    if role != "agent-0" && role != "agent-1" {
        return Err(GatewayError::Domain(format!("unknown receiver role {role}")));
    }
    if !["normal", "disabled", "constant", "random", "shuffled"].contains(&branch) {
        return Err(GatewayError::Domain(format!("unknown branch {branch}")));
    }
    Ok(Delivery {
        case_id: case_id.to_string(),
        receiver_role: role.to_string(),
        token: token.map(str::to_string),
        branch: branch.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_control_with_same_token() {
        let d = route("c1", "agent-0", Some("tok01"), "shuffled").unwrap();
        assert_eq!(d.token.as_deref(), Some("tok01"));
    }

    #[test]
    fn rejects_unknown_role() {
        assert!(route("c1", "agent-2", None, "normal").is_err());
    }
}
