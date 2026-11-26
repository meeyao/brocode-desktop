#!/bin/sh
set -eu
OUT=${LEASE_OUT:-captures/scrollbar}
mkdir -p "$OUT"
cargo build --locked --release -p brocode-desktop
TARGET=$(cargo metadata --format-version 1 --no-deps | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
BIN="$TARGET/release/brocode-desktop"
COMMON="--fixture apps/desktop/assets/fixture.json --scene session --zoom 1.6 --size 1280x820 --scale 2 --theme dark"
"$BIN" $COMMON --script apps/desktop/scripts/scrollbar.json --pointer --at 1.0 --snap "$OUT/scrolled.png"
"$BIN" $COMMON --script apps/desktop/scripts/scrollbar.json --pointer --at 3.5 --snap "$OUT/hover.png"
"$BIN" $COMMON --script apps/desktop/scripts/scrollbar.json --pointer --at 4.3 --snap "$OUT/drag.png"
"$BIN" $COMMON --script apps/desktop/scripts/scrollbar.json --pointer --frames 1560 --fps 240 --video "$OUT/scrollbar-240.mp4"
ls -la "$OUT"
