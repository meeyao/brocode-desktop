#!/bin/sh
# Stills and a clip of the storybook's Timeline/Image gallery story, dark and light.
set -u
OUT=captures/gallery/${1:-shots}
mkdir -p "$OUT"
cargo build -p brocode-desktop || exit 1
TARGET=$(cargo metadata --format-version 1 --no-deps | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
BIN="$TARGET/debug/brocode-desktop"
shot() { # name variant at theme
  "$BIN" --playground 'Timeline/Image gallery' --variant "$2" --whole-window --size 1440x900 --scale 2 --theme "$4" --at "$3" --snap "$OUT/$1-$4.png" >"$OUT/$1-$4.log" 2>&1 || echo "FAILED $1 $4"
}
for theme in dark light; do
  shot gallery-comfortable Comfortable 1.0 $theme
  shot gallery-narrow Narrow 1.0 $theme
  shot gallery-wide Wide 1.0 $theme
  shot viewer-open Comfortable 2.0 $theme
  shot viewer-step1 Comfortable 2.8 $theme
  shot viewer-step3 Comfortable 4.4 $theme
  shot viewer-last Comfortable 6.9 $theme
  shot viewer-back Comfortable 7.6 $theme
  shot viewer-zoomed Comfortable 8.4 $theme
  shot viewer-fitted Comfortable 9.2 $theme
  shot viewer-closed Comfortable 10.2 $theme
done
"$BIN" --playground 'Timeline/Image gallery' --variant Comfortable --size 1440x900 --scale 1 --theme dark --at 2.0 --snap "$OUT/tree.png" --tree "$OUT/viewer-open-tree.json" >"$OUT/tree.log" 2>&1 || echo "FAILED tree"
"$BIN" --playground 'Timeline/Image gallery' --variant Comfortable --size 1440x900 --scale 1 --theme dark --duration 10.2 --fps 30 --video "$OUT/steps-dark.mp4" >"$OUT/video.log" 2>&1 || echo "FAILED video"
ls -la "$OUT"
