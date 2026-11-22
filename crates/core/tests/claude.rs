use aium_core::parsers::claude::{dedupe, discover_claude_files, parse_claude_file};
use aium_core::UsageEvent;
use std::io::Write;
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/claude")
        .join(name)
}

fn parse_all(names: &[&str]) -> Vec<UsageEvent> {
    let mut all = Vec::new();
    for n in names {
        all.extend(parse_claude_file(&fixture(n), 0).unwrap().events);
    }
    dedupe(all)
}

fn by_key<'a>(events: &'a [UsageEvent], key: &str) -> Vec<&'a UsageEvent> {
    events.iter().filter(|e| e.dedup_key == key).collect()
}

#[test]
fn repeated_message_id_counts_once() {
    let events = parse_all(&["a.jsonl"]);
    let m1 = by_key(&events, "msg_1");
    assert_eq!(m1.len(), 1);
    assert_eq!(m1[0].output, 40);
    assert_eq!(events.len(), 2);
}

#[test]
fn same_session_in_two_files_counts_once() {
    let events = parse_all(&["a.jsonl", "b.jsonl"]);
    assert_eq!(by_key(&events, "msg_1").len(), 1);
    assert_eq!(events.len(), 2);
}

#[test]
fn synthetic_model_skipped() {
    let events = parse_all(&["a.jsonl"]);
    assert!(events.iter().all(|e| e.model != "<synthetic>"));
}

#[test]
fn sidechain_lines_included() {
    let events = parse_all(&["a.jsonl"]);
    let m2 = by_key(&events, "msg_2");
    assert_eq!(m2.len(), 1);
    assert_eq!(m2[0].input, 50);
}

#[test]
fn cache_write_split_by_ttl() {
    let events = parse_all(&["a.jsonl"]);
    let m1 = by_key(&events, "msg_1")[0];
    assert_eq!(m1.cache_write_5m, 200);
    assert_eq!(m1.cache_write_1h, 100);
    assert_eq!(m1.cache_read, 1000);
    assert_eq!(m1.input, 100);
}

#[test]
fn cwd_taken_from_line() {
    let events = parse_all(&["a.jsonl"]);
    assert_eq!(by_key(&events, "msg_1")[0].project, "/work/proj");
    assert_eq!(by_key(&events, "msg_2")[0].project, "/work/other");
}

#[test]
fn timestamps_are_epoch_millis() {
    let events = parse_all(&["a.jsonl"]);
    let t1 = by_key(&events, "msg_1")[0].ts_ms;
    let t2 = by_key(&events, "msg_2")[0].ts_ms;
    assert_eq!(t2 - t1, 3_600_000 - 1_000);
}

#[test]
fn partial_last_line_keeps_offset() {
    let dir = std::env::temp_dir().join(format!("aium-claude-partial-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("s.jsonl");
    let text = std::fs::read_to_string(fixture("a.jsonl")).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    let first = format!("{}\n", lines[5]);
    let second = lines[1];
    let (head, tail) = second.split_at(second.len() / 2);
    std::fs::write(&path, format!("{first}{head}")).unwrap();

    let out = parse_claude_file(&path, 0).unwrap();
    assert_eq!(out.events.len(), 1);
    assert_eq!(out.next_offset, first.len() as u64);

    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    writeln!(f, "{tail}").unwrap();
    let out2 = parse_claude_file(&path, out.next_offset).unwrap();
    assert_eq!(out2.events.len(), 1);
    assert_eq!(out2.events[0].dedup_key, "msg_1");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn discovers_main_and_subagent_files_only() {
    let home = std::env::temp_dir().join(format!("aium-claude-home-{}", std::process::id()));
    let proj = home.join(".claude/projects/-work-proj");
    std::fs::create_dir_all(proj.join("s1/subagents")).unwrap();
    std::fs::create_dir_all(proj.join("s1/tool-results")).unwrap();
    std::fs::write(proj.join("s1.jsonl"), "").unwrap();
    std::fs::write(proj.join("s1/subagents/agent-1.jsonl"), "").unwrap();
    std::fs::write(proj.join("s1/tool-results/x.jsonl"), "").unwrap();
    std::fs::write(proj.join("notes.txt"), "").unwrap();
    let files = discover_claude_files(&home);
    assert_eq!(files.len(), 2);
    std::fs::remove_dir_all(&home).ok();
}
