#!/bin/sh
# Stamps the release version into apps/desktop/Cargo.toml and Cargo.lock so
# `brocode-desktop --version` reports it. Release/PR CI builds only; never committed.
#
#   apps/desktop/scripts/set-crate-version.sh <source root> <version>
set -eu

root=$1
version=$2
manifest="$root/apps/desktop/Cargo.toml"
current=$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$manifest" | head -1)
[ "$version" = "$current" ] && exit 0
printf '%s\n' "$version" | grep -Eq "^(0\.0\.0-dev-[1-9][0-9]*|0\.0\.0-pr-[1-9][0-9]*-[1-9][0-9]*|0\.0\.0-branch-[1-9][0-9]*|[0-9]+\.[0-9]+\.[0-9]+(-beta\.[0-9]+)?)\$" || {
  echo "Refusing unexpected version string: $version" >&2
  exit 1
}
awk -v current="$current" -v version="$version" '
  !done && $0 == "version = \"" current "\"" { $0 = "version = \"" version "\""; done = 1 }
  { print }
' "$manifest" > "$manifest.next"
mv "$manifest.next" "$manifest"
awk -v current="$current" -v version="$version" '
  /^name = "brocode-desktop"$/ { package = 1 }
  package && $0 == "version = \"" current "\"" { $0 = "version = \"" version "\""; package = 0 }
  { print }
' "$root/Cargo.lock" > "$root/Cargo.lock.next"
mv "$root/Cargo.lock.next" "$root/Cargo.lock"
grep -q "^version = \"$version\"" "$manifest"
grep -A1 '^name = "brocode-desktop"$' "$root/Cargo.lock" | grep -q "^version = \"$version\"$"
echo "Building Brocode as $version." >&2
