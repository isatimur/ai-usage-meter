use aium_core::privacy::{check, State};
use std::path::PathBuf;

fn home(name: &str) -> PathBuf {
    let h = std::env::temp_dir().join(format!("aium-privacy-{}-{}", name, std::process::id()));
    std::fs::remove_dir_all(&h).ok();
    std::fs::create_dir_all(h.join(".claude")).unwrap();
    std::fs::create_dir_all(h.join(".codex")).unwrap();
    h
}

fn no_env(_: &str) -> Option<String> {
    None
}

fn state(findings: &[aium_core::privacy::Finding], setting: &str) -> State {
    findings
        .iter()
        .find(|f| f.setting == setting)
        .unwrap()
        .state
}

#[test]
fn nothing_set_means_defaults() {
    let h = home("defaults");
    let f = check(&h, &no_env);
    assert!(f.iter().all(|f| f.state == State::Default));
    let telemetry = f.iter().find(|f| f.setting == "DISABLE_TELEMETRY").unwrap();
    assert!(telemetry.default_on);
    let otel = f.iter().find(|f| f.setting == "[otel]").unwrap();
    assert!(!otel.default_on);
}

#[test]
fn claude_switch_from_settings_file_and_from_env() {
    let h = home("claude");
    std::fs::write(
        h.join(".claude/settings.json"),
        r#"{"env":{"DISABLE_TELEMETRY":"1","DISABLE_ERROR_REPORTING":"0"}}"#,
    )
    .unwrap();
    let f = check(&h, &|k| {
        (k == "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC").then(|| "true".to_string())
    });
    assert_eq!(state(&f, "DISABLE_TELEMETRY"), State::Off);
    assert_eq!(state(&f, "DISABLE_ERROR_REPORTING"), State::Default);
    assert_eq!(
        state(&f, "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC"),
        State::Off
    );
}

#[test]
fn opt_in_switch_reads_as_on() {
    let h = home("optin");
    let f = check(&h, &|k| {
        (k == "CLAUDE_CODE_ENABLE_TELEMETRY").then(|| "1".to_string())
    });
    assert_eq!(state(&f, "CLAUDE_CODE_ENABLE_TELEMETRY"), State::On);
}

#[test]
fn codex_sections_are_read_by_name() {
    let h = home("codex");
    std::fs::write(
        h.join(".codex/config.toml"),
        "model = \"x\"\n[analytics]\nenabled = false # no thanks\n\n[otel.exporter]\nkind = \"none\"\n[feedback]\nenabled = true\n",
    )
    .unwrap();
    let f = check(&h, &no_env);
    assert_eq!(state(&f, "[analytics] enabled"), State::Off);
    assert_eq!(state(&f, "[feedback] enabled"), State::Default);
    assert_eq!(state(&f, "[otel]"), State::On);
}

#[test]
fn unreadable_or_broken_files_fall_back_to_defaults() {
    let h = home("broken");
    std::fs::write(h.join(".claude/settings.json"), "{not json").unwrap();
    std::fs::write(h.join(".codex/config.toml"), "\u{0}\u{0}[[[").unwrap();
    let f = check(&h, &no_env);
    assert!(f.iter().all(|f| f.state == State::Default));
}

#[test]
fn opencode_share_modes_and_gemini_statistics() {
    let h = home("others");
    std::fs::create_dir_all(h.join(".config/opencode")).unwrap();
    std::fs::create_dir_all(h.join(".gemini")).unwrap();
    std::fs::write(
        h.join(".config/opencode/opencode.json"),
        r#"{"share":"auto"}"#,
    )
    .unwrap();
    std::fs::write(
        h.join(".gemini/settings.json"),
        r#"{"usageStatisticsEnabled":false}"#,
    )
    .unwrap();
    let f = check(&h, &no_env);
    assert_eq!(state(&f, "share"), State::On);
    assert_eq!(state(&f, "usageStatisticsEnabled"), State::Off);
    assert_eq!(state(&f, "OPENCODE_DISABLE_MODELS_FETCH"), State::Default);

    std::fs::write(
        h.join(".config/opencode/opencode.json"),
        r#"{"share":"manual"}"#,
    )
    .unwrap();
    let f = check(&h, &|k| {
        (k == "OPENCODE_DISABLE_SHARE").then(|| "1".to_string())
    });
    assert_eq!(state(&f, "share"), State::Off);
    let f = check(&h, &no_env);
    assert_eq!(state(&f, "share"), State::Default);
}
