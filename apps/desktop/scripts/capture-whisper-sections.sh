#!/bin/sh
# Shoots Experimental > Sidebar sections' story (Shell/Whisper sections) offscreen: each variant
# at rest in dark and light, and the Walkthrough at 60 fps.
# `capture-whisper-sections.sh [OUT]`.
set -eu
OUT=${1:-captures/whisper-sections}
mkdir -p "$OUT"
cargo build -p brocode-desktop
TARGET=$(cargo metadata --format-version 1 --no-deps | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
BIN="$TARGET/debug/brocode-desktop"
story() { "$BIN" --fixture apps/desktop/assets/fixture.json --playground "Shell/Whisper sections" --scale 2 "$@"; }
for theme in dark light; do
  story --theme "$theme" --variant "Default" --at 1.2 --snap "$OUT/default-$theme.png"
  story --theme "$theme" --variant "Multi-selected" --at 1.2 --snap "$OUT/selected-$theme.png"
  story --theme "$theme" --variant "Naming new section" --at 1.2 --snap "$OUT/naming-$theme.png"
  story --theme "$theme" --variant "Collapsed section" --at 1.2 --snap "$OUT/collapsed-$theme.png"
done
story --theme dark --variant "Walkthrough" --fps 60 --duration 7.4 --video "$OUT/walkthrough-dark.mp4"
