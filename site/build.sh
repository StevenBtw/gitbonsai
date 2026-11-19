#!/usr/bin/env bash
# Builds gitbonsai.dev into a folder (default _site): the page, the hero trees
# and a timelapse video per example and theme. Needs a release build of
# gitbonsai (or GITBONSAI=<path>), python3 with cairosvg and pillow, ffmpeg,
# and the TypeScript compiler (`npm ci --prefix site`).
#
#   cargo build --release && npm ci --prefix site && bash site/build.sh _site
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
cp "$site/og.png" "$out/og.png"   # link-preview card; bump ?v= in index.html when it changes
"$site/node_modules/.bin/tsc" -p "$site" --outDir "$work/js"
cp "$work/js/app.js" "$site/index.html" "$site/style.css" "$site/tokens.css" "$site/fonts.css" "$site/CNAME" "$work/"
for f in index.html app.js style.css tokens.css fonts.css CNAME; do
  sed "s/@VERSION@/$version/g" "$work/$f" > "$out/$f"
done
# The whole example workflow, so what people copy is what the README links.
python3 - "$out/index.html" "$site/../examples/profile-workflow.yml" <<'EOF'
import html, sys
page, workflow = sys.argv[1], sys.argv[2]
text = open(page, encoding="utf-8").read()
assert text.count("@WORKFLOW@") == 1, "index.html needs one @WORKFLOW@"
wf = open(workflow, encoding="utf-8").read().rstrip("\n")
open(page, "w", encoding="utf-8", newline="\n").write(text.replace("@WORKFLOW@", html.escape(wf, quote=False)))
EOF

# The hero's plates, in gallery order; the page opens on the Showcase. Each tree
# is rendered once; hero.py writes plates.json (the label) and the thumbnails.
plate() {
  local id=$1 name=$2
  shift 2
  "$bin" render "$@" --output-dir "$work/hero/$id" -q
  for t in light dark; do
    cp "$work/hero/$id/bonsai-$t.svg" "$out/hero/$id-$t.svg"
  done
  printf '%s\t%s\n' "$id" "$name" >> "$work/plates.tsv"
}
plate sprout Sprout fixture:newbie --as-of 2026-08-01
plate newcomer Newcomer fixture:newbie
# The Showcase wears a terracotta glaze, so the examples show more than one pot.
plate showcase Showcase fixture:showcase --config "$site/showcase.toml"
for who in stevenbtw:@StevenBtw jacobwilliams:@jacobwilliams tj:@tj torvalds:@torvalds steipete:@steipete; do
  plate "${who%%:*}" "${who#*:}" "$site/snapshots/${who%%:*}.snapshot.json"
done
python3 "$site/hero.py" "$work/plates.tsv" "$out"

"$bin" timelapse fixture:showcase --config "$site/showcase.toml" --every 1m --output-dir "$work/tl/showcase" -q
for who in stevenbtw tj torvalds steipete; do
  "$bin" timelapse "$site/snapshots/$who.snapshot.json" --every 1m --output-dir "$work/tl/$who" -q
done
python3 "$site/mkvideo.py" "$work/tl" "$out"
