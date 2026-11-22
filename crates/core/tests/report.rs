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

fn cache(name: &str) -> Cache {
    let home = std::env::temp_dir().join(format!("aium-rep-{}-{}", name, std::process::id()));
    std::fs::remove_dir_all(&home).ok();
    let dir = home.join(".claude/projects/-w-proj");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(fixture("claude", "a.jsonl"), dir.join("a.jsonl")).unwrap();
    let mut c = Cache::open(&home.join("cache.db"), "UTC").unwrap();
    c.refresh(&home, &PriceTable::from_json_str(TABLE).unwrap())
        .unwrap();
    c
}

#[test]
fn daily_note_has_date_totals_and_session_table() {
    let note = cache("note").daily_note_at(ms("2026-11-10T12:00:00Z"));
    assert!(note.starts_with("---\ndate: 2026-11-10\n"), "{note}");
    assert!(note.contains("# AI tool usage, 2026-11-10"), "{note}");
    assert!(note.contains("estimated at API prices"), "{note}");
    assert!(note.contains("| Claude | 1 | 45 |"), "{note}");
    assert!(note.contains("| claude-opus-4-5 |"), "{note}");
    assert!(note.contains("## Sessions"), "{note}");
    assert!(
        note.contains("| proj | Claude | claude-opus-4-5 | 11:00 |"),
        "{note}"
    );
}

#[test]
fn daily_note_for_a_quiet_day_says_so() {
    let note = cache("quiet").daily_note_at(ms("2026-11-12T12:00:00Z"));
    assert!(note.contains("No sessions on this day."), "{note}");
}
