use crate::{CostSource, Tool, UsageEvent};
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;
use std::path::Path;

fn u(v: &Value, pointer: &str) -> u64 {
    v.pointer(pointer).and_then(Value::as_u64).unwrap_or(0)
}

/// Reads assistant `message` rows only. Session and part totals overlap with
/// messages, so they are never read. `since_ms` filters on `time_updated`,
/// because a message grows while it streams.
pub fn read_opencode(db: &Path, since_ms: i64) -> rusqlite::Result<Vec<UsageEvent>> {
    let conn = Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let mut stmt = conn.prepare(
        "SELECT id, session_id, time_created, data FROM message \
         WHERE time_updated >= ?1 AND json_extract(data, '$.role') = 'assistant'",
    )?;
    let rows = stmt.query_map([since_ms], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
            r.get::<_, String>(3)?,
        ))
    })?;

    let mut out = Vec::new();
    for row in rows {
        let (id, session, created, data) = row?;
        let Ok(v) = serde_json::from_str::<Value>(&data) else {
            continue;
        };
        let cost = v.get("cost").and_then(Value::as_f64).unwrap_or(0.0);
        let (cost_usd, cost_source) = if cost > 0.0 {
            (Some(cost), CostSource::Stored)
        } else {
            (None, CostSource::Unknown)
        };
        out.push(UsageEvent {
            tool: Tool::OpenCode,
            ts_ms: v
                .pointer("/time/created")
                .and_then(Value::as_i64)
                .unwrap_or(created),
            model: v
                .get("modelID")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string(),
            project: v
                .pointer("/path/cwd")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            session,
            input: u(&v, "/tokens/input"),
            output: u(&v, "/tokens/output"),
            cache_read: u(&v, "/tokens/cache/read"),
            cache_write_5m: u(&v, "/tokens/cache/write"),
            cache_write_1h: 0,
            reasoning: u(&v, "/tokens/reasoning"),
            cost_usd,
            cost_source,
            dedup_key: id,
        });
    }
    Ok(out)
}
