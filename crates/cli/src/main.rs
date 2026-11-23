use aium_core::aggregate::{Range, Totals};
use aium_core::cache::Cache;
use aium_core::pricing::PriceTable;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;

struct Args {
    range: Range,
    range_name: String,
    json: bool,
    oneline: bool,
    home: Option<PathBuf>,
    tz: Option<String>,
    now_ms: Option<i64>,
}

fn parse_args() -> Result<Args, String> {
    let mut it = std::env::args().skip(1);
    if it.next().as_deref() != Some("summary") {
        return Err("usage: aium privacy [--json] [--home PATH]\n       aium summary [--range today|week|month] [--json|--oneline] [--home PATH] [--tz IANA] [--now ISO8601]".into());
    }
    let mut args = Args {
        range: Range::Today,
        range_name: "today".into(),
        json: false,
        oneline: false,
        home: None,
        tz: None,
        now_ms: None,
    };
    while let Some(flag) = it.next() {
        let mut value = |name: &str| it.next().ok_or(format!("{name} needs a value"));
        match flag.as_str() {
            "--json" => args.json = true,
            "--oneline" => args.oneline = true,
            "--range" => {
                let v = value("--range")?;
                args.range =
                    Range::parse(&v).ok_or(format!("unknown range '{v}' (today, week, month)"))?;
                args.range_name = v;
            }
            "--home" => args.home = Some(PathBuf::from(value("--home")?)),
            "--tz" => args.tz = Some(value("--tz")?),
            "--now" => {
                let v = value("--now")?;
                let ts: jiff::Timestamp = v.parse().map_err(|e| format!("bad --now: {e}"))?;
                args.now_ms = Some(ts.as_millisecond());
            }
            other => return Err(format!("unknown flag '{other}'")),
        }
    }
    Ok(args)
}

fn line(name: &str, t: &Totals) -> String {
    format!(
        "  {:<28} in {:>10}  out {:>9}  cache-r {:>11}  cache-w {:>9}  ${:>9.4}{}  sessions {}",
        name,
        t.input,
        t.output,
        t.cache_read,
        t.cache_write,
        t.cost_usd,
        if t.cost_incomplete { "+" } else { " " },
        t.sessions
    )
}

/// "today $4.21 · claude $3.10 · codex $1.11" for shell prompts and status lines.
/// A trailing "+" marks a cost that misses some unpriced models.
fn oneline(range: &str, s: &aium_core::aggregate::Summary) -> String {
    let money = |t: &Totals| {
        format!(
            "${:.2}{}",
            t.cost_usd,
            if t.cost_incomplete { "+" } else { "" }
        )
    };
    let mut tools: Vec<_> = s.by_tool.iter().collect();
    tools.sort_by(|a, b| b.1.cost_usd.total_cmp(&a.1.cost_usd).then(a.0.cmp(b.0)));
    let mut out = format!("{range} {}", money(&s.total));
    for (name, t) in tools {
        out.push_str(&format!(" · {} {}", name.to_lowercase(), money(t)));
    }
    out
}

fn section(title: &str, rows: &BTreeMap<String, Totals>, limit: usize) {
    println!("{title}:");
    let mut sorted: Vec<_> = rows.iter().collect();
    sorted.sort_by(|a, b| b.1.cost_usd.total_cmp(&a.1.cost_usd).then(a.0.cmp(b.0)));
    for (name, t) in sorted.iter().take(limit) {
        println!("{}", line(name, t));
    }
    if sorted.len() > limit {
        println!(
            "  ... and {} more (use --json for all)",
            sorted.len() - limit
        );
    }
}

/// Shows what would change. Writes only with `--yes`, after a backup of each file.
fn run_disable(home: &std::path::Path, yes: bool) -> Result<(), Box<dyn std::error::Error>> {
    let changes = aium_core::privacy_fix::plan(home).map_err(|p| p.join("\n"))?;
    if changes.is_empty() {
        println!("Nothing to change: the switches I can set are already off, or the tools are not installed.");
        return Ok(());
    }
    for c in &changes {
        println!("{}\n  {}", c.summary, c.file.display());
        for l in &c.diff {
            println!("    {l}");
        }
        println!();
    }
    if !yes {
        println!("Dry run: nothing was written. Run again with --yes to apply. Each file gets a backup first.");
        return Ok(());
    }
    let stamp = jiff::ZonedDateTime::now().strftime("%Y%m%d-%H%M%S").to_string();
    let backups = aium_core::privacy_fix::apply(&changes, &stamp)?;
    for b in &backups {
        println!("Backup: {}", b.display());
    }
    println!("Done. Restart the tools for the change to take effect.");
    Ok(())
}

