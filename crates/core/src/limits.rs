//! Plan limits for Claude and Codex ("how much of my 5-hour and weekly
//! allowance is used"). Both endpoints are private and undocumented; the field
//! names below were read from real responses on 2026-11-16. They may change
//! without notice, so every field is optional and a failure shows up as an error
//! in the UI, never as a crash or a wrong number.
//!
//! Tokens are used for one request and never stored or logged.

use crate::pace::{self, Pace};
use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;
use std::time::Duration;

pub const CLAUDE_BASE: &str = "https://api.anthropic.com";
pub const CODEX_BASE: &str = "https://chatgpt.com";

#[derive(Debug)]
pub enum LimitsError {
    Unauthorized,
    Http(u16),
    Transport(String),
    BadResponse(String),
}

impl fmt::Display for LimitsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unauthorized => write!(
                f,
                "login expired, open Claude Code or Codex and sign in again"
            ),
            Self::Http(c) => write!(f, "the server answered with code {c}"),
            Self::Transport(e) => write!(f, "cannot reach the server: {e}"),
            Self::BadResponse(e) => write!(f, "unexpected answer from the server: {e}"),
        }
    }
}

impl std::error::Error for LimitsError {}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct LimitWindow {
    pub label: String,
    /// Share of the allowance already used, 0 to 100.
    pub percent: f64,
    pub resets_at_ms: Option<i64>,
    /// Forecast at fetch time; `None` when the window length or reset time is unknown.
    #[serde(default)]
    pub pace: Option<Pace>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct ToolLimits {
    pub plan: Option<String>,
    pub windows: Vec<LimitWindow>,
}

fn bad<T>(what: &str) -> Result<T, LimitsError> {
    Err(LimitsError::BadResponse(what.into()))
}

fn parse_iso(v: &Value) -> Option<i64> {
    v.as_str()?
        .parse::<Timestamp>()
        .ok()
        .map(|t| t.as_millisecond())
}

pub fn parse_claude(body: &str) -> Result<ToolLimits, LimitsError> {
    let v: Value =
        serde_json::from_str(body).map_err(|e| LimitsError::BadResponse(e.to_string()))?;
    if !v.is_object() {
        return bad("expected an object");
    }
    let mut windows = Vec::new();
    for (key, label) in [("five_hour", "5 hours"), ("seven_day", "Week")] {
        let w = &v[key];
        if let Some(percent) = w["utilization"].as_f64() {
            windows.push(LimitWindow {
                label: label.into(),
                percent,
                resets_at_ms: parse_iso(&w["resets_at"]),
                pace: None,
            });
        }
    }
    if let Some(list) = v["limits"].as_array() {
        for l in list.iter().filter(|l| l["kind"] == "weekly_scoped") {
            let Some(percent) = l["percent"].as_f64() else {
                continue;
            };
            let name = l["scope"]["model"]["display_name"]
                .as_str()
                .unwrap_or("separate limit");
            windows.push(LimitWindow {
                label: format!("Week · {name}"),
                percent,
                resets_at_ms: parse_iso(&l["resets_at"]),
                pace: None,
            });
        }
    }
    Ok(ToolLimits {
        plan: None,
        windows,
    })
}

pub fn parse_codex(body: &str) -> Result<ToolLimits, LimitsError> {
    let v: Value =
        serde_json::from_str(body).map_err(|e| LimitsError::BadResponse(e.to_string()))?;
    if !v.is_object() {
        return bad("expected an object");
    }
    let mut windows = Vec::new();
    for (key, label) in [("primary_window", "5 hours"), ("secondary_window", "Week")] {
        let w = &v["rate_limit"][key];
        if let Some(percent) = w["used_percent"].as_f64() {
            windows.push(LimitWindow {
                label: label.into(),
                percent,
                resets_at_ms: w["reset_at"].as_i64().map(|s| s * 1000),
                pace: None,
            });
        }
    }
    Ok(ToolLimits {
        plan: v["plan_type"].as_str().map(str::to_string),
        windows,
    })
}

fn get(url: &str, headers: &[(&str, String)]) -> Result<String, LimitsError> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(15)))
        .build()
        .into();
    let mut req = agent.get(url);
    for (k, v) in headers {
        req = req.header(*k, v);
    }
    // The transport error text never contains the headers, so the token cannot leak through it.
    let mut resp = req
        .call()
        .map_err(|e| LimitsError::Transport(e.to_string()))?;
    match resp.status().as_u16() {
        200..=299 => resp
            .body_mut()
            .read_to_string()
            .map_err(|e| LimitsError::Transport(e.to_string())),
        401 | 403 => Err(LimitsError::Unauthorized),
        other => Err(LimitsError::Http(other)),
    }
}

pub fn fetch_claude_with(base: &str, token: &str) -> Result<ToolLimits, LimitsError> {
    let url = format!("{}/api/oauth/usage", base.trim_end_matches('/'));
    let body = get(
        &url,
        &[
            ("Authorization", format!("Bearer {token}")),
            ("anthropic-beta", "oauth-2025-04-20".into()),
        ],
    )?;
    parse_claude(&body).map(with_pace)
}

pub fn fetch_codex_with(
    base: &str,
    token: &str,
    account_id: &str,
) -> Result<ToolLimits, LimitsError> {
    let url = format!("{}/backend-api/wham/usage", base.trim_end_matches('/'));
    let body = get(
        &url,
        &[
            ("Authorization", format!("Bearer {token}")),
            ("ChatGPT-Account-Id", account_id.to_string()),
        ],
    )?;
    parse_codex(&body).map(with_pace)
}

/// Adds the forecast to every window that has a known length and reset time.
pub fn with_pace(mut t: ToolLimits) -> ToolLimits {
    let now = Timestamp::now().as_millisecond();
    for w in &mut t.windows {
        w.pace = match (w.resets_at_ms, pace::window_ms_for_label(&w.label)) {
            (Some(reset), Some(len)) => pace::compute(w.percent, reset, len, now),
            _ => None,
        };
    }
    t
}
