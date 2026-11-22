use aium_core::privacy::{check, State};
use aium_core::privacy_fix::{apply, plan};
use std::path::PathBuf;

fn home(name: &str) -> PathBuf {
    let h = std::env::temp_dir().join(format!("aium-fix-{}-{}", name, std::process::id()));
    std::fs::remove_dir_all(&h).ok();
    std::fs::create_dir_all(h.join(".claude")).unwrap();
    std::fs::create_dir_all(h.join(".codex")).unwrap();
    h
}

fn no_env(_: &str) -> Option<String> {
    None
}

fn state(h: &std::path::Path, setting: &str) -> State {
    check(h, &no_env)
        .iter()
        .find(|f| f.setting == setting)
        .unwrap()
        .state
}

#[test]
fn nothing_installed_means_nothing_to_do() {
    let h = std::env::temp_dir().join(format!("aium-fix-empty-{}", std::process::id()));
    std::fs::create_dir_all(&h).unwrap();
    assert!(plan(&h).unwrap().is_empty());
}

#[test]
fn claude_settings_keep_order_and_other_keys() {
    let h = home("claude");
    let file = h.join(".claude/settings.json");
    std::fs::write(
        &file,
        r#"{"zeta":1,"env":{"KEEP":"yes"},"alpha":{"deep":[1,2]}}"#,
    )
    .unwrap();
    let changes = plan(&h).unwrap();
    // plan() alone writes nothing
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        r#"{"zeta":1,"env":{"KEEP":"yes"},"alpha":{"deep":[1,2]}}"#
    );
    let backups = apply(&changes, "t1").unwrap();
    assert_eq!(backups.len(), 1);
    assert!(std::fs::read_to_string(&backups[0])
        .unwrap()
        .contains(r#""zeta":1"#));

    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.find("zeta").unwrap() < text.find("env").unwrap());
    assert!(text.find("env").unwrap() < text.find("alpha").unwrap());
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["env"]["KEEP"], "yes");
    assert_eq!(v["alpha"]["deep"][1], 2);
    assert_eq!(state(&h, "DISABLE_TELEMETRY"), State::Off);
    assert_eq!(state(&h, "DISABLE_ERROR_REPORTING"), State::Off);
    assert_eq!(state(&h, "CLAUDE_CODE_DISABLE_FEEDBACK_SURVEY"), State::Off);
    // the switch that also stops updates stays untouched
    assert_eq!(
        state(&h, "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC"),
        State::Default
    );
}

#[test]
fn applying_twice_changes_nothing_the_second_time() {
    let h = home("twice");
    std::fs::write(h.join(".claude/settings.json"), "{}").unwrap();
    apply(&plan(&h).unwrap(), "t1").unwrap();
    assert!(plan(&h).unwrap().is_empty());
}

#[test]
fn codex_toml_edits_in_place_or_appends() {
    let h = home("codex");
    let file = h.join(".codex/config.toml");
    std::fs::write(
        &file,
        "model = \"x\"\n\n[analytics]\nenabled = true # on\nother = 1\n\n[tui]\ntheme = \"a\"\n",
    )
    .unwrap();
    apply(&plan(&h).unwrap(), "t1").unwrap();
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(
        text.contains("[analytics]\nenabled = false\nother = 1"),
        "{text}"
    );
    assert!(text.contains("[feedback]\nenabled = false"), "{text}");
    assert!(text.contains("model = \"x\""), "{text}");
    assert!(text.contains("[tui]\ntheme = \"a\""), "{text}");
    assert_eq!(state(&h, "[analytics] enabled"), State::Off);
    assert_eq!(state(&h, "[feedback] enabled"), State::Off);
    assert!(plan(&h).unwrap().is_empty());
}

#[test]
fn codex_section_without_the_key_gets_it_inserted() {
    let h = home("codex-insert");
    std::fs::write(h.join(".codex/config.toml"), "[analytics]\nfoo = 1\n").unwrap();
    apply(&plan(&h).unwrap(), "t1").unwrap();
    let text = std::fs::read_to_string(h.join(".codex/config.toml")).unwrap();
    assert!(
        text.starts_with("[analytics]\nenabled = false\nfoo = 1"),
        "{text}"
    );
}

#[test]
fn shapes_it_cannot_edit_safely_are_refused_and_left_alone() {
    let h = home("refuse");
    let toml = "analytics = { enabled = true }\n";
    std::fs::write(h.join(".codex/config.toml"), toml).unwrap();
    std::fs::write(h.join(".claude/settings.json"), "{not json").unwrap();
    let err = plan(&h).unwrap_err();
    assert_eq!(err.len(), 2, "{err:?}");
    assert_eq!(
        std::fs::read_to_string(h.join(".codex/config.toml")).unwrap(),
        toml
    );
    assert_eq!(
        std::fs::read_to_string(h.join(".claude/settings.json")).unwrap(),
        "{not json"
    );
}

#[test]
fn a_file_that_changed_since_the_plan_is_not_overwritten() {
    let h = home("race");
    let file = h.join(".claude/settings.json");
    std::fs::write(&file, "{}").unwrap();
    let changes = plan(&h).unwrap();
    std::fs::write(&file, r#"{"edited":"meanwhile"}"#).unwrap();
    let err = apply(&changes, "t1").unwrap_err();
    assert!(err.contains("changed since"), "{err}");
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        r#"{"edited":"meanwhile"}"#
    );
}

#[test]
fn gemini_statistics_switch() {
    let h = home("gemini");
    std::fs::create_dir_all(h.join(".gemini")).unwrap();
    std::fs::write(
        h.join(".gemini/settings.json"),
        r#"{"selectedAuthType":"x"}"#,
    )
    .unwrap();
    apply(&plan(&h).unwrap(), "t1").unwrap();
    assert_eq!(state(&h, "usageStatisticsEnabled"), State::Off);
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(h.join(".gemini/settings.json")).unwrap())
            .unwrap();
    assert_eq!(v["selectedAuthType"], "x");
}
