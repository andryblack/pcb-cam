#!/usr/bin/env bash
#
# Build an unsigned/ad-hoc-signed PCB-CAM.app for macOS and zip it.
#
# Usage:  ./scripts/package-macos.sh
# Output: dist/PCB-CAM.app and dist/PCB-CAM-macos.zip

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "error: this packaging script is for macOS only" >&2
    exit 1
fi

if ! command -v cargo >/dev/null 2>&1; then
    echo "error: cargo is not installed" >&2
    exit 1
fi

APP_NAME="PCB-CAM"
VERSION="$(cargo pkgid -p pcb-cam | sed -E 's/.*[@#]//')"
APP_DIR="$ROOT/dist/$APP_NAME.app"
ZIP_PATH="$ROOT/dist/$APP_NAME-macos.zip"

echo "Building PCB-CAM $VERSION..."
cargo build --release --locked -p pcb-cam

rm -rf "$APP_DIR"
mkdir -p "$APP_DIR/Contents/MacOS" "$APP_DIR/Contents/Resources"

cp target/release/pcb-cam "$APP_DIR/Contents/MacOS/pcb-cam"
cp assets/icon.icns "$APP_DIR/Contents/Resources/AppIcon.icns"

cat > "$APP_DIR/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>$APP_NAME</string>
    <key>CFBundleDisplayName</key>
    <string>$APP_NAME</string>
    <key>CFBundleIdentifier</key>
    <string>com.pcb-cam.app</string>
    <key>CFBundleVersion</key>
    <string>$VERSION</string>
    <key>CFBundleShortVersionString</key>
    <string>$VERSION</string>
    <key>CFBundleExecutable</key>
    <string>pcb-cam</string>
    <key>CFBundleIconFile</key>
    <string>AppIcon</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleSignature</key>
    <string>????</string>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>LSApplicationCategoryType</key>
    <string>public.app-category.developer-tools</string>
</dict>
</plist>
PLIST

if command -v codesign >/dev/null 2>&1; then
    codesign --force --deep --sign - "$APP_DIR" >/dev/null 2>&1 || true
fi

rm -f "$ZIP_PATH"
ditto -c -k --keepParent "$APP_DIR" "$ZIP_PATH"

echo
echo "Created:"
echo "  $APP_DIR"
echo "  $ZIP_PATH"
