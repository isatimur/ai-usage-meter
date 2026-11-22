use aium_core::pricing::PriceTable;
use aium_core::{CostSource, Tool, UsageEvent};

const TABLE: &str = r#"{
  "claude-x": {
    "input_cost_per_token": 3e-6,
    "output_cost_per_token": 15e-6,
    "cache_read_input_token_cost": 0.3e-6,
    "cache_creation_input_token_cost": 3.75e-6,
    "cache_creation_input_token_cost_above_1hr": 6e-6
  },
  "no-cache-model": {
    "input_cost_per_token": 1e-6,
    "output_cost_per_token": 2e-6
  }
}"#;

fn ev(model: &str) -> UsageEvent {
    UsageEvent {
        tool: Tool::Claude,
        ts_ms: 0,
        model: model.into(),
        project: String::new(),
        session: String::new(),
        input: 0,
        output: 0,
        cache_read: 0,
        cache_write_5m: 0,
        cache_write_1h: 0,
        reasoning: 0,
        cost_usd: None,
        cost_source: CostSource::Unknown,
        dedup_key: String::new(),
    }
}

fn close(a: Option<f64>, b: f64) {
    let a = a.expect("cost should be known");
    assert!((a - b).abs() < 1e-9, "{a} != {b}");
}

#[test]
fn prices_known_model() {
    let t = PriceTable::from_json_str(TABLE).unwrap();
    let mut e = ev("claude-x");
    e.input = 1_000_000;
    t.price(&mut e);
    close(e.cost_usd, 3.0);
    assert_eq!(e.cost_source, CostSource::Computed);

    let mut e = ev("claude-x");
    e.output = 1_000_000;
    e.cache_read = 1_000_000;
    e.cache_write_5m = 1_000_000;
    e.cache_write_1h = 1_000_000;
    t.price(&mut e);
    close(e.cost_usd, 15.0 + 0.3 + 3.75 + 6.0);
}

#[test]
fn unknown_model_stays_unknown() {
    let t = PriceTable::from_json_str(TABLE).unwrap();
    let mut e = ev("mystery-model");
    e.input = 1000;
    t.price(&mut e);
    assert_eq!(e.cost_usd, None);
    assert_eq!(e.cost_source, CostSource::Unknown);
}

#[test]
fn stored_cost_is_not_overwritten() {
    let t = PriceTable::from_json_str(TABLE).unwrap();
    let mut e = ev("claude-x");
    e.input = 1_000_000;
    e.cost_usd = Some(9.99);
    e.cost_source = CostSource::Stored;
    t.price(&mut e);
    assert_eq!(e.cost_usd, Some(9.99));
    assert_eq!(e.cost_source, CostSource::Stored);
}

#[test]
fn provider_prefix_is_stripped() {
    let t = PriceTable::from_json_str(TABLE).unwrap();
    let mut e = ev("anthropic/claude-x");
    e.input = 1_000_000;
    t.price(&mut e);
    close(e.cost_usd, 3.0);
}

#[test]
fn missing_component_price_is_unknown() {
    let t = PriceTable::from_json_str(TABLE).unwrap();
    let mut e = ev("no-cache-model");
    e.input = 1000;
    e.cache_read = 1000;
    t.price(&mut e);
    assert_eq!(e.cost_usd, None);
    assert_eq!(e.cost_source, CostSource::Unknown);
}

#[test]
fn override_file_wins() {
    let path = std::env::temp_dir().join(format!("aium-override-{}.json", std::process::id()));
    std::fs::write(
        &path,
        r#"{"claude-x": {"input_cost_per_token": 1e-5, "output_cost_per_token": 1e-5}}"#,
    )
    .unwrap();
    let t = PriceTable::from_json_str(TABLE)
        .unwrap()
        .with_overrides(&path)
        .unwrap();
    std::fs::remove_file(&path).ok();
    let mut e = ev("claude-x");
    e.input = 1_000_000;
    t.price(&mut e);
    close(e.cost_usd, 10.0);
}

#[test]
fn bundled_table_prices_a_real_model() {
    let t = PriceTable::bundled();
    let mut e = ev("claude-opus-4-5");
    e.input = 1_000_000;
    t.price(&mut e);
    assert!(e.cost_usd.unwrap() > 0.0);
}
