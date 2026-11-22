//! OpenRouter client. Field names follow OpenRouter's published API docs and
//! are NOT yet verified against a live response; every field is optional or
//! defaulted so a missing one never breaks parsing.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::Duration;

pub const DEFAULT_BASE: &str = "https://openrouter.ai";
const DEFAULT_BACKOFF_MS: u64 = 500;
const MAX_TRIES: u32 = 3;

#[derive(Debug)]
pub enum OpenRouterError {
    NeedsManagementKey,
    Unauthorized,
    RateLimited,
    Http(u16),
    Transport(String),
    BadResponse(String),
}

impl fmt::Display for OpenRouterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NeedsManagementKey => {
                write!(f, "this endpoint needs an OpenRouter management key")
            }
            Self::Unauthorized => write!(f, "OpenRouter rejected the key"),
            Self::RateLimited => write!(f, "OpenRouter rate limit hit, try again later"),
            Self::Http(c) => write!(f, "OpenRouter returned HTTP {c}"),
            Self::Transport(e) => write!(f, "could not reach OpenRouter: {e}"),
            Self::BadResponse(e) => write!(f, "unexpected OpenRouter response: {e}"),
        }
    }
}

impl std::error::Error for OpenRouterError {}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct KeyUsage {
    pub label: Option<String>,
    pub limit: Option<f64>,
    pub limit_remaining: Option<f64>,
    pub usage: f64,
    pub usage_daily: f64,
    pub usage_weekly: f64,
    pub usage_monthly: f64,
    pub is_free_tier: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Credits {
    pub total_credits: f64,
    pub total_usage: f64,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct ActivityRow {
    pub date: String,
    pub model: String,
    pub requests: u64,
    pub usage: f64,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub reasoning_tokens: u64,
}

#[derive(Deserialize)]
struct Envelope<T> {
    data: T,
}

fn get<T: serde::de::DeserializeOwned>(
    base: &str,
    path: &str,
    key: &str,
    backoff_ms: u64,
) -> Result<T, OpenRouterError> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(15)))
        .build()
        .into();
    let url = format!("{}{}", base.trim_end_matches('/'), path);

    for attempt in 0..MAX_TRIES {
        let mut resp = agent
            .get(&url)
            .header("Authorization", &format!("Bearer {key}"))
            .call()
            .map_err(|e| OpenRouterError::Transport(e.to_string()))?;
        match resp.status().as_u16() {
            200..=299 => {
                let body = resp
                    .body_mut()
                    .read_to_string()
                    .map_err(|e| OpenRouterError::Transport(e.to_string()))?;
                let env: Envelope<T> = serde_json::from_str(&body)
                    .map_err(|e| OpenRouterError::BadResponse(e.to_string()))?;
                return Ok(env.data);
            }
            401 => return Err(OpenRouterError::Unauthorized),
            403 => return Err(OpenRouterError::NeedsManagementKey),
            429 if attempt + 1 < MAX_TRIES => {
                std::thread::sleep(Duration::from_millis(backoff_ms << attempt));
            }
            429 => return Err(OpenRouterError::RateLimited),
            other => return Err(OpenRouterError::Http(other)),
        }
    }
    Err(OpenRouterError::RateLimited)
}

pub fn fetch_key_usage_with(
    base: &str,
    key: &str,
    backoff_ms: u64,
) -> Result<KeyUsage, OpenRouterError> {
    get(base, "/api/v1/key", key, backoff_ms)
}

pub fn fetch_credits_with(
    base: &str,
    mgmt_key: &str,
    backoff_ms: u64,
) -> Result<Credits, OpenRouterError> {
    get(base, "/api/v1/credits", mgmt_key, backoff_ms)
}

pub fn fetch_activity_with(
    base: &str,
    mgmt_key: &str,
    backoff_ms: u64,
) -> Result<Vec<ActivityRow>, OpenRouterError> {
    get(base, "/api/v1/activity", mgmt_key, backoff_ms)
}

pub fn fetch_key_usage(base: &str, key: &str) -> Result<KeyUsage, OpenRouterError> {
    fetch_key_usage_with(base, key, DEFAULT_BACKOFF_MS)
}

pub fn fetch_credits(base: &str, mgmt_key: &str) -> Result<Credits, OpenRouterError> {
    fetch_credits_with(base, mgmt_key, DEFAULT_BACKOFF_MS)
}

pub fn fetch_activity(base: &str, mgmt_key: &str) -> Result<Vec<ActivityRow>, OpenRouterError> {
    fetch_activity_with(base, mgmt_key, DEFAULT_BACKOFF_MS)
}
