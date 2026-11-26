#!/bin/sh
# Launches a packaged Brocode.app with a throwaway HOME and an unreachable --server port
# for a few seconds and fails if it exits early. Verifies headless snapshot rendering first when
# OPENCODE_RUNE_SMOKE_HEADLESS=1 (for CI runners with no GUI WindowServer session).
#   apps/desktop/scripts/smoke-app.sh "<Brocode.app>"
set -eu

[ "$#" -eq 1 ] || { echo "Usage: $0 <Brocode.app>" >&2; exit 1; }
bundle=$1
seconds=${OPENCODE_RUNE_SMOKE_SECONDS:-10}
home=$(mktemp -d "${TMPDIR:-/tmp}/brocode-smoke.XXXXXX")
pid=
trap '[ -z "$pid" ] || kill -KILL "$pid" 2>/dev/null || true; rm -rf "$home"' EXIT HUP INT TERM

exe="$bundle/Contents/MacOS/brocode-desktop"
[ -x "$exe" ] || { echo "Missing executable $exe" >&2; exit 1; }

if [ "${OPENCODE_RUNE_SMOKE_HEADLESS:-0}" = "1" ]; then
  HOME="$home" "$exe" --server http://127.0.0.1:9 --scene no-server --snap "$home/no-server.png" > "$home/headless.log" 2>&1 || {
    cat "$home/headless.log" >&2 || true
    echo "Brocode headless snapshot smoke failed" >&2
    exit 1
  }
  [ -s "$home/no-server.png" ] || { echo "Expected non-empty $home/no-server.png" >&2; exit 1; }
  echo "Brocode rendered headless snapshot from $bundle."
  # The office viewer's engines load (from their library, when the bundle ships them as one).
  HOME="$home" "$exe" --office-check > "$home/office.log" 2>&1 || {
    cat "$home/office.log" >&2 || true
    echo "Brocode could not load the office viewer's engines" >&2
    exit 1
  }
  cat "$home/office.log"
  [ ! -e "$bundle/Contents/Frameworks/libopencode_office.dylib" ] || grep -q 'in their library, loaded' "$home/office.log" || {
    echo "The bundle ships the office library, but the exe does not load it" >&2
    exit 1
  }
  # A panic unwinds and is caught, a panicking thread ends alone (the app relies on both).
  HOME="$home" "$exe" --unwind-check || {
    echo "Brocode does not unwind a panic as it must" >&2
    exit 1
  }
  exit 0
fi

HOME="$home" "$exe" --server http://127.0.0.1:9 > "$home/output.log" 2>&1 &
pid=$!
sleep "$seconds"
if ! kill -0 "$pid" 2>/dev/null; then
  status=0
  wait "$pid" || status=$?
  pid=
  cat "$home/output.log" >&2 || true
  echo "Brocode smoke test failed: exited with status $status within $seconds seconds" >&2
  exit 1
fi
kill -TERM "$pid"
waited=0
while kill -0 "$pid" 2>/dev/null && [ "$waited" -lt 10 ]; do
  sleep 1
  waited=$((waited + 1))
done
if kill -0 "$pid" 2>/dev/null; then
  echo "Brocode was still running 10 seconds after SIGTERM; killed it." >&2
fi
echo "Brocode ran for $seconds seconds from $bundle."
