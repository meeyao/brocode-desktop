#!/bin/sh
# Records scrolling up a session, switching to the next tab at 1.6 s (⌥↓) and back at 2.6 s
# (⌥↑) on the fixture, at 120 fps, plus a quarter-speed cut for review.
set -eu
OUT=${1:-captures/scroll-memory}
mkdir -p "$OUT"
cargo build -p brocode-desktop
TARGET=$(cargo metadata --format-version 1 --no-deps | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
"$TARGET/debug/brocode-desktop" --fixture apps/desktop/assets/fixture.json \
  --scene session --script apps/desktop/scripts/scroll-memory.json --pointer \
  --size 1280x820 --scale 2 --theme dark --frames 480 --fps 120 \
  --video "$OUT/scroll-memory.mp4" --trace "$OUT/scroll-memory.jsonl" --trace-all
ffmpeg -y -loglevel error -i "$OUT/scroll-memory.mp4" -vf "setpts=4*PTS" -r 30 "$OUT/scroll-memory-quarter.mp4"
