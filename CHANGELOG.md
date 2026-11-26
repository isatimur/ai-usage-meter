# Changelog

All notable changes go here. Format: [Keep a Changelog](https://keepachangelog.com/). Versions follow [SemVer](https://semver.org/).

## [Unreleased]

## [0.1.7] - 2026-11-26

- Fixed: macOS asked for Keychain access again and again. When the Claude login had expired, the app read the Keychain on every 5-minute poll and found the same expired token each time. It now waits 30 minutes before it reads again, and keeps showing "login expired" in between.
- Fixed: the app now remembers which Keychain item held the login and reads only that one, so one prompt, not one per item.
- Added a trace of every Keychain read to the macOS log (no token, no account name). Read it with `/usr/bin/log show --last 1h --predicate 'subsystem == "app.aiusagemeter.AIUsageMeter"'`.

## [0.1.6] - 2026-11-24

- Fixed: the app reported version 1.0 whatever the release. It now reports the release version, and the build fails if the two differ.

## [0.1.5] - 2026-11-24

- Added an app icon: a gauge over usage bars.

## [0.1.4] - 2026-11-24

- The release build is now notarized by Apple and the ticket is stapled, so macOS opens it without a warning.
- Fixed: earlier builds were signed with a debug entitlement (`get-task-allow`). Apple refuses to notarize that. The build now drops it and fails if it comes back.

## [0.1.3] - 2026-11-23

- Not released. Apple rejected the build because of the debug entitlement. Use 0.1.4.

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
