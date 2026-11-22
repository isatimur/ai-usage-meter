use aium_core::openrouter::{
    fetch_activity_with, fetch_credits_with, fetch_key_usage_with, OpenRouterError,
};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

/// Serves the scripted (status, body) replies in order, one per connection.
/// Returns the base URL and the raw requests seen so far.
fn mock(replies: Vec<(u16, &'static str)>) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let seen2 = seen.clone();
    std::thread::spawn(move || {
        for (status, body) in replies {
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
            seen2.lock().unwrap().push(req);
            let reply = format!(
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(reply.as_bytes()).ok();
        }
    });
    (base, seen)
}

#[test]
fn parses_key_usage_and_sends_bearer() {
    let (base, seen) = mock(vec![(
        200,
        r#"{"data":{"label":"my-key","limit":10.0,"limit_remaining":7.5,"usage":2.5,"usage_daily":0.5,"usage_weekly":1.5,"usage_monthly":2.5,"is_free_tier":false}}"#,
    )]);
    let k = fetch_key_usage_with(&base, "test-key", 1).unwrap();
    assert_eq!(k.label.as_deref(), Some("my-key"));
    assert_eq!(k.limit, Some(10.0));
    assert_eq!(k.limit_remaining, Some(7.5));
    assert_eq!(
        (k.usage, k.usage_daily, k.usage_weekly, k.usage_monthly),
        (2.5, 0.5, 1.5, 2.5)
    );
    let req = seen.lock().unwrap()[0].to_lowercase();
    assert!(req.starts_with("get /api/v1/key "), "{req}");
    assert!(req.contains("authorization: bearer test-key"), "{req}");
}

#[test]
fn missing_optional_fields_still_parse() {
    let (base, _) = mock(vec![(200, r#"{"data":{"usage":1.25}}"#)]);
    let k = fetch_key_usage_with(&base, "test-key", 1).unwrap();
    assert_eq!(k.usage, 1.25);
    assert_eq!(k.limit, None);
    assert_eq!(k.usage_daily, 0.0);
}

#[test]
fn credits_403_is_not_a_crash() {
    let (base, _) = mock(vec![(403, r#"{"error":{"message":"forbidden"}}"#)]);
    let err = fetch_credits_with(&base, "normal-key", 1).unwrap_err();
    assert!(matches!(err, OpenRouterError::NeedsManagementKey));
}

#[test]
fn unauthorized_key_is_reported() {
    let (base, _) = mock(vec![(401, "{}")]);
    let err = fetch_key_usage_with(&base, "bad", 1).unwrap_err();
    assert!(matches!(err, OpenRouterError::Unauthorized));
}

#[test]
fn backs_off_on_429_then_succeeds() {
    let (base, seen) = mock(vec![
        (429, "{}"),
        (429, "{}"),
        (200, r#"{"data":{"usage":3.0}}"#),
    ]);
    let k = fetch_key_usage_with(&base, "test-key", 1).unwrap();
    assert_eq!(k.usage, 3.0);
    assert_eq!(seen.lock().unwrap().len(), 3);
}

#[test]
fn gives_up_after_three_429s() {
    let (base, seen) = mock(vec![(429, "{}"), (429, "{}"), (429, "{}")]);
    let err = fetch_key_usage_with(&base, "test-key", 1).unwrap_err();
    assert!(matches!(err, OpenRouterError::RateLimited));
    assert_eq!(seen.lock().unwrap().len(), 3);
}

#[test]
fn parses_credits_and_activity() {
    let (base, _) = mock(vec![(
        200,
        r#"{"data":{"total_credits":50.0,"total_usage":12.5}}"#,
    )]);
    let c = fetch_credits_with(&base, "mgmt", 1).unwrap();
    assert_eq!((c.total_credits, c.total_usage), (50.0, 12.5));

    let (base, seen) = mock(vec![(
        200,
        r#"{"data":[{"date":"2026-11-10","model":"x/y","requests":3,"usage":0.12,"prompt_tokens":100,"completion_tokens":20}]}"#,
    )]);
    let rows = fetch_activity_with(&base, "mgmt", 1).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].date, "2026-11-10");
    assert_eq!(rows[0].model, "x/y");
    assert_eq!(
        (
            rows[0].requests,
            rows[0].prompt_tokens,
            rows[0].completion_tokens
        ),
        (3, 100, 20)
    );
    assert!(seen.lock().unwrap()[0]
        .to_lowercase()
        .starts_with("get /api/v1/activity "));
}
