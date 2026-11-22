use aium_core::aggregate::Range;
use aium_core::cache::Cache;
use aium_core::pricing::PriceTable;
use std::io::Write;
use std::path::{Path, PathBuf};

const TABLE: &str = r#"{
  "claude-opus-4-5": {
    "input_cost_per_token": 3e-6,
    "output_cost_per_token": 15e-6,
    "cache_read_input_token_cost": 0.3e-6,
    "cache_creation_input_token_cost": 3.75e-6,
    "cache_creation_input_token_cost_above_1hr": 6e-6
  }
}"#;

fn prices() -> PriceTable {
    PriceTable::from_json_str(TABLE).unwrap()
}

fn fixture(kind: &str, name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(kind)
        .join(name)
}

fn new_home(name: &str) -> PathBuf {
    let h = std::env::temp_dir().join(format!("aium-agg-{}-{}", name, std::process::id()));
    std::fs::remove_dir_all(&h).ok();
    std::fs::create_dir_all(h.join(".claude/projects/-w-proj")).unwrap();
    h
}

fn put_claude(home: &Path, file: &str) -> PathBuf {
    let dest = home.join(".claude/projects/-w-proj").join(file);
    std::fs::copy(fixture("claude", file), &dest).unwrap();
    dest
}

fn put_codex(home: &Path) {
    let dir = home.join(".codex/sessions/2026/11/10");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(
        fixture("codex", "r1.jsonl"),
        dir.join("rollout-2026-11-10T09-00-00-t1.jsonl"),
    )
    .unwrap();
}

fn open(home: &Path, tz: &str) -> Cache {
    Cache::open(&home.join("cache.db"), tz).unwrap()
}

fn ms(iso: &str) -> i64 {
    iso.parse::<jiff::Timestamp>().unwrap().as_millisecond()
}

const NOW: &str = "2026-11-10T12:00:00Z";

#[test]
fn second_refresh_reads_only_appended_bytes() {
    let home = new_home("append");
    let file = put_claude(&home, "a.jsonl");
    let size = std::fs::metadata(&file).unwrap().len();
    let mut cache = open(&home, "UTC");

    let s1 = cache.refresh(&home, &prices()).unwrap();
    assert_eq!(s1.bytes_read, size);
    assert_eq!(s1.events_added, 2);

    let s2 = cache.refresh(&home, &prices()).unwrap();
    assert_eq!(s2.bytes_read, 0);
    assert_eq!(s2.events_added, 0);

    let line = r#"{"type":"assistant","timestamp":"2026-11-10T11:30:00.000Z","sessionId":"s1","cwd":"/work/proj","message":{"id":"msg_3","model":"claude-opus-4-5","usage":{"input_tokens":7,"output_tokens":3}}}"#;
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&file)
        .unwrap();
    writeln!(f, "{line}").unwrap();
    let s3 = cache.refresh(&home, &prices()).unwrap();
    assert_eq!(s3.bytes_read, line.len() as u64 + 1);
    assert_eq!(s3.events_added, 1);
}

#[test]
fn duplicate_across_files_counted_once() {
    let home = new_home("dup");
    put_claude(&home, "a.jsonl");
    put_claude(&home, "b.jsonl");
    let mut cache = open(&home, "UTC");
    cache.refresh(&home, &prices()).unwrap();
    let s = cache.summary_at(Range::Month, ms(NOW));
    assert_eq!(s.total.output, 40 + 5);
    assert_eq!(s.total.input, 100 + 50);
}

#[test]
fn totals_and_breakdowns_are_correct() {
    let home = new_home("totals");
    put_claude(&home, "a.jsonl");
    let mut cache = open(&home, "UTC");
    cache.refresh(&home, &prices()).unwrap();
    let s = cache.summary_at(Range::Month, ms(NOW));
    assert_eq!(s.total.cache_read, 1000);
    assert_eq!(s.total.cache_write, 300);
    assert_eq!(s.total.sessions, 1);
    assert!(
        (s.total.cost_usd - 0.002775).abs() < 1e-9,
        "cost {}",
        s.total.cost_usd
    );
    assert_eq!(s.by_tool["Claude"].output, 45);
    assert_eq!(s.by_model["claude-opus-4-5"].input, 150);
    assert_eq!(s.by_project["/work/proj"].output, 40);
    assert_eq!(s.by_project["/work/other"].output, 5);
    assert!(!s.cost_incomplete);
}

#[test]
fn unknown_cost_sets_cost_incomplete() {
    let home = new_home("unknown");
    put_claude(&home, "a.jsonl");
    put_codex(&home);
    let mut cache = open(&home, "UTC");
    cache.refresh(&home, &prices()).unwrap();
    let s = cache.summary_at(Range::Month, ms(NOW));
    assert!(s.cost_incomplete);
    assert!(s.by_tool.contains_key("Codex"));
    assert_eq!(s.by_tool["Codex"].output, 150);
}

#[test]
fn tz_change_is_rejected() {
    let home = new_home("tz");
    drop(open(&home, "UTC"));
    let err = Cache::open(&home.join("cache.db"), "Europe/Berlin");
    assert!(err.is_err());
    assert!(Cache::open(&home.join("cache.db"), "UTC").is_ok());
}

#[test]
fn day_buckets_use_pinned_tz() {
    let home_utc = new_home("tz-utc");
    put_claude(&home_utc, "a.jsonl");
    let mut c = open(&home_utc, "UTC");
    c.refresh(&home_utc, &prices()).unwrap();
    let days: Vec<String> = c
        .summary_at(Range::Month, ms(NOW))
        .by_day
        .into_iter()
        .map(|d| d.0)
        .collect();
    assert_eq!(days, ["2026-11-10"]);

    let home_k = new_home("tz-kiri");
    put_claude(&home_k, "a.jsonl");
    let mut k = open(&home_k, "Pacific/Kiritimati");
    k.refresh(&home_k, &prices()).unwrap();
    let days: Vec<String> = k
        .summary_at(Range::Month, ms(NOW))
        .by_day
        .into_iter()
        .map(|d| d.0)
        .collect();
    assert_eq!(days, ["2026-11-11"]);
}

