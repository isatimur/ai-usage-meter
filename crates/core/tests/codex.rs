use aium_core::parsers::codex::{codex_thread_fallback, parse_codex_file};
use aium_core::{CostSource, UsageEvent};
use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/codex")
        .join(name)
}

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("aium-codex-{}-{}", name, std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn events(name: &str) -> Vec<UsageEvent> {
    parse_codex_file(&fixture(name), 0).unwrap().events
}

#[test]
fn sums_last_token_usage_not_total() {
    let ev = events("r1.jsonl");
    assert_eq!(ev.iter().map(|e| e.output).sum::<u64>(), 150);
    assert_eq!(ev.iter().map(|e| e.input).sum::<u64>(), 200 + 500);
}

#[test]
fn repeated_identical_token_count_counts_once() {
    assert_eq!(events("r1.jsonl").len(), 2);
}

#[test]
fn cached_input_subtracted() {
    let ev = events("r1.jsonl");
    assert_eq!(ev[0].input, 200);
    assert_eq!(ev[0].cache_read, 800);
    assert_eq!(ev[0].reasoning, 40);
}

#[test]
fn model_and_project_from_context() {
    let ev = events("r1.jsonl");
    assert_eq!(ev[0].model, "gpt-5.4");
    assert_eq!(ev[0].project, "/w/codex");
    assert_eq!(ev[0].session, "t1");
}

#[test]
fn token_usage_record_dedupes_on_response_id() {
    let ev = events("r2.jsonl");
    assert_eq!(ev.len(), 2);
    let mut keys: Vec<_> = ev.iter().map(|e| e.dedup_key.as_str()).collect();
    keys.sort();
    assert_eq!(keys, ["resp_1", "resp_2"]);
    assert_eq!(ev.iter().map(|e| e.input).sum::<u64>(), 1000 + 400);
}

#[test]
fn reads_zst_archive() {
    let dir = tmp("zst");
    let path = dir.join("rollout.jsonl.zst");
    let raw = std::fs::read(fixture("r1.jsonl")).unwrap();
    std::fs::write(&path, zstd::encode_all(&raw[..], 3).unwrap()).unwrap();
    let ev = parse_codex_file(&path, 0).unwrap().events;
    assert_eq!(ev.len(), 2);
    assert_eq!(ev[0].input, 200);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn offset_parse_emits_only_new_lines_but_keeps_context() {
    let dir = tmp("offset");
    let path = dir.join("r.jsonl");
    std::fs::copy(fixture("r1.jsonl"), &path).unwrap();
    let first = parse_codex_file(&path, 0).unwrap();
    assert_eq!(first.events.len(), 2);

    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    writeln!(f, r#"{{"timestamp":"2026-11-10T09:00:09.000Z","type":"event_msg","payload":{{"type":"token_count","info":{{"total_token_usage":{{"input_tokens":3300,"cached_input_tokens":2300,"cache_write_input_tokens":0,"output_tokens":160,"reasoning_output_tokens":50,"total_tokens":3460}},"last_token_usage":{{"input_tokens":300,"cached_input_tokens":0,"cache_write_input_tokens":0,"output_tokens":10,"reasoning_output_tokens":0,"total_tokens":310}}}}}}}}"#).unwrap();
    let second = parse_codex_file(&path, first.next_offset).unwrap();
    assert_eq!(second.events.len(), 1);
    assert_eq!(second.events[0].model, "gpt-5.4");
    assert_eq!(second.events[0].input, 300);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn fallback_only_for_missing_rollouts() {
    let dir = tmp("db");
    let db = dir.join("state_5.sqlite");
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute_batch(
        "CREATE TABLE threads (id TEXT, cwd TEXT, model TEXT, created_at_ms INTEGER, tokens_used INTEGER);
         INSERT INTO threads VALUES ('t1', '/a', 'gpt-5.4', 1700000000000, 111);
         INSERT INTO threads VALUES ('t2', '/b', 'gpt-5.5', 1700000001000, 5000);
         INSERT INTO threads VALUES ('t3', '/c', 'gpt-5.5', 1700000002000, 0);",
    )
    .unwrap();
    drop(conn);
    let known: HashSet<String> = ["t1".to_string()].into();
    let ev = codex_thread_fallback(&db, &known).unwrap();
    assert_eq!(ev.len(), 1);
    assert_eq!(ev[0].session, "t2");
    assert_eq!(ev[0].input, 5000);
    assert_eq!(ev[0].cost_source, CostSource::Unknown);
    assert_eq!(ev[0].project, "/b");
    std::fs::remove_dir_all(&dir).ok();
}
