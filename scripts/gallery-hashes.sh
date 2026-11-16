#!/usr/bin/env bash
# The sha256 of every SVG `gitbonsai gallery` writes, in the format CI
# compares with gallery.sha256. The version stamp in each SVG's metadata is
# left out, so a version bump changes no hash. After a change to how trees
# look:
#   scripts/gallery-hashes.sh > gallery.sha256
# With a directory, hashes the SVGs already rendered there (CI does this).
set -euo pipefail
if [ $# -gt 0 ]; then
  dir=$1
else
  dir=$(mktemp -d)
  trap 'rm -rf "$dir"' EXIT
  cargo run --release --quiet -- gallery --output-dir "$dir" --quiet
fi
cd "$dir"
for f in *.svg; do
  sum=$(sed -E 's/<metadata>gitbonsai [^;]*;/<metadata>gitbonsai;/' "$f" | sha256sum | cut -d' ' -f1)
  printf '%s  %s\n' "$sum" "$f"
done | sort -k2
