#!/usr/bin/env bash
# Trains the Apple silicon release profile (apps/desktop/pgo/aarch64-apple-darwin.profdata, its
# stamp) and the link order (apps/desktop/pgo/startup-order.txt) on a Mac, from the training exe
# that PR CI builds. No build on the Mac; about four minutes.
#
#   apps/desktop/scripts/pgo-train-macos.sh <the training .app> [<server URL>]
#
# The app is the build-brocode-pr-macos-arm64 artifact of a draft pull request from a branch
# with "order-train" in its name (build-app.sh builds that one instrumented), unzipped. Run this
# from a checkout of that pull request's commit: the fixture and the input scripts are read from
# this tree, and the stamp names its commit. Never install that build.
#
# What it runs, each part a process of its own in a scratch root (your own profile, settings and
# saved windows are not read or written):
#   with a window (5 processes):
#     first        the first launch of a new profile, to its content
#     again 1, 2   later launches of that profile, to their content
#     input 1, 2   a launch, then the window replays pgo-train-window.json (--window-script): wheel
#                  scrolls and flings in the timeline, a line typed into the composer, undo, redo,
#                  select all, delete, the model picker opened and filtered, the pointer over the
#                  sidebar. Nothing is sent.
#     With a server URL these open an OpenCode server's sessions (--server; its password in
#     OPENCODE_SERVER_PASSWORD); without one, the fixture's session scene.
#   headless (14 processes, no window), on the fixture, in English and German: Home, a session
#     typed in (undo-redo.json) and scrolled (scroll.json), the demo (an agent turn streams),
#     the queue, Settings, tab badges.
# Office previews, the terminal and the browser pane are not in it: they build as cold code.
#
# Needs the llvm-profdata of the toolchain that built the exe (`rustup component add llvm-tools`,
# or LLVM_PROFDATA=<path>); Xcode's cannot read the raw profiles.
# OPENCODE_RUNE_TRAIN_LAUNCH=<command> starts the windowed parts instead of `open`: it is called as
#   <command> <app> <K=V> ... -- <app arguments>   and must return when the app has quit.
# OPENCODE_RUNE_TRAIN_RAW=<folder> keeps the raw profiles there (default: a temporary folder).
set -euo pipefail
[ $# -ge 1 ] || { sed -n '2,8p' "$0" >&2; exit 2; }
APP=$(cd "$1" && pwd)
SERVER=${2:-}
ROOT=$(cd "$(dirname "$0")/../../.." && pwd)
EXE="$APP/Contents/MacOS/brocode-desktop"
# rustc is the repository's (rust-toolchain.toml), whatever folder this is called from.
cd "$ROOT"
[ -x "$EXE" ] || { echo "no exe at $EXE" >&2; exit 1; }
profdata=${LLVM_PROFDATA:-$(rustc --print sysroot)/lib/rustlib/$(rustc -vV | sed -n 's/^host: //p')/bin/llvm-profdata}
[ -x "$profdata" ] || { echo "no llvm-profdata at $profdata: rustup component add llvm-tools, or set LLVM_PROFDATA" >&2; exit 1; }
codesign --verify --deep --strict "$APP" || { echo "$APP does not pass codesign --verify: macOS would refuse to open it" >&2; exit 1; }

work=$(mktemp -d -t pgo-train)
trap 'rm -rf "$work"' EXIT
raw=${OPENCODE_RUNE_TRAIN_RAW:-$work/raw}
rm -rf "$raw"
mkdir -p "$raw"
scripts=$ROOT/apps/desktop/scripts

# Every folder the app writes is under <root>.
sealed() {
  local r=$1
  mkdir -p "$r/home" "$r/tmp" "$r/xdg/config/opencode" "$r/xdg/state" "$r/xdg/data" "$r/xdg/cache"
  SEAL=(RUNE_DIRS_ROOT="$r" OPENCODE_RUNE_HOME="$r" HOME="$r/home" TMPDIR="$r/tmp" XDG_CONFIG_HOME="$r/xdg/config"
    XDG_STATE_HOME="$r/xdg/state" XDG_DATA_HOME="$r/xdg/data" XDG_CACHE_HOME="$r/xdg/cache"
    OPENCODE_CONFIG_DIR="$r/xdg/config/opencode" OPENCODE_RUNE_OTEL=0 OPENCODE_BROWSER_MOCK_KEYCHAIN=1
    OPENCODE_RUNE_HOLD_UPDATER_DORMANT=1 PATH=/usr/bin:/bin:/usr/sbin:/sbin)
  [ -z "${OPENCODE_SERVER_PASSWORD:-}" ] || SEAL+=(OPENCODE_SERVER_PASSWORD="$OPENCODE_SERVER_PASSWORD")
}

# window <name> <ms until the app quits by itself> [app arguments]: one windowed part.
window() {
  local name=$1 stay=$2
  shift 2
  sealed "$work/window"
  local env=("${SEAL[@]}" LLVM_PROFILE_FILE="$raw/$name-%p.profraw")
  local args=(--launch-trace "$work/window/trace-$name.jsonl" --launch-trace-quit "$stay" "$@")
  if [ -n "$SERVER" ]; then
    args=(--server "$SERVER" "${args[@]}")
  else
    args=(--fixture "$ROOT/apps/desktop/assets/fixture.json" --scene session "${args[@]}")
  fi
  if [ -n "${OPENCODE_RUNE_TRAIN_LAUNCH:-}" ]; then
    $OPENCODE_RUNE_TRAIN_LAUNCH "$APP" "${env[@]}" -- "${args[@]}"
  else
    local flags=()
    for e in "${env[@]}"; do flags+=(--env "$e"); done
    open -n -W -a "$APP" "${flags[@]}" --args "${args[@]}"
  fi
  ls "$raw/$name"-*.profraw > /dev/null 2>&1 || { echo "$name wrote no profile" >&2; exit 1; }
  echo "$name: done" >&2
}

# headless <name> <scene> <language> [<input script> [<seconds>]]: one part without a window.
headless() {
  local name=$1 scene=$2 lang=$3 script=${4:-} at=${5:-4}
  sealed "$work/headless/$name"
  local args=(--fixture assets/fixture.json --scene "$scene" --language "$lang" --size 1280x800 --scale 1 --theme light --snap "$work/headless/$name/snap.png")
  if [ -n "$script" ]; then args+=(--script "$scripts/$script" --at "$at"); else args+=(--at 2); fi
  (cd "$ROOT/apps/desktop" && env -i "${SEAL[@]}" LLVM_PROFILE_FILE="$raw/$name-%p.profraw" "$EXE" "${args[@]}" > "$work/headless/$name/out.txt" 2>&1) ||
    { echo "$name failed:" >&2; tail -5 "$work/headless/$name/out.txt" >&2; exit 1; }
  [ -s "$work/headless/$name/snap.png" ] || { echo "$name drew nothing" >&2; exit 1; }
  echo "$name: done" >&2
}

window first 5000
window again-1 4000
window again-2 4000
window input-1 15000 --window-script "$scripts/pgo-train-window.json"
window input-2 15000 --window-script "$scripts/pgo-train-window.json"
for lang in en de; do
  headless "home-$lang" home "$lang"
  headless "session-type-$lang" session "$lang" undo-redo.json
  headless "session-scroll-$lang" session "$lang" scroll.json
  headless "demo-$lang" demo "$lang" send.json 12
  headless "queue-$lang" queue "$lang" queue.json
  headless "settings-$lang" settings "$lang"
  headless "tabs-$lang" tab-badges "$lang"
done

count=$(ls "$raw"/*.profraw | wc -l | tr -d ' ')
where="on the fixture"
[ -z "$SERVER" ] || where="against a server"
out=$ROOT/apps/desktop/pgo/aarch64-apple-darwin
# The launch order has its own file (startup-order.txt): the profile carries no traces.
"$profdata" merge --sparse --temporal-profile-trace-reservoir-size=0 -o "$out.profdata" "$raw"/*.profraw
{
  echo "commit $(git -C "$ROOT" rev-parse HEAD)"
  sed -n 's/^rune = .*rev = "\([0-9a-f]*\)".*/rune \1/p' "$ROOT/Cargo.toml"
  echo "rustc $(rustc -V)"
  echo "processes $count"
  echo "parts 5 with a window (the first launch, two later launches, two with scrolling and typing; $where), 14 headless (Home, session typing and scrolling, demo, queue, Settings, tab badges; English and German)"
  "$profdata" show "$out.profdata" | grep -E '^Total functions: '
} > "$out.txt"
cat "$out.txt" >&2
# The link order, from the launches alone.
OPENCODE_RUNE_TRAINED_AT=$(git -C "$ROOT" rev-parse HEAD) LLVM_PROFDATA=$profdata "$scripts/startup-order.sh" "$raw"/first-*.profraw "$raw"/again-*.profraw
