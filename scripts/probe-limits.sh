#!/bin/bash
# One-off probe: shows the shape of the Claude and Codex usage responses.
# It reads your local login tokens, sends them only to Anthropic and OpenAI,
# and prints the response with email and ids removed. It never prints a token.
set -u
command -v jq >/dev/null || { echo "jq is required (brew install jq)"; exit 1; }

REDACT='del(.. | .email?, .user_id?, .account_id?)'

echo "== claude =="
CT=$(security find-generic-password -s "Claude Code-credentials" -w 2>/dev/null \
  | jq -r '.claudeAiOauth.accessToken // empty')
if [ -z "$CT" ]; then
  echo "no Claude token found"
else
  curl -s -m 20 \
    -H "Authorization: Bearer $CT" \
    -H "anthropic-beta: oauth-2025-04-20" \
    https://api.anthropic.com/api/oauth/usage | jq -c "$REDACT"
fi
unset CT

echo "== codex =="
AUTH="$HOME/.codex/auth.json"
if [ ! -f "$AUTH" ]; then
  echo "no ~/.codex/auth.json"
else
  XT=$(jq -r '.tokens.access_token // empty' "$AUTH")
  AID=$(jq -r '.tokens.account_id // empty' "$AUTH")
  if [ -z "$XT" ]; then
    echo "no Codex access token in auth.json"
  else
    curl -s -m 20 \
      -H "Authorization: Bearer $XT" \
      -H "ChatGPT-Account-Id: $AID" \
      https://chatgpt.com/backend-api/wham/usage | jq -c "$REDACT"
  fi
fi
unset XT AID
