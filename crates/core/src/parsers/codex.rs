use super::ParseOutput;
use crate::{CostSource, Tool, UsageEvent};
use serde_json::Value;
use std::collections::HashSet;
use std::path::Path;
use std::str::FromStr;

fn u(v: &Value, key: &str) -> u64 {
    v.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

fn is_relevant(line: &[u8]) -> bool {
    [
        &b"session_meta"[..],
        b"turn_context",
        b"token_count",
        b"token_usage_record",
    ]
    .iter()
    .any(|n| contains(line, n))
}

struct Ctx {
    session: String,
    project: String,
    model: String,
}

fn make_event(ctx: &Ctx, line: &Value, usage: &Value, key: String) -> Option<UsageEvent> {
    let ts_ms = jiff::Timestamp::from_str(line.get("timestamp")?.as_str()?)
        .ok()?
        .as_millisecond();
    let cached = u(usage, "cached_input_tokens");
    Some(UsageEvent {
        tool: Tool::Codex,
        ts_ms,
        model: if ctx.model.is_empty() {
            "unknown".into()
        } else {
            ctx.model.clone()
        },
        project: ctx.project.clone(),
        session: ctx.session.clone(),
        input: u(usage, "input_tokens").saturating_sub(cached),
        output: u(usage, "output_tokens"),
        cache_read: cached,
        cache_write_5m: u(usage, "cache_write_input_tokens"),
        cache_write_1h: 0,
        // reasoning is part of output (total_tokens = input + output); informational only
        reasoning: u(usage, "reasoning_output_tokens"),
        cost_usd: None,
        cost_source: CostSource::Unknown,
        dedup_key: key,
    })
}

/// Reads the whole file so model, cwd and session context are known, but
/// emits only events from lines that start at or after `from_offset`.
/// `.jsonl.zst` archives are decoded whole; their `next_offset` is the
/// compressed size.
pub fn parse_codex_file(path: &Path, from_offset: u64) -> std::io::Result<ParseOutput> {
    let raw = std::fs::read(path)?;
    let is_zst = path.extension().is_some_and(|e| e == "zst");
    let (data, from, zst_len) = if is_zst {
        (zstd::decode_all(&raw[..])?, 0usize, Some(raw.len() as u64))
    } else {
        (raw, from_offset as usize, None)
    };

    let complete_len = match zst_len {
        Some(_) => data.len(),
        None => match data.iter().rposition(|b| *b == b'\n') {
            Some(i) => i + 1,
            None => {
                return Ok(ParseOutput {
                    events: Vec::new(),
                    next_offset: from_offset,
                })
            }
        },
    };
    let next_offset = zst_len.unwrap_or(complete_len as u64);

    let mut relevant: Vec<(usize, Value)> = Vec::new();
    let mut pos = 0usize;
    for line in data[..complete_len].split(|b| *b == b'\n') {
        let start = pos;
        pos += line.len() + 1;
        if line.is_empty() || !is_relevant(line) {
            continue;
        }
        if let Ok(v) = serde_json::from_slice::<Value>(line) {
            relevant.push((start, v));
        }
    }

    let has_records = relevant
        .iter()
        .any(|(_, v)| v.get("type").and_then(Value::as_str) == Some("token_usage_record"));

    let mut ctx = Ctx {
        session: String::new(),
        project: String::new(),
        model: String::new(),
    };
    let mut prev_total: Option<Value> = None;
    let mut seen_responses: HashSet<String> = HashSet::new();
    let mut events = Vec::new();

    for (start, v) in &relevant {
        let payload = v.get("payload").unwrap_or(&Value::Null);
        match v.get("type").and_then(Value::as_str) {
            Some("session_meta") => {
                if let Some(id) = payload.get("id").and_then(Value::as_str) {
                    ctx.session = id.to_string();
                }
                if let Some(cwd) = payload.get("cwd").and_then(Value::as_str) {
                    ctx.project = cwd.to_string();
                }
            }
            Some("turn_context") => {
                if let Some(m) = payload.get("model").and_then(Value::as_str) {
                    ctx.model = m.to_string();
                }
                if let Some(cwd) = payload.get("cwd").and_then(Value::as_str) {
                    ctx.project = cwd.to_string();
                }
            }
            Some("event_msg")
                if !has_records
                    && payload.get("type").and_then(Value::as_str) == Some("token_count") =>
            {
                let Some(info) = payload.get("info").filter(|i| !i.is_null()) else {
                    continue;
                };
                let (Some(total), Some(last)) =
                    (info.get("total_token_usage"), info.get("last_token_usage"))
                else {
                    continue;
                };
                if prev_total.as_ref() == Some(total) {
                    continue;
                }
                prev_total = Some(total.clone());
                if *start >= from {
                    let key = format!("{}:{}", ctx.session, u(total, "total_tokens"));
                    events.extend(make_event(&ctx, v, last, key));
                }
            }
            Some("token_usage_record") => {
                let Some(id) = payload.get("response_id").and_then(Value::as_str) else {
                    continue;
                };
                if !seen_responses.insert(id.to_string()) {
                    continue;
                }
                if *start >= from {
                    if let Some(usage) = payload.get("usage") {
                        events.extend(make_event(&ctx, v, usage, id.to_string()));
                    }
                }
            }
            _ => {}
        }
    }
    Ok(ParseOutput {
        events,
        next_offset,
    })
}

fn collect_rollouts(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for path in entries.flatten().map(|e| e.path()) {
        if path.is_dir() {
            collect_rollouts(&path, out);
        } else if path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("rollout-") && n.ends_with(".jsonl"))
        {
            out.push(path);
        }
    }
}

