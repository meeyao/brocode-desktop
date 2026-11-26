#!/bin/sh
# Records Settings › Interface layout switching to Side tabs at 0.7 s and back to Top tabs at
# 2.2 s on the fixture, at 120 fps, plus a quarter-speed cut for review.
set -eu
OUT=${1:-captures/layout-switch}
mkdir -p "$OUT"
cargo build -p brocode-desktop
TARGET=$(cargo metadata --format-version 1 --no-deps | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
"$TARGET/debug/brocode-desktop" --fixture apps/desktop/assets/fixture.json \
  --scene settings --script apps/desktop/scripts/layout-switch.json --pointer \
  --size 1280x820 --scale 2 --theme dark --frames 420 --fps 120 \
  --video "$OUT/layout-switch.mp4" --trace "$OUT/layout-switch.jsonl" --trace-all
ffmpeg -y -loglevel error -i "$OUT/layout-switch.mp4" -vf "setpts=4*PTS" -r 30 "$OUT/layout-switch-quarter.mp4"
