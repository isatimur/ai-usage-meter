//! C ABI for the Swift shell. Every function takes C strings and returns a
//! heap-allocated JSON C string (`{"error": "..."}` on failure) that the
//! caller must release with `aium_free`. Nothing here panics across the
//! boundary.

use crate::aggregate::Range;
use crate::cache::Cache;
use crate::openrouter::{fetch_key_usage, DEFAULT_BASE};
use crate::pricing::PriceTable;
use serde_json::json;
use std::ffi::{c_char, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::Mutex;

/// Opened by the first `aium_refresh` and kept for the process lifetime.
static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

fn to_c(s: String) -> *mut c_char {
    CString::new(s)
        .unwrap_or_else(|_| CString::new(r#"{"error":"output contained a NUL byte"}"#).unwrap())
        .into_raw()
}

fn run(f: impl FnOnce() -> Result<String, String>) -> *mut c_char {
    let out = match catch_unwind(AssertUnwindSafe(f)) {
        Ok(Ok(json)) => json,
        Ok(Err(msg)) => json!({ "error": msg }).to_string(),
        Err(_) => json!({ "error": "internal error" }).to_string(),
    };
    to_c(out)
}

unsafe fn arg(p: *const c_char, what: &str) -> Result<String, String> {
    if p.is_null() {
        return Err(format!("{what} is null"));
    }
    CStr::from_ptr(p)
        .to_str()
        .map(str::to_string)
        .map_err(|_| format!("{what} is not valid UTF-8"))
}

fn lock() -> std::sync::MutexGuard<'static, Option<Cache>> {
    CACHE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Scans `home` for new usage and updates the cache.
/// Returns `{"ok":true,"bytes_read":N,"events_added":N}`.
///
/// # Safety
/// `home` must be null or a valid NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn aium_refresh(home: *const c_char) -> *mut c_char {
    let home = arg(home, "home");
    run(move || {
        let home = PathBuf::from(home?);
        let data_dir = home.join("Library/Application Support/ai-usage-meter");
        let mut prices = PriceTable::bundled();
        let overrides = data_dir.join("prices.json");
        if overrides.exists() {
            prices = prices
                .with_overrides(&overrides)
                .map_err(|e| e.to_string())?;
        }
        let mut guard = lock();
        if guard.is_none() {
            let tz = jiff::tz::TimeZone::system()
                .iana_name()
                .unwrap_or("UTC")
                .to_string();
            *guard = Some(
                Cache::open_pinned(&data_dir.join("cache.db"), &tz).map_err(|e| e.to_string())?,
            );
        }
        let stats = guard
            .as_mut()
            .expect("cache was just opened")
            .refresh(&home, &prices)
            .map_err(|e| e.to_string())?;
        Ok(json!({
            "ok": true,
            "bytes_read": stats.bytes_read,
            "events_added": stats.events_added,
            "errors": stats.errors,
        })
        .to_string())
    })
}

/// Returns the `Summary` JSON for `"today"`, `"week"` or `"month"`.
/// Call `aium_refresh` first.
///
/// # Safety
/// `range` must be null or a valid NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn aium_summary(range: *const c_char) -> *mut c_char {
    let range = arg(range, "range");
    run(move || {
        let range = range?;
        let parsed = Range::parse(&range)
            .ok_or_else(|| format!("unknown range '{range}' (today, week, month)"))?;
        let guard = lock();
        let cache = guard.as_ref().ok_or("call aium_refresh first")?;
        serde_json::to_string(&cache.summary(parsed)).map_err(|e| e.to_string())
    })
}

/// Returns a JSON array of the newest `limit` sessions in `"today"`, `"week"`
/// or `"month"`, newest first. Call `aium_refresh` first.
///
/// # Safety
/// `range` must be null or a valid NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn aium_sessions(range: *const c_char, limit: u32) -> *mut c_char {
    let range = arg(range, "range");
    run(move || {
        let range = range?;
        let parsed = Range::parse(&range)
            .ok_or_else(|| format!("unknown range '{range}' (today, week, month)"))?;
        let guard = lock();
        let cache = guard.as_ref().ok_or("call aium_refresh first")?;
        serde_json::to_string(&cache.sessions(parsed, limit as usize)).map_err(|e| e.to_string())
    })
}

fn limits_json(
    r: Result<crate::limits::ToolLimits, crate::limits::LimitsError>,
) -> serde_json::Value {
    match r {
        Ok(l) => serde_json::to_value(l).unwrap_or(serde_json::Value::Null),
        Err(e) => json!({ "error": e.to_string() }),
    }
}

