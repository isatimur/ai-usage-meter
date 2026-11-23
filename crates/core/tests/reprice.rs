use aium_core::aggregate::Range;
use aium_core::cache::Cache;
use aium_core::pricing::PriceTable;
use std::path::{Path, PathBuf};

fn table(input: f64) -> PriceTable {
    PriceTable::from_json_str(&format!(
        r#"{{"claude-opus-4-5": {{
            "input_cost_per_token": {input},
            "output_cost_per_token": 15e-6,
            "cache_read_input_token_cost": 0.3e-6,
            "cache_creation_input_token_cost": 3.75e-6,
            "cache_creation_input_token_cost_above_1hr": 6e-6
        }}}}"#
    ))
    .unwrap()
}

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/claude/a.jsonl")
}

fn month_cost(cache: &Cache) -> f64 {
    let now = "2026-11-10T12:00:00Z"
        .parse::<jiff::Timestamp>()
        .unwrap()
        .as_millisecond();
    cache.summary_at(Range::Month, now).total.cost_usd
}

#[test]
fn a_changed_price_reprices_events_already_in_the_cache() {
    let home = std::env::temp_dir().join(format!("aium-reprice-{}", std::process::id()));
    std::fs::remove_dir_all(&home).ok();
    let dir = home.join(".claude/projects/-w-proj");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(fixture(), dir.join("a.jsonl")).unwrap();

    let mut cache = Cache::open(&home.join("cache.db"), "UTC").unwrap();
    cache.refresh(&home, &table(3e-6)).unwrap();
    let before = month_cost(&cache);
    assert!(before > 0.0);

    // The user doubles the input price. No new log lines arrive.
    cache.refresh(&home, &table(6e-6)).unwrap();
    let after = month_cost(&cache);
    assert!(after > before, "cost stayed {before} after a price change");

    // Same table again: nothing moves.
    cache.refresh(&home, &table(6e-6)).unwrap();
    assert_eq!(month_cost(&cache), after);
}
