#!/usr/bin/env bash
# Installs a build as "Brocode.app" in ~/Applications and relaunches it without the window
# blinking (the repo's scripts/mac-app). Pass app arguments after the binary path:
#   apps/desktop/scripts/bundle.sh target/release/brocode-desktop --live
#   MAC_APP_FLAGS="--wait-idle 20" apps/desktop/scripts/bundle.sh ...   (unattended rebuilds)
set -euo pipefail
cd "$(dirname "$0")/.."
BIN=${1:-../../target/release/brocode-desktop}
shift || true
test -x "$BIN" || { echo "no binary at $BIN (build with: cargo build --release -p brocode-desktop)" >&2; exit 1; }
BUNDLE_ID=${OPENCODE_RUNE_BUNDLE_ID:-com.apibrother.brocode.dev}
# shellcheck disable=SC2086
exec ../../scripts/mac-app --name "Brocode" --id "$BUNDLE_ID" --exe brocode-desktop \
  --bin "$BIN" --icon assets/icon.icns ${MAC_APP_FLAGS:-} -- "$@"
