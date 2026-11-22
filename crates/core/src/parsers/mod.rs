pub mod claude;
pub mod codex;
pub mod opencode;

use crate::{Tool, UsageEvent};
use std::path::{Path, PathBuf};

pub struct ParseOutput {
    pub events: Vec<UsageEvent>,
    pub next_offset: u64,
}

/// A tool whose usage sits in files that grow over time (for example JSONL logs).
///
/// To add one: implement this trait, add a `Tool` variant, and list the source in
/// [`file_sources`]. The cache reads each file from the offset it stopped at, so
/// `parse` only has to return events from `from_offset` on and the next offset.
///
/// Sources that are not plain files (OpenCode reads a SQLite database) or that
/// need extra clean-up (the Codex thread fallback) stay as named steps in
/// `Cache::refresh`.
pub trait FileSource: Sync {
    fn tool(&self) -> Tool;
    /// Every file that may hold usage under `home`.
    fn discover(&self, home: &Path) -> Vec<PathBuf>;
    /// Events from `from_offset` on, and the offset to continue from next time.
    fn parse(&self, path: &Path, from_offset: u64) -> std::io::Result<ParseOutput>;
}

/// All file-based sources, in the order they are read.
pub fn file_sources() -> &'static [&'static dyn FileSource] {
    &[&claude::ClaudeSource, &codex::CodexSource]
}
