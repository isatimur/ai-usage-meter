use super::ParseOutput;
use crate::{CostSource, Tool, UsageEvent};
use serde_json::Value;
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::str::FromStr;

fn u(v: &Value, key: &str) -> u64 {
    v.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn parse_line(line: &Value) -> Option<UsageEvent> {
    if line.get("type")?.as_str()? != "assistant" {
        return None;
    }
    let msg = line.get("message")?;
    let model = msg.get("model")?.as_str()?;
    if model == "<synthetic>" {
        return None;
    }
    let id = msg.get("id")?.as_str()?;
    let usage = msg.get("usage")?;
    let ts_ms = jiff::Timestamp::from_str(line.get("timestamp")?.as_str()?)
        .ok()?
        .as_millisecond();

    let (w5, w1) = match usage.get("cache_creation") {
        Some(cc) => (
            u(cc, "ephemeral_5m_input_tokens"),
            u(cc, "ephemeral_1h_input_tokens"),
        ),
        None => (u(usage, "cache_creation_input_tokens"), 0),
    };
    let reasoning = usage
        .get("output_tokens_details")
        .map(|d| u(d, "thinking_tokens"))
        .unwrap_or(0);

    Some(UsageEvent {
        tool: Tool::Claude,
        ts_ms,
        model: model.to_string(),
        project: line
            .get("cwd")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        session: line
            .get("sessionId")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        input: u(usage, "input_tokens"),
        output: u(usage, "output_tokens"),
        cache_read: u(usage, "cache_read_input_tokens"),
        cache_write_5m: w5,
        cache_write_1h: w1,
        reasoning,
        cost_usd: None,
        cost_source: CostSource::Unknown,
        dedup_key: id.to_string(),
    })
}

/// Parses complete lines from `from_offset`. A partial last line is left
/// unread: `next_offset` points just past the last newline.
pub fn parse_claude_file(path: &Path, from_offset: u64) -> std::io::Result<ParseOutput> {
    let mut file = std::fs::File::open(path)?;
    file.seek(SeekFrom::Start(from_offset))?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)?;

    let Some(last_nl) = buf.iter().rposition(|b| *b == b'\n') else {
        return Ok(ParseOutput {
            events: Vec::new(),
            next_offset: from_offset,
        });
    };
    let complete = &buf[..=last_nl];
    let events = complete
        .split(|b| *b == b'\n')
        .filter(|l| !l.is_empty())
        .filter_map(|l| serde_json::from_slice::<Value>(l).ok())
        .filter_map(|v| parse_line(&v))
        .collect();
    Ok(ParseOutput {
        events,
        next_offset: from_offset + last_nl as u64 + 1,
    })
}

/// Claude Code session logs under `~/.claude/projects`.
pub struct ClaudeSource;

impl super::FileSource for ClaudeSource {
    fn tool(&self) -> Tool {
        Tool::Claude
    }

    fn discover(&self, home: &Path) -> Vec<PathBuf> {
        discover_claude_files(home)
    }

    fn parse(&self, path: &Path, from_offset: u64) -> std::io::Result<ParseOutput> {
        let out = parse_claude_file(path, from_offset)?;
        Ok(ParseOutput {
            events: dedupe(out.events),
            next_offset: out.next_offset,
        })
    }
}

/// Global dedup on `dedup_key` (message id). Keeps the copy with the
/// highest `output`, because streamed copies grow as output arrives.
pub fn dedupe(events: Vec<UsageEvent>) -> Vec<UsageEvent> {
    let mut best: HashMap<String, UsageEvent> = HashMap::new();
    for e in events {
        match best.get(&e.dedup_key) {
            Some(cur) if cur.output >= e.output => {}
            _ => {
                best.insert(e.dedup_key.clone(), e);
            }
        }
    }
    let mut out: Vec<UsageEvent> = best.into_values().collect();
    out.sort_by(|a, b| {
        a.ts_ms
            .cmp(&b.ts_ms)
            .then_with(|| a.dedup_key.cmp(&b.dedup_key))
    });
    out
}

/// Session files directly under a project dir, plus `subagents/agent-*.jsonl`.
pub fn discover_claude_files(home: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let Ok(projects) = std::fs::read_dir(home.join(".claude/projects")) else {
        return files;
    };
    for project in projects.flatten().map(|e| e.path()).filter(|p| p.is_dir()) {
        let Ok(entries) = std::fs::read_dir(&project) else {
            continue;
        };
        for entry in entries.flatten().map(|e| e.path()) {
            if entry.is_file() && entry.extension().is_some_and(|x| x == "jsonl") {
                files.push(entry);
            } else if entry.is_dir() {
                let Ok(subs) = std::fs::read_dir(entry.join("subagents")) else {
                    continue;
                };
                for sub in subs.flatten().map(|e| e.path()) {
                    let is_agent = sub
                        .file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with("agent-") && n.ends_with(".jsonl"));
                    if is_agent {
                        files.push(sub);
                    }
                }
            }
        }
    }
    files.sort();
    files
}
