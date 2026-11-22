# What the tools send

A measurement, not a verdict. It records what two tools sent on one Mac on one day, so you can check it and repeat it.

- **Date:** 2026-11-22. macOS, Apple Silicon.
- **Versions:** Claude Code 2.1.330, Codex 0.170.0.
- **Method:** each tool ran once, logged in, in an empty folder, with the prompt "Reply with the single word ok.". Traffic went through a local mitmproxy. The proxy CA was passed to that one process (`NODE_EXTRA_CA_CERTS`, `SSL_CERT_FILE`); the system trust store was not changed.
- **What was recorded:** host, method, path, header names, and the JSON key names of request bodies with value types. Header values, tokens and every other body value were dropped by the proxy script before they were written. Prompt and answer text was not recorded.
- **First attempt:** the script kept URL paths whole, and a path contained a credential that a plugin puts in the URL. The script now masks every path segment that is not short and plain. If you repeat this, mask paths from the start.

## Claude Code

Requests to Anthropic in one short run:

| Request | What it is |
|---|---|
| `POST /api/event_logging/v2/batch` (4 times, about 1 MB and 400 events each) | Usage events. See below. |
| `POST /api/eval/sdk-…` | Feature-flag evaluation. |
| `GET /api/claude_cli/bootstrap`, `/api/oauth/account/settings`, `/api/organizations/…/model_selector/cc`, plugin and marketplace lists | Account and settings fetches. |
| `POST /v1/messages` | The model call. |
| `POST http-intake.logs.us5.datadoghq.com/api/v2/logs` | Logs sent to Datadog. |

The event batches held events of type `ClaudeCodeInternalEvent`. Most were `tengu_skill_loaded` (289 of 400 in the first batch) and `tengu_feature_ok` (87). Each event carried these field names: `device_id`, `session_id`, `auth.account_uuid`, `auth.organization_uuid`, `model`, `user_type`, `betas`, `entrypoint`, `client_type`, and an `env` object with `platform`, `arch`, `terminal`, `shell`, `node_version`, `package_managers`, `runtimes`, `version`, `build_time` and several `is_*` flags. A skill event also had `skill_name`.

The `additional_metadata` field is a JSON string. Its keys are mostly counts, flags and durations (`subscription_type`, `file_count`, `mcpClientCount`, `is_git`, `has_remote`, `remote_host_class`, `permissionMode`, and many `has_*` flags). Plugin and marketplace names appear under keys that end in `_redacted` or `_hash`.

**Not known:** the values. This run did not record them, so it does not say whether `skill_name` is a plain name or a hash. It also does not say how much of the 400-event batches was a backlog from earlier sessions.

## Codex

Requests to `chatgpt.com` in one short run:

| Request | What it is |
|---|---|
| `POST /backend-api/codex/analytics-events/events` (6 times) | Analytics events. See below. |
| `POST ab.chatgpt.com/otlp/v1/metrics` | Metrics export to an experiments host. |
| `GET /backend-api/codex/models`, `/wham/accounts/check`, `/wham/usage`, `/wham/settings/user`, plugin lists | Account and settings fetches. |
| `GET /backend-api/codex/responses` | The model call. |

The analytics requests carry `authorization`, `cookie` and `chatgpt-account-id` headers. The event types were `codex_thread_initialized`, `codex_hook_run` (4 times) and `codex_turn_event`. Field names include `thread_id`, `session_id`, `turn_id`, `model`, `model_provider`, `sandbox_policy`, `approval_policy`, `reasoning_effort`, `sandbox_network_access`, runtime OS, version and architecture, counts of shell commands, file changes, MCP calls and web searches, token counts, `hook_name` and `active_plugin_ids_at_turn_start`.

**Not known:** the values, and what `hook_name` and plugin ids contain on a machine with your own hooks and plugins.

## What this does not show

- Other tools (OpenCode, Gemini CLI). Only their switches were read, not their traffic.
- Other versions, other accounts, long sessions, or the content of a real working session.
- What either company does with the data.
- Whether each documented switch stops each request. `aium privacy` lists the switches. A second capture with the switches set would test them.

## Repeat it

Install mitmproxy, write a small addon that records the fields above and nothing else, start `mitmdump`, and run each tool with `HTTPS_PROXY` pointing at it and the CA given to that process only. Run from an empty folder. Unset any `CLAUDE_CODE_*` variables inherited from a parent session.
