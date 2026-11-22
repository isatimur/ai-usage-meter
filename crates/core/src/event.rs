use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tool {
    Claude,
    Codex,
    OpenCode,
    OpenRouter,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum CostSource {
    Stored,
    Computed,
    Unknown,
}

impl Tool {
    pub fn as_str(&self) -> &'static str {
        match self {
            Tool::Claude => "Claude",
            Tool::Codex => "Codex",
            Tool::OpenCode => "OpenCode",
            Tool::OpenRouter => "OpenRouter",
        }
    }

    pub fn parse(s: &str) -> Option<Tool> {
        [Tool::Claude, Tool::Codex, Tool::OpenCode, Tool::OpenRouter]
            .into_iter()
            .find(|t| t.as_str() == s)
    }
}

impl CostSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            CostSource::Stored => "Stored",
            CostSource::Computed => "Computed",
            CostSource::Unknown => "Unknown",
        }
    }

    pub fn parse(s: &str) -> CostSource {
        match s {
            "Stored" => CostSource::Stored,
            "Computed" => CostSource::Computed,
            _ => CostSource::Unknown,
        }
    }
}

/// `input` is uncached input only.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct UsageEvent {
    pub tool: Tool,
    pub ts_ms: i64,
    pub model: String,
    pub project: String,
    pub session: String,
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write_5m: u64,
    pub cache_write_1h: u64,
    pub reasoning: u64,
    pub cost_usd: Option<f64>,
    pub cost_source: CostSource,
    pub dedup_key: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_roundtrips_through_json() {
        let ev = UsageEvent {
            tool: Tool::Claude,
            ts_ms: 1_700_000_000_000,
            model: "claude-opus-5".into(),
            project: "/tmp/proj".into(),
            session: "s1".into(),
            input: 10,
            output: 20,
            cache_read: 30,
            cache_write_5m: 40,
            cache_write_1h: 50,
            reasoning: 5,
            cost_usd: Some(0.25),
            cost_source: CostSource::Computed,
            dedup_key: "msg_1".into(),
        };
        let json = serde_json::to_string(&ev).unwrap();
        let back: UsageEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(ev, back);
    }
}