#[test]
fn partial_line_completed_on_next_refresh() {
    let home = new_home("partial");
    let text = std::fs::read_to_string(fixture("claude", "a.jsonl")).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    let path = home.join(".claude/projects/-w-proj/p.jsonl");
    let (head, tail) = lines[5].split_at(lines[5].len() / 2);
    std::fs::write(&path, format!("{}\n{}", lines[0], head)).unwrap();

    let mut cache = open(&home, "UTC");
    assert_eq!(cache.refresh(&home, &prices()).unwrap().events_added, 1);
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    writeln!(f, "{tail}").unwrap();
    assert_eq!(cache.refresh(&home, &prices()).unwrap().events_added, 1);
    let s = cache.summary_at(Range::Month, ms(NOW));
    assert_eq!(s.by_model["claude-opus-4-5"].output, 10 + 5);
}

#[test]
fn ranges_filter_by_day() {
    let home = new_home("ranges");
    let mk = |ts: &str, id: &str, out: u64| {
        format!(
            r#"{{"type":"assistant","timestamp":"{ts}","sessionId":"s","cwd":"/p","message":{{"id":"{id}","model":"claude-opus-4-5","usage":{{"input_tokens":0,"output_tokens":{out}}}}}}}"#
        )
    };
    let body = [
        mk("2026-11-10T08:00:00.000Z", "a", 1),
        mk("2026-11-05T08:00:00.000Z", "b", 2),
        mk("2026-11-02T08:00:00.000Z", "c", 4),
        mk("2026-10-31T08:00:00.000Z", "d", 8),
    ]
    .join("\n");
    std::fs::write(
        home.join(".claude/projects/-w-proj/r.jsonl"),
        format!("{body}\n"),
    )
    .unwrap();
    let mut cache = open(&home, "UTC");
    cache.refresh(&home, &prices()).unwrap();
    assert_eq!(cache.summary_at(Range::Today, ms(NOW)).total.output, 1);
    assert_eq!(cache.summary_at(Range::Week, ms(NOW)).total.output, 1 + 2);
    assert_eq!(
        cache.summary_at(Range::Month, ms(NOW)).total.output,
        1 + 2 + 4
    );
}

#[test]
fn codex_fallback_is_replaced_when_rollout_appears() {
    let home = new_home("fallback");
    let codex = home.join(".codex");
    std::fs::create_dir_all(&codex).unwrap();
    let conn = rusqlite::Connection::open(codex.join("state_5.sqlite")).unwrap();
    conn.execute_batch(
        "CREATE TABLE threads (id TEXT, cwd TEXT, model TEXT, created_at_ms INTEGER, tokens_used INTEGER);
         INSERT INTO threads VALUES ('t1', '/w/codex', 'gpt-5.4', 1783900000000, 9999);",
    )
    .unwrap();
    drop(conn);
    let mut cache = open(&home, "UTC");
    cache.refresh(&home, &prices()).unwrap();
    assert_eq!(
        cache
            .summary_at(Range::Month, 1783900000000 + 1000)
            .total
            .input,
        9999
    );

    put_codex(&home);
    cache.refresh(&home, &prices()).unwrap();
    let s = cache.summary_at(Range::Month, ms(NOW));
    assert_eq!(s.by_tool["Codex"].output, 150);
    assert_eq!(s.by_tool["Codex"].input, 700);
}

#[test]
fn pinned_open_keeps_stored_tz_instead_of_failing() {
    let home = new_home("pinned");
    put_claude(&home, "a.jsonl");
    let mut c = open(&home, "UTC");
    c.refresh(&home, &prices()).unwrap();
    drop(c);
    let mut again = Cache::open_pinned(&home.join("cache.db"), "Pacific/Kiritimati").unwrap();
    again.refresh(&home, &prices()).unwrap();
    let days: Vec<String> = again
        .summary_at(Range::Month, ms(NOW))
        .by_day
        .into_iter()
        .map(|d| d.0)
        .collect();
    assert_eq!(days, ["2026-11-10"]);
}

#[test]
fn bad_file_does_not_stop_refresh() {
    let home = new_home("badfile");
    put_claude(&home, "a.jsonl");
    let arch = home.join(".codex/archived_sessions");
    std::fs::create_dir_all(&arch).unwrap();
    std::fs::write(arch.join("rollout-x.jsonl.zst"), b"this is not zstd").unwrap();
    let mut cache = open(&home, "UTC");
    let stats = cache.refresh(&home, &prices()).unwrap();
    assert_eq!(stats.events_added, 2);
    assert_eq!(stats.errors.len(), 1, "{:?}", stats.errors);
    assert_eq!(cache.summary_at(Range::Month, ms(NOW)).total.output, 45);
}

#[test]
fn unpriced_buckets_are_marked() {
    let home = new_home("unpriced");
    put_claude(&home, "a.jsonl");
    put_codex(&home);
    let mut cache = open(&home, "UTC");
    cache.refresh(&home, &prices()).unwrap();
    let s = cache.summary_at(Range::Month, ms(NOW));
    assert!(s.total.cost_incomplete);
    assert!(s.by_tool["Codex"].cost_incomplete);
    assert!(!s.by_tool["Claude"].cost_incomplete);
    assert!(s.by_model["gpt-5.4"].cost_incomplete);
    assert!(!s.by_model["claude-opus-4-5"].cost_incomplete);
}
