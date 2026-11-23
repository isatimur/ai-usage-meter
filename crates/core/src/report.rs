//! Markdown daily note for Obsidian.

use crate::aggregate::{SessionInfo, Summary, Totals};
use jiff::tz::TimeZone;
use jiff::Timestamp;
use std::collections::BTreeMap;
use std::fmt::Write;

fn cost(t: &Totals) -> String {
    format!(
        "${:.2}{}",
        t.cost_usd,
        if t.cost_incomplete { "+" } else { "" }
    )
}

/// A table cell must not contain a pipe or a newline.
fn cell(s: &str) -> String {
    s.replace('|', "/").replace('\n', " ")
}

fn last_component(path: &str) -> String {
    if path.is_empty() {
        return "(unknown)".into();
    }
    path.trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(path)
        .to_string()
}

fn table(
    out: &mut String,
    title: &str,
    rows: &BTreeMap<String, Totals>,
    name: impl Fn(&str) -> String,
) {
    let mut sorted: Vec<_> = rows.iter().collect();
    sorted.sort_by(|a, b| b.1.cost_usd.total_cmp(&a.1.cost_usd).then(a.0.cmp(b.0)));
    let _ = writeln!(out, "\n## {title}\n");
    let _ = writeln!(
        out,
        "| Name | Sessions | Output tokens | Input tokens | Cost |"
    );
    let _ = writeln!(out, "|---|---|---|---|---|");
    for (k, t) in sorted {
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} | {} |",
            cell(&name(k)),
            t.sessions,
            t.output,
            t.input + t.cache_read + t.cache_write,
            cost(t)
        );
    }
}

pub fn daily_note(
    date: &str,
    summary: &Summary,
    sessions: &[SessionInfo],
    tz: &TimeZone,
) -> String {
    let t = &summary.total;
    let mut out = String::new();
    let _ = writeln!(out, "---\ndate: {date}\ntags: [ai-usage]\n---\n");
    let _ = writeln!(out, "# AI tool usage, {date}\n");
    let _ = writeln!(
        out,
        "Total: **{}** (estimated at API prices, not your bill; on a subscription you pay less). \
         Sessions: {}. Input tokens: {}, output tokens: {}.",
        cost(t),
        t.sessions,
        t.input + t.cache_read + t.cache_write,
        t.output
    );
    if summary.cost_incomplete {
        let _ = writeln!(
            out,
            "\nA \"+\" means some models have no price, so the real total is higher."
        );
    }
    table(&mut out, "By tool", &summary.by_tool, |k| k.to_string());
    table(&mut out, "By model", &summary.by_model, |k| k.to_string());
    table(&mut out, "By project", &summary.by_project, last_component);

    let _ = writeln!(out, "\n## Sessions\n");
    if sessions.is_empty() {
        let _ = writeln!(out, "No sessions on this day.");
    } else {
        let _ = writeln!(out, "| Project | Tool | Model | Last activity | Cost |");
        let _ = writeln!(out, "|---|---|---|---|---|");
        for s in sessions {
            let last = Timestamp::from_millisecond(s.last_ts_ms)
                .map(|ts| {
                    ts.to_zoned_datetime(tz.clone())
                        .strftime("%H:%M")
                        .to_string()
                })
                .unwrap_or_default();
            let _ = writeln!(
                out,
                "| {} | {} | {} | {} | {} |",
                cell(&last_component(&s.project)),
                cell(&s.tool),
                cell(&s.model),
                last,
                cost(&s.totals)
            );
        }
    }
    out
}
