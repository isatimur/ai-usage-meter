#!/bin/sh
# Fails when the built app carries a debug entitlement. Apple refuses to notarize such an app.
# Usage: scripts/check-entitlements.sh path/to/AIUsageMeter.app
set -e
APP="${1:?usage: check-entitlements.sh path/to/App.app}"
ENT="$(codesign -d --entitlements - "$APP" 2>&1)"
if echo "$ENT" | grep -q "com.apple.security.get-task-allow"; then
  echo "error: $APP is signed with com.apple.security.get-task-allow (a debug entitlement)." >&2
  exit 1
fi
echo "ok: no debug entitlement in $APP"
