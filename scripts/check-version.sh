#!/bin/sh
# Fails if the built app reports a different version than the release.
# Usage: check-version.sh path/to/App.app expected-version
set -e
APP="${1:?usage: check-version.sh path/to/App.app expected-version}"
WANT="${2:?usage: check-version.sh path/to/App.app expected-version}"
GOT="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$APP/Contents/Info.plist")"
if [ "$GOT" != "$WANT" ]; then
  echo "error: $APP reports version $GOT, expected $WANT." >&2
  exit 1
fi
echo "ok: $APP reports version $GOT"
