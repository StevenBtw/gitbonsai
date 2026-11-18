#!/usr/bin/env bash
# Builds gitbonsai.dev into a folder (default _site): the page, the hero trees
# and a timelapse video per example and theme. Needs a release build of
# gitbonsai (or GITBONSAI=<path>), python3 with cairosvg and pillow, and ffmpeg.
#
#   cargo build --release && bash site/build.sh _site
set -euo pipefail
site=$(cd "$(dirname "$0")" && pwd)
out=${1:-_site}
bin=${GITBONSAI:-$site/../target/release/gitbonsai}
version=$(sed -n 's/^version = "\(.*\)"/\1/p' "$site/../Cargo.toml" | head -1)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

rm -rf "$out"
mkdir -p "$out/hero"
cp -r "$site/fonts" "$out/fonts"
for f in index.html app.js style.css tokens.css fonts.css CNAME; do
  sed "s/@VERSION@/$version/g" "$site/$f" > "$out/$f"
done

"$bin" render fixture:showcase --output-dir "$work/showcase" -q
"$bin" render "$site/snapshots/stevenbtw.snapshot.json" --output-dir "$work/tree" -q
for t in light dark; do
  cp "$work/showcase/bonsai-$t.svg" "$out/hero/showcase-$t.svg"
  cp "$work/tree/bonsai-$t.svg" "$out/hero/tree-$t.svg"
done

"$bin" timelapse fixture:showcase --every 1m --output-dir "$work/tl/showcase" -q
"$bin" timelapse fixture:newbie --every 2w --output-dir "$work/tl/newcomer" -q
for who in stevenbtw tj torvalds steipete; do
  "$bin" timelapse "$site/snapshots/$who.snapshot.json" --every 1m --output-dir "$work/tl/$who" -q
done
python3 "$site/mkvideo.py" "$work/tl" "$out"
