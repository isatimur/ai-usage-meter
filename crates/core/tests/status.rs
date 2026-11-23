use aium_core::status::{fetch_with, parse, Level, StatusError};
use std::io::{Read, Write};
use std::net::TcpListener;

// Shape copied from the real Claude status page (2026-11-23), trimmed.
const OK: &str = r#"{"page":{"name":"Claude"},"status":{"indicator":"none","description":"All Systems Operational"}}"#;

#[test]
fn indicator_maps_to_a_level() {
    let level = |i: &str| {
        parse(&format!(
            r#"{{"status":{{"indicator":"{i}","description":"x"}}}}"#
        ))
        .unwrap()
        .level
    };
    assert_eq!(level("none"), Level::Operational);
    assert_eq!(level("minor"), Level::Minor);
    assert_eq!(level("maintenance"), Level::Minor);
    assert_eq!(level("major"), Level::Major);
    assert_eq!(level("critical"), Level::Critical);
}

#[test]
fn description_is_kept_as_written() {
    assert_eq!(parse(OK).unwrap().description, "All Systems Operational");
}

#[test]
fn unknown_or_missing_indicator_is_an_error_not_a_green_badge() {
    assert!(matches!(
        parse(r#"{"status":{"indicator":"weird"}}"#),
        Err(StatusError::Shape)
    ));
    assert!(matches!(parse(r#"{"status":{}}"#), Err(StatusError::Shape)));
    assert!(matches!(parse("not json"), Err(StatusError::Shape)));
}

fn serve_once(
    status_line: &'static str,
    body: &'static str,
) -> (String, std::thread::JoinHandle<String>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", l.local_addr().unwrap());
    let h = std::thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        let mut buf = [0u8; 2048];
        let n = s.read(&mut buf).unwrap();
        let req = String::from_utf8_lossy(&buf[..n]).to_string();
        let resp = format!(
            "HTTP/1.1 {status_line}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        s.write_all(resp.as_bytes()).unwrap();
        req
    });
    (base, h)
}

#[test]
fn fetch_reads_the_status_path_and_sends_no_credentials() {
    let (base, h) = serve_once("200 OK", OK);
    let s = fetch_with(&base).unwrap();
    assert_eq!(s.level, Level::Operational);
    let req = h.join().unwrap().to_lowercase();
    assert!(req.starts_with("get /api/v2/status.json"));
    assert!(!req.contains("authorization") && !req.contains("cookie"));
}

#[test]
fn http_error_is_reported() {
    let (base, _h) = serve_once("503 Service Unavailable", "");
    assert!(matches!(fetch_with(&base), Err(StatusError::Http(503))));
}