/// Codex rollout files: live ones plus zstd archives.
pub struct CodexSource;

impl super::FileSource for CodexSource {
    fn tool(&self) -> Tool {
        Tool::Codex
    }

    fn discover(&self, home: &Path) -> Vec<std::path::PathBuf> {
        discover_codex_files(home)
    }

    fn parse(&self, path: &Path, from_offset: u64) -> std::io::Result<ParseOutput> {
        parse_codex_file(path, from_offset)
    }
}

/// Live rollouts under `sessions/` plus zstd archives under `archived_sessions/`.
pub fn discover_codex_files(home: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    collect_rollouts(&home.join(".codex/sessions"), &mut files);
    if let Ok(entries) = std::fs::read_dir(home.join(".codex/archived_sessions")) {
        files.extend(
            entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.to_str().is_some_and(|s| s.ends_with(".jsonl.zst"))),
        );
    }
    files.sort();
    files
}

/// Approximate events for threads whose rollout file is gone: one event per
/// thread, `tokens_used` as input, cost unknown.
pub fn codex_thread_fallback(
    db: &Path,
    known_sessions: &HashSet<String>,
) -> rusqlite::Result<Vec<UsageEvent>> {
    let conn = rusqlite::Connection::open_with_flags(
        db,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    let mut stmt = conn.prepare(
        "SELECT id, cwd, model, created_at_ms, tokens_used FROM threads WHERE tokens_used > 0",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, Option<i64>>(3)?,
            r.get::<_, i64>(4)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, cwd, model, created, tokens) = row?;
        if known_sessions.contains(&id) {
            continue;
        }
        out.push(UsageEvent {
            tool: Tool::Codex,
            ts_ms: created.unwrap_or(0),
            model: model.unwrap_or_else(|| "unknown".into()),
            project: cwd.unwrap_or_default(),
            session: id.clone(),
            input: tokens.max(0) as u64,
            output: 0,
            cache_read: 0,
            cache_write_5m: 0,
            cache_write_1h: 0,
            reasoning: 0,
            cost_usd: None,
            cost_source: CostSource::Unknown,
            dedup_key: format!("thread:{id}"),
        });
    }
    Ok(out)
}
