#!/bin/sh
# Records a streaming run stopped from the composer at zoom 1.6 on the fixture: Stop is pressed
# at 2.4 s and again at 3.0 s; the fixture's server confirms 1.2 s after the press.
set -eu
OUT=${1:-captures/stop}
mkdir -p "$OUT"
cargo build --release -p brocode-desktop
TARGET=$(cargo metadata --format-version 1 --no-deps | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
"$TARGET/release/brocode-desktop" --fixture apps/desktop/assets/fixture.json \
  --scene streaming --zoom 1.6 --script apps/desktop/scripts/stop.json --pointer \
  --size 1280x820 --scale 2 --theme dark --frames 360 --fps 60 \
  --video "$OUT/stop.mp4" --trace "$OUT/stop.jsonl" --trace-all
