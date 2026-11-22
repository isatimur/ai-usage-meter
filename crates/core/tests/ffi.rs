use aium_core::ffi::{
    aium_daily_note, aium_free, aium_limits, aium_openrouter_key_usage, aium_refresh,
    aium_sessions, aium_summary,
};
use serde_json::Value;
use std::ffi::{c_char, CStr, CString};
use std::ptr;

unsafe fn take(p: *mut c_char) -> Value {
    assert!(!p.is_null());
    let v = serde_json::from_str(CStr::from_ptr(p).to_str().unwrap()).unwrap();
    aium_free(p);
    v
}

fn c(s: &str) -> CString {
    CString::new(s).unwrap()
}

#[test]
fn refresh_then_summary_roundtrip() {
    let home = std::env::temp_dir().join(format!("aium-ffi-{}", std::process::id()));
    std::fs::remove_dir_all(&home).ok();
    let proj = home.join(".claude/projects/-w-p");
    std::fs::create_dir_all(&proj).unwrap();
    let now = jiff::Timestamp::now();
    let line = format!(
        r#"{{"type":"assistant","timestamp":"{now}","sessionId":"s","cwd":"/p","message":{{"id":"m1","model":"claude-opus-4-5","usage":{{"input_tokens":11,"output_tokens":22}}}}}}"#
    );
    std::fs::write(proj.join("s.jsonl"), format!("{line}\n")).unwrap();

    unsafe {
        let r = take(aium_refresh(c(home.to_str().unwrap()).as_ptr()));
        assert_eq!(r["ok"], true, "{r}");
        assert_eq!(r["events_added"], 1);

        let s = take(aium_summary(c("today").as_ptr()));
        assert_eq!(s["total"]["input"], 11, "{s}");
        assert_eq!(s["total"]["output"], 22);
        assert_eq!(s["by_tool"]["Claude"]["sessions"], 1);

        let list = take(aium_sessions(c("today").as_ptr(), 10));
        assert_eq!(list[0]["tool"], "Claude", "{list}");
        assert_eq!(list[0]["session"], "s");
        assert_eq!(list[0]["active"], true);
        assert_eq!(list[0]["totals"]["output"], 22);

        let note = take(aium_daily_note());
        assert_eq!(note["date"].as_str().unwrap().len(), 10, "{note}");
        assert!(
            note["markdown"]
                .as_str()
                .unwrap()
                .contains("# AI tool usage"),
            "{note}"
        );
    }
    std::fs::remove_dir_all(&home).ok();
}

#[test]
fn bad_input_returns_error_json_not_a_crash() {
    unsafe {
        let r = take(aium_refresh(ptr::null()));
        assert!(r["error"].as_str().is_some(), "{r}");
        let r = take(aium_summary(c("decade").as_ptr()));
        assert!(r["error"].as_str().unwrap().contains("range"), "{r}");
        let r = take(aium_summary(ptr::null()));
        assert!(r["error"].as_str().is_some());
        let r = take(aium_openrouter_key_usage(c("").as_ptr()));
        assert!(r["error"].as_str().unwrap().contains("key"), "{r}");
        aium_free(ptr::null_mut());
    }
}

#[test]
fn limits_without_tokens_skip_the_network() {
    unsafe {
        let r = take(aium_limits(ptr::null(), ptr::null(), ptr::null()));
        assert!(r["claude"].is_null() && r["codex"].is_null(), "{r}");
        let r = take(aium_limits(
            c("").as_ptr(),
            c("  ").as_ptr(),
            c("").as_ptr(),
        ));
        assert!(r["claude"].is_null() && r["codex"].is_null(), "{r}");
    }
}

#[test]
fn header_declares_every_exported_function() {
    let header = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("include/aium.h"),
    )
    .unwrap();
    for name in [
        "aium_refresh",
        "aium_summary",
        "aium_sessions",
        "aium_daily_note",
        "aium_limits",
        "aium_openrouter_key_usage",
        "aium_free",
    ] {
        assert!(header.contains(name), "aium.h is missing {name}");
    }
}
