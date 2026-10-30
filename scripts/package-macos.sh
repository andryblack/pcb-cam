#!/usr/bin/env bash
#
# Build an unsigned/ad-hoc-signed PCB-CAM.app for macOS and zip it.
#
# Usage:
#   ./scripts/package-macos.sh [--universal] [--binary PATH] [--output ZIP]
#
# Options:
#   --universal     Build a universal (arm64 + x86_64) binary.
#   --binary PATH   Package an existing binary instead of building.
#   --output ZIP    Write the zip to ZIP (default: dist/PCB-CAM-macos.zip).
#   -h, --help      Show this help.
#
# The app bundle is always written to dist/PCB-CAM.app.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

usage() {
    cat <<'EOF'
Usage: ./scripts/package-macos.sh [--universal] [--binary PATH] [--output ZIP]

Build an unsigned/ad-hoc-signed PCB-CAM.app for macOS and zip it.

Options:
  --universal     Build a universal (arm64 + x86_64) binary.
  --binary PATH   Package an existing binary instead of building.
  --output ZIP    Write the zip to ZIP (default: dist/PCB-CAM-macos.zip).
  -h, --help      Show this help.

The app bundle is always written to dist/PCB-CAM.app.
EOF
}

universal=false
binary=""
zip_path=""

while (($#)); do
    case "$1" in
        --universal)
            universal=true
            ;;
        --binary)
            if (($# < 2)); then
                echo "error: --binary requires a path" >&2
                exit 2
            fi
            binary="$2"
            shift
            ;;
        --output)
            if (($# < 2)); then
                echo "error: --output requires a path" >&2
                exit 2
            fi
            zip_path="$2"
            shift
            ;;
        -h | --help)
            usage
            exit 0
            ;;
        *)
            echo "error: unknown argument: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
    shift
done

if [[ "$universal" == true && -n "$binary" ]]; then
    echo "error: --universal and --binary are mutually exclusive" >&2
    exit 2
fi

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
if [[ -z "$VERSION" ]]; then
    echo "error: could not determine the PCB-CAM version" >&2
    exit 1
fi
APP_DIR="$ROOT/dist/$APP_NAME.app"
if [[ -z "$zip_path" ]]; then
    zip_path="$ROOT/dist/$APP_NAME-macos.zip"
fi

if [[ "$universal" == true ]]; then
    if ! command -v rustup >/dev/null 2>&1; then
        echo "error: rustup is required to build a universal binary" >&2
        exit 1
    fi
    if ! command -v lipo >/dev/null 2>&1; then
        echo "error: lipo is required to build a universal binary" >&2
        exit 1
    fi

    rustup target add aarch64-apple-darwin x86_64-apple-darwin
    export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-11.0}"

    echo "Building PCB-CAM $VERSION for aarch64-apple-darwin..."
    cargo build --release --locked -p pcb-cam --target aarch64-apple-darwin
    echo "Building PCB-CAM $VERSION for x86_64-apple-darwin..."
    cargo build --release --locked -p pcb-cam --target x86_64-apple-darwin

    mkdir -p "$ROOT/target/universal-apple-darwin/release"
    lipo -create \
        "$ROOT/target/aarch64-apple-darwin/release/pcb-cam" \
        "$ROOT/target/x86_64-apple-darwin/release/pcb-cam" \
        -output "$ROOT/target/universal-apple-darwin/release/pcb-cam"
    binary="$ROOT/target/universal-apple-darwin/release/pcb-cam"
elif [[ -z "$binary" ]]; then
    echo "Building PCB-CAM $VERSION..."
    cargo build --release --locked -p pcb-cam
    binary="$ROOT/target/release/pcb-cam"
fi

if [[ ! -x "$binary" ]]; then
    echo "error: binary not found or not executable: $binary" >&2
    exit 1
fi

rm -rf "$APP_DIR"
mkdir -p "$APP_DIR/Contents/MacOS" "$APP_DIR/Contents/Resources"

cp "$binary" "$APP_DIR/Contents/MacOS/pcb-cam"
cp assets/icon.icns "$APP_DIR/Contents/Resources/AppIcon.icns"
cp LICENSE "$APP_DIR/Contents/Resources/LICENSE.txt"
cp THIRD_PARTY_LICENSES.md "$APP_DIR/Contents/Resources/THIRD_PARTY_LICENSES.txt"

for file in LICENSE.txt THIRD_PARTY_LICENSES.txt; do
    if [[ ! -s "$APP_DIR/Contents/Resources/$file" ]]; then
        echo "error: missing or empty $file in the app bundle" >&2
        exit 1
    fi
done

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
    <key>NSHumanReadableCopyright</key>
    <string>Copyright © 2026 andryblack. Third-party notices: Contents/Resources/THIRD_PARTY_LICENSES.txt</string>
</dict>
</plist>
PLIST

if command -v codesign >/dev/null 2>&1; then
    codesign --force --deep --sign - "$APP_DIR" >/dev/null 2>&1 || true
fi

mkdir -p "$(dirname "$zip_path")"
rm -f "$zip_path"
ditto -c -k --keepParent --norsrc "$APP_DIR" "$zip_path"

echo
echo "Created:"
echo "  $APP_DIR"
echo "  $zip_path"
