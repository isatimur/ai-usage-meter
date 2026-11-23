use crate::aggregate::{list_sessions, summarize, Range, SessionInfo, Summary};
use crate::parsers::codex::codex_thread_fallback;
use crate::parsers::opencode::read_opencode;
use crate::parsers::{file_sources, ParseOutput};
use crate::pricing::PriceTable;
use crate::{CostSource, Tool, UsageEvent};
use jiff::tz::TimeZone;
use jiff::Timestamp;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashSet;
use std::path::Path;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// OpenCode rows are re-read from this far before the high-water mark,
/// because a message keeps growing while it streams.
const OPENCODE_OVERLAP_MS: i64 = 10 * 60 * 1000;

#[derive(Debug, Default, Clone, PartialEq)]
pub struct RefreshStats {
    pub bytes_read: u64,
    pub events_added: usize,
    /// Files or sources that failed and were skipped; the refresh went on without them.
    pub errors: Vec<String>,
}

pub struct Cache {
    conn: Connection,
    tz: TimeZone,
}

const UPSERT: &str = "INSERT INTO events (tool, dedup_key, ts_ms, day, model, project, session, \
    input, output, cache_read, cache_write_5m, cache_write_1h, reasoning, cost_usd, cost_source) \
    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15) \
    ON CONFLICT(tool, dedup_key) DO UPDATE SET ts_ms = excluded.ts_ms, day = excluded.day, \
    model = excluded.model, project = excluded.project, session = excluded.session, \
    input = excluded.input, output = excluded.output, cache_read = excluded.cache_read, \
    cache_write_5m = excluded.cache_write_5m, cache_write_1h = excluded.cache_write_1h, \
    reasoning = excluded.reasoning, cost_usd = excluded.cost_usd, cost_source = excluded.cost_source \
    WHERE excluded.output > events.output";

impl Cache {
    /// Opens or creates the cache. The bucket timezone is pinned on first use;
    /// opening with a different one is an error, so past days never re-bucket.
    pub fn open(path: &Path, tz: &str) -> Result<Cache> {
        let zone = TimeZone::get(tz)?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS events (
                tool TEXT NOT NULL, dedup_key TEXT NOT NULL, ts_ms INTEGER NOT NULL, day TEXT NOT NULL,
                model TEXT NOT NULL, project TEXT NOT NULL, session TEXT NOT NULL,
                input INTEGER NOT NULL, output INTEGER NOT NULL, cache_read INTEGER NOT NULL,
                cache_write_5m INTEGER NOT NULL, cache_write_1h INTEGER NOT NULL, reasoning INTEGER NOT NULL,
                cost_usd REAL, cost_source TEXT NOT NULL,
                PRIMARY KEY (tool, dedup_key));
             CREATE INDEX IF NOT EXISTS events_day ON events (day);
             CREATE TABLE IF NOT EXISTS files (path TEXT PRIMARY KEY, offset INTEGER NOT NULL, size INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
        )?;
        let stored: Option<String> = conn
            .query_row("SELECT value FROM meta WHERE key = 'tz'", [], |r| r.get(0))
            .optional()?;
        match stored {
            Some(s) if s != tz => {
                return Err(format!("cache timezone is {s}, refusing to open with {tz}").into())
            }
            Some(_) => {}
            None => {
                conn.execute("INSERT INTO meta (key, value) VALUES ('tz', ?1)", [tz])?;
            }
        }
        Ok(Cache { conn, tz: zone })
    }

    /// Like `open`, but an existing cache keeps the timezone it was created with
    /// (so travelling or a system timezone change never locks the app out).
    /// `default_tz` is used only when the cache is new.
    pub fn open_pinned(path: &Path, default_tz: &str) -> Result<Cache> {
        let stored: Option<String> = if path.exists() {
            Connection::open(path).ok().and_then(|c| {
                c.query_row("SELECT value FROM meta WHERE key = 'tz'", [], |r| r.get(0))
                    .ok()
            })
        } else {
            None
        };
        Self::open(path, stored.as_deref().unwrap_or(default_tz))
    }

    /// One bad file or source never stops the rest: failures are collected in
    /// `RefreshStats::errors` and the refresh continues.
    pub fn refresh(&mut self, home: &Path, prices: &PriceTable) -> Result<RefreshStats> {
        let mut stats = RefreshStats::default();

        for source in file_sources() {
            for path in source.discover(home) {
                let r = self.refresh_file(&path, prices, &mut stats, |p, off| source.parse(p, off));
                if let Err(e) = r {
                    stats.errors.push(format!("{}: {e}", path.display()));
                }
            }
        }
        if let Err(e) = self.refresh_codex_fallback(home, prices, &mut stats) {
            stats.errors.push(format!("codex thread fallback: {e}"));
        }
        if let Err(e) = self.refresh_opencode(home, prices, &mut stats) {
            stats.errors.push(format!("opencode: {e}"));
        }
        if let Err(e) = self.reprice(prices) {
            stats.errors.push(format!("reprice: {e}"));
        }
        Ok(stats)
    }

