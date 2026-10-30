#!/usr/bin/env bash
#
# Regenerate THIRD_PARTY_LICENSES.md from Cargo.lock.
#
# Usage:
#   ./scripts/gen-licenses.sh          # rewrite the notices file
#   ./scripts/gen-licenses.sh --check  # fail if the file is out of date
#
# Requires cargo-about with the CLI feature:
#   cargo install --locked cargo-about --version 0.9.3 --features cli

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

check=false
case "${1:-}" in
    "") ;;
    --check) check=true ;;
    *)
        echo "usage: $0 [--check]" >&2
        exit 2
        ;;
esac

if ! command -v cargo-about >/dev/null 2>&1; then
    echo "error: cargo-about is not installed" >&2
    echo "install it with: cargo install --locked cargo-about --version 0.9.3 --features cli" >&2
    exit 1
fi

if ! command -v python3 >/dev/null 2>&1; then
    echo "error: python3 is required to normalize the generated notices" >&2
    exit 1
fi

tmp="$(mktemp)"
combined="$(mktemp)"
trap 'rm -f "$tmp" "$combined"' EXIT

# cargo-about renders the dependency notices. The font notices are maintained
# by hand because they live in font files, not in the package license metadata.
# Fetching first and then running offline keeps the output reproducible.
cargo fetch --locked
cargo about generate \
    --workspace \
    --fail \
    --locked \
    --offline \
    --config about.toml \
    --output-file "$tmp" \
    about.hbs

# cargo-about falls back to a generic MIT template for crates that do not
# package a license file. The per-crate copyright lines rendered above carry
# the actual attribution, so replace the remaining template placeholders and
# normalize line endings.
python3 - "$tmp" <<'PY'
import sys

path = sys.argv[1]
with open(path, encoding="utf-8") as file:
    text = file.read()

text = text.replace(
    "MIT License Copyright (c) <year> <copyright holders>",
    "MIT License\n\nCopyright (c) the copyright holders listed above.",
)
text = text.replace(
    "Copyright (c) <year> <copyright holders>",
    "Copyright (c) the copyright holders listed above.",
)

with open(path, "w", encoding="utf-8", newline="\n") as file:
    file.write(text)
PY

{
    cat "$tmp"
    printf '\n'
    cat third-party/fonts.md
} > "$combined"

if "$check"; then
    if ! diff -u THIRD_PARTY_LICENSES.md "$combined"; then
        echo "error: THIRD_PARTY_LICENSES.md is out of date" >&2
        echo "run ./scripts/gen-licenses.sh and commit the result" >&2
        exit 1
    fi
    echo "THIRD_PARTY_LICENSES.md is up to date"
else
    mv "$combined" THIRD_PARTY_LICENSES.md
    chmod 644 THIRD_PARTY_LICENSES.md
    echo "wrote THIRD_PARTY_LICENSES.md"
fi
