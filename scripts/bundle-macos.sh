#!/usr/bin/env bash
# Assemble a macOS .app bundle around the built binary so it has an icon and a
# proper Dock/Finder presence. Usage: scripts/bundle-macos.sh [binary] [outdir]
set -euo pipefail

BIN="${1:-target/release/timetracker}"
OUT="${2:-dist}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

APP="$OUT/FableTime.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

cp "$BIN" "$APP/Contents/MacOS/timetracker"
chmod +x "$APP/Contents/MacOS/timetracker"
cp "$ROOT/assets/icon.icns" "$APP/Contents/Resources/icon.icns"
cp "$ROOT/macos/Info.plist" "$APP/Contents/Info.plist"

echo "built $APP"
