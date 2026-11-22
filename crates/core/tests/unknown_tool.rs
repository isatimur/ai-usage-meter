//! A cache row from a tool this build does not know must never be re-priced or counted as another tool.

use aium_core::cache::Cache;
use aium_core::pricing::PriceTable;

const TABLE: &str = r#"{
  "claude-opus-4-5": { "input_cost_per_token": 3e-6, "output_cost_per_token": 15e-6 }
}"#;

#[test]
fn row_from_unknown_tool_is_left_alone() {
    let dir = std::env::temp_dir().join(format!("aium-unknown-tool-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    let db = dir.join("cache.db");

    // Create the schema, then add a row as a future version with another tool would.
    drop(Cache::open(&db, "UTC").unwrap());
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute(
        "INSERT INTO events (tool, dedup_key, ts_ms, day, model, project, session, input, output, \
         cache_read, cache_write_5m, cache_write_1h, reasoning, cost_usd, cost_source) \
         VALUES ('FutureTool', 'k1', 1700000000000, '2023-11-14', 'claude-opus-4-5', '', 's', \
         1000, 1000, 0, 0, 0, 0, NULL, 'Unknown')",
        [],
    )
    .unwrap();
    drop(conn);

    let mut cache = Cache::open(&db, "UTC").unwrap();
    let prices = PriceTable::from_json_str(TABLE).unwrap();
    cache.refresh(&dir, &prices).unwrap();

    let conn = rusqlite::Connection::open(&db).unwrap();
    let (source, tool): (String, String) = conn
        .query_row(
            "SELECT cost_source, tool FROM events WHERE dedup_key = 'k1'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(tool, "FutureTool");
    assert_eq!(
        source, "Unknown",
        "an unknown tool's row must not be priced as Claude"
    );
}
