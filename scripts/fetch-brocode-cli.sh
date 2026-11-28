#!/usr/bin/env bash
# Downloads one brocode CLI build listed in https://dl.apibrother.com/latest.json, checks its
# SHA-256 against the manifest, and writes it into DEST as brocode-cli[.exe] with
# brocode-cli.version beside it: where the app looks for its bundled sidecar
# (apps/desktop/src/workspace/servers/sidecar/cli.rs).
#
#   scripts/fetch-brocode-cli.sh brocode-windows-x64.exe DEST
#   scripts/fetch-brocode-cli.sh brocode-linux-x64 DEST
#   scripts/fetch-brocode-cli.sh brocode-darwin-arm64 DEST
#
# BROCODE_BASE points at another mirror. Needs curl, jq and sha256sum (or shasum).
set -euo pipefail

name=${1:?usage: fetch-brocode-cli.sh NAME DEST}
dest=${2:?usage: fetch-brocode-cli.sh NAME DEST}
base=${BROCODE_BASE:-https://dl.apibrother.com}

manifest=$(curl -fsSL --retry 3 "$base/latest.json")
sha=$(jq -r --arg n "$name" '.files[] | select(.name == $n) | .sha256' <<<"$manifest")
if [ -z "$sha" ]; then
  echo "fetch-brocode-cli: $base/latest.json has no $name (it has: $(jq -r '[.files[].name] | join(", ")' <<<"$manifest"))" >&2
  exit 1
fi
version=$(jq -r .version <<<"$manifest")
case $name in *.exe) out=brocode-cli.exe ;; *) out=brocode-cli ;; esac

mkdir -p "$dest"
curl -fsSL --retry 3 -o "$dest/$out" "$base/$name"
got=$({ sha256sum "$dest/$out" 2>/dev/null || shasum -a 256 "$dest/$out"; } | cut -d' ' -f1)
if [ "$got" != "$sha" ]; then
  rm -f "$dest/$out"
  echo "fetch-brocode-cli: $name has SHA-256 $got, latest.json says $sha" >&2
  exit 1
fi
chmod +x "$dest/$out"
printf '%s\n' "$version" > "$dest/brocode-cli.version"
echo "fetch-brocode-cli: $name $version -> $dest/$out"