    pub fn summary(&self, range: Range) -> Summary {
        self.summary_at(range, Timestamp::now().as_millisecond())
    }

    pub fn summary_at(&self, range: Range, now_ms: i64) -> Summary {
        summarize(&self.conn, &self.tz, range, now_ms)
    }

    pub fn sessions(&self, range: Range, limit: usize) -> Vec<SessionInfo> {
        self.sessions_at(range, Timestamp::now().as_millisecond(), limit)
    }

    pub fn sessions_at(&self, range: Range, now_ms: i64, limit: usize) -> Vec<SessionInfo> {
        list_sessions(&self.conn, &self.tz, range, now_ms, limit)
    }

    /// The local date (`YYYY-MM-DD`, in the cache's timezone) that contains `now_ms`.
    pub fn local_date_at(&self, now_ms: i64) -> String {
        Timestamp::from_millisecond(now_ms)
            .expect("valid now")
            .to_zoned_datetime(self.tz.clone())
            .date()
            .to_string()
    }

    /// Markdown note for the local day that contains `now_ms`.
    pub fn daily_note_at(&self, now_ms: i64) -> String {
        crate::report::daily_note(
            &self.local_date_at(now_ms),
            &self.summary_at(Range::Today, now_ms),
            &self.sessions_at(Range::Today, now_ms, 200),
            &self.tz,
        )
    }

    pub fn daily_note(&self) -> String {
        self.daily_note_at(Timestamp::now().as_millisecond())
    }

    fn refresh_file(
        &mut self,
        path: &Path,
        prices: &PriceTable,
        stats: &mut RefreshStats,
        parse: impl Fn(&Path, u64) -> std::io::Result<ParseOutput>,
    ) -> Result<()> {
        let key = path.to_string_lossy().to_string();
        let size = std::fs::metadata(path)?.len();
        let known: Option<(u64, u64)> = self
            .conn
            .query_row(
                "SELECT offset, size FROM files WHERE path = ?1",
                [&key],
                |r| Ok((r.get::<_, i64>(0)? as u64, r.get::<_, i64>(1)? as u64)),
            )
            .optional()?;
        let offset = match known {
            Some((_, prev_size)) if prev_size == size => return Ok(()),
            Some((off, prev_size)) if size >= prev_size => off,
            _ => 0,
        };

        let mut out = parse(path, offset)?;
        for ev in &mut out.events {
            prices.price(ev);
        }
        stats.events_added += self.upsert(&out.events)?;
        stats.bytes_read += out.next_offset.saturating_sub(offset);
        self.conn.execute(
            "INSERT INTO files (path, offset, size) VALUES (?1, ?2, ?3) \
             ON CONFLICT(path) DO UPDATE SET offset = excluded.offset, size = excluded.size",
            params![key, out.next_offset as i64, size as i64],
        )?;
        Ok(())
    }

    fn refresh_codex_fallback(
        &mut self,
        home: &Path,
        prices: &PriceTable,
        stats: &mut RefreshStats,
    ) -> Result<()> {
        self.conn.execute(
            "DELETE FROM events WHERE tool = 'Codex' AND dedup_key LIKE 'thread:%' AND session IN \
             (SELECT DISTINCT session FROM events WHERE tool = 'Codex' AND dedup_key NOT LIKE 'thread:%')",
            [],
        )?;
        let db = home.join(".codex/state_5.sqlite");
        if !db.exists() {
            return Ok(());
        }
        let known: HashSet<String> = {
            let mut stmt = self.conn.prepare(
                "SELECT DISTINCT session FROM events WHERE tool = 'Codex' AND dedup_key NOT LIKE 'thread:%'",
            )?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
            rows.flatten().collect()
        };
        if let Ok(mut events) = codex_thread_fallback(&db, &known) {
            for ev in &mut events {
                prices.price(ev);
            }
            stats.events_added += self.upsert(&events)?;
        }
        Ok(())
    }

