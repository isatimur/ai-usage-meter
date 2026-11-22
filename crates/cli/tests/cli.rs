use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixture_home(name: &str) -> PathBuf {
    let home = std::env::temp_dir().join(format!("aium-cli-{}-{}", name, std::process::id()));
    std::fs::remove_dir_all(&home).ok();
    let proj = home.join(".claude/projects/-work-proj");
    std::fs::create_dir_all(&proj).unwrap();
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../core/tests/fixtures/claude/a.jsonl");
    std::fs::copy(src, proj.join("a.jsonl")).unwrap();
    home
}

fn run(home: &Path, extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_aium"))
        .args(["summary", "--home"])
        .arg(home)
        .args(["--tz", "UTC", "--now", "2026-11-10T12:00:00Z"])
        .args(extra)
        .output()
        .unwrap()
}

#[test]
fn json_summary_matches_fixture_totals() {
    let home = fixture_home("json");
    let out = run(&home, &["--json", "--range", "month"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let t = &v["total"];
    assert_eq!(t["input"], 150);
    assert_eq!(t["output"], 45);
    assert_eq!(t["cache_read"], 1000);
    assert_eq!(t["cache_write"], 300);
    assert_eq!(t["sessions"], 1);
    assert!(t["cost_usd"].as_f64().unwrap() > 0.0);
    assert_eq!(v["cost_incomplete"], false);
}

#[test]
fn table_output_lists_the_tool() {
    let home = fixture_home("table");
    let out = run(&home, &[]);
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("Claude"), "{text}");
    assert!(text.contains("claude-opus-4-5"), "{text}");
}

#[test]
fn unknown_range_fails() {
    let home = fixture_home("badrange");
    let out = run(&home, &["--range", "decade"]);
    assert!(!out.status.success());
}

#[test]
fn oneline_is_a_single_status_line() {
    let home = fixture_home("oneline");
    let out = run(&home, &["--oneline", "--range", "month"]);
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert_eq!(text.lines().count(), 1, "{text}");
    assert!(text.starts_with("month $"), "{text}");
    assert!(text.contains("· claude $"), "{text}");
}
