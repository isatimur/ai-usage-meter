use jiff::tz::TimeZone;
use jiff::{Timestamp, ToSpan};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Range {
    Today,
    /// The last 7 days, including today.
    Week,
    /// The calendar month so far.
    Month,
}

impl Range {
    pub fn parse(s: &str) -> Option<Range> {
        match s {
            "today" => Some(Range::Today),
            "week" => Some(Range::Week),
            "month" => Some(Range::Month),
            _ => None,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Totals {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub cost_usd: f64,
    pub sessions: u64,
    /// True when some usage in this bucket has no price, so `cost_usd` is a lower bound.
    #[serde(default)]
    pub cost_incomplete: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Summary {
    pub total: Totals,
    pub by_tool: BTreeMap<String, Totals>,
    pub by_model: BTreeMap<String, Totals>,
    pub by_project: BTreeMap<String, Totals>,
    pub by_day: Vec<(String, Totals)>,
    pub cost_incomplete: bool,
}

#[derive(Default)]
struct Acc {
    totals: Totals,
    sessions: HashSet<(String, String)>,
}

impl Acc {
    fn finish(mut self) -> Totals {
        self.totals.sessions = self.sessions.len() as u64;
        self.totals
    }
}

struct Row {
    tool: String,
    model: String,
    project: String,
    session: String,
    day: String,
    input: u64,
    output: u64,
    cache_read: u64,
    cache_write: u64,
    cost: Option<f64>,
}

fn add(acc: &mut Acc, r: &Row) {
    acc.totals.input += r.input;
    acc.totals.output += r.output;
    acc.totals.cache_read += r.cache_read;
    acc.totals.cache_write += r.cache_write;
    acc.totals.cost_usd += r.cost.unwrap_or(0.0);
    if r.cost.is_none() && r.input + r.output + r.cache_read + r.cache_write > 0 {
        acc.totals.cost_incomplete = true;
    }
    acc.sessions.insert((r.tool.clone(), r.session.clone()));
}

fn range_days(tz: &TimeZone, range: Range, now_ms: i64) -> (jiff::civil::Date, jiff::civil::Date) {
    let today = Timestamp::from_millisecond(now_ms)
        .expect("valid now")
        .to_zoned_datetime(tz.clone())
        .date();
    let first = match range {
        Range::Today => today,
        Range::Week => today.checked_sub(6.days()).expect("date in range"),
        Range::Month => today.first_of_month(),
    };
    (first, today)
}

/// A session counts as live when its last event is this recent.
const ACTIVE_WINDOW_MS: i64 = 10 * 60 * 1000;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionInfo {
    pub tool: String,
    pub session: String,
    /// The project of the session's first event.
    pub project: String,
    /// The model with the most output tokens in the session.
    pub model: String,
    pub first_ts_ms: i64,
    pub last_ts_ms: i64,
    pub active: bool,
    pub totals: Totals,
}

struct SessionAcc {
    info: SessionInfo,
    acc: Acc,
    output_by_model: BTreeMap<String, u64>,
}

/// The newest `limit` sessions that have events in `range`, newest first.
pub(crate) fn list_sessions(
    conn: &Connection,
    tz: &TimeZone,
    range: Range,
    now_ms: i64,
    limit: usize,
) -> Vec<SessionInfo> {
    let (first, today) = range_days(tz, range, now_ms);
    let mut stmt = conn
        .prepare(
            "SELECT tool, model, project, session, day, input, output, cache_read, \
             cache_write_5m + cache_write_1h, cost_usd, ts_ms FROM events \
             WHERE day >= ?1 AND day <= ?2 ORDER BY ts_ms",
        )
        .expect("session query is valid");
    let rows = stmt
        .query_map([first.to_string(), today.to_string()], |r| {
            Ok((
                Row {
                    tool: r.get(0)?,
                    model: r.get(1)?,
                    project: r.get(2)?,
                    session: r.get(3)?,
                    day: r.get(4)?,
                    input: r.get::<_, i64>(5)? as u64,
                    output: r.get::<_, i64>(6)? as u64,
                    cache_read: r.get::<_, i64>(7)? as u64,
                    cache_write: r.get::<_, i64>(8)? as u64,
                    cost: r.get(9)?,
                },
                r.get::<_, i64>(10)?,
            ))
        })
        .expect("session rows");

    let mut by_key: BTreeMap<(String, String), SessionAcc> = BTreeMap::new();
    for (row, ts) in rows.flatten() {
        let key = (row.tool.clone(), row.session.clone());
        let s = by_key.entry(key).or_insert_with(|| SessionAcc {
            info: SessionInfo {
                tool: row.tool.clone(),
                session: row.session.clone(),
                project: row.project.clone(),
                model: String::new(),
                first_ts_ms: ts,
                last_ts_ms: ts,
                active: false,
                totals: Totals::default(),
            },
            acc: Acc::default(),
            output_by_model: BTreeMap::new(),
        });
        s.info.last_ts_ms = s.info.last_ts_ms.max(ts);
        *s.output_by_model.entry(row.model.clone()).or_default() += row.output;
        add(&mut s.acc, &row);
    }

    let mut out: Vec<SessionInfo> = by_key
        .into_values()
        .map(|mut s| {
            s.info.model = s
                .output_by_model
                .iter()
                .max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0)))
                .map(|(m, _)| m.clone())
                .unwrap_or_default();
            s.info.active =
                now_ms - s.info.last_ts_ms <= ACTIVE_WINDOW_MS && s.info.last_ts_ms <= now_ms;
            s.info.totals = s.acc.finish();
            s.info
        })
        .collect();
    out.sort_by_key(|s| std::cmp::Reverse(s.last_ts_ms));
    out.truncate(limit);
    out
}

pub(crate) fn summarize(conn: &Connection, tz: &TimeZone, range: Range, now_ms: i64) -> Summary {
    let (first, today) = range_days(tz, range, now_ms);

    let mut stmt = conn
        .prepare(
            "SELECT tool, model, project, session, day, input, output, cache_read, \
             cache_write_5m + cache_write_1h, cost_usd FROM events WHERE day >= ?1 AND day <= ?2",
        )
        .expect("summary query is valid");
    let rows = stmt
        .query_map([first.to_string(), today.to_string()], |r| {
            Ok(Row {
                tool: r.get(0)?,
                model: r.get(1)?,
                project: r.get(2)?,
                session: r.get(3)?,
                day: r.get(4)?,
                input: r.get::<_, i64>(5)? as u64,
                output: r.get::<_, i64>(6)? as u64,
                cache_read: r.get::<_, i64>(7)? as u64,
                cache_write: r.get::<_, i64>(8)? as u64,
                cost: r.get(9)?,
            })
        })
        .expect("summary rows");

    let mut total = Acc::default();
    let mut by_tool: BTreeMap<String, Acc> = BTreeMap::new();
    let mut by_model: BTreeMap<String, Acc> = BTreeMap::new();
    let mut by_project: BTreeMap<String, Acc> = BTreeMap::new();
    let mut by_day: BTreeMap<String, Acc> = BTreeMap::new();
    let mut cost_incomplete = false;

    for row in rows.flatten() {
        if row.cost.is_none() && row.input + row.output + row.cache_read + row.cache_write > 0 {
            cost_incomplete = true;
        }
        add(&mut total, &row);
        add(by_tool.entry(row.tool.clone()).or_default(), &row);
        add(by_model.entry(row.model.clone()).or_default(), &row);
        add(by_project.entry(row.project.clone()).or_default(), &row);
        add(by_day.entry(row.day.clone()).or_default(), &row);
    }

    Summary {
        total: total.finish(),
        by_tool: finish(by_tool),
        by_model: finish(by_model),
        by_project: finish(by_project),
        by_day: finish(by_day).into_iter().collect(),
        cost_incomplete,
    }
}

fn finish(m: BTreeMap<String, Acc>) -> BTreeMap<String, Totals> {
    m.into_iter().map(|(k, v)| (k, v.finish())).collect()
}