    fn refresh_opencode(
        &mut self,
        home: &Path,
        prices: &PriceTable,
        stats: &mut RefreshStats,
    ) -> Result<()> {
        let db = home.join(".local/share/opencode/opencode.db");
        if !db.exists() {
            return Ok(());
        }
        let since: i64 = self
            .conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'opencode_since_ms'",
                [],
                |r| r.get::<_, String>(0),
            )
            .optional()?
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let mut events = read_opencode(&db, (since - OPENCODE_OVERLAP_MS).max(0))?;
        for ev in &mut events {
            prices.price(ev);
        }
        stats.events_added += self.upsert(&events)?;
        if let Some(max) = events.iter().map(|e| e.ts_ms).max() {
            self.conn.execute(
                "INSERT INTO meta (key, value) VALUES ('opencode_since_ms', ?1) \
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                [max.max(since).to_string()],
            )?;
        }
        Ok(())
    }

    /// Prices stored events again. Unpriced events always get another try (new
    /// model in the table, or an override added later). When the price table
    /// differs from the one used last time, events we priced ourselves
    /// (`Computed`) are recalculated too. Costs the tool wrote itself (`Stored`)
    /// are never touched. Approximate Codex thread fallbacks stay unpriced.
    fn reprice(&mut self, prices: &PriceTable) -> Result<()> {
        let fp = prices.fingerprint();
        let last: Option<String> = self
            .conn
            .query_row("SELECT value FROM meta WHERE key = 'price_fp'", [], |r| {
                r.get(0)
            })
            .optional()?;
        let changed = last.as_deref() != Some(fp.as_str());
        self.reprice_sources(prices, changed)?;
        if changed {
            self.conn.execute(
                "INSERT INTO meta (key, value) VALUES ('price_fp', ?1) \
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                [&fp],
            )?;
        }
        Ok(())
    }

    fn reprice_sources(&mut self, prices: &PriceTable, include_computed: bool) -> Result<()> {
        let sources = if include_computed {
            "('Unknown', 'Computed')"
        } else {
            "('Unknown')"
        };
        let pending: Vec<(i64, UsageEvent)> = {
            let mut stmt = self.conn.prepare(&format!(
                "SELECT rowid, tool, dedup_key, ts_ms, model, project, session, input, output, cache_read, \
                 cache_write_5m, cache_write_1h, reasoning FROM events \
                 WHERE cost_source IN {sources} AND dedup_key NOT LIKE 'thread:%' \
                 AND input + output + cache_read + cache_write_5m + cache_write_1h > 0"
            ))?;
            let rows = stmt.query_map([], |r| {
                let tool: String = r.get(1)?;
                // A row from a tool this build does not know is skipped, never counted as Claude.
                let Some(tool) = Tool::parse(&tool) else {
                    return Ok(None);
                };
                Ok(Some((
                    r.get::<_, i64>(0)?,
                    UsageEvent {
                        tool,
                        dedup_key: r.get(2)?,
                        ts_ms: r.get(3)?,
                        model: r.get(4)?,
                        project: r.get(5)?,
                        session: r.get(6)?,
                        input: r.get::<_, i64>(7)? as u64,
                        output: r.get::<_, i64>(8)? as u64,
                        cache_read: r.get::<_, i64>(9)? as u64,
                        cache_write_5m: r.get::<_, i64>(10)? as u64,
                        cache_write_1h: r.get::<_, i64>(11)? as u64,
                        reasoning: r.get::<_, i64>(12)? as u64,
                        cost_usd: None,
                        cost_source: CostSource::Unknown,
                    },
                )))
            })?;
            rows.flatten().flatten().collect()
        };
        for (rowid, mut ev) in pending {
            prices.price(&mut ev);
            if let Some(cost) = ev.cost_usd {
                self.conn.execute(
                    "UPDATE events SET cost_usd = ?1, cost_source = ?2 WHERE rowid = ?3",
                    params![cost, ev.cost_source.as_str(), rowid],
                )?;
            }
        }
        Ok(())
    }

    fn upsert(&mut self, events: &[UsageEvent]) -> Result<usize> {
        let tx = self.conn.transaction()?;
        let mut changed = 0;
        {
            let mut stmt = tx.prepare(UPSERT)?;
            for e in events {
                let day = Timestamp::from_millisecond(e.ts_ms)?
                    .to_zoned_datetime(self.tz.clone())
                    .date()
                    .to_string();
                changed += stmt.execute(params![
                    e.tool.as_str(),
                    e.dedup_key,
                    e.ts_ms,
                    day,
                    e.model,
                    e.project,
                    e.session,
                    e.input as i64,
                    e.output as i64,
                    e.cache_read as i64,
                    e.cache_write_5m as i64,
                    e.cache_write_1h as i64,
                    e.reasoning as i64,
                    e.cost_usd,
                    e.cost_source.as_str(),
                ])?;
            }
        }
        tx.commit()?;
        Ok(changed)
    }
}
