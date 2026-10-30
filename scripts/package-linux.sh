#!/usr/bin/env bash
#
# Build a Linux release tarball for PCB-CAM.
#
# Usage:  ./scripts/package-linux.sh
# Output: dist/pcb-cam-<version>-linux-<arch>.tar.gz
#
# The archive contains the release binary, LICENSE and THIRD_PARTY_LICENSES.md.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if [[ "$(uname -s)" != "Linux" ]]; then
    echo "error: this packaging script is for Linux only" >&2
    exit 1
fi

if ! command -v cargo >/dev/null 2>&1; then
    echo "error: cargo is not installed" >&2
    exit 1
fi

VERSION="$(cargo pkgid -p pcb-cam | sed -E 's/.*[@#]//')"
if [[ -z "$VERSION" ]]; then
    echo "error: could not determine the PCB-CAM version" >&2
    exit 1
fi
ARCH="$(uname -m)"
STAGE_NAME="pcb-cam-$VERSION-linux-$ARCH"
STAGE_DIR="$ROOT/dist/$STAGE_NAME"
TARBALL="$ROOT/dist/$STAGE_NAME.tar.gz"

echo "Building PCB-CAM $VERSION for linux-$ARCH..."
cargo build --release --locked -p pcb-cam

rm -rf "$STAGE_DIR"
mkdir -p "$STAGE_DIR"
cp target/release/pcb-cam LICENSE THIRD_PARTY_LICENSES.md "$STAGE_DIR/"
chmod +x "$STAGE_DIR/pcb-cam"

for file in pcb-cam LICENSE THIRD_PARTY_LICENSES.md; do
    if [[ ! -s "$STAGE_DIR/$file" ]]; then
        echo "error: missing or empty $file in the release staging directory" >&2
        exit 1
    fi
done

rm -f "$TARBALL"
tar -C "$ROOT/dist" -czf "$TARBALL" "$STAGE_NAME"
tar -tzf "$TARBALL" | grep -qx "$STAGE_NAME/pcb-cam"

echo
echo "Created:"
echo "  $TARBALL"
