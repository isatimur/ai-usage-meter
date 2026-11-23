# Changelog

All notable changes go here. Format: [Keep a Changelog](https://keepachangelog.com/). Versions follow [SemVer](https://semver.org/).

## [Unreleased]

## [0.1.3] - 2026-11-23

- The release build is now notarized by Apple and the ticket is stapled, so macOS opens it without a warning.
## [0.1.2] - 2026-11-23

- Fixed: the popover no longer grows taller than the screen. The content scrolls and the footer stays in view.
- CI now also runs the Swift tests as x86_64, so the Intel part of the universal app is exercised.
## [0.1.1] - 2026-11-23

- Fixed: the `.sha256` file now works with `shasum -c` (it held a `dist/` path before).
- Fixed: Claude plan limits disappeared when Claude Code left two items in the Keychain and macOS returned the one with no login. The app now tries every item, newest first.
- Added Swift unit tests (Keychain item choice, JSON contract with the Rust core), run in CI.
- Fixed: a changed price (a `prices.json` override or a new bundled table) now reprices past events. Before, only events with no price were updated.

## [0.1.0]

First public release. It includes:

- Usage and cost for Claude Code, Codex and OpenCode, with optional OpenRouter key usage.
- Today, 7-day and month views; totals by tool, model and project; live sessions; daily chart.
- Plan limits for Claude and Codex (opt-in), with a straight-line run-out forecast.
- Notifications at 75% and 90% of a window and on forecast run-out (opt-in).
- Monthly budget with a month-end projection.
- Click a session to jump to its terminal.
- Daily Obsidian notes.
- Provider status (opt-in): an incident line for Claude and OpenAI, from their public status pages.
- `aium` command-line tool, with `--json` and a one-line `--oneline` output for status lines.
- Privacy section in Settings, and `aium privacy`: show which telemetry, error-report and feedback switches are set for Claude Code, Codex, OpenCode and Gemini CLI, and how to turn them off.
- `aium privacy --disable` and a Settings button: turn off those switches with a diff, a backup and a confirm step.
- `docs/what-tools-send.md`: a measured record of what Claude Code and Codex sent in one run.
- Universal macOS build (Apple Silicon and Intel), signed and notarized DMG through the release workflow.
