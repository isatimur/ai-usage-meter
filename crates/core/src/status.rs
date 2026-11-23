//! Provider status from the public Statuspage JSON (no login, no token).

use serde::Serialize;
use std::time::Duration;

pub const CLAUDE_STATUS: &str = "https://status.claude.com";
pub const OPENAI_STATUS: &str = "https://status.openai.com";

/// How bad it is right now, from the page's own `indicator` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Operational,
    Minor,
    Major,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProviderStatus {
    pub level: Level,
    /// The page's own words, for example "All Systems Operational".
    pub description: String,
}

#[derive(Debug)]
pub enum StatusError {
    Transport(String),
    Http(u16),
    Shape,
}

impl std::fmt::Display for StatusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transport(e) => write!(f, "status page unreachable: {e}"),
            Self::Http(c) => write!(f, "status page returned HTTP {c}"),
            Self::Shape => write!(f, "status page returned an unexpected body"),
        }
    }
}

/// Reads `{"status":{"indicator":...,"description":...}}`.
pub fn parse(body: &str) -> Result<ProviderStatus, StatusError> {
    let v: serde_json::Value = serde_json::from_str(body).map_err(|_| StatusError::Shape)?;
    let s = &v["status"];
    let level = match s["indicator"].as_str().ok_or(StatusError::Shape)? {
        "none" => Level::Operational,
        "minor" | "maintenance" => Level::Minor,
        "major" => Level::Major,
        "critical" => Level::Critical,
        _ => return Err(StatusError::Shape),
    };
    Ok(ProviderStatus {
        level,
        description: s["description"].as_str().unwrap_or("").to_string(),
    })
}

pub fn fetch_with(base: &str) -> Result<ProviderStatus, StatusError> {
    let url = format!("{}/api/v2/status.json", base.trim_end_matches('/'));
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(10)))
        .build()
        .into();
    let mut resp = agent
        .get(&url)
        .call()
        .map_err(|e| StatusError::Transport(e.to_string()))?;
    let code = resp.status().as_u16();
    if !(200..=299).contains(&code) {
        return Err(StatusError::Http(code));
    }
    let body = resp
        .body_mut()
        .read_to_string()
        .map_err(|e| StatusError::Transport(e.to_string()))?;
    parse(&body)
}