/// Fetches plan limits. A null or blank token skips that tool (its field is
/// `null`). Returns `{"claude": <limits>|{"error":..}|null, "codex": ...}`.
/// Tokens are used for this call only and never stored.
///
/// # Safety
/// Each argument must be null or a valid NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn aium_limits(
    claude_token: *const c_char,
    codex_token: *const c_char,
    codex_account: *const c_char,
) -> *mut c_char {
    let opt = |p: *const c_char| -> Option<String> {
        if p.is_null() {
            return None;
        }
        let s = CStr::from_ptr(p).to_str().ok()?.trim().to_string();
        (!s.is_empty()).then_some(s)
    };
    let (claude, codex, account) = (opt(claude_token), opt(codex_token), opt(codex_account));
    run(move || {
        let (c, x) = std::thread::scope(|s| {
            let c = s.spawn(|| {
                claude.as_deref().map(|t| {
                    limits_json(crate::limits::fetch_claude_with(
                        crate::limits::CLAUDE_BASE,
                        t,
                    ))
                })
            });
            let x = s.spawn(|| {
                codex.as_deref().map(|t| {
                    limits_json(crate::limits::fetch_codex_with(
                        crate::limits::CODEX_BASE,
                        t,
                        account.as_deref().unwrap_or(""),
                    ))
                })
            });
            (c.join().ok().flatten(), x.join().ok().flatten())
        });
        Ok(json!({ "claude": c, "codex": x }).to_string())
    })
}

/// Returns `{"date":"YYYY-MM-DD","markdown":"..."}`: today's note for Obsidian.
/// Call `aium_refresh` first.
#[no_mangle]
pub extern "C" fn aium_daily_note() -> *mut c_char {
    run(|| {
        let guard = lock();
        let cache = guard.as_ref().ok_or("call aium_refresh first")?;
        let now = jiff::Timestamp::now().as_millisecond();
        Ok(json!({
            "date": cache.local_date_at(now),
            "markdown": cache.daily_note_at(now),
        })
        .to_string())
    })
}

/// Returns the monthly budget forecast as JSON, or `{"forecast":null}` when the
/// budget is not positive. `spent` is this month's estimated cost.
#[no_mangle]
pub extern "C" fn aium_budget(spent: f64, budget: f64) -> *mut c_char {
    run(|| {
        let now = jiff::ZonedDateTime::now();
        let f =
            crate::budget::forecast(spent, budget, now.day() as u32, now.days_in_month() as u32);
        Ok(json!({ "forecast": f }).to_string())
    })
}

/// Telemetry and feedback switches of the supported tools, from local config
/// and the environment only. Returns {"findings": [...]}.
#[no_mangle]
pub extern "C" fn aium_privacy() -> *mut c_char {
    run(|| {
        let home = std::env::var("HOME").map_err(|_| "HOME is not set".to_string())?;
        let findings =
            crate::privacy::check(std::path::Path::new(&home), &|k| std::env::var(k).ok());
        Ok(json!({ "findings": findings }).to_string())
    })
}

/// Plans (apply = 0) or applies (apply != 0) the privacy changes: telemetry and
/// feedback switches turned off in each tool's own config. Applying saves a
/// backup of each file first. Returns {"changes": [...], "backups": [...]}.
#[no_mangle]
pub extern "C" fn aium_privacy_disable(apply: i32) -> *mut c_char {
    run(|| {
        let home = std::env::var("HOME").map_err(|_| "HOME is not set".to_string())?;
        let home = std::path::Path::new(&home);
        let changes = crate::privacy_fix::plan(home).map_err(|p| p.join("\n"))?;
        let backups = if apply != 0 && !changes.is_empty() {
            let stamp = jiff::ZonedDateTime::now()
                .strftime("%Y%m%d-%H%M%S")
                .to_string();
            crate::privacy_fix::apply(&changes, &stamp)?
        } else {
            Vec::new()
        };
        Ok(json!({ "changes": changes, "backups": backups }).to_string())
    })
}

/// Fetches OpenRouter key usage. The key is used for this call only and is
/// never stored.
///
/// # Safety
/// `key` must be null or a valid NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn aium_openrouter_key_usage(key: *const c_char) -> *mut c_char {
    let key = arg(key, "key");
    run(move || {
        let key = key?;
        if key.trim().is_empty() {
            return Err("key is empty".into());
        }
        let usage = fetch_key_usage(DEFAULT_BASE, &key).map_err(|e| e.to_string())?;
        serde_json::to_string(&usage).map_err(|e| e.to_string())
    })
}

/// Releases a string returned by any `aium_*` function. Null is a no-op.
///
/// # Safety
/// `s` must be null or a pointer returned by this library, freed at most once.
#[no_mangle]
pub unsafe extern "C" fn aium_free(s: *mut c_char) {
    if !s.is_null() {
        drop(CString::from_raw(s));
    }
}
