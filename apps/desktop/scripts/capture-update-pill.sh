#!/bin/sh
# Records Components › Update pill hovered (update-pill.json): the sidebar's pill in at 1 s and out at
# 2.6, the titlebar's in at 3.4 and out at 5, then the sidebar's left mid-open and hovered again
# while it closes. Each theme at 120 fps with a quarter-speed cut, a motion trace and stills.
# A debug build (release has no playground). `capture-update-pill.sh [TAG] [OUT]`.
set -eu
case "$(hostname)" in *Studio*) echo "refusing on $(hostname)" >&2; exit 75;; esac
TAG=${1:-after}
OUT=${2:-captures/update-pill}
mkdir -p "$OUT"
cargo build --locked -p brocode-desktop
TARGET=$(cargo metadata --format-version 1 --no-deps | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
BIN="$TARGET/debug/brocode-desktop"
COMMON="--fixture apps/desktop/assets/fixture.json --playground Components/Update\ pill --script apps/desktop/scripts/update-pill.json --pointer --size 1100x640 --scale 2"
for theme in dark light; do
  eval "$BIN" $COMMON --theme $theme --fps 120 --video "$OUT/$TAG-$theme.mp4" --trace "$OUT/$TAG-$theme.jsonl" --trace-all
  eval "$BIN" $COMMON --theme $theme --at 0.8 --snap "$OUT/$TAG-$theme-rest.png"
  eval "$BIN" $COMMON --theme $theme --at 2.0 --snap "$OUT/$TAG-$theme-side.png"
  eval "$BIN" $COMMON --theme $theme --at 4.4 --snap "$OUT/$TAG-$theme-top.png"
  ffmpeg -y -loglevel error -i "$OUT/$TAG-$theme.mp4" -vf "setpts=4*PTS" -r 60 "$OUT/$TAG-$theme-quarter.mp4"
done
# Installing (no arrow drop, the spinner in place) and Reduce Motion (everything lands at once).
eval "$BIN" $COMMON --theme dark --variant Installing --fps 60 --video "$OUT/$TAG-dark-installing.mp4"
eval "$BIN" $COMMON --theme dark --reduced-motion --fps 60 --video "$OUT/$TAG-dark-reduced.mp4"
ls -la "$OUT"
