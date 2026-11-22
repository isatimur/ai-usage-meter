use aium_core::aggregate::Range;
use aium_core::cache::Cache;
use aium_core::pricing::PriceTable;
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

fn fixture(kind: &str, name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(kind)
        .join(name)
}

fn ms(iso: &str) -> i64 {
    iso.parse::<jiff::Timestamp>().unwrap().as_millisecond()
}

fn cache_with_claude_fixture(name: &str) -> Cache {
    let home = std::env::temp_dir().join(format!("aium-sess-{}-{}", name, std::process::id()));
    std::fs::remove_dir_all(&home).ok();
    let dir = home.join(".claude/projects/-w-proj");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(fixture("claude", "a.jsonl"), dir.join("a.jsonl")).unwrap();
    let mut cache = Cache::open(&home.join("cache.db"), "UTC").unwrap();
    cache
        .refresh(&home, &PriceTable::from_json_str(TABLE).unwrap())
        .unwrap();
    cache
}

#[test]
fn session_row_sums_events_and_names_project_and_model() {
    let cache = cache_with_claude_fixture("sum");
    let list = cache.sessions_at(Range::Today, ms("2026-11-10T12:00:00Z"), 50);
    assert_eq!(list.len(), 1);
    let s = &list[0];
    assert_eq!(s.tool, "Claude");
    assert_eq!(s.session, "s1");
    assert_eq!(s.project, "/work/proj");
    assert_eq!(s.model, "claude-opus-4-5");
    // msg_1 repeats at :00, :01, :02; the cache keeps the row with the most output (:01).
    assert_eq!(s.first_ts_ms, ms("2026-11-10T10:00:01Z"));
    assert_eq!(s.last_ts_ms, ms("2026-11-10T11:00:00Z"));
    assert_eq!(s.totals.output, 45);
    assert!(s.totals.cost_usd > 0.0);
}

#[test]
fn session_is_active_only_within_ten_minutes_of_last_event() {
    let cache = cache_with_claude_fixture("active");
    let quiet = cache.sessions_at(Range::Today, ms("2026-11-10T11:30:00Z"), 50);
    assert!(!quiet[0].active);
    let live = cache.sessions_at(Range::Today, ms("2026-11-10T11:05:00Z"), 50);
    assert!(live[0].active);
}

#[test]
fn limit_keeps_the_newest_sessions() {
    let cache = cache_with_claude_fixture("limit");
    assert_eq!(
        cache
            .sessions_at(Range::Today, ms("2026-11-10T12:00:00Z"), 0)
            .len(),
        0
    );
}
