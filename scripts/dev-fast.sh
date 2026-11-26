#!/usr/bin/env bash
# Fast edit-compile-run loop for Brocode on macOS/Linux.
# Uses the `dev-fast` profile (no DWARF, 64 codegen units), skips Vello + BetterOffice +
# 6 MB font packs via `--no-default-features --features knobs`, and enables Luau hot reload.
#
# Usage:
#   scripts/dev-fast.sh [app args...]
#   OPENCODE_DEV_OFFICE=1 scripts/dev-fast.sh [app args...]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

FEATURES="knobs"
if [ "${OPENCODE_DEV_OFFICE:-0}" = "1" ]; then
  FEATURES="knobs,office"
fi

exec cargo run \
  --profile dev-fast \
  -p brocode-desktop \
  --no-default-features \
  --features "$FEATURES" \
  -- --live --plugin-hot "$@"
