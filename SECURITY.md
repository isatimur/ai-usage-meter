# Security

## Reporting a problem

Use GitHub's private "Report a vulnerability" form on this repository. Do not open a public issue for a security problem. Expect a first answer within a week.

## What the app touches

- It reads local logs of Claude Code, Codex and OpenCode.
- With the limits switch on, it reads one token from the Keychain item "Claude Code-credentials" and one from `~/.codex/auth.json`. It keeps the Claude token in memory only. It sends each token to the provider's own server and nowhere else.
- An OpenRouter key, if you add one, is stored in your Keychain.
- The app has no server of its own and sends no telemetry.

A bug that leaks a token, writes one to disk or a log, or sends it to another host is a security bug. Please report it.
