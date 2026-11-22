use aium_core::limits::{
    fetch_claude_with, fetch_codex_with, parse_claude, parse_codex, LimitsError,
};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

// Shapes copied from real responses (2026-11-16), trimmed to the fields we read.
const CLAUDE: &str = r#"{"five_hour":{"utilization":12,"resets_at":"2026-11-16T23:59:59.931975+00:00"},
"seven_day":{"utilization":47,"resets_at":"2026-11-22T03:59:59.931994+00:00"},
"seven_day_opus":null,
"limits":[
 {"kind":"session","group":"session","percent":12,"resets_at":"2026-11-16T23:59:59.931975+00:00","scope":null},
 {"kind":"weekly_all","group":"weekly","percent":47,"resets_at":"2026-11-22T03:59:59.931994+00:00","scope":null},
 {"kind":"weekly_scoped","group":"weekly","percent":30,"resets_at":"2026-11-22T03:59:59.932209+00:00","scope":{"model":{"id":null,"display_name":"Fable"},"surface":null}}]}"#;

const CODEX: &str = r#"{"plan_type":"plus","rate_limit":{"allowed":true,"limit_reached":false,
"primary_window":{"used_percent":0,"limit_window_seconds":18000,"reset_after_seconds":18000,"reset_at":1794876078},
"secondary_window":{"used_percent":74,"limit_window_seconds":604800,"reset_after_seconds":182869,"reset_at":1795040947}},
"credits":{"has_credits":false,"balance":"0"}}"#;

fn ms(iso: &str) -> i64 {
    iso.parse::<jiff::Timestamp>().unwrap().as_millisecond()
}

#[test]
fn claude_windows_are_session_week_and_scoped_week() {
    let l = parse_claude(CLAUDE).unwrap();
    let w: Vec<_> = l
        .windows
        .iter()
        .map(|w| (w.label.as_str(), w.percent))
        .collect();
    assert_eq!(
        w,
        vec![("5 hours", 12.0), ("Week", 47.0), ("Week · Fable", 30.0)]
    );
    assert_eq!(
        l.windows[0].resets_at_ms,
        Some(ms("2026-11-16T23:59:59.931975Z"))
    );
}

#[test]
fn claude_skips_windows_the_plan_does_not_have() {
    let l = parse_claude(
        r#"{"five_hour":null,"seven_day":{"utilization":5,"resets_at":null},"limits":[]}"#,
    )
    .unwrap();
    assert_eq!(l.windows.len(), 1);
    assert_eq!(l.windows[0].label, "Week");
    assert_eq!(l.windows[0].resets_at_ms, None);
}

#[test]
fn codex_windows_and_plan() {
    let l = parse_codex(CODEX).unwrap();
    assert_eq!(l.plan.as_deref(), Some("plus"));
    let w: Vec<_> = l
        .windows
        .iter()
        .map(|w| (w.label.as_str(), w.percent))
        .collect();
    assert_eq!(w, vec![("5 hours", 0.0), ("Week", 74.0)]);
    assert_eq!(l.windows[1].resets_at_ms, Some(1_795_040_947_000));
}

#[test]
fn garbage_is_a_bad_response_not_a_panic() {
    assert!(matches!(
        parse_claude("not json"),
        Err(LimitsError::BadResponse(_))
    ));
    assert!(matches!(
        parse_codex("[]"),
        Err(LimitsError::BadResponse(_))
    ));
}

fn mock(status: u16, body: &'static str) -> (String, Arc<Mutex<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(String::new()));
    let seen2 = seen.clone();
    std::thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let mut buf = [0u8; 4096];
        let mut req = String::new();
        loop {
            let n = stream.read(&mut buf).unwrap_or(0);
            req.push_str(&String::from_utf8_lossy(&buf[..n]));
            if n == 0 || req.contains("\r\n\r\n") {
                break;
            }
        }
        *seen2.lock().unwrap() = req;
        let reply = format!(
            "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(reply.as_bytes()).ok();
    });
    (base, seen)
}

#[test]
fn claude_fetch_sends_bearer_and_beta_header() {
    let (base, seen) = mock(200, CLAUDE);
    let l = fetch_claude_with(&base, "tok-claude").unwrap();
    assert_eq!(l.windows.len(), 3);
    let req = seen.lock().unwrap().to_lowercase();
    assert!(req.starts_with("get /api/oauth/usage "), "{req}");
    assert!(req.contains("authorization: bearer tok-claude"), "{req}");
    assert!(req.contains("anthropic-beta: oauth-2025-04-20"), "{req}");
}

#[test]
fn codex_fetch_sends_bearer_and_account_id() {
    let (base, seen) = mock(200, CODEX);
    let l = fetch_codex_with(&base, "tok-codex", "acc-1").unwrap();
    assert_eq!(l.windows.len(), 2);
    let req = seen.lock().unwrap().to_lowercase();
    assert!(req.starts_with("get /backend-api/wham/usage "), "{req}");
    assert!(req.contains("authorization: bearer tok-codex"), "{req}");
    assert!(req.contains("chatgpt-account-id: acc-1"), "{req}");
}

#[test]
fn rejected_token_is_unauthorized_and_never_echoed() {
    let (base, _) = mock(401, r#"{"error":"expired"}"#);
    let e = fetch_claude_with(&base, "secret-token-123").unwrap_err();
    assert!(matches!(e, LimitsError::Unauthorized));
    assert!(!e.to_string().contains("secret-token-123"));
}
