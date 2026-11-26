#!/bin/sh
# Stills of Composer › Quote cards, every variant, dark and light (a debug build: release has no
# playground). $1 names the set.
set -eu
TAG=${1:-before}
OUT=captures/quote-$TAG
rm -rf "$OUT"
mkdir -p "$OUT"
cargo build --locked -p brocode-desktop
TARGET=$(cargo metadata --format-version 1 --no-deps | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
BIN="$TARGET/debug/brocode-desktop"
for theme in dark light; do
  for v in One Three Long "Cut mid-word"; do
    name=$(echo "$v" | tr 'A-Z ' 'a-z-')
    "$BIN" --fixture apps/desktop/assets/fixture.json --playground 'Composer/Quote cards' --variant "$v" \
      --size 1600x1100 --scale 2 --theme $theme --at 1.5 --snap "$OUT/$name-$theme.png"
  done
done
ls -la "$OUT"
