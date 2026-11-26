#!/bin/sh
# Records the closed sidebar's hover peek on the fixture at 120 fps: in from the start edge at
# 0.6 s and out at 2.2 s (sidebar-peek.json), then the toggle docking, closing, and docking from
# a peek (sidebar-dock.json), each with a quarter-speed cut, plus the open peek and a strip of
# its first frames. `capture-sidebar-peek.sh [TAG] [OUT]`.
set -eu
TAG=${1:-peek}
OUT=${2:-captures/sidebar-peek}
mkdir -p "$OUT"
cargo build -p brocode-desktop
TARGET=$(cargo metadata --format-version 1 --no-deps | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
BIN="$TARGET/debug/brocode-desktop"
COMMON="--fixture apps/desktop/assets/fixture.json --scene sidebar-closed --pointer --size 1280x820 --scale 2 --theme dark"
$BIN $COMMON --script apps/desktop/scripts/sidebar-peek.json --fps 120 --video "$OUT/$TAG-peek.mp4" --trace "$OUT/$TAG-peek.jsonl" --trace-all
$BIN $COMMON --script apps/desktop/scripts/sidebar-dock.json --fps 120 --video "$OUT/$TAG-dock.mp4"
$BIN $COMMON --script apps/desktop/scripts/sidebar-peek.json --snap "$OUT/$TAG-open.png" --at 1.3
$BIN $COMMON --script apps/desktop/scripts/sidebar-peek.json --start 0.6 --frames 14 --every 2 --cols 7 --strip "$OUT/$TAG-in-strip.png" --crop 0,0,420,820
for clip in peek dock; do
  ffmpeg -y -loglevel error -i "$OUT/$TAG-$clip.mp4" -vf "setpts=4*PTS" -r 30 "$OUT/$TAG-$clip-quarter.mp4"
done
