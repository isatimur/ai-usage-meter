#!/bin/sh
# Builds the universal app and packs it into a DMG: dist/AIUsageMeter-<version>.dmg
#
# Local test build (ad-hoc signature, no notarization):
#   scripts/package.sh
# Release build (Developer ID signature, notarized, stapled):
#   AIUM_SIGN_IDENTITY="Developer ID Application: Your Name (TEAMID)" \
#   AIUM_NOTARY_PROFILE="your-notarytool-profile" scripts/package.sh
# Create the notary profile once with:
#   xcrun notarytool store-credentials your-notarytool-profile
set -e
cd "$(dirname "$0")/.."

VERSION="${AIUM_VERSION:-$(awk -F'"' '/^version/ {print $2; exit}' crates/core/Cargo.toml)}"
VERSION="${VERSION#v}"
APP="target/xcode/Build/Products/Release/AIUsageMeter.app"
DMG="dist/AIUsageMeter-$VERSION.dmg"

scripts/build-app.sh

rm -rf dist/stage && mkdir -p dist/stage
cp -R "$APP" dist/stage/
ln -s /Applications dist/stage/Applications
rm -f "$DMG"
hdiutil create -volname "AI Usage Meter" -srcfolder dist/stage -ov -format UDZO "$DMG" >/dev/null
rm -rf dist/stage

if [ -n "$AIUM_SIGN_IDENTITY" ]; then
  codesign --force --timestamp --sign "$AIUM_SIGN_IDENTITY" "$DMG"
fi

if [ -n "$AIUM_SIGN_IDENTITY" ] && [ -n "$AIUM_NOTARY_PROFILE" ]; then
  xcrun notarytool submit "$DMG" --keychain-profile "$AIUM_NOTARY_PROFILE" --wait
  xcrun stapler staple "$DMG"
else
  echo "Not notarized: set AIUM_SIGN_IDENTITY and AIUM_NOTARY_PROFILE for a release build."
fi

shasum -a 256 "$DMG" | tee "$DMG.sha256"
echo "Packed: $DMG"
