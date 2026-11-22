use aium_core::parsers::opencode::read_opencode;
use aium_core::CostSource;
use rusqlite::Connection;
use std::path::PathBuf;

fn make_db(name: &str, wal: bool) -> (PathBuf, Connection) {
    let dir = std::env::temp_dir().join(format!("aium-opencode-{}-{}", name, std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("opencode.db");
    std::fs::remove_file(&path).ok();
    let conn = Connection::open(&path).unwrap();
    if wal {
        conn.pragma_update(None, "journal_mode", "WAL").unwrap();
    }
    conn.execute_batch(
        "CREATE TABLE message (id TEXT PRIMARY KEY, session_id TEXT NOT NULL, time_created INTEGER NOT NULL, time_updated INTEGER NOT NULL, data TEXT NOT NULL);
         CREATE TABLE session (id TEXT PRIMARY KEY, cost REAL, tokens_input INTEGER);
         INSERT INTO session VALUES ('s1', 99.0, 999999);",
    )
    .unwrap();
    let rows = [
        (
            "m1",
            1000,
            1000,
            r#"{"role":"assistant","modelID":"claude-x","providerID":"anthropic","path":{"cwd":"/p"},"cost":0.5,"time":{"created":1000},"tokens":{"input":100,"output":20,"reasoning":5,"cache":{"read":1000,"write":50}}}"#,
        ),
        (
            "m2",
            1100,
            1100,
            r#"{"role":"user","time":{"created":1100}}"#,
        ),
        (
            "m3",
            1200,
            1200,
            r#"{"role":"assistant","modelID":"free-model","providerID":"openrouter","path":{"cwd":"/q"},"cost":0,"time":{"created":1200},"tokens":{"input":10,"output":1,"reasoning":0,"cache":{"read":0,"write":0}}}"#,
        ),
        (
            "m4",
            4000,
            5000,
            r#"{"role":"assistant","modelID":"claude-x","providerID":"anthropic","path":{"cwd":"/p"},"cost":0.25,"time":{"created":4000},"tokens":{"input":1,"output":2,"reasoning":0,"cache":{"read":0,"write":0}}}"#,
        ),
    ];
    for (id, created, updated, data) in rows {
        conn.execute(
            "INSERT INTO message VALUES (?1, 's1', ?2, ?3, ?4)",
            rusqlite::params![id, created, updated, data],
        )
        .unwrap();
    }
    (path, conn)
}

#[test]
fn reads_assistant_messages_only() {
    let (path, _conn) = make_db("assistant", false);
    let ev = read_opencode(&path, 0).unwrap();
    let mut keys: Vec<_> = ev.iter().map(|e| e.dedup_key.as_str()).collect();
    keys.sort();
    assert_eq!(keys, ["m1", "m3", "m4"]);
}

#[test]
fn maps_fields_and_uses_stored_cost() {
    let (path, _conn) = make_db("fields", false);
    let ev = read_opencode(&path, 0).unwrap();
    let m1 = ev.iter().find(|e| e.dedup_key == "m1").unwrap();
    assert_eq!(m1.model, "claude-x");
    assert_eq!(m1.project, "/p");
    assert_eq!(m1.session, "s1");
    assert_eq!((m1.input, m1.output, m1.reasoning), (100, 20, 5));
    assert_eq!((m1.cache_read, m1.cache_write_5m), (1000, 50));
    assert_eq!(m1.ts_ms, 1000);
    assert_eq!(m1.cost_usd, Some(0.5));
    assert_eq!(m1.cost_source, CostSource::Stored);
    let m3 = ev.iter().find(|e| e.dedup_key == "m3").unwrap();
    assert_eq!(m3.cost_usd, None);
    assert_eq!(m3.cost_source, CostSource::Unknown);
}

#[test]
fn does_not_add_session_totals() {
    let (path, _conn) = make_db("totals", false);
    let ev = read_opencode(&path, 0).unwrap();
    let cost: f64 = ev.iter().filter_map(|e| e.cost_usd).sum();
    assert!((cost - 0.75).abs() < 1e-9, "cost was {cost}");
    assert_eq!(ev.iter().map(|e| e.input).sum::<u64>(), 111);
}

#[test]
fn since_ms_filters_on_update_time() {
    let (path, _conn) = make_db("since", false);
    let ev = read_opencode(&path, 2000).unwrap();
    assert_eq!(ev.len(), 1);
    assert_eq!(ev[0].dedup_key, "m4");
}

#[test]
fn works_with_wal_file_present() {
    let (path, _conn) = make_db("wal", true);
    let wal = PathBuf::from(format!("{}-wal", path.display()));
    assert!(wal.exists(), "test setup should leave a -wal file");
    let ev = read_opencode(&path, 0).unwrap();
    assert_eq!(ev.len(), 3);
}
