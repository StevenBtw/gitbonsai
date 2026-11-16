#!/usr/bin/env bash
# The sha256 of every SVG `gitbonsai gallery` writes, in the format CI
# compares with gallery.sha256. After a change to how trees look:
#   scripts/gallery-hashes.sh > gallery.sha256
set -euo pipefail
dir=$(mktemp -d)
trap 'rm -rf "$dir"' EXIT
cargo run --release --quiet -- gallery --output-dir "$dir" --quiet
cd "$dir"
# Windows marks binary reads with '*'; drop it so only content differs.
sha256sum *.svg | sed 's/ \*/  /' | sort -k2