fn run_privacy() -> Result<(), Box<dyn std::error::Error>> {
    use aium_core::privacy::{check, State};
    let mut json = false;
    let mut disable = false;
    let mut yes = false;
    let mut home = None;
    let mut it = std::env::args().skip(2);
    while let Some(flag) = it.next() {
        match flag.as_str() {
            "--json" => json = true,
            "--disable" => disable = true,
            "--yes" => yes = true,
            "--home" => home = Some(PathBuf::from(it.next().ok_or("--home needs a value")?)),
            other => return Err(format!("unknown flag '{other}'").into()),
        }
    }
    let home = match home {
        Some(h) => h,
        None => PathBuf::from(std::env::var("HOME")?),
    };
    if disable {
        return run_disable(&home, yes);
    }
    let findings = check(&home, &|k| std::env::var(k).ok());
    if json {
        println!("{}", serde_json::to_string_pretty(&findings)?);
        return Ok(());
    }
    println!("Telemetry and feedback switches on this Mac (local config only, nothing was sent or captured):\n");
    let mut tool = "";
    for f in &findings {
        if f.tool != tool {
            println!("{}", f.tool);
            tool = f.tool;
        }
        let status = match (f.state, f.default_on) {
            (State::Off, _) => "OFF (you turned it off)",
            (State::On, _) => "ON  (you turned it on)",
            (State::Default, true) => "ON  (default, not switched off)",
            (State::Default, false) => "off (default, not configured)",
        };
        println!("  {status:<32} {}  ({})", f.controls, f.setting);
        if f.state == State::Default && f.default_on {
            println!("  {:<32} to turn off: {}", "", f.how_to_turn_off);
        }
    }
    println!("\nThis shows the switches a tool ships. It does not show what a tool sends. Only a traffic capture can.");
    Ok(())
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().nth(1).as_deref() == Some("privacy") {
        return run_privacy();
    }
    let args = parse_args()?;
    let real_home = PathBuf::from(std::env::var("HOME")?);
    let (home, data_dir) = match &args.home {
        Some(h) => (h.clone(), h.join(".ai-usage-meter")),
        None => (
            real_home.clone(),
            real_home.join("Library/Application Support/ai-usage-meter"),
        ),
    };
    let system_tz = jiff::tz::TimeZone::system()
        .iana_name()
        .unwrap_or("UTC")
        .to_string();

    let mut prices = PriceTable::bundled();
    let overrides = data_dir.join("prices.json");
    if overrides.exists() {
        prices = prices.with_overrides(&overrides)?;
    }

    let cache_path = data_dir.join("cache.db");
    let mut cache = match &args.tz {
        Some(tz) => Cache::open(&cache_path, tz)?,
        None => Cache::open_pinned(&cache_path, &system_tz)?,
    };
    let stats = cache.refresh(&home, &prices)?;
    for e in &stats.errors {
        eprintln!("aium: skipped: {e}");
    }
    let summary = match args.now_ms {
        Some(now) => cache.summary_at(args.range, now),
        None => cache.summary(args.range),
    };

    if args.json {
        println!("{}", serde_json::to_string_pretty(&summary)?);
        return Ok(());
    }
    if args.oneline {
        println!("{}", oneline(&args.range_name, &summary));
        return Ok(());
    }
    println!(
        "Range: {}{}",
        args.range_name,
        if summary.cost_incomplete {
            "  (cost incomplete: some models have no price)"
        } else {
            ""
        }
    );
    println!("{}", line("TOTAL", &summary.total));
    section("By tool", &summary.by_tool, 10);
    section("By model", &summary.by_model, 10);
    section("By project", &summary.by_project, 10);
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("aium: {e}");
            ExitCode::from(2)
        }
    }
}
