# Contributing

Thanks for helping. Keep changes small and focused.

## Setup

```sh
cargo test --workspace      # Rust core and CLI
scripts/build-app.sh        # SwiftUI app (needs Xcode and xcodegen)
```

All tests must pass before you open a pull request.

## Where things live

- `crates/core/src` — parsers, cache, pricing, aggregation, limits, report.
- `crates/core/tests` — integration tests. Test data is synthetic. Never commit real logs, tokens or account ids.
- `app/Sources` — SwiftUI shell. Keep logic in Rust and keep the shell thin.

## Add a new tool

For a tool that keeps usage in files that grow (for example JSONL logs):

1. Add a variant to `Tool` in `crates/core/src/event.rs`.
2. Create `crates/core/src/parsers/<tool>.rs`. Implement the `FileSource` trait from `parsers/mod.rs`: `tool`, `discover(home)` and `parse(path, from_offset)`. Follow `claude.rs`.
3. List your source in `file_sources()` in `parsers/mod.rs`. The cache does the rest: offsets, dedup, pricing, aggregation.
4. Add synthetic fixtures and tests in `crates/core/tests`. Never use real logs.
5. Add price entries if the models are missing.
6. Document what the parser reads in the README table.

A tool that keeps usage in a database (OpenCode) or needs extra clean-up (the Codex thread fallback) does not fit the trait. Add a named step to `Cache::refresh` instead.

## Rules

- Never print or store a token. Tests already check that errors do not echo one.
- Private endpoints need an off switch and a plain note in the README.
- Commit messages use Conventional Commits (`feat:`, `fix:`, `docs:`).
