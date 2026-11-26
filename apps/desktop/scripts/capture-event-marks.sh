#!/bin/sh
# Stills of Timeline › Event marks (t1582) at rest, hovered and opened, plus Subagent rows › Reports.
# A debug build (release has no playground). $1 names the set.
set -eu
# Renders headlessly, but never on the desk Mac: it keeps its screen and cores.
case "$(hostname)" in *Studio*) echo "refusing on $(hostname)" >&2; exit 75;; esac
hostname
TAG=${1:-after}
OUT=captures/marks-$TAG
mkdir -p "$OUT"
cargo build --locked -p brocode-desktop
TARGET=$(cargo metadata --format-version 1 --no-deps | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
BIN="$TARGET/debug/brocode-desktop"
COMMON="--fixture apps/desktop/assets/fixture.json --size 1100x900 --scale 2"
# Points of the first subagent row, the shells' finished row and the Updates fold.
at() { printf '{"events":[{"t":1.2,"type":"%s","x":%s,"y":%s}]}' "$1" "$2" "$3" > "$OUT/$1-$2.json"; echo "$OUT/$1-$2.json"; }
snap() { "$BIN" $COMMON --playground 'Timeline/Event marks' "$@"; }
for theme in dark light; do
  for v in Subagents Shells Notices; do
    n=$(echo "$v" | tr 'A-Z' 'a-z')
    snap --variant "$v" --theme $theme --at 1.5 --snap "$OUT/$n-$theme.png"
  done
  snap --variant Subagents --theme $theme --script "$(at move 429 259)" --pointer --whole-window --at 1.5 --snap "$OUT/subagents-hover-$theme.png"
  snap --variant Shells --theme $theme --script "$(at move 291 561)" --pointer --whole-window --at 1.5 --snap "$OUT/shells-hover-$theme.png"
  snap --variant Shells --theme $theme --script "$(at tap 291 561)" --pointer --whole-window --at 2.5 --snap "$OUT/shells-open-$theme.png"
  snap --variant Notices --theme $theme --script "$(at tap 274 561)" --pointer --whole-window --at 2 --snap "$OUT/notices-open-$theme.png"
done
"$BIN" $COMMON --playground 'Timeline/Subagent rows' --variant Reports --theme dark --at 7 --snap "$OUT/reports-dark.png"
ls -la "$OUT"
