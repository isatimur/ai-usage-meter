//! Turn off the telemetry and feedback switches that are on by default.
//!
//! `plan` only reads. `apply` writes, after it saved a backup next to each file
//! and checked that the file did not change since the plan. Nothing here touches
//! a shell profile or a file outside the tools' own config.

use serde::Serialize;
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
pub struct Change {
    pub file: PathBuf,
    pub summary: String,
    /// Lines that go away ("- ...") and lines that appear ("+ ...").
    pub diff: Vec<String>,
    #[serde(skip)]
    before: String,
    #[serde(skip)]
    after: String,
}

/// Keys set under "env" in Claude Code's settings.
const CLAUDE_ENV: &[&str] = &[
    "DISABLE_TELEMETRY",
    "DISABLE_ERROR_REPORTING",
    "CLAUDE_CODE_DISABLE_FEEDBACK_SURVEY",
];

fn line_diff(before: &str, after: &str) -> Vec<String> {
    let mut b: Vec<&str> = before.lines().collect();
    let mut out = Vec::new();
    let mut added = Vec::new();
    for l in after.lines() {
        if let Some(i) = b.iter().position(|x| *x == l) {
            b.remove(i);
        } else {
            added.push(format!("+ {l}"));
        }
    }
    out.extend(b.into_iter().map(|l| format!("- {l}")));
    out.extend(added);
    out
}

fn pretty(v: &Value) -> String {
    let mut s = serde_json::to_string_pretty(v).unwrap_or_default();
    s.push('\n');
    s
}

/// Sets `path` (object keys) to `value` in a JSON text. None when nothing
/// changes. Err when the text is not a JSON object.
fn json_set(text: &str, path: &[&str], value: Value) -> Result<Option<String>, String> {
    let mut root: Value = if text.trim().is_empty() {
        Value::Object(Map::new())
    } else {
        serde_json::from_str(text).map_err(|e| format!("not valid JSON ({e})"))?
    };
    if !root.is_object() {
        return Err("not a JSON object".into());
    }
    let mut cur = &mut root;
    for (i, key) in path.iter().enumerate() {
        let obj = cur.as_object_mut().ok_or("unexpected value in the way")?;
        if i + 1 == path.len() {
            if obj.get(*key) == Some(&value) {
                return Ok(None);
            }
            obj.insert((*key).to_string(), value);
            return Ok(Some(pretty(&root)));
        }
        cur = obj
            .entry((*key).to_string())
            .or_insert_with(|| Value::Object(Map::new()));
    }
    Ok(None)
}

/// Sets `key = false` under `[section]`. None when it is already false.
/// Err when the file defines the section in a form this code does not edit.
fn toml_disable(text: &str, section: &str, key: &str) -> Result<Option<String>, String> {
    let header = format!("[{section}]");
    let inline = |l: &str| {
        let l = l.trim_start();
        l.strip_prefix(section)
            .is_some_and(|r| r.trim_start().starts_with(['=', '.']))
    };
    if text.lines().any(inline) {
        return Err(format!(
            "{section} is defined as an inline or dotted key; edit it by hand"
        ));
    }
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let bare = |l: &str| l.split('#').next().unwrap_or("").trim().to_string();
    let Some(start) = lines.iter().position(|l| bare(l) == header) else {
        let mut out = text.to_string();
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&format!("{header}\n{key} = false\n"));
        return Ok(Some(out));
    };
    let end = lines[start + 1..]
        .iter()
        .position(|l| bare(l).starts_with('['))
        .map_or(lines.len(), |i| start + 1 + i);
    for i in start + 1..end {
        if let Some((k, v)) = bare(&lines[i]).split_once('=') {
            if k.trim() == key {
                if v.trim() == "false" {
                    return Ok(None);
                }
                lines[i] = format!("{key} = false");
                return Ok(Some(lines.join("\n") + "\n"));
            }
        }
    }
    lines.insert(start + 1, format!("{key} = false"));
    Ok(Some(lines.join("\n") + "\n"))
}

pub fn plan(home: &Path) -> Result<Vec<Change>, Vec<String>> {
    let mut out = Vec::new();
    let mut problems = Vec::new();
    let mut add =
        |file: PathBuf, summary: &str, edit: &dyn Fn(&str) -> Result<Option<String>, String>| {
            let before = std::fs::read_to_string(&file).unwrap_or_default();
            match edit(&before) {
                Ok(Some(after)) => out.push(Change {
                    diff: line_diff(&before, &after),
                    file,
                    summary: summary.to_string(),
                    before,
                    after,
                }),
                Ok(None) => {}
                Err(e) => problems.push(format!("{}: {e}", file.display())),
            }
        };

    if home.join(".claude").is_dir() {
        add(
            home.join(".claude/settings.json"),
            "Claude Code: turn off telemetry, error reports and feedback surveys",
            &|t| {
                let mut text = t.to_string();
                let mut changed = false;
                for k in CLAUDE_ENV {
                    if let Some(next) = json_set(&text, &["env", k], Value::String("1".into()))? {
                        text = next;
                        changed = true;
                    }
                }
                Ok(changed.then_some(text))
            },
        );
    }
    if home.join(".codex").is_dir() {
        add(
            home.join(".codex/config.toml"),
            "Codex: turn off analytics and feedback",
            &|t| {
                let mut text = t.to_string();
                let mut changed = false;
                for s in ["analytics", "feedback"] {
                    if let Some(next) = toml_disable(&text, s, "enabled")? {
                        text = next;
                        changed = true;
                    }
                }
                Ok(changed.then_some(text))
            },
        );
    }
    if home.join(".gemini").is_dir() {
        add(
            home.join(".gemini/settings.json"),
            "Gemini CLI: turn off usage statistics",
            &|t| json_set(t, &["usageStatisticsEnabled"], Value::Bool(false)),
        );
    }
    if problems.is_empty() {
        Ok(out)
    } else {
        Err(problems)
    }
}

/// Writes every change. Returns the backup files. Stops at the first problem;
/// files written before it stay written and keep their backups.
pub fn apply(changes: &[Change], stamp: &str) -> Result<Vec<PathBuf>, String> {
    let mut backups = Vec::new();
    for c in changes {
        let now = std::fs::read_to_string(&c.file).unwrap_or_default();
        if now != c.before {
            return Err(format!(
                "{} changed since the plan was made. Nothing was written to it. Run again.",
                c.file.display()
            ));
        }
        if c.file.exists() {
            let name = c
                .file
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("file");
            let backup = c.file.with_file_name(format!("{name}.aium-backup-{stamp}"));
            std::fs::copy(&c.file, &backup).map_err(|e| format!("backup failed: {e}"))?;
            backups.push(backup);
        }
        let tmp = c.file.with_extension("aium-tmp");
        std::fs::write(&tmp, &c.after).map_err(|e| format!("write failed: {e}"))?;
        if let Ok(meta) = std::fs::metadata(&c.file) {
            std::fs::set_permissions(&tmp, meta.permissions()).ok();
        }
        std::fs::rename(&tmp, &c.file).map_err(|e| format!("replace failed: {e}"))?;
    }
    Ok(backups)
}
