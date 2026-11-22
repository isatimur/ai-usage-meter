//! Which telemetry and feedback switches are set for each tool on this machine.
//!
//! Read-only. It looks at process environment and local config files, and at
//! nothing else. It reports the switches a tool documents or ships. It does
//! not claim to know what a tool sends: only a traffic capture can show that.

use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    /// You turned it off.
    Off,
    /// Not set. The tool's default applies, and for telemetry that default is on.
    Default,
    /// You turned on something that is off by default.
    On,
}

#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub tool: &'static str,
    pub setting: &'static str,
    pub controls: &'static str,
    pub state: State,
    /// What "Default" means for this setting: true when the tool sends it unless you opt out.
    pub default_on: bool,
    pub how_to_turn_off: &'static str,
}

struct EnvSwitch {
    name: &'static str,
    controls: &'static str,
    /// true: setting it turns something off. false: setting it turns something on.
    off_switch: bool,
}

const CLAUDE_SWITCHES: &[EnvSwitch] = &[
    EnvSwitch {
        name: "DISABLE_TELEMETRY",
        controls: "Usage events and metrics sent to Anthropic",
        off_switch: true,
    },
    EnvSwitch {
        name: "DISABLE_ERROR_REPORTING",
        controls: "Error reports",
        off_switch: true,
    },
    EnvSwitch {
        name: "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC",
        controls: "Update checks, feedback and other non-essential requests",
        off_switch: true,
    },
    EnvSwitch {
        name: "CLAUDE_CODE_DISABLE_FEEDBACK_SURVEY",
        controls: "Feedback survey prompts",
        off_switch: true,
    },
    EnvSwitch {
        name: "CLAUDE_CODE_ENABLE_TELEMETRY",
        controls: "OpenTelemetry export to a collector you choose",
        off_switch: false,
    },
];

fn truthy(v: &str) -> bool {
    matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes")
}

/// `env` entries from a Claude Code settings file, as (name, value) pairs.
fn settings_env(path: &Path) -> Vec<(String, String)> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };
    let Some(map) = json.get("env").and_then(|e| e.as_object()) else {
        return Vec::new();
    };
    map.iter()
        .map(|(k, v)| {
            let s = match v {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            (k.clone(), s)
        })
        .collect()
}

/// Value of `key` under `[section]` in a TOML file. Reads only what it needs:
/// section headers and `key = value` lines. Returns None when absent.
fn toml_value(text: &str, section: &str, key: &str) -> Option<String> {
    let mut current = "";
    for raw in text.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            current = name.trim();
            continue;
        }
        if current == section {
            if let Some((k, v)) = line.split_once('=') {
                if k.trim() == key {
                    return Some(v.trim().trim_matches('"').to_string());
                }
            }
        }
    }
    None
}

fn toml_has_section(text: &str, section: &str) -> bool {
    text.lines().any(|l| {
        l.split('#')
            .next()
            .unwrap_or("")
            .trim()
            .strip_prefix('[')
            .and_then(|l| l.strip_suffix(']'))
            .is_some_and(|n| {
                let n = n.trim();
                n == section || n.starts_with(&format!("{section}."))
            })
    })
}

pub fn check(home: &Path, env: &dyn Fn(&str) -> Option<String>) -> Vec<Finding> {
    let mut out = Vec::new();

    let mut file_env = settings_env(&home.join(".claude/settings.json"));
    file_env.extend(settings_env(&home.join(".claude/settings.local.json")));
    for sw in CLAUDE_SWITCHES {
        let value = env(sw.name).or_else(|| {
            file_env
                .iter()
                .rev()
                .find(|(k, _)| k == sw.name)
                .map(|(_, v)| v.clone())
        });
        let set = value.as_deref().is_some_and(truthy);
        let state = match (set, sw.off_switch) {
            (true, true) => State::Off,
            (true, false) => State::On,
            (false, _) => State::Default,
        };
        out.push(Finding {
            tool: "Claude Code",
            setting: sw.name,
            controls: sw.controls,
            state,
            default_on: sw.off_switch,
            how_to_turn_off: if sw.off_switch {
                "Set it to 1 in the \"env\" object of ~/.claude/settings.json"
            } else {
                "Leave it unset"
            },
        });
    }

    let codex = std::fs::read_to_string(home.join(".codex/config.toml")).unwrap_or_default();
    for (section, controls) in [
        ("analytics", "Product analytics sent to OpenAI"),
        ("feedback", "Feedback submission"),
    ] {
        let state = match toml_value(&codex, section, "enabled").as_deref() {
            Some("false") => State::Off,
            _ => State::Default,
        };
        out.push(Finding {
            tool: "Codex",
            setting: if section == "analytics" {
                "[analytics] enabled"
            } else {
                "[feedback] enabled"
            },
            controls,
            state,
            default_on: true,
            how_to_turn_off: if section == "analytics" {
                "Put \"enabled = false\" under [analytics] in ~/.codex/config.toml"
            } else {
                "Put \"enabled = false\" under [feedback] in ~/.codex/config.toml"
            },
        });
    }
    out.push(Finding {
        tool: "Codex",
        setting: "[otel]",
        controls: "OpenTelemetry export to a collector you choose",
        state: if toml_has_section(&codex, "otel") {
            State::On
        } else {
            State::Default
        },
        default_on: false,
        how_to_turn_off: "Remove the [otel] section from ~/.codex/config.toml",
    });

    // OpenCode: sharing is manual unless you set it to auto, and the model
    // list is fetched from the network unless you turn that off.
    let opencode_cfg: serde_json::Value =
        std::fs::read_to_string(home.join(".config/opencode/opencode.json"))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or(serde_json::Value::Null);
    let share_cfg = opencode_cfg.get("share").and_then(|v| v.as_str());
    let env_on = |name: &str| env(name).as_deref().is_some_and(truthy);
    out.push(Finding {
        tool: "OpenCode",
        setting: "share",
        controls: "Uploading your conversations to opencode.ai as shareable links",
        state: if env_on("OPENCODE_DISABLE_SHARE") || share_cfg == Some("disabled") {
            State::Off
        } else if env_on("OPENCODE_AUTO_SHARE") || share_cfg == Some("auto") {
            State::On
        } else {
            State::Default
        },
        default_on: false,
        how_to_turn_off: "Set \"share\": \"disabled\" in ~/.config/opencode/opencode.json",
    });
    out.push(Finding {
        tool: "OpenCode",
        setting: "OPENCODE_DISABLE_MODELS_FETCH",
        controls: "Downloading the model list from the network",
        state: if env_on("OPENCODE_DISABLE_MODELS_FETCH") {
            State::Off
        } else {
            State::Default
        },
        default_on: true,
        how_to_turn_off: "Set OPENCODE_DISABLE_MODELS_FETCH=1 in your shell profile",
    });

    // Gemini CLI: usage statistics default to on.
    let gemini_off = std::fs::read_to_string(home.join(".gemini/settings.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|v| v.get("usageStatisticsEnabled").and_then(|b| b.as_bool()))
        == Some(false);
    out.push(Finding {
        tool: "Gemini CLI",
        setting: "usageStatisticsEnabled",
        controls: "Usage statistics sent to Google",
        state: if gemini_off {
            State::Off
        } else {
            State::Default
        },
        default_on: true,
        how_to_turn_off: "Set \"usageStatisticsEnabled\": false in ~/.gemini/settings.json",
    });

    out
}
