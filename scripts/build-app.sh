#!/bin/sh
# Builds the Rust core for Apple Silicon and Intel, joins the two into one library,
# generates the Xcode project, and builds a universal app.
#
# The app is signed ad-hoc by default. An ad-hoc signature changes on every build, so macOS
# forgets "Always Allow" for the Keychain each time. For a stable signature, name a code-signing
# identity: AIUM_SIGN_IDENTITY="Developer ID Application: Your Name (TEAMID)" scripts/build-app.sh
# With an identity the app is also built with the hardened runtime, which notarization requires.
set -e
cd "$(dirname "$0")/.."

export MACOSX_DEPLOYMENT_TARGET=14.0
# A Homebrew Rust ships only the host target. Prefer rustup, which can add the other one:
# rustup target add aarch64-apple-darwin x86_64-apple-darwin
if command -v rustup >/dev/null 2>&1; then RUSTC="$(rustup which rustc)"; export RUSTC; fi
for t in aarch64-apple-darwin x86_64-apple-darwin; do
  cargo build --release -p aium-core --target "$t"
done
mkdir -p target/universal
lipo -create \
  target/aarch64-apple-darwin/release/libaium_core.a \
  target/x86_64-apple-darwin/release/libaium_core.a \
  -output target/universal/libaium_core.a

cd app
xcodegen generate
if [ -n "$AIUM_SIGN_IDENTITY" ]; then
  xcodebuild -project AIUsageMeter.xcodeproj -scheme AIUsageMeter -configuration Release \
    -derivedDataPath ../target/xcode -destination "generic/platform=macOS" CODE_SIGN_IDENTITY="$AIUM_SIGN_IDENTITY" \
    ENABLE_HARDENED_RUNTIME=YES OTHER_CODE_SIGN_FLAGS="--timestamp" build
else
  xcodebuild -project AIUsageMeter.xcodeproj -scheme AIUsageMeter -configuration Release \
    -derivedDataPath ../target/xcode -destination "generic/platform=macOS" CODE_SIGN_IDENTITY="-" build
fi
echo "Built: target/xcode/Build/Products/Release/AIUsageMeter.app"
