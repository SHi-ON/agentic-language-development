//! Runtime environment loading.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("{0}")]
    Invalid(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DeploymentMode {
    Prototype,
    ResearchGrade,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeEnvironment {
    pub dtsf_port: u16,
    pub twin_packs_dir: String,
    pub deployment_mode: DeploymentMode,
    pub evidence_dir: String,
    pub database_path: String,
    pub signer_seeds_file: Option<String>,
    pub log_level: LogLevel,
    pub anchor_class: String,
    pub base_network: String,
    pub base_rpc_url_file: Option<String>,
    pub anchor_key_file: Option<String>,
}

fn parse_port(source: &HashMap<String, String>, name: &str, default: u16) -> Result<u16, ConfigError> {
    match source.get(name) {
        None => Ok(default),
        Some(v) if v.is_empty() => Ok(default),
        Some(v) => v.parse::<u16>().map_err(|_| {
            ConfigError::Invalid(format!("{name} must be a positive integer"))
        }).and_then(|p| if p == 0 { Err(ConfigError::Invalid(format!("{name} must be a positive integer"))) } else { Ok(p) }),
    }
}

pub fn load_runtime_environment(
    source: &HashMap<String, String>,
) -> Result<RuntimeEnvironment, ConfigError> {
    for forbidden in [
        "ALD_RUN_SIGNER_SEEDS_JSON",
        "ALD_BASE_RPC_URL",
        "ALD_ANCHOR_KEY",
    ] {
        if source.contains_key(forbidden) {
            return Err(ConfigError::Invalid(format!(
                "{forbidden} is a direct secret value; use fort run --mode files"
            )));
        }
    }
    let deployment_mode = match source
        .get("ALD_DEPLOYMENT_MODE")
        .map(String::as_str)
        .unwrap_or("prototype")
    {
        "prototype" => DeploymentMode::Prototype,
        "research-grade" => DeploymentMode::ResearchGrade,
        other => {
            return Err(ConfigError::Invalid(format!(
                "ALD_DEPLOYMENT_MODE must be one of: prototype, research-grade (got {other})"
            )));
        }
    };
    let log_level = match source.get("ALD_LOG_LEVEL").map(String::as_str).unwrap_or("info") {
        "debug" => LogLevel::Debug,
        "info" => LogLevel::Info,
        "warn" => LogLevel::Warn,
        "error" => LogLevel::Error,
        other => {
            return Err(ConfigError::Invalid(format!(
                "ALD_LOG_LEVEL must be one of: debug, info, warn, error (got {other})"
            )));
        }
    };
    Ok(RuntimeEnvironment {
        dtsf_port: parse_port(source, "ALD_DTSF_PORT", 4123)?,
        twin_packs_dir: source
            .get("ALD_TWIN_PACKS_DIR")
            .cloned()
            .unwrap_or_else(|| "./twin-packs".to_string()),
        deployment_mode,
        evidence_dir: source
            .get("ALD_EVIDENCE_DIR")
            .cloned()
            .unwrap_or_else(|| "./evidence".to_string()),
        database_path: source
            .get("ALD_DATABASE_PATH")
            .cloned()
            .unwrap_or_else(|| "./evidence/evidence.sqlite".to_string()),
        signer_seeds_file: source.get("ALD_SIGNER_SEEDS_FILE").cloned(),
        log_level,
        anchor_class: source
            .get("ALD_ANCHOR_CLASS")
            .cloned()
            .unwrap_or_else(|| "simulated".to_string()),
        base_network: source
            .get("ALD_BASE_NETWORK")
            .cloned()
            .unwrap_or_else(|| "base-sepolia".to_string()),
        base_rpc_url_file: source.get("ALD_BASE_RPC_URL_FILE").cloned(),
        anchor_key_file: source.get("ALD_ANCHOR_KEY_FILE").cloned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let env = load_runtime_environment(&HashMap::new()).unwrap();
        assert_eq!(env.dtsf_port, 4123);
        assert_eq!(env.deployment_mode, DeploymentMode::Prototype);
    }

    #[test]
    fn rejects_direct_secrets() {
        let mut m = HashMap::new();
        m.insert("ALD_ANCHOR_KEY".to_string(), "x".to_string());
        assert!(load_runtime_environment(&m).is_err());
    }
}
